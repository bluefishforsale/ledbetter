//! A Deck: a playhead that renders a layer stack to its own canvas. M3 gives
//! each deck its own stack directly; the shared 16-place Storage bank that decks
//! point into (CONTEXT.md "Deck") arrives at M5.

use crate::layer::Layer;

pub struct Deck {
    pub layers: Vec<Layer>,
    /// Beats per cycle: how many beats one full effect loop spans (1..=16).
    /// 1 = re-trigger every beat (fastest), 16 = loop once per 16 beats.
    pub beats_per_cycle: u32,
}

impl Deck {
    pub fn new(layers: Vec<Layer>) -> Self {
        Deck { layers, beats_per_cycle: 1 }
    }

    /// Effect phase in [0,1) for this deck at the monotonic beat count.
    pub fn phase(&self, beats: f32) -> f32 {
        (beats / self.beats_per_cycle.max(1) as f32).rem_euclid(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beats_per_cycle_slows_the_loop() {
        let mut d = Deck::new(vec![]);
        d.beats_per_cycle = 4;
        // 4 beats in, a 4-beats-per-cycle deck has completed exactly one loop.
        assert!(d.phase(4.0).abs() < 1e-6);
        // 2 beats in, it is halfway.
        assert!((d.phase(2.0) - 0.5).abs() < 1e-6);
    }
}
