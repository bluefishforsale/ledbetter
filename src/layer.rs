//! Layers: a stack of Effects composited bottom-to-top into the Canvas.
//! Each Layer has a Map (canvas-space transform), a mix mode, and opacity
//! (CONTEXT.md "Layer"). The bottom layer has nothing beneath it, so its mix
//! mode is ignored — it lays down the base.

use serde::{Deserialize, Serialize};

use crate::canvas::Canvas;
use crate::effect::{Effect, Params};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MixMode {
    Normal,
    Multiply,
    Screen,
    Lighten, // HTP — highest takes precedence
    Add,     // linear dodge
    Mask,    // gate the layers below by this layer's luminance
}

impl MixMode {
    pub const ALL: [MixMode; 6] = [
        MixMode::Normal,
        MixMode::Multiply,
        MixMode::Screen,
        MixMode::Lighten,
        MixMode::Add,
        MixMode::Mask,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MixMode::Normal => "Normal",
            MixMode::Multiply => "Multiply",
            MixMode::Screen => "Screen",
            MixMode::Lighten => "Lighten",
            MixMode::Add => "Add",
            MixMode::Mask => "Mask",
        }
    }
}

/// Canvas-space transform of where a layer's effect is sampled (CONTEXT.md).
/// Scale zooms about center; effects are periodic/continuous so the raw
/// (unbounded) coord is passed through — seamless, never clamped or tiled.
/// ponytail: offset + scale only. Rotation lands when a look needs it.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Map {
    pub offset: (f32, f32),
    pub scale: (f32, f32),
}

impl Default for Map {
    fn default() -> Self {
        Map { offset: (0.0, 0.0), scale: (1.0, 1.0) }
    }
}

impl Map {
    /// Map an output coord (nx,ny) to the effect's sample coord (unbounded).
    fn apply(&self, nx: f32, ny: f32) -> (f32, f32) {
        (
            (nx - 0.5) / self.scale.0 + 0.5 - self.offset.0,
            (ny - 0.5) / self.scale.1 + 0.5 - self.offset.1,
        )
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Layer {
    pub effect: Effect,
    pub map: Map,
    pub mix: MixMode,
    pub opacity: f32,
    pub enabled: bool,
    /// Beats per cycle: how many beats one loop of this layer's effect spans.
    /// Fractions run faster than one cycle/beat (1/4 = 4 cycles/beat); whole
    /// numbers run slower (32/1 = one cycle per 32 beats). Per-layer.
    pub beats_per_cycle: f32,
    pub params: Params,
}

impl Layer {
    pub fn new(effect: Effect) -> Self {
        Layer {
            effect,
            map: Map::default(),
            mix: MixMode::Normal,
            opacity: 1.0,
            enabled: true,
            beats_per_cycle: 4.0,
            params: Params::default(),
        }
    }

    /// This layer's effect phase in [0,1) at the monotonic beat count.
    fn phase(&self, beats: f32) -> f32 {
        (beats / self.beats_per_cycle.max(0.0001)).rem_euclid(1.0)
    }
}

/// Composite the enabled layers into the canvas at the monotonic beat count;
/// each layer animates at its own beats-per-cycle.
pub fn render(layers: &[Layer], canvas: &mut Canvas, beats: f32) {
    let (w, h) = (canvas.w, canvas.h);
    for y in 0..h {
        let ny = y as f32 / (h - 1).max(1) as f32;
        for x in 0..w {
            let nx = x as f32 / (w - 1).max(1) as f32;
            let mut acc = [0.0f32; 3];
            let mut first = true;
            for l in layers.iter().filter(|l| l.enabled) {
                let (mx, my) = l.map.apply(nx, ny);
                let top = to_f32(l.effect.pixel(mx, my, l.phase(beats), l.params));
                if first {
                    acc = top; // bottom layer: mix ignored
                    first = false;
                } else {
                    acc = blend(acc, top, l.mix, l.opacity);
                }
            }
            canvas.set(x, y, to_u8(acc));
        }
    }
}

fn to_f32(c: [u8; 3]) -> [f32; 3] {
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]
}

fn to_u8(c: [f32; 3]) -> [u8; 3] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Blend `top` onto `below` with a mix mode, then lerp by opacity.
fn blend(below: [f32; 3], top: [f32; 3], mode: MixMode, opacity: f32) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    for i in 0..3 {
        let (b, t) = (below[i], top[i]);
        let m = match mode {
            MixMode::Normal => t,
            MixMode::Multiply => b * t,
            MixMode::Screen => 1.0 - (1.0 - b) * (1.0 - t),
            MixMode::Lighten => b.max(t),
            MixMode::Add => (b + t).min(1.0),
            MixMode::Mask => b * luma(top),
        };
        out[i] = b + (m - b) * opacity.clamp(0.0, 1.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BELOW: [f32; 3] = [0.6, 0.5, 0.4];
    const TOP: [f32; 3] = [0.5, 0.5, 0.5];

    #[test]
    fn multiply_darkens() {
        let r = blend(BELOW, TOP, MixMode::Multiply, 1.0);
        assert!((r[0] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn screen_lightens() {
        let r = blend([0.5, 0.5, 0.5], [0.5, 0.5, 0.5], MixMode::Screen, 1.0);
        assert!((r[0] - 0.75).abs() < 1e-6);
    }

    #[test]
    fn lighten_is_htp_max() {
        let r = blend([0.2, 0.9, 0.4], [0.7, 0.1, 0.4], MixMode::Lighten, 1.0);
        assert_eq!(r, [0.7, 0.9, 0.4]);
    }

    #[test]
    fn add_clamps_to_one() {
        let r = blend([0.8, 0.0, 0.0], [0.5, 0.0, 0.0], MixMode::Add, 1.0);
        assert_eq!(r[0], 1.0);
    }

    #[test]
    fn opacity_half_is_midpoint() {
        // Normal at 0.5 opacity == halfway between below and top.
        let r = blend([0.0, 0.0, 0.0], [1.0, 1.0, 1.0], MixMode::Normal, 0.5);
        assert_eq!(r, [0.5, 0.5, 0.5]);
    }

    #[test]
    fn mask_black_top_gates_below_to_black() {
        let r = blend([1.0, 1.0, 1.0], [0.0, 0.0, 0.0], MixMode::Mask, 1.0);
        assert_eq!(r, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn beats_per_cycle_slows_the_layer() {
        let mut l = Layer::new(Effect::Color);
        l.beats_per_cycle = 4.0;
        assert!(l.phase(4.0).abs() < 1e-6); // one loop after 4 beats
        assert!((l.phase(2.0) - 0.5).abs() < 1e-6); // halfway at 2 beats
    }

    #[test]
    fn map_scale_zooms_about_center() {
        // scale 2x: a point at the edge maps closer to centre (zoomed in).
        let m = Map { offset: (0.0, 0.0), scale: (2.0, 2.0) };
        let (mx, _) = m.apply(1.0, 0.5); // (1.0-0.5)/2 + 0.5 = 0.75
        assert!((mx - 0.75).abs() < 1e-6);
    }
}
