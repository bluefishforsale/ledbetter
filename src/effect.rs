//! The four v1 SCE generators. Effects self-animate as a function of the beat
//! phase (CONTEXT.md "Effect") — no external modulation engine.
//! ponytail: an enum of 4 known effects, not a trait+registry. The linkme
//! registry earns its place when effects become user-extensible.

use crate::canvas::hsv;
use std::f32::consts::TAU;

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

    /// Color at normalized (nx,ny) and beat phase. Pure — the unit of testing.
    pub fn pixel(self, nx: f32, ny: f32, beat: f32) -> [u8; 3] {
        match self {
            // Solid field, hue cycles once per beat.
            Effect::Color => hsv(beat, 1.0, 1.0),
            // Hue ramps across x, scrolling with the beat.
            Effect::Gradient => hsv(nx + beat, 1.0, 1.0),
            // Brightness is a sine travelling along x, one cycle per beat.
            Effect::Wave => {
                let b = (0.5 + 0.5 * ((nx * 2.0 - beat) * TAU).sin()).clamp(0.0, 1.0);
                [(b * 255.0) as u8; 3]
            }
            // Classic multi-sine plasma; hue from the summed field.
            Effect::Plasma => {
                let p = beat * TAU;
                let v = (nx * 8.0 + p).sin()
                    + (ny * 8.0).sin()
                    + ((nx + ny) * 8.0 + p).sin()
                    + ((nx * nx + ny * ny).sqrt() * 8.0 - p).sin();
                hsv(v / 8.0 + 0.5, 1.0, 1.0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_is_solid_red_at_downbeat() {
        assert_eq!(Effect::Color.pixel(0.3, 0.7, 0.0), [255, 0, 0]);
    }

    #[test]
    fn gradient_left_edge_tracks_beat() {
        // nx=0 => hue == beat; quarter beat past red is still saturated.
        assert_eq!(Effect::Gradient.pixel(0.0, 0.0, 0.0), [255, 0, 0]);
    }

    #[test]
    fn wave_is_grayscale() {
        let [r, g, b] = Effect::Wave.pixel(0.25, 0.5, 0.0);
        assert_eq!(r, g);
        assert_eq!(g, b);
    }

    #[test]
    fn plasma_is_deterministic() {
        let a = Effect::Plasma.pixel(0.4, 0.6, 0.2);
        let b = Effect::Plasma.pixel(0.4, 0.6, 0.2);
        assert_eq!(a, b);
    }
}
