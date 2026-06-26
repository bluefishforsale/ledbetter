//! egui preview: render the selected effect into the Canvas each frame, show it
//! as a texture, and keep sending Art-Net. ponytail: rendering runs in the egui
//! update loop for now; the dedicated render thread + lock-free publish (ADR-0002)
//! arrives at M3 when two decks and multiple outputs make it pay.

use eframe::egui;

use crate::canvas::Canvas;
use crate::clock::BeatClock;
use crate::effect::Effect;
use crate::output::ArtNet;
use crate::patch::{self, Pixel};

pub struct App {
    canvas: Canvas,
    clock: BeatClock,
    effect: Effect,
    patch: Vec<Pixel>,
    artnet: Option<ArtNet>,
    tex: Option<egui::TextureHandle>,
}

impl App {
    pub fn new(target: String) -> Self {
        App {
            canvas: Canvas::new(128, 128),
            clock: BeatClock::new(120.0),
            effect: Effect::Plasma,
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
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let beat = self.clock.phase();
        self.effect.render(&mut self.canvas, beat);

        if let Some(artnet) = self.artnet.as_mut() {
            let dmx = patch::render_frame(&self.canvas, &self.patch);
            let _ = artnet.send(0, &dmx);
        }

        let img = self.canvas_image();
        match &mut self.tex {
            Some(tex) => tex.set(img, Default::default()),
            None => self.tex = Some(ctx.load_texture("canvas", img, Default::default())),
        }

        egui::TopBottomPanel::top("controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_label("Effect")
                    .selected_text(self.effect.name())
                    .show_ui(ui, |ui| {
                        for e in Effect::ALL {
                            ui.selectable_value(&mut self.effect, e, e.name());
                        }
                    });
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

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(tex) = &self.tex {
                let size = ui.available_size();
                ui.add(egui::Image::new(tex).fit_to_exact_size(size));
            }
        });

        ctx.request_repaint();
    }
}
