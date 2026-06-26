//! The four v1 SCE generators. Effects self-animate as a function of the beat
//! phase (CONTEXT.md "Effect") — no external modulation engine.
//! ponytail: an enum of 4 known effects, not a trait+registry. The linkme
//! registry earns its place when effects become user-extensible.

use crate::canvas::hsv;
use std::f32::consts::{FRAC_PI_4, PI, TAU};

/// Per-layer effect parameters. Gradient and Wave use all three; Color and
/// Plasma ignore them. Kept off the Effect enum so the picker stays a simple
/// Copy value and params survive switching effects.
#[derive(Clone, Copy)]
pub struct Params {
    pub dir: u8,    // 0..8, direction in 45° steps
    pub pitch: f32, // spatial repetitions across the canvas
    pub width: f32, // feature/band size as a fraction of each period
}

impl Default for Params {
    fn default() -> Self {
        Params { dir: 0, pitch: 1.0, width: 0.5 }
    }
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
}

impl Effect {
    pub const ALL: [Effect; 4] = [Effect::Color, Effect::Gradient, Effect::Wave, Effect::Plasma];

    pub fn name(self) -> &'static str {
        match self {
            Effect::Color => "Color",
            Effect::Gradient => "Gradient",
            Effect::Wave => "Wave",
            Effect::Plasma => "Plasma",
        }
    }

    /// Whether this effect uses the directional Params (direction/pitch/width).
    pub fn directional(self) -> bool {
        matches!(self, Effect::Gradient | Effect::Wave)
    }

    /// Color at normalized (nx,ny) and effect phase. Pure — the unit of testing.
    pub fn pixel(self, nx: f32, ny: f32, phase: f32, p: Params) -> [u8; 3] {
        match self {
            // Solid field, hue cycles once per loop.
            Effect::Color => hsv(phase, 1.0, 1.0),
            // A rainbow band of width `p.width`, repeating `p.pitch` times along
            // the chosen direction, scrolling with the phase.
            Effect::Gradient => {
                let f = band_coord(nx, ny, p, phase);
                let w = p.width.max(0.001);
                if f < w { hsv(f / w, 1.0, 1.0) } else { [0, 0, 0] }
            }
            // A soft brightness band, same geometry as Gradient.
            Effect::Wave => {
                let f = band_coord(nx, ny, p, phase);
                let w = p.width.max(0.001);
                let b = if f < w { (PI * f / w).sin() } else { 0.0 };
                [(b * 255.0) as u8; 3]
            }
            // Classic multi-sine plasma; hue from the summed field.
            Effect::Plasma => {
                let t = phase * TAU;
                let v = (nx * 8.0 + t).sin()
                    + (ny * 8.0).sin()
                    + ((nx + ny) * 8.0 + t).sin()
                    + ((nx * nx + ny * ny).sqrt() * 8.0 - t).sin();
                hsv(v / 8.0 + 0.5, 1.0, 1.0)
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
    fn gradient_outside_band_is_black() {
        // width 0.1 leaves most of the period black.
        let p = Params { dir: 0, pitch: 1.0, width: 0.1 };
        let lit = (0..100)
            .filter(|i| Effect::Gradient.pixel(*i as f32 / 100.0, 0.5, 0.0, p) != [0, 0, 0])
            .count();
        assert!(lit > 0 && lit < 100, "lit pixels: {lit}");
    }

    #[test]
    fn direction_rotates_the_pattern() {
        // Horizontal vs vertical direction give different values off-center.
        let h = Params { dir: 0, pitch: 2.0, width: 1.0 };
        let v = Params { dir: 2, pitch: 2.0, width: 1.0 };
        assert_ne!(
            Effect::Gradient.pixel(0.8, 0.2, 0.0, h),
            Effect::Gradient.pixel(0.8, 0.2, 0.0, v)
        );
    }

    #[test]
    fn plasma_is_deterministic() {
        let p = Params::default();
        assert_eq!(
            Effect::Plasma.pixel(0.4, 0.6, 0.2, p),
            Effect::Plasma.pixel(0.4, 0.6, 0.2, p)
        );
    }
}
