//! M0 walking skeleton: beat-less clock -> Color effect -> Canvas -> Patch
//! sample -> Art-Net out. Proves the render pipeline end to end with std only.
//! Later milestones split this into modules (canvas/effect/patch/output/...).

use std::net::UdpSocket;
use std::time::{Duration, Instant};

const CANVAS_W: usize = 128;
const CANVAS_H: usize = 128;
const FRAME: Duration = Duration::from_micros(22_727); // ~44 Hz
const ARTNET_PORT: u16 = 6454;

type Rgb = [u8; 3];

/// Fixed-resolution RGB framebuffer. The universal substrate (CONTEXT.md).
struct Canvas {
    w: usize,
    h: usize,
    px: Vec<Rgb>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Self {
        Canvas { w, h, px: vec![[0; 3]; w * h] }
    }

    fn fill(&mut self, c: Rgb) {
        self.px.iter_mut().for_each(|p| *p = c);
    }

    /// Nearest-neighbour sample at normalized (u,v) in [0,1]. Bilinear comes
    /// with the real Canvas module; nearest is enough to prove the path.
    fn sample(&self, u: f32, v: f32) -> Rgb {
        let x = ((u.clamp(0.0, 1.0) * (self.w - 1) as f32).round()) as usize;
        let y = ((v.clamp(0.0, 1.0) * (self.h - 1) as f32).round()) as usize;
        self.px[y * self.w + x]
    }
}

/// One physical output point: a canvas sample coord and a DMX channel offset.
struct Pixel {
    u: f32,
    v: f32,
    channel: usize, // 0-based index into the universe's 512-byte frame
}

/// HSV (h in [0,1)) -> RGB. Cheap, just to make the canvas move over time.
fn hsv(h: f32, s: f32, v: f32) -> Rgb {
    let h6 = (h.rem_euclid(1.0)) * 6.0;
    let i = h6.floor() as i32;
    let f = h6 - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8]
}

/// Art-Net ArtDmx packet: 8-byte id + opcode 0x5000 + header + DMX data.
fn artnet_dmx(universe: u16, seq: u8, data: &[u8]) -> Vec<u8> {
    let len = data.len();
    let mut p = Vec::with_capacity(18 + len);
    p.extend_from_slice(b"Art-Net\0");
    p.extend_from_slice(&[0x00, 0x50]); // OpOutput/ArtDmx, little-endian 0x5000
    p.extend_from_slice(&[0x00, 0x0e]); // protocol version 14, big-endian
    p.push(seq);
    p.push(0); // physical
    p.push((universe & 0xff) as u8); // SubUni
    p.push(((universe >> 8) & 0x7f) as u8); // Net
    p.extend_from_slice(&[(len >> 8) as u8, (len & 0xff) as u8]); // length, big-endian
    p.extend_from_slice(data);
    p
}

/// A horizontal strip of `n` evenly-spaced RGB pixels across the canvas middle.
fn strip_patch(n: usize) -> Vec<Pixel> {
    (0..n)
        .map(|i| Pixel {
            u: if n == 1 { 0.5 } else { i as f32 / (n - 1) as f32 },
            v: 0.5,
            channel: i * 3,
        })
        .collect()
}

fn render_frame(canvas: &Canvas, patch: &[Pixel]) -> [u8; 512] {
    let mut dmx = [0u8; 512];
    for px in patch {
        let [r, g, b] = canvas.sample(px.u, px.v);
        if px.channel + 2 < 512 {
            dmx[px.channel] = r;
            dmx[px.channel + 1] = g;
            dmx[px.channel + 2] = b;
        }
    }
    dmx
}

fn main() -> std::io::Result<()> {
    let target = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("127.0.0.1:{ARTNET_PORT}"));
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    let mut canvas = Canvas::new(CANVAS_W, CANVAS_H);
    let patch = strip_patch(50);

    println!("ledbetter M0: 50px strip -> Art-Net universe 0 -> {target}");
    let start = Instant::now();
    let mut seq: u8 = 1;
    loop {
        let t = start.elapsed().as_secs_f32();
        canvas.fill(hsv(t * 0.1, 1.0, 1.0)); // slow hue cycle
        let dmx = render_frame(&canvas, &patch);
        let pkt = artnet_dmx(0, seq, &dmx);
        sock.send_to(&pkt, &target)?;
        seq = seq.wrapping_add(1).max(1);
        if seq % 44 == 0 {
            let c = canvas.sample(0.0, 0.5);
            println!("t={t:5.1}s  px0=rgb{c:?}");
        }
        std::thread::sleep(FRAME);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artnet_header_is_wellformed() {
        let data = [7u8; 512];
        let p = artnet_dmx(0x0103, 5, &data);
        assert_eq!(&p[0..8], b"Art-Net\0");
        assert_eq!(&p[8..10], &[0x00, 0x50]); // opcode
        assert_eq!(&p[10..12], &[0x00, 0x0e]); // protocol 14
        assert_eq!(p[12], 5); // sequence
        assert_eq!(p[14], 0x03); // SubUni (low byte of universe)
        assert_eq!(p[15], 0x01); // Net (high 7 bits)
        assert_eq!(&p[16..18], &[0x02, 0x00]); // length 512, big-endian
        assert_eq!(&p[18..], &data);
    }

    #[test]
    fn strip_samples_canvas_color() {
        let mut c = Canvas::new(16, 16);
        c.fill([10, 20, 30]);
        let patch = strip_patch(4);
        let dmx = render_frame(&c, &patch);
        // first pixel at channel 0
        assert_eq!(&dmx[0..3], &[10, 20, 30]);
        // fourth pixel at channel 9
        assert_eq!(&dmx[9..12], &[10, 20, 30]);
    }

    #[test]
    fn loopback_one_frame() {
        // Proves the output path: send a real frame, receive it, check it parses.
        let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        rx.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let addr = rx.local_addr().unwrap();
        let tx = UdpSocket::bind("127.0.0.1:0").unwrap();

        let mut c = Canvas::new(8, 8);
        c.fill([255, 0, 0]);
        let patch = strip_patch(3);
        let dmx = render_frame(&c, &patch);
        tx.send_to(&artnet_dmx(0, 1, &dmx), addr).unwrap();

        let mut buf = [0u8; 600];
        let n = rx.recv(&mut buf).unwrap();
        assert_eq!(&buf[0..8], b"Art-Net\0");
        assert_eq!(&buf[18..21], &[255, 0, 0]); // first pixel red made the wire
        assert_eq!(n, 18 + 512);
    }
}
