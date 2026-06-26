//! The patch: physical Pixels that sample the Canvas. M1 keeps a single
//! Strip helper; the Controller -> Output -> Strip -> Pixel tree (ADR-0001)
//! arrives at M4.

use crate::canvas::Canvas;

pub struct Pixel {
    pub u: f32,
    pub v: f32,
    pub channel: usize, // 0-based offset into the 512-byte universe frame
}

/// A horizontal strip of `n` evenly-spaced RGB pixels across the canvas middle.
pub fn strip(n: usize) -> Vec<Pixel> {
    (0..n)
        .map(|i| Pixel {
            u: if n <= 1 { 0.5 } else { i as f32 / (n - 1) as f32 },
            v: 0.5,
            channel: i * 3,
        })
        .collect()
}

/// Sample the canvas at every pixel into a 512-byte DMX frame.
pub fn render_frame(canvas: &Canvas, patch: &[Pixel]) -> [u8; 512] {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_samples_solid_canvas() {
        let mut c = Canvas::new(16, 16);
        c.px.iter_mut().for_each(|p| *p = [10, 20, 30]);
        let dmx = render_frame(&c, &strip(4));
        assert_eq!(&dmx[0..3], &[10, 20, 30]);
        assert_eq!(&dmx[9..12], &[10, 20, 30]);
    }
}
