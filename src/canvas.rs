//! The Canvas: a fixed-resolution RGB framebuffer, the universal substrate.
//! Effects render into it; Pixels bilinear-sample it (CONTEXT.md).

pub type Rgb = [u8; 3];

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Rgb>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Self {
        Canvas { w, h, px: vec![[0; 3]; w * h] }
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, c: Rgb) {
        self.px[y * self.w + x] = c;
    }

    #[inline]
    fn at(&self, x: usize, y: usize) -> Rgb {
        self.px[y * self.w + x]
    }

    /// Bilinear sample at normalized (u,v) in [0,1].
    pub fn sample(&self, u: f32, v: f32) -> Rgb {
        let fx = u.clamp(0.0, 1.0) * (self.w - 1) as f32;
        let fy = v.clamp(0.0, 1.0) * (self.h - 1) as f32;
        let x0 = fx.floor() as usize;
        let y0 = fy.floor() as usize;
        let x1 = (x0 + 1).min(self.w - 1);
        let y1 = (y0 + 1).min(self.h - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;

        let (p00, p10, p01, p11) =
            (self.at(x0, y0), self.at(x1, y0), self.at(x0, y1), self.at(x1, y1));
        let mut out = [0u8; 3];
        for (ch, o) in out.iter_mut().enumerate() {
            let top = p00[ch] as f32 + (p10[ch] as f32 - p00[ch] as f32) * tx;
            let bot = p01[ch] as f32 + (p11[ch] as f32 - p01[ch] as f32) * tx;
            *o = (top + (bot - top) * ty).round() as u8;
        }
        out
    }
}

/// HSV (h in [0,1)) -> RGB. Shared by the effects.
pub fn hsv(h: f32, s: f32, v: f32) -> Rgb {
    let h6 = h.rem_euclid(1.0) * 6.0;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilinear_interpolates_midpoint() {
        let mut c = Canvas::new(2, 1);
        c.set(0, 0, [0, 0, 0]);
        c.set(1, 0, [100, 200, 40]);
        assert_eq!(c.sample(0.5, 0.0), [50, 100, 20]);
    }

    #[test]
    fn hsv_primaries() {
        assert_eq!(hsv(0.0, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(hsv(1.0 / 3.0, 1.0, 1.0), [0, 255, 0]);
    }
}
