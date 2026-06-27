//! egui preview: render both decks, crossfade to the output canvas, show it, and
//! feed Art-Net. ponytail: still single-threaded in the egui update loop; the
//! dedicated render thread + lock-free publish (ADR-0002) lands when multiple
//! outputs make it pay.

use eframe::egui;

use crate::canvas::Canvas;
use crate::clock::BeatClock;
use crate::crossfader::{self, FadeType};
use crate::deck::Deck;
use crate::effect::{DIR_ARROWS, Effect};
use crate::layer::{Layer, MixMode};
use crate::output::{ArtNet, Sacn, Transport};
use crate::palette;
use crate::patch::{Controller, Output, PixelFormat, Rig, Wiring};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    A,
    B,
}

pub struct App {
    clock: BeatClock,
    deck_a: Deck,
    deck_b: Deck,
    canvas_a: Canvas,
    canvas_b: Canvas,
    out: Canvas,
    xfade: f32,
    fade: FadeType,
    focus: Focus,
    rig: Rig,
    tex: Option<egui::TextureHandle>,
}

impl App {
    pub fn new(target: String) -> Self {
        let (w, h) = (128, 128);
        // Demo rig: shows M4's claim — one canvas driving a mix of transports
        // and pixel formats at once. Replaced by a loaded Patch file at M5.
        // The middle RGB strip on Art-Net is the one you can sniff/see; the
        // others (GRB on sACN, an RGBW+dimmer USB-DMX node, a serpentine WLED
        // matrix) exercise every format and transport. USB-DMX/WLED are no-op
        // stubs until tested on hardware.
        let line = |fmt| Output::line(fmt, 50, (0.0, 0.5), (1.0, 0.5));
        let mut controllers = Vec::new();
        if let Ok(a) = ArtNet::new(target) {
            controllers.push(Controller::new(Transport::ArtNet(a), 0, vec![line(PixelFormat::Rgb)]));
        }
        if let Ok(s) = Sacn::new(*b"ledbetter-cid-01") {
            controllers.push(Controller::new(Transport::Sacn(s), 1, vec![line(PixelFormat::Grb)]));
        }
        controllers.push(Controller::new(
            Transport::UsbDmx,
            2,
            vec![
                Output::matrix(PixelFormat::Rgbw, 8, 8, Wiring::Contiguous),
                Output::line(PixelFormat::Mono, 1, (0.5, 0.5), (0.5, 0.5)),
            ],
        ));
        controllers.push(Controller::new(
            Transport::Wled,
            3,
            vec![Output::matrix(PixelFormat::Rgb, 8, 8, Wiring::Serpentine)],
        ));
        App {
            clock: BeatClock::new(120.0),
            deck_a: Deck::new(vec![Layer::new(Effect::Plasma)]),
            deck_b: Deck::new(vec![Layer::new(Effect::Gradient)]),
            canvas_a: Canvas::new(w, h),
            canvas_b: Canvas::new(w, h),
            out: Canvas::new(w, h),
            xfade: 0.0,
            fade: FadeType::Cross,
            focus: Focus::A,
            rig: Rig { controllers },
            tex: None,
        }
    }

    fn canvas_image(&self) -> egui::ColorImage {
        let mut rgba = Vec::with_capacity(self.out.w * self.out.h * 4);
        for [r, g, b] in &self.out.px {
            rgba.extend_from_slice(&[*r, *g, *b, 255]);
        }
        egui::ColorImage::from_rgba_unmultiplied([self.out.w, self.out.h], &rgba)
    }

    fn layer_panel(ui: &mut egui::Ui, layers: &mut Vec<Layer>) {
        if ui.button("+ Add layer").clicked() {
            layers.push(Layer::new(Effect::Color));
        }
        ui.separator();
        let mut remove = None;
        for i in (0..layers.len()).rev() {
            let l = &mut layers[i];
            ui.push_id(i, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut l.enabled, "");
                    egui::ComboBox::from_id_salt("fx")
                        .selected_text(l.effect.name())
                        .show_ui(ui, |ui| {
                            for e in Effect::ALL {
                                ui.selectable_value(&mut l.effect, e, e.name());
                            }
                        });
                    if i > 0 {
                        egui::ComboBox::from_id_salt("mix")
                            .selected_text(l.mix.name())
                            .show_ui(ui, |ui| {
                                for m in MixMode::ALL {
                                    ui.selectable_value(&mut l.mix, m, m.name());
                                }
                            });
                    } else {
                        ui.label("base");
                    }
                    if ui.button("🗑").clicked() {
                        remove = Some(i);
                    }
                });
                ui.add(egui::Slider::new(&mut l.opacity, 0.0..=1.0).text("opacity"));
                ui.horizontal(|ui| {
                    ui.label("speed");
                    for n in [1u32, 2, 4, 8, 16] {
                        if ui.selectable_label(l.beats_per_cycle == n, format!("{n}/1")).clicked() {
                            l.beats_per_cycle = n;
                        }
                    }
                });
                if l.effect.directional() {
                    ui.horizontal(|ui| {
                        ui.label("dir");
                        for d in 0..8u8 {
                            if ui.selectable_label(l.params.dir == d, DIR_ARROWS[d as usize]).clicked()
                            {
                                l.params.dir = d;
                            }
                        }
                    });
                    ui.add(egui::Slider::new(&mut l.params.pitch, 0.5..=16.0).text("pitch"));
                }
                match l.effect {
                    Effect::Wave => {
                        ui.add(egui::Slider::new(&mut l.params.width, 0.05..=1.0).text("width"));
                    }
                    Effect::Gradient => {
                        egui::ComboBox::from_id_salt("palette")
                            .selected_text("palette…")
                            .show_ui(ui, |ui| {
                                for pal in palette::PALETTES {
                                    if ui.selectable_label(false, pal.name).clicked() {
                                        palette::apply(&mut l.params, pal);
                                    }
                                }
                            });
                        ui.add(egui::Slider::new(&mut l.params.n_colors, 2..=8).text("colors"));
                        ui.horizontal(|ui| {
                            for k in 0..l.params.n_colors.clamp(2, 8) as usize {
                                ui.color_edit_button_srgb(&mut l.params.colors[k]);
                            }
                        });
                    }
                    Effect::Plasma => {
                        ui.add(egui::Slider::new(&mut l.params.pitch, 0.1..=8.0).text("zoom"));
                    }
                    Effect::Color => {}
                }
                ui.collapsing("Map", |ui| {
                    ui.add(egui::Slider::new(&mut l.map.offset.0, -1.0..=1.0).text("offset x"));
                    ui.add(egui::Slider::new(&mut l.map.offset.1, -1.0..=1.0).text("offset y"));
                    ui.add(egui::Slider::new(&mut l.map.scale.0, 0.1..=4.0).text("scale x"));
                    ui.add(egui::Slider::new(&mut l.map.scale.1, 0.1..=4.0).text("scale y"));
                });
                ui.separator();
            });
        }
        if let Some(i) = remove {
            layers.remove(i);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let beats = self.clock.beats();
        crate::layer::render(&self.deck_a.layers, &mut self.canvas_a, beats);
        crate::layer::render(&self.deck_b.layers, &mut self.canvas_b, beats);
        crossfader::blend(&self.canvas_a, &self.canvas_b, self.xfade, self.fade, &mut self.out);

        self.rig.send(&self.out);

        let img = self.canvas_image();
        match &mut self.tex {
            Some(tex) => tex.set(img, Default::default()),
            None => self.tex = Some(ctx.load_texture("canvas", img, Default::default())),
        }

        egui::TopBottomPanel::top("transport").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let mut bpm = self.clock.bpm();
                if ui.add(egui::Slider::new(&mut bpm, 20.0..=300.0).text("BPM")).changed() {
                    self.clock.set_bpm(bpm);
                }
                if ui.button("Tap").clicked() {
                    self.clock.tap();
                }
                ui.label(format!("beat {:.2}", beats.rem_euclid(1.0)));
            });
        });

        egui::TopBottomPanel::bottom("crossfader").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("A");
                ui.add(egui::Slider::new(&mut self.xfade, 0.0..=1.0).show_value(false));
                ui.label("B");
                egui::ComboBox::from_id_salt("fade")
                    .selected_text(self.fade.name())
                    .show_ui(ui, |ui| {
                        for f in FadeType::ALL {
                            ui.selectable_value(&mut self.fade, f, f.name());
                        }
                    });
            });
        });

        egui::SidePanel::right("decks").default_width(280.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Edit:");
                ui.selectable_value(&mut self.focus, Focus::A, "Deck A");
                ui.selectable_value(&mut self.focus, Focus::B, "Deck B");
            });
            let deck = match self.focus {
                Focus::A => &mut self.deck_a,
                Focus::B => &mut self.deck_b,
            };
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| Self::layer_panel(ui, &mut deck.layers));
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(tex) = &self.tex {
                let size = ui.available_size();
                ui.add(egui::Image::new(tex).fit_to_exact_size(size));
            }
        });

        ctx.request_repaint();
    }
}
