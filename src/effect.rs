//! The four v1 SCE generators. Effects self-animate as a function of the beat
//! phase (CONTEXT.md "Effect") — no external modulation engine.
//! ponytail: an enum of 4 known effects, not a trait+registry. The linkme
//! registry earns its place when effects become user-extensible.

use crate::canvas::{hsluv_lerp, hsv};
use std::f32::consts::{FRAC_PI_4, PI, TAU};

/// Per-layer effect parameters. Different effects use different fields (see
/// each match arm). Kept off the Effect enum so the picker stays a simple Copy
/// value and params survive switching effects. Fixed-size palette keeps Copy.
#[derive(Clone, Copy)]
pub struct Params {
    pub dir: u8,                // 0..8, direction in 45° steps (Gradient/Wave)
    pub pitch: f32,             // spatial repetitions (Gradient/Wave) / zoom (Plasma)
    pub width: f32,             // band size as a fraction of each period (Wave)
    pub n_colors: u8,           // Gradient palette size, 2..=8
    pub colors: [[u8; 3]; 8],   // Gradient palette
}

impl Default for Params {
    fn default() -> Self {
        Params {
            dir: 0,
            pitch: 1.0,
            width: 0.5,
            n_colors: 3,
            colors: [
                [255, 0, 0],
                [0, 255, 0],
                [0, 0, 255],
                [255, 255, 0],
                [0, 255, 255],
                [255, 0, 255],
                [255, 255, 255],
                [255, 128, 0],
            ],
        }
    }
}

/// Sample the palette as a seamless loop at position `t` (wraps last -> first),
/// interpolating in HSLuv so ramps stay perceptually even.
fn palette_at(p: &Params, t: f32) -> [u8; 3] {
    let n = p.n_colors.clamp(2, 8) as usize;
    let x = t.rem_euclid(1.0) * n as f32;
    let i = (x.floor() as usize) % n;
    let j = (i + 1) % n;
    hsluv_lerp(p.colors[i], p.colors[j], x - x.floor())
}

/// Deterministic hash of a coordinate to [0,1) — for Sparkle's per-pixel seed.
fn hash2(x: f32, y: f32) -> f32 {
    let v = (x * 127.1 + y * 311.7).sin() * 43758.547;
    v.fract().abs()
}

/// Arrow glyphs for the 8 directions (screen space, y down).
pub const DIR_ARROWS: [&str; 8] = ["→", "↘", "↓", "↙", "←", "↖", "↑", "↗"];

fn dir_vec(dir: u8) -> (f32, f32) {
    let a = (dir % 8) as f32 * FRAC_PI_4;
    (a.cos(), a.sin())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Color,
    Gradient,
    Wave,
    Plasma,
    Radial,
    Sparkle,
}

impl Effect {
    pub const ALL: [Effect; 6] = [
        Effect::Color,
        Effect::Gradient,
        Effect::Wave,
        Effect::Plasma,
        Effect::Radial,
        Effect::Sparkle,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Effect::Color => "Color",
            Effect::Gradient => "Gradient",
            Effect::Wave => "Wave",
            Effect::Plasma => "Plasma",
            Effect::Radial => "Radial",
            Effect::Sparkle => "Sparkle",
        }
    }

    /// Whether this effect exposes the direction + pitch + width controls.
    pub fn directional(self) -> bool {
        matches!(self, Effect::Gradient | Effect::Wave)
    }

    /// Color at normalized (nx,ny) and effect phase. Pure — the unit of testing.
    pub fn pixel(self, nx: f32, ny: f32, phase: f32, p: Params) -> [u8; 3] {
        match self {
            // Solid field, hue cycles once per loop.
            Effect::Color => hsv(phase, 1.0, 1.0),
            // Seamless repeating ramp through the chosen palette — no gaps.
            Effect::Gradient => palette_at(&p, band_coord(nx, ny, p, phase)),
            // A soft brightness band of size `p.width`, repeating along `dir`.
            Effect::Wave => {
                let f = band_coord(nx, ny, p, phase);
                let w = p.width.max(0.001);
                let b = if f < w { (PI * f / w).sin() } else { 0.0 };
                [(b * 255.0) as u8; 3]
            }
            // Organic plasma: rotated (non-separable) waves plus two orbiting
            // radial sources, with incommensurate frequencies so it never
            // collapses into a grid. `p.pitch` zooms about center.
            Effect::Plasma => {
                let z = p.pitch.max(0.05);
                let x = (nx - 0.5) / z;
                let y = (ny - 0.5) / z;
                let t = phase * TAU;
                // orbiting source centers
                let (c1x, c1y) = (0.4 * (t * 0.9).sin(), 0.4 * (t * 1.1).cos());
                let (c2x, c2y) = (0.45 * (t * 1.3 + 2.0).cos(), 0.4 * (t * 0.7).sin());
                let d1 = ((x - c1x).powi(2) + (y - c1y).powi(2)).sqrt();
                let d2 = ((x - c2x).powi(2) + (y - c2y).powi(2)).sqrt();
                let v = (x * 6.0 + t).sin()
                    + ((x * 0.6 + y * 0.8) * 7.3 - t * 1.2).sin() // rotated, not axis-aligned
                    + (d1 * 9.1 - t * 2.0).sin()
                    + (d2 * 11.7 + t).sin();
                hsv(v / 4.0 + 0.5, 1.0, 1.0)
            }
            // Concentric palette rings expanding from the canvas center; on a
            // spoke layout this radiates out every bar. pitch = ring count.
            Effect::Radial => {
                let r = ((nx - 0.5).powi(2) + (ny - 0.5).powi(2)).sqrt();
                palette_at(&p, r * p.pitch - phase)
            }
            // Per-pixel random twinkle, palette-colored. width = spark
            // density/duration; each pixel fires on its own offset.
            Effect::Sparkle => {
                let seed = hash2((nx * 997.0).floor(), (ny * 997.0).floor());
                let local = (phase + seed).rem_euclid(1.0);
                let w = p.width.max(0.01);
                if local < w {
                    let b = 1.0 - local / w;
                    let c = palette_at(&p, seed);
                    [
                        (c[0] as f32 * b) as u8,
                        (c[1] as f32 * b) as u8,
                        (c[2] as f32 * b) as u8,
                    ]
                } else {
                    [0, 0, 0]
                }
            }
        }
    }
}

/// Position within the repeating band pattern, in [0,1): project (nx,ny) onto
/// the direction, scale by pitch, scroll by phase, wrap.
fn band_coord(nx: f32, ny: f32, p: Params, phase: f32) -> f32 {
    let (c, s) = dir_vec(p.dir);
    let t = (nx - 0.5) * c + (ny - 0.5) * s;
    (t * p.pitch - phase).rem_euclid(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_is_solid_red_at_downbeat() {
        assert_eq!(Effect::Color.pixel(0.3, 0.7, 0.0, Params::default()), [255, 0, 0]);
    }

    #[test]
    fn wave_is_grayscale() {
        let [r, g, b] = Effect::Wave.pixel(0.25, 0.5, 0.0, Params::default());
        assert_eq!(r, g);
        assert_eq!(g, b);
    }

    #[test]
    fn gradient_has_no_black_gaps() {
        // Default palette's first two colours are bright red/green.
        let p = Params { n_colors: 2, ..Default::default() };
        for i in 0..200 {
            let c = Effect::Gradient.pixel(i as f32 / 200.0, 0.5, 0.0, p);
            assert_ne!(c, [0, 0, 0], "black gap at {i}");
        }
    }

    #[test]
    fn direction_rotates_the_pattern() {
        let h = Params { dir: 0, pitch: 2.0, ..Default::default() };
        let v = Params { dir: 2, ..h };
        assert_ne!(
            Effect::Gradient.pixel(0.8, 0.2, 0.0, h),
            Effect::Gradient.pixel(0.8, 0.2, 0.0, v)
        );
    }

    #[test]
    fn radial_changes_with_distance_from_center() {
        let p = Params { pitch: 4.0, ..Default::default() };
        let center = Effect::Radial.pixel(0.5, 0.5, 0.0, p);
        let edge = Effect::Radial.pixel(0.0, 0.0, 0.0, p);
        assert_ne!(center, edge);
    }

    #[test]
    fn sparkle_is_deterministic() {
        let p = Params::default();
        assert_eq!(
            Effect::Sparkle.pixel(0.4, 0.6, 0.2, p),
            Effect::Sparkle.pixel(0.4, 0.6, 0.2, p)
        );
    }

    #[test]
    fn plasma_zoom_changes_output() {
        let a = Params { pitch: 1.0, ..Default::default() };
        let b = Params { pitch: 4.0, ..a };
        assert_ne!(
            Effect::Plasma.pixel(0.9, 0.1, 0.0, a),
            Effect::Plasma.pixel(0.9, 0.1, 0.0, b)
        );
    }
}
