//! A WLED Strip around the TV, driven over DDP (UDP): one color per LED,
//! straight to the controller, no bridge in between.

use crate::color::Rgb;
use crate::config::WledConfig;
use crate::hue::{Channel, Position};
use crate::lights::Sink;
use anyhow::{Context, Result, bail};
use tokio::net::UdpSocket;

const DDP_PORT: u16 = 4048;

/// One Channel per LED, in wiring order, placed on the screen edge it sits
/// behind (Hue coordinates, so the renderer maps a Strip like an Area).
// ponytail: the LED index is the u8 channel id, so 256 LEDs at most; widen `channel_id` for longer strips (#31).
pub fn channels(cfg: &WledConfig) -> Result<Vec<Channel>> {
    // Corners and sides clockwise, seen from the couch: side i runs from corner i to corner i + 1.
    const CORNERS: [(f32, f32); 4] = [(-1.0, -1.0), (-1.0, 1.0), (1.0, 1.0), (1.0, -1.0)];
    let leds = [cfg.left, cfg.top, cfg.right, cfg.bottom];
    let start = cfg.start as usize;
    let mut out = Vec::new();
    for k in 0..4 {
        let (side, from, to) = if cfg.clockwise {
            let s = (start + k) % 4;
            (s, s, (s + 1) % 4)
        } else {
            let s = (start + 3 - k) % 4;
            (s, (s + 1) % 4, s)
        };
        let ((x0, z0), (x1, z1)) = (CORNERS[from], CORNERS[to]);
        for j in 0..leds[side] {
            let f = (j as f32 + 0.5) / leds[side] as f32;
            let id = u8::try_from(out.len()).ok().context("a Strip longer than 256 LEDs isn't supported yet")?;
            let position = Position { x: x0 + (x1 - x0) * f, y: 0.0, z: z0 + (z1 - z0) * f };
            out.push(Channel { channel_id: id, position });
        }
    }
    if out.is_empty() {
        bail!("the WLED Strip has no LEDs: set its LEDs per side");
    }
    Ok(out)
}

/// The controller's real-time input. WLED takes the strip over at the first
/// frame and goes back to its own state when frames stop (its real-time
/// timeout, 2.5 s by default).
pub struct WledSink {
    host: String,
    gamma: f32,
    socket: Option<UdpSocket>,
    packet: Vec<u8>,
}

impl WledSink {
    pub fn new(cfg: &WledConfig) -> Self {
        Self { host: cfg.host.clone(), gamma: cfg.gamma, socket: None, packet: Vec::new() }
    }
}

impl Sink for WledSink {
    async fn open(&mut self) -> Result<()> {
        let addr = if self.host.contains(':') { self.host.clone() } else { format!("{}:{DDP_PORT}", self.host) };
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(&addr).await.with_context(|| format!("UDP connection to {addr}"))?;
        self.socket = Some(socket);
        Ok(())
    }

    /// `colors` come in LED order (the order of `channels`).
    fn send(&mut self, colors: &[(u8, Rgb)]) -> Result<()> {
        let Some(socket) = &self.socket else { bail!("stream not open") };
        encode(&mut self.packet, colors, self.gamma);
        match socket.try_send(&self.packet) {
            // A full buffer drops this frame: the next one is a few ms away.
            Err(e) if e.kind() != std::io::ErrorKind::WouldBlock => Err(e.into()),
            _ => Ok(()),
        }
    }

    async fn close(&mut self) {
        self.socket = None;
    }
}

/// One DDP packet for the whole Strip: 8-bit RGB, pushed at once.
// ponytail: a single packet, about 480 LEDs at most; split by offset for more (moot under the 256 LED limit).
pub fn encode(buf: &mut Vec<u8>, colors: &[(u8, Rgb)], gamma: f32) {
    buf.clear();
    // Version 1 + push, no sequence number, RGB 8 bits, default output, offset 0.
    buf.extend_from_slice(&[0x41, 0, 0x0B, 1, 0, 0, 0, 0]);
    buf.extend_from_slice(&((colors.len() * 3) as u16).to_be_bytes());
    let power = 1.0 / gamma.max(0.1);
    for (_, c) in colors {
        buf.extend(c.map(|v| (v.clamp(0.0, 1.0).powf(power) * 255.0).round() as u8));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Corner;

    #[test]
    fn leds_follow_the_wiring_around_the_screen() {
        // From the bottom right corner: up the right side, the top right to left, down the left side.
        let cfg = WledConfig {
            start: Corner::BottomRight,
            clockwise: false,
            right: 35,
            top: 65,
            left: 35,
            ..Default::default()
        };
        let ch = channels(&cfg).unwrap();
        assert_eq!(ch.len(), 135);
        let at = |i: usize| (ch[i].channel_id, ch[i].position.x, ch[i].position.z);
        let (id, x, z) = at(0);
        assert!(id == 0 && x == 1.0 && z < -0.95, "LED 0 at the bottom right: {x} {z}");
        let (_, x, z) = at(34);
        assert!(x == 1.0 && z > 0.95, "LED 34 at the top right: {x} {z}");
        let (_, x, z) = at(35);
        assert!(x > 0.95 && z == 1.0, "LED 35 on the top, right end: {x} {z}");
        let (_, x, z) = at(99);
        assert!(x < -0.95 && z == 1.0, "LED 99 on the top, left end: {x} {z}");
        let (id, x, z) = at(134);
        assert!(id == 134 && x == -1.0 && z < -0.95, "LED 134 at the bottom left: {x} {z}");

        // Clockwise from the bottom left, the same strip runs the other way.
        let ch = channels(&WledConfig { start: Corner::BottomLeft, clockwise: true, ..cfg.clone() }).unwrap();
        assert!(ch[0].position.x == -1.0 && ch[0].position.z < -0.95 && ch[134].position.x == 1.0);

        assert!(channels(&WledConfig::default()).is_err(), "no LEDs");
        assert!(channels(&WledConfig { top: 300, ..cfg }).is_err(), "too long");
    }

    #[test]
    fn ddp_layout() {
        let mut b = Vec::new();
        encode(&mut b, &[(0, [1.0, 0.0, 0.5]), (1, [0.0, 2.0, 0.0])], 1.0);
        assert_eq!(b, [0x41, 0, 0x0B, 1, 0, 0, 0, 0, 0, 6, 255, 0, 128, 0, 255, 0]);
        // A gamma-encoded strip gets brighter bytes for the same light.
        encode(&mut b, &[(0, [0.5; 3])], 2.0);
        assert_eq!(b[10], 180);
    }
}
