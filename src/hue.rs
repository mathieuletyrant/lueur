//! Pont Philips Hue : API REST v2 (appairage, zones de divertissement,
//! start/stop) et flux Entertainment en DTLS 1.2 PSK sur UDP 2100.

use crate::color::{linear_to_srgb, Rgb};
use anyhow::{anyhow, bail, Context, Result};
use openssl::ssl::{HandshakeError, Ssl, SslContext, SslMethod, SslOptions, SslStream, SslVerifyMode, SslVersion};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::UdpSocket;
use std::time::Duration;

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        // Le pont présente un certificat signé par Signify, pour son ID et pas son IP.
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client HTTP")
}

#[derive(Debug, Clone)]
pub struct Credentials {
    pub username: String,
    pub clientkey: String,
}

/// Appairage : à appeler après un appui sur le bouton du pont.
/// `Ok(None)` tant que le bouton n'a pas été pressé.
pub async fn pair(host: &str, base: Option<&str>) -> Result<Option<Credentials>> {
    let url = format!("{}/api", base.map(str::to_string).unwrap_or_else(|| format!("https://{host}")));
    let res: Value = http()
        .post(url)
        .json(&json!({"devicetype": "lueur#umbrel", "generateclientkey": true}))
        .send()
        .await?
        .json()
        .await?;
    let first = res.get(0).ok_or_else(|| anyhow!("réponse inattendue : {res}"))?;
    if let Some(s) = first.get("success") {
        return Ok(Some(Credentials {
            username: s["username"].as_str().unwrap_or_default().into(),
            clientkey: s["clientkey"].as_str().unwrap_or_default().into(),
        }));
    }
    match first["error"]["type"].as_i64() {
        Some(101) => Ok(None),
        _ => bail!("appairage refusé : {}", first["error"]["description"]),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Channel {
    pub channel_id: u8,
    pub position: Position,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Metadata {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Area {
    pub id: String,
    pub metadata: Metadata,
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub configuration_type: String,
}

pub struct Bridge {
    /// `https://<pont>`, ou une autre URL pour les tests.
    base: String,
    creds: Credentials,
    http: reqwest::Client,
}

impl Bridge {
    pub fn new(host: &str, creds: Credentials, base: Option<&str>) -> Self {
        Self {
            base: base.map(str::to_string).unwrap_or_else(|| format!("https://{host}")),
            creds,
            http: http(),
        }
    }

    async fn clip(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> Result<Value> {
        let mut req = self
            .http
            .request(method, format!("{}/clip/v2/resource/{path}", self.base))
            .header("hue-application-key", &self.creds.username);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let v: Value = req.send().await.context("pont Hue injoignable")?.json().await?;
        if let Some(errs) = v["errors"].as_array().filter(|e| !e.is_empty()) {
            bail!("erreur du pont : {}", errs[0]["description"]);
        }
        Ok(v)
    }

    pub async fn areas(&self) -> Result<Vec<Area>> {
        let v = self.clip(reqwest::Method::GET, "entertainment_configuration", None).await?;
        Ok(serde_json::from_value(v["data"].clone())?)
    }

    /// Zone par nom (insensible à la casse) ou par id ; la seule zone si `want` est vide.
    pub async fn find_area(&self, want: Option<&str>) -> Result<Area> {
        let areas = self.areas().await?;
        match want {
            Some(w) => areas
                .into_iter()
                .find(|a| a.id == w || a.metadata.name.eq_ignore_ascii_case(w))
                .ok_or_else(|| anyhow!("zone de divertissement « {w} » introuvable")),
            None if areas.len() == 1 => Ok(areas.into_iter().next().unwrap()),
            None if areas.is_empty() => bail!("aucune zone de divertissement : créez-en une dans l'app Hue"),
            None => bail!(
                "plusieurs zones, précisez hue.area : {}",
                areas.iter().map(|a| a.metadata.name.as_str()).collect::<Vec<_>>().join(", ")
            ),
        }
    }

    pub async fn set_streaming(&self, area_id: &str, on: bool) -> Result<()> {
        let action = if on { "start" } else { "stop" };
        self.clip(reqwest::Method::PUT, &format!("entertainment_configuration/{area_id}"), Some(json!({"action": action})))
            .await?;
        Ok(())
    }

    pub fn credentials(&self) -> &Credentials {
        &self.creds
    }
}

/// UDP connecté, exposé en Read/Write pour qu'OpenSSL y fasse passer le DTLS.
#[derive(Debug)]
struct Udp(UdpSocket);

impl Read for Udp {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.recv(buf)
    }
}

impl Write for Udp {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.send(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Flux Entertainment ouvert vers le pont.
pub struct Stream {
    ssl: SslStream<Udp>,
    area_id: String,
    seq: u8,
    packet: Vec<u8>,
}

impl Stream {
    /// Bloquant (handshake DTLS) : à lancer hors de la boucle async.
    pub fn connect(addr: &str, creds: &Credentials, area_id: &str) -> Result<Self> {
        let psk = hex::decode(&creds.clientkey).context("clientkey invalide (hexadécimal attendu)")?;
        let identity = creds.username.clone();
        let mut ctx = SslContext::builder(SslMethod::dtls())?;
        ctx.set_verify(SslVerifyMode::NONE);
        ctx.set_min_proto_version(Some(SslVersion::DTLS1_2))?;
        ctx.set_max_proto_version(Some(SslVersion::DTLS1_2))?;
        ctx.set_cipher_list("PSK-AES128-GCM-SHA256")?;
        // Notre « BIO » est un simple Read/Write : OpenSSL ne peut pas sonder la MTU.
        ctx.set_options(SslOptions::NO_QUERY_MTU);
        ctx.set_psk_client_callback(move |_ssl, _hint, id_buf, psk_buf| {
            let id = identity.as_bytes();
            if id.len() + 1 > id_buf.len() || psk.len() > psk_buf.len() {
                return Err(openssl::error::ErrorStack::get());
            }
            id_buf[..id.len()].copy_from_slice(id);
            id_buf[id.len()] = 0;
            psk_buf[..psk.len()].copy_from_slice(&psk);
            Ok(psk.len())
        });
        let ctx = ctx.build();
        let mut ssl = Ssl::new(&ctx)?;
        ssl.set_mtu(1200)?;

        let sock = UdpSocket::bind("0.0.0.0:0")?;
        sock.connect(addr).with_context(|| format!("connexion UDP à {addr}"))?;
        sock.set_read_timeout(Some(Duration::from_millis(1500)))?;

        let mut attempt = ssl.connect(Udp(sock));
        let mut tries = 0;
        let ssl = loop {
            match attempt {
                Ok(s) => break s,
                // Délai dépassé : on relance, OpenSSL retransmet la dernière salve.
                Err(HandshakeError::WouldBlock(mid)) if tries < 6 => {
                    tries += 1;
                    attempt = mid.handshake();
                }
                Err(e) => bail!("handshake DTLS avec le pont échoué : {e}"),
            }
        };
        Ok(Self { ssl, area_id: area_id.into(), seq: 0, packet: Vec::with_capacity(52 + 7 * 20) })
    }

    /// Envoie une image : une couleur (lumière linéaire) par canal.
    pub fn send(&mut self, colors: &[(u8, Rgb)]) -> Result<()> {
        encode(&mut self.packet, &self.area_id, self.seq, colors);
        self.seq = self.seq.wrapping_add(1);
        self.ssl.write_all(&self.packet)?;
        Ok(())
    }

    pub fn close(mut self) {
        self.ssl.shutdown().ok();
    }
}

/// Paquet HueStream v2 (espace RGB, 16 bits par composante).
pub fn encode(buf: &mut Vec<u8>, area_id: &str, seq: u8, colors: &[(u8, Rgb)]) {
    buf.clear();
    buf.extend_from_slice(b"HueStream");
    buf.extend_from_slice(&[0x02, 0x00, seq, 0x00, 0x00, 0x00, 0x00]);
    buf.extend_from_slice(area_id.as_bytes());
    for (id, c) in colors.iter().take(20) {
        buf.push(*id);
        for v in c {
            let v = (linear_to_srgb(*v) * 65535.0).round() as u16;
            buf.extend_from_slice(&v.to_be_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn huestream_layout() {
        let id = "1a8d99cc-967b-44f2-9202-43f976c0fa6b";
        let mut b = Vec::new();
        encode(&mut b, id, 7, &[(0, [1.0, 0.0, 0.0]), (3, [0.0, 0.0, 1.0])]);
        assert_eq!(&b[..9], b"HueStream");
        assert_eq!(&b[9..16], &[2, 0, 7, 0, 0, 0, 0]);
        assert_eq!(&b[16..52], id.as_bytes());
        assert_eq!(&b[52..59], &[0, 0xff, 0xff, 0, 0, 0, 0]);
        assert_eq!(&b[59..66], &[3, 0, 0, 0, 0, 0xff, 0xff]);
    }
}
