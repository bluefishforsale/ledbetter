//! Show-file persistence: the portable creative state (looks), canvas-space
//! only — no patch/ports (ADR-0003 invariant). YAML via serde, atomic write.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::crossfader::FadeType;
use crate::layer::Layer;

#[derive(Serialize, Deserialize)]
pub struct ShaderState {
    pub enabled: bool,
    pub src: String,
    pub sliders: [f32; 8],
}

#[derive(Serialize, Deserialize)]
pub struct DeckState {
    pub layers: Vec<Layer>,
    pub shader: ShaderState,
}

#[derive(Serialize, Deserialize)]
pub struct ShowFile {
    pub deck_a: DeckState,
    pub deck_b: DeckState,
    pub fade: FadeType,
    pub bpm: f32,
}

/// Serialize to YAML and write atomically (temp file + rename).
pub fn save(path: &Path, show: &ShowFile) -> Result<(), String> {
    let yaml = serde_yaml::to_string(show).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("ledbetter.tmp");
    std::fs::write(&tmp, yaml).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn load(path: &Path) -> Result<ShowFile, String> {
    let yaml = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_yaml::from_str(&yaml).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::Effect;

    #[test]
    fn roundtrips_through_yaml() {
        let sf = ShowFile {
            deck_a: DeckState {
                layers: vec![Layer::new(Effect::Plasma)],
                shader: ShaderState { enabled: false, src: "a".into(), sliders: [0.5; 8] },
            },
            deck_b: DeckState {
                layers: vec![Layer::new(Effect::Gradient), Layer::new(Effect::Wave)],
                shader: ShaderState { enabled: true, src: "b".into(), sliders: [0.1; 8] },
            },
            fade: FadeType::Multiply,
            bpm: 128.0,
        };
        let yaml = serde_yaml::to_string(&sf).unwrap();
        let back: ShowFile = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.bpm, 128.0);
        assert_eq!(back.deck_a.layers.len(), 1);
        assert_eq!(back.deck_b.layers.len(), 2);
        assert!(back.deck_b.shader.enabled);
        assert!(matches!(back.fade, FadeType::Multiply));
    }
}
