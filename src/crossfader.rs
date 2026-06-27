//! The crossfader: blends the two decks' *rendered canvases* (not their layers)
//! into the output canvas (CONTEXT.md "Crossfader", ADR-0002 pipeline).
//! Position 0.0 = full Deck A, 1.0 = full Deck B.

use crate::canvas::Canvas;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FadeType {
    Multiply, // geometric: darkens through the middle (a^(1-p) * b^p)
    Cross,    // linear A -> B
    White,    // additive, bright through the middle
    Black,    // through black at the middle
}

impl FadeType {
    pub const ALL: [FadeType; 4] =
        [FadeType::Multiply, FadeType::Cross, FadeType::White, FadeType::Black];

    pub fn name(self) -> &'static str {
        match self {
            FadeType::Multiply => "Multiply",
            FadeType::Cross => "Cross",
            FadeType::White => "White",
            FadeType::Black => "Black",
        }
    }
}

/// Blend two pixels by crossfader position and fade type.
pub fn fade_pixel(a: [u8; 3], b: [u8; 3], pos: f32, fade: FadeType) -> [u8; 3] {
    let p = pos.clamp(0.0, 1.0);
    // Multiply is multiplicative (per-channel geometric interpolation), not a
    // weighted sum, so it gets its own path. Endpoints still pass through.
    if let FadeType::Multiply = fade {
        let mut out = [0u8; 3];
        for i in 0..3 {
            let an = a[i] as f32 / 255.0;
            let bn = b[i] as f32 / 255.0;
            out[i] = (an.powf(1.0 - p) * bn.powf(p) * 255.0).round().min(255.0) as u8;
        }
        return out;
    }
    let (ga, gb) = match fade {
        FadeType::Cross => (1.0 - p, p),
        // both reach full gain at the midpoint -> additive bright
        FadeType::White => ((2.0 * (1.0 - p)).min(1.0), (2.0 * p).min(1.0)),
        // both fall to zero at the midpoint -> through black
        FadeType::Black => ((1.0 - 2.0 * p).max(0.0), (2.0 * p - 1.0).max(0.0)),
        FadeType::Multiply => unreachable!(),
    };
    let mut out = [0u8; 3];
    for i in 0..3 {
        let v = a[i] as f32 * ga + b[i] as f32 * gb;
        out[i] = v.min(255.0) as u8;
    }
    out
}

/// Blend canvas A and B into `out` (all same dimensions).
pub fn blend(a: &Canvas, b: &Canvas, pos: f32, fade: FadeType, out: &mut Canvas) {
    for ((o, pa), pb) in out.px.iter_mut().zip(&a.px).zip(&b.px) {
        *o = fade_pixel(*pa, *pb, pos, fade);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: [u8; 3] = [200, 100, 0];
    const B: [u8; 3] = [0, 50, 200];

    #[test]
    fn endpoints_pass_through_for_every_fade() {
        for f in FadeType::ALL {
            assert_eq!(fade_pixel(A, B, 0.0, f), A, "{} at 0", f.name());
            assert_eq!(fade_pixel(A, B, 1.0, f), B, "{} at 1", f.name());
        }
    }

    #[test]
    fn cross_midpoint_is_average() {
        assert_eq!(fade_pixel(A, B, 0.5, FadeType::Cross), [100, 75, 100]);
    }

    #[test]
    fn white_midpoint_is_additive() {
        // both at full gain -> A + B, clamped
        assert_eq!(fade_pixel(A, B, 0.5, FadeType::White), [200, 150, 200]);
    }

    #[test]
    fn black_midpoint_is_black() {
        assert_eq!(fade_pixel(A, B, 0.5, FadeType::Black), [0, 0, 0]);
    }

    #[test]
    fn multiply_midpoint_is_geometric_mean() {
        // sqrt(a*b) per channel: sqrt(200*0)=0, etc. — darker than linear.
        let r = fade_pixel([100, 0, 64], [100, 200, 64], 0.5, FadeType::Multiply);
        assert_eq!(r[0], 100); // sqrt(100*100)=100
        assert_eq!(r[1], 0); // sqrt(0*200)=0
        assert_eq!(r[2], 64); // sqrt(64*64)=64
    }
}
