//! A Deck: a playhead that renders a layer stack to its own canvas. Each Layer
//! carries its own speed (beats-per-cycle), so the deck just holds the stack.
//! The shared 16-place Storage bank that decks point into (CONTEXT.md "Deck")
//! arrives at M5.

use crate::layer::Layer;

pub struct Deck {
    pub layers: Vec<Layer>,
}

impl Deck {
    pub fn new(layers: Vec<Layer>) -> Self {
        Deck { layers }
    }
}
