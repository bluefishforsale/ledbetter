//! A Deck: a playhead that renders a layer stack to its own canvas. M3 gives
//! each deck its own stack directly; the shared 16-place Storage bank that decks
//! point into (CONTEXT.md "Deck") arrives at M5.

use crate::layer::Layer;

pub struct Deck {
    pub layers: Vec<Layer>,
    pub pitch: f32, // beat multiplier on the master clock (CONTEXT.md "Pitch")
}

impl Deck {
    pub fn new(layers: Vec<Layer>) -> Self {
        Deck { layers, pitch: 1.0 }
    }

    /// Effective beat phase for this deck at master phase `beat`.
    pub fn beat(&self, beat: f32) -> f32 {
        (beat * self.pitch).rem_euclid(1.0)
    }
}
