//! egui preview: composite the layer stack into the Canvas each frame, show it
//! as a texture, and keep sending Art-Net. ponytail: rendering runs in the egui
//! update loop for now; the dedicated render thread + lock-free publish (ADR-0002)
//! arrives at M3 when two decks and multiple outputs make it pay.

use eframe::egui;

use crate::canvas::Canvas;
use crate::clock::BeatClock;
use crate::effect::Effect;
use crate::layer::{self, Layer, MixMode};
use crate::output::ArtNet;
use crate::patch::{self, Pixel};

pub struct App {
    canvas: Canvas,
    clock: BeatClock,
    layers: Vec<Layer>,
    patch: Vec<Pixel>,
    artnet: Option<ArtNet>,
    tex: Option<egui::TextureHandle>,
}

impl App {
    pub fn new(target: String) -> Self {
        App {
            canvas: Canvas::new(128, 128),
            clock: BeatClock::new(120.0),
            layers: vec![Layer::new(Effect::Plasma)],
            patch: patch::strip(50),
            artnet: ArtNet::new(target).ok(),
            tex: None,
        }
    }

    fn canvas_image(&self) -> egui::ColorImage {
        let mut rgba = Vec::with_capacity(self.canvas.w * self.canvas.h * 4);
        for [r, g, b] in &self.canvas.px {
            rgba.extend_from_slice(&[*r, *g, *b, 255]);
        }
        egui::ColorImage::from_rgba_unmultiplied([self.canvas.w, self.canvas.h], &rgba)
    }

    fn layer_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Layers");
        ui.label("(bottom = base, last = top)");
        if ui.button("+ Add layer").clicked() {
            self.layers.push(Layer::new(Effect::Color));
        }
        ui.separator();

        let mut remove = None;
        // Show top layer first for an intuitive stack view.
        for i in (0..self.layers.len()).rev() {
            let l = &mut self.layers[i];
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
                ui.collapsing("Map", |ui| {
                    ui.add(egui::Slider::new(&mut l.map.offset.0, -1.0..=1.0).text("offset x"));
                    ui.add(egui::Slider::new(&mut l.map.offset.1, -1.0..=1.0).text("offset y"));
                    ui.add(egui::Slider::new(&mut l.map.scale.0, 0.1..=4.0).text("scale x"));
                    ui.add(egui::Slider::new(&mut l.map.scale.1, 0.1..=4.0).text("scale y"));
                    ui.checkbox(&mut l.map.tile, "tile");
                });
                ui.separator();
            });
        }
        if let Some(i) = remove {
            self.layers.remove(i);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let beat = self.clock.phase();
        layer::render(&self.layers, &mut self.canvas, beat);

        if let Some(artnet) = self.artnet.as_mut() {
            let dmx = patch::render_frame(&self.canvas, &self.patch);
            let _ = artnet.send(0, &dmx);
        }

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
                ui.label(format!("beat {beat:.2}"));
            });
        });

        egui::SidePanel::right("layers").default_width(260.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.layer_panel(ui));
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
