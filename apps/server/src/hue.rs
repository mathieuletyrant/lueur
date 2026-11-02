//! Philips Hue bridge: REST API v2 (pairing, entertainment areas,
//! start/stop) and the Entertainment stream over DTLS 1.2 PSK on UDP 2100.

use crate::color::{Rgb, linear_to_srgb};
use anyhow::{Context, Result, anyhow, bail};
use openssl::ssl::{HandshakeError, Ssl, SslContext, SslMethod, SslOptions, SslStream, SslVerifyMode, SslVersion};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::UdpSocket;
use std::time::Duration;

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        // The bridge presents a certificate signed by Signify, for its ID rather than its IP.
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(5))
        .build()
        .expect("HTTP client")
}

#[derive(Debug, Clone)]
pub struct Credentials {
    pub username: String,
    pub clientkey: String,
}

/// Pairing: call after pressing the bridge's link button.
/// Returns `Ok(None)` while the button has not been pressed.
pub async fn pair(host: &str, base: Option<&str>) -> Result<Option<Credentials>> {
    let url = format!("{}/api", base.map(str::to_string).unwrap_or_else(|| format!("https://{host}")));
    let res: Value = http()
        .post(url)
        .json(&json!({"devicetype": "lueur#umbrel", "generateclientkey": true}))
        .send()
        .await?
        .json()
        .await?;
    let first = res.get(0).ok_or_else(|| anyhow!("unexpected response: {res}"))?;
    if let Some(s) = first.get("success") {
        return Ok(Some(Credentials {
            username: s["username"].as_str().unwrap_or_default().into(),
            clientkey: s["clientkey"].as_str().unwrap_or_default().into(),
        }));
    }
    match first["error"]["type"].as_i64() {
        Some(101) => Ok(None),
        _ => bail!("pairing refused: {}", first["error"]["description"]),
    }
}

/// A bridge found on the network.
#[derive(Debug, Clone, Serialize)]
pub struct Found {
    pub ip: String,
    pub id: Option<String>,
    pub name: Option<String>,
    pub model: Option<String>,
}

/// Looks for bridges through the Hue cloud (works from inside Docker) and
/// mDNS (works without internet, needs the host network), in parallel.
pub async fn discover() -> Vec<Found> {
    let cloud = async {
        #[derive(Deserialize)]
        struct Entry {
            id: String,
            internalipaddress: String,
        }
        let r: Vec<Entry> = http().get("https://discovery.meethue.com").send().await.ok()?.json().await.ok()?;
        Some(r.into_iter().map(|e| (e.internalipaddress, Some(e.id))).collect::<Vec<_>>())
    };
    let mdns = tokio::task::spawn_blocking(|| mdns(Duration::from_secs(3)));
    let (cloud, mdns) = tokio::join!(cloud, mdns);
    let mut by_ip: BTreeMap<String, Option<String>> = BTreeMap::new();
    for (ip, id) in cloud.unwrap_or_default().into_iter().chain(mdns.unwrap_or_default()) {
        let e = by_ip.entry(ip).or_default();
        if e.is_none() {
            *e = id;
        }
    }
    let infos: Vec<_> = by_ip
        .into_iter()
        .map(|(ip, id)| {
            tokio::spawn(async move {
                let c = info(&ip).await;
                Found {
                    id: c.as_ref().and_then(|c| c["bridgeid"].as_str().map(str::to_lowercase)).or(id),
                    name: c.as_ref().and_then(|c| c["name"].as_str().map(String::from)),
                    model: c.as_ref().and_then(|c| c["modelid"].as_str().map(String::from)),
                    ip,
                }
            })
        })
        .collect();
    let mut found = Vec::new();
    for f in infos {
        found.extend(f.await.ok());
    }
    found
}

fn mdns(wait: Duration) -> Vec<(String, Option<String>)> {
    let Ok(d) = mdns_sd::ServiceDaemon::new() else {
        return vec![];
    };
    let Ok(rx) = d.browse("_hue._tcp.local.") else {
        return vec![];
    };
    let end = std::time::Instant::now() + wait;
    let mut out = Vec::new();
    while let Some(left) = end.checked_duration_since(std::time::Instant::now()) {
        let Ok(ev) = rx.recv_timeout(left) else { break };
        if let mdns_sd::ServiceEvent::ServiceResolved(s) = ev {
            let id = s.get_property_val_str("bridgeid").map(str::to_lowercase);
            out.extend(s.get_addresses_v4().into_iter().map(|ip| (ip.to_string(), id.clone())));
        }
    }
    d.shutdown().ok();
    out
}

/// Public bridge config (name, model, id): answers without credentials.
pub async fn info(host: &str) -> Option<Value> {
    let c: Value = http().get(format!("https://{host}/api/0/config")).send().await.ok()?.json().await.ok()?;
    c.get("bridgeid").is_some().then_some(c)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Channel {
    pub channel_id: u8,
    pub position: Position,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Metadata {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    /// `https://<bridge>`, or another URL for tests.
    base: String,
    creds: Credentials,
    http: reqwest::Client,
}

impl Bridge {
    pub fn new(host: &str, creds: Credentials, base: Option<&str>) -> Self {
        Self { base: base.map(str::to_string).unwrap_or_else(|| format!("https://{host}")), creds, http: http() }
    }

    async fn clip(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> Result<Value> {
        let mut req = self
            .http
            .request(method, format!("{}/clip/v2/resource/{path}", self.base))
            .header("hue-application-key", &self.creds.username);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let v: Value = req.send().await.context("Hue bridge unreachable")?.json().await?;
        if let Some(errs) = v["errors"].as_array().filter(|e| !e.is_empty()) {
            bail!("bridge error: {}", errs[0]["description"]);
        }
        Ok(v)
    }

    pub async fn areas(&self) -> Result<Vec<Area>> {
        let v = self.clip(reqwest::Method::GET, "entertainment_configuration", None).await?;
        Ok(serde_json::from_value(v["data"].clone())?)
    }

    /// Area by name (case-insensitive) or id; the only area when `want` is empty.
    pub async fn find_area(&self, want: Option<&str>) -> Result<Area> {
        let areas = self.areas().await?;
        match want {
            Some(w) => areas
                .into_iter()
                .find(|a| a.id == w || a.metadata.name.eq_ignore_ascii_case(w))
                .ok_or_else(|| anyhow!("entertainment area \"{w}\" not found")),
            None if areas.len() == 1 => Ok(areas.into_iter().next().unwrap()),
            None if areas.is_empty() => bail!("no entertainment area: create one in the Hue app"),
            None => bail!(
                "several areas, set hue.area: {}",
                areas.iter().map(|a| a.metadata.name.as_str()).collect::<Vec<_>>().join(", ")
            ),
        }
    }

    pub async fn set_streaming(&self, area_id: &str, on: bool) -> Result<()> {
        let action = if on { "start" } else { "stop" };
        self.clip(
            reqwest::Method::PUT,
            &format!("entertainment_configuration/{area_id}"),
            Some(json!({"action": action})),
        )
        .await?;
        Ok(())
    }

    pub fn credentials(&self) -> &Credentials {
        &self.creds
    }
}

/// Connected UDP socket, exposed as Read/Write so OpenSSL can run DTLS over it.
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

/// Open Entertainment stream to the bridge.
pub struct Stream {
    ssl: SslStream<Udp>,
    area_id: String,
    seq: u8,
    packet: Vec<u8>,
}

impl Stream {
    /// Blocking (DTLS handshake): run it outside the async loop.
    pub fn connect(addr: &str, creds: &Credentials, area_id: &str) -> Result<Self> {
        let psk = hex::decode(&creds.clientkey).context("invalid clientkey (hex expected)")?;
        let identity = creds.username.clone();
        let mut ctx = SslContext::builder(SslMethod::dtls())?;
        ctx.set_verify(SslVerifyMode::NONE);
        ctx.set_min_proto_version(Some(SslVersion::DTLS1_2))?;
        ctx.set_max_proto_version(Some(SslVersion::DTLS1_2))?;
        ctx.set_cipher_list("PSK-AES128-GCM-SHA256")?;
        // Our "BIO" is a plain Read/Write: OpenSSL cannot query the MTU.
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
        sock.connect(addr).with_context(|| format!("UDP connection to {addr}"))?;
        sock.set_read_timeout(Some(Duration::from_millis(1500)))?;

        let mut attempt = ssl.connect(Udp(sock));
        let mut tries = 0;
        let ssl = loop {
            match attempt {
                Ok(s) => break s,
                // Timed out: try again, OpenSSL retransmits the last flight.
                Err(HandshakeError::WouldBlock(mid)) if tries < 6 => {
                    tries += 1;
                    attempt = mid.handshake();
                }
                Err(e) => bail!("DTLS handshake with the bridge failed: {e}"),
            }
        };
        Ok(Self { ssl, area_id: area_id.into(), seq: 0, packet: Vec::with_capacity(52 + 7 * 20) })
    }

    /// Sends one frame: one color (linear light) per channel.
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

/// HueStream v2 packet (RGB color space, 16 bits per component).
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
