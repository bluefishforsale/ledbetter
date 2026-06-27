//! Curated named palettes for the Gradient effect. Selecting one fills a
//! Layer's params (n_colors + colors); individual swatches stay editable after.

use crate::effect::Params;

pub struct Palette {
    pub name: &'static str,
    pub colors: &'static [[u8; 3]],
}

/// Copy a palette into the params (capped at the 8-colour storage).
pub fn apply(params: &mut Params, pal: &Palette) {
    let n = pal.colors.len().clamp(2, 8);
    params.n_colors = n as u8;
    for (slot, c) in params.colors.iter_mut().zip(pal.colors.iter()).take(n) {
        *slot = *c;
    }
}

pub const PALETTES: &[Palette] = &[
    Palette { name: "Primaries", colors: &[[255, 0, 0], [0, 255, 0], [0, 0, 255]] },
    Palette { name: "Secondaries", colors: &[[0, 255, 255], [255, 0, 255], [255, 255, 0]] },
    Palette {
        name: "Rainbow",
        colors: &[
            [255, 0, 0],
            [255, 127, 0],
            [255, 255, 0],
            [0, 200, 0],
            [0, 0, 255],
            [75, 0, 130],
            [148, 0, 211],
        ],
    },
    Palette {
        name: "Pastels",
        colors: &[[255, 179, 186], [255, 223, 186], [255, 255, 186], [186, 255, 201], [186, 225, 255]],
    },
    Palette {
        name: "Spring",
        colors: &[[255, 183, 197], [168, 230, 207], [220, 237, 193], [255, 211, 182]],
    },
    Palette {
        name: "Summer",
        colors: &[[255, 209, 102], [6, 214, 160], [17, 138, 178], [239, 71, 111]],
    },
    Palette {
        name: "Autumn",
        colors: &[[123, 36, 28], [186, 77, 32], [214, 140, 42], [122, 99, 42], [80, 50, 20]],
    },
    Palette {
        name: "Winter",
        colors: &[[8, 24, 58], [37, 78, 112], [102, 155, 188], [191, 215, 234], [255, 255, 255]],
    },
    Palette {
        name: "Dawn",
        colors: &[[32, 30, 60], [92, 60, 90], [199, 108, 108], [244, 162, 97], [255, 214, 153]],
    },
    Palette {
        name: "Sunset",
        colors: &[[255, 94, 77], [255, 149, 0], [255, 0, 110], [131, 56, 236]],
    },
    Palette {
        name: "Ocean",
        colors: &[[3, 4, 94], [0, 119, 182], [0, 180, 216], [72, 202, 228], [202, 240, 248]],
    },
    Palette {
        name: "Forest",
        colors: &[[20, 83, 45], [45, 106, 79], [82, 121, 111], [116, 143, 84], [180, 167, 100]],
    },
    Palette {
        name: "Fire",
        colors: &[[40, 0, 0], [180, 0, 0], [255, 80, 0], [255, 190, 0], [255, 255, 200]],
    },
    Palette {
        name: "Ice",
        colors: &[[3, 4, 94], [2, 62, 138], [0, 150, 199], [144, 224, 239], [240, 252, 255]],
    },
    Palette {
        name: "Neon",
        colors: &[[255, 0, 170], [0, 255, 255], [170, 0, 255], [0, 255, 102]],
    },
    Palette {
        name: "Vaporwave",
        colors: &[[255, 113, 206], [1, 205, 254], [5, 255, 161], [185, 103, 255]],
    },
    Palette {
        name: "Candy",
        colors: &[[255, 105, 180], [255, 182, 193], [135, 206, 250], [152, 251, 152]],
    },
    Palette {
        name: "Earth",
        colors: &[[110, 68, 45], [160, 120, 85], [200, 170, 120], [120, 140, 90]],
    },
    Palette {
        name: "Grayscale",
        colors: &[[0, 0, 0], [64, 64, 64], [128, 128, 128], [192, 192, 192], [255, 255, 255]],
    },
    Palette { name: "Black/White", colors: &[[0, 0, 0], [255, 255, 255]] },
    Palette { name: "Police", colors: &[[255, 0, 0], [0, 0, 255]] },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_fits_and_has_two_plus_colors() {
        for p in PALETTES {
            assert!((2..=8).contains(&p.colors.len()), "{} has {}", p.name, p.colors.len());
        }
    }

    #[test]
    fn apply_sets_count_and_colors() {
        let mut params = Params::default();
        apply(&mut params, &PALETTES[20]); // Police
        assert_eq!(params.n_colors, 2);
        assert_eq!(params.colors[0], [255, 0, 0]);
        assert_eq!(params.colors[1], [0, 0, 255]);
    }
}
