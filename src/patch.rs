//! The patch: the Controller -> Output -> Strip -> Pixel tree (ADR-0001).
//! Each Output is an ordered strip of pixels with a format and canvas positions;
//! addresses are *derived* by auto-incrementing universes/channels from the
//! Controller base. A Rig fans the canvas out to every Controller's transport.

use std::collections::BTreeMap;
use std::f32::consts::TAU;

use crate::canvas::Canvas;
use crate::output::Transport;

/// How a pixel's RGB sample is laid into DMX channels.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Mono, // 1 channel: luminance (dimmer -> PAR)
    Rgb,  // 3 channels
    Grb,  // 3 channels, green first (common on addressable strips)
    Rgbw, // 4 channels, white extracted
}

impl PixelFormat {
    pub fn channels(self) -> usize {
        match self {
            PixelFormat::Mono => 1,
            PixelFormat::Rgb | PixelFormat::Grb => 3,
            PixelFormat::Rgbw => 4,
        }
    }

    /// Encode an RGB sample into `out` (length must be >= channels()).
    pub fn encode(self, c: [u8; 3], out: &mut [u8]) {
        match self {
            PixelFormat::Mono => out[0] = luma8(c),
            PixelFormat::Rgb => out[..3].copy_from_slice(&c),
            PixelFormat::Grb => {
                out[0] = c[1];
                out[1] = c[0];
                out[2] = c[2];
            }
            PixelFormat::Rgbw => {
                let w = c[0].min(c[1]).min(c[2]);
                out[0] = c[0] - w;
                out[1] = c[1] - w;
                out[2] = c[2] - w;
                out[3] = w;
            }
        }
    }
}

fn luma8(c: [u8; 3]) -> u8 {
    (0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32)
        .round()
        .min(255.0) as u8
}

/// Physical wiring order of a matrix Output.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Wiring {
    Contiguous,
    Serpentine, // every other row reversed (boustrophedon)
}

/// An ordered strip of pixels on one physical port.
pub struct Output {
    pub format: PixelFormat,
    pub positions: Vec<(f32, f32)>, // canvas-space (u,v) per pixel, in wire order
}

impl Output {
    /// A straight strip of `n` pixels from `start` to `end` in canvas space.
    pub fn line(format: PixelFormat, n: usize, start: (f32, f32), end: (f32, f32)) -> Self {
        let positions = (0..n)
            .map(|i| {
                let t = if n <= 1 { 0.0 } else { i as f32 / (n - 1) as f32 };
                (start.0 + (end.0 - start.0) * t, start.1 + (end.1 - start.1) * t)
            })
            .collect();
        Output { format, positions }
    }

    /// Remap this output's positions into a sub-rect of the canvas.
    pub fn placed(mut self, min: (f32, f32), max: (f32, f32)) -> Self {
        for p in self.positions.iter_mut() {
            p.0 = min.0 + p.0 * (max.0 - min.0);
            p.1 = min.1 + p.1 * (max.1 - min.1);
        }
        self
    }

    /// A `w`x`h` matrix filling the canvas; serpentine reverses odd rows.
    pub fn matrix(format: PixelFormat, w: usize, h: usize, wiring: Wiring) -> Self {
        let mut positions = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let px = if wiring == Wiring::Serpentine && y % 2 == 1 { w - 1 - x } else { x };
                let u = if w <= 1 { 0.5 } else { px as f32 / (w - 1) as f32 };
                let v = if h <= 1 { 0.5 } else { y as f32 / (h - 1) as f32 };
                positions.push((u, v));
            }
        }
        Output { format, positions }
    }
}

/// `n` strips radiating from the canvas center to the rim — a spikey-circle
/// arrangement. A Radial effect then radiates out every spoke.
pub fn spokes(n: usize, per: usize, format: PixelFormat) -> Vec<Output> {
    (0..n)
        .map(|i| {
            let a = i as f32 / n.max(1) as f32 * TAU;
            Output::line(format, per, (0.5, 0.5), (0.5 + 0.45 * a.cos(), 0.5 + 0.45 * a.sin()))
        })
        .collect()
}

pub struct Pixel {
    pub u: f32,
    pub v: f32,
    pub universe: u16,
    pub channel: usize,
    pub format: PixelFormat,
}

/// Derive per-pixel addresses by packing outputs contiguously from a base
/// universe, rolling to the next universe when a pixel won't fit in 512 chans.
pub fn derive(base_universe: u16, outputs: &[Output]) -> Vec<Pixel> {
    let mut pixels = Vec::new();
    let mut universe = base_universe;
    let mut channel = 0usize;
    for o in outputs {
        let w = o.format.channels();
        for &(u, v) in &o.positions {
            if channel + w > 512 {
                universe += 1;
                channel = 0;
            }
            pixels.push(Pixel { u, v, universe, channel, format: o.format });
            channel += w;
        }
    }
    pixels
}

/// A Controller: a transport plus its derived pixels.
pub struct Controller {
    pub transport: Transport,
    pub pixels: Vec<Pixel>,
}

impl Controller {
    pub fn new(transport: Transport, base_universe: u16, outputs: Vec<Output>) -> Self {
        let pixels = derive(base_universe, &outputs);
        Controller { transport, pixels }
    }
}

/// The whole output rig: every Controller, each with its own transport.
pub struct Rig {
    pub controllers: Vec<Controller>,
}

impl Rig {
    /// Every output pixel's canvas position and its sampled color — for the
    /// rig (dot) preview, which shows the real fixtures rather than the canvas.
    pub fn preview(&self, canvas: &Canvas) -> Vec<(f32, f32, [u8; 3])> {
        let mut out = Vec::new();
        for c in &self.controllers {
            for p in &c.pixels {
                out.push((p.u, p.v, canvas.sample(p.u, p.v)));
            }
        }
        out
    }

    /// Sample the canvas per pixel, pack per-universe DMX frames, send each
    /// Controller's universes over its transport.
    pub fn send(&mut self, canvas: &Canvas) {
        for c in &mut self.controllers {
            let mut bufs: BTreeMap<u16, [u8; 512]> = BTreeMap::new();
            for p in &c.pixels {
                let rgb = canvas.sample(p.u, p.v);
                let n = p.format.channels();
                if p.channel + n <= 512 {
                    let buf = bufs.entry(p.universe).or_insert([0u8; 512]);
                    p.format.encode(rgb, &mut buf[p.channel..p.channel + n]);
                }
            }
            for (uni, buf) in &bufs {
                let _ = c.transport.send(*uni, buf);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_encodes_luminance() {
        let mut out = [0u8; 1];
        PixelFormat::Mono.encode([255, 255, 255], &mut out);
        assert_eq!(out[0], 255);
        PixelFormat::Mono.encode([0, 0, 0], &mut out);
        assert_eq!(out[0], 0);
    }

    #[test]
    fn grb_swaps_red_and_green() {
        let mut out = [0u8; 3];
        PixelFormat::Grb.encode([10, 20, 30], &mut out);
        assert_eq!(out, [20, 10, 30]);
    }

    #[test]
    fn rgbw_extracts_white() {
        let mut out = [0u8; 4];
        PixelFormat::Rgbw.encode([40, 60, 50], &mut out);
        assert_eq!(out, [0, 20, 10, 40]); // w=min=40 subtracted
    }

    #[test]
    fn derive_rolls_universe_when_full() {
        // 200 RGB pixels = 600 channels > 512, so pixel ~171 starts universe+1.
        let out = Output::line(PixelFormat::Rgb, 200, (0.0, 0.0), (1.0, 0.0));
        let px = derive(4, &[out]);
        assert_eq!(px[0].universe, 4);
        assert_eq!(px[0].channel, 0);
        // 170 RGB pixels fill 510 channels of universe 4 (indices 0..=169);
        // pixel 170 needs 3 more than the 2 left, so it rolls to universe 5.
        assert_eq!(px[169].universe, 4);
        assert_eq!(px[169].channel, 507);
        assert_eq!(px[170].universe, 5);
        assert_eq!(px[170].channel, 0);
    }

    #[test]
    fn serpentine_reverses_odd_rows() {
        let m = Output::matrix(PixelFormat::Rgb, 4, 2, Wiring::Serpentine);
        // row 0 left->right: index 0 at u=0, index 3 at u=1
        assert_eq!(m.positions[0].0, 0.0);
        assert_eq!(m.positions[3].0, 1.0);
        // row 1 reversed: index 4 at u=1 (right), index 7 at u=0 (left)
        assert_eq!(m.positions[4].0, 1.0);
        assert_eq!(m.positions[7].0, 0.0);
    }
}
