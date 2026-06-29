//! Show-file persistence: the portable creative state (looks), canvas-space
//! only — no patch/ports (ADR-0003 invariant). YAML via serde, atomic write.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::crossfader::FadeType;
use crate::layer::Layer;

#[derive(Clone, Serialize, Deserialize)]
pub struct ShaderState {
    pub enabled: bool,
    pub src: String,
    pub sliders: [f32; 8],
}

/// One storage place: a recallable look (layer stack + shader content).
#[derive(Clone, Serialize, Deserialize)]
pub struct DeckState {
    pub layers: Vec<Layer>,
    pub shader: ShaderState,
}

#[derive(Serialize, Deserialize)]
pub struct ShowFile {
    pub bank: Vec<DeckState>,
    pub deck_a_place: usize,
    pub deck_b_place: usize,
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
            bank: vec![
                DeckState {
                    layers: vec![Layer::new(Effect::Plasma)],
                    shader: ShaderState { enabled: false, src: "a".into(), sliders: [0.5; 8] },
                },
                DeckState {
                    layers: vec![Layer::new(Effect::Gradient), Layer::new(Effect::Wave)],
                    shader: ShaderState { enabled: true, src: "b".into(), sliders: [0.1; 8] },
                },
            ],
            deck_a_place: 0,
            deck_b_place: 1,
            fade: FadeType::Multiply,
            bpm: 128.0,
        };
        let yaml = serde_yaml::to_string(&sf).unwrap();
        let back: ShowFile = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.bpm, 128.0);
        assert_eq!(back.bank.len(), 2);
        assert_eq!(back.bank[1].layers.len(), 2);
        assert!(back.bank[1].shader.enabled);
        assert_eq!(back.deck_b_place, 1);
        assert!(matches!(back.fade, FadeType::Multiply));
    }
}
