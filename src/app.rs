//! egui preview: render both decks, crossfade to the output canvas, show it, and
//! feed Art-Net. ponytail: still single-threaded in the egui update loop; the
//! dedicated render thread + lock-free publish (ADR-0002) lands when multiple
//! outputs make it pay.

use std::sync::Arc;

use eframe::egui;
use eframe::glow;

use crate::canvas::Canvas;
use crate::clock::BeatClock;
use crate::crossfader::{self, FadeType};
use crate::deck::Deck;
use crate::effect::{DIR_ARROWS, Effect};
use crate::layer::{Layer, MixMode};
use crate::output::{ArtNet, Sacn, Transport};
use crate::palette;
use crate::patch::{self, Controller, Output, PixelFormat, Rig, Wiring};
use crate::shader::{self, GpuShader};

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Rig,
    Canvas,
}

/// A deck's optional GLSL shader override. When enabled it renders the deck's
/// canvas on the GPU instead of the CPU layer stack.
struct DeckShader {
    enabled: bool,
    src: String,
    dirty: bool,
    compiled: Option<GpuShader>,
    error: Option<String>,
}

impl DeckShader {
    fn new(enabled: bool) -> Self {
        DeckShader { enabled, src: shader::EXAMPLE.to_string(), dirty: true, compiled: None, error: None }
    }
}

/// A slider whose label is clickable — clicking the label resets to `default`.
fn rslider<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    label: &str,
    default: N,
) {
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(value, range));
        if ui
            .add(egui::Label::new(label).sense(egui::Sense::click()))
            .on_hover_text("click to reset")
            .clicked()
        {
            *value = default;
        }
    });
}

/// Render a deck: GLSL shader if enabled and the GL context is available,
/// otherwise the CPU layer stack (also the fallback on compile error).
fn render_deck(
    sh: &mut DeckShader,
    layers: &[Layer],
    canvas: &mut Canvas,
    beats: f32,
    gl: Option<&Arc<glow::Context>>,
) {
    if sh.enabled && let Some(gl) = gl {
        if sh.dirty {
            match GpuShader::new(gl.clone(), canvas.w, canvas.h, &sh.src) {
                Ok(s) => {
                    sh.compiled = Some(s);
                    sh.error = None;
                }
                Err(e) => {
                    sh.compiled = None;
                    sh.error = Some(e);
                }
            }
            sh.dirty = false;
        }
        if let Some(s) = sh.compiled.as_mut() {
            s.render(beats);
            s.copy_into(canvas);
            return;
        }
    }
    crate::layer::render(layers, canvas, beats);
}

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
    view: View,
    shader_a: DeckShader,
    shader_b: DeckShader,
    tex: Option<egui::TextureHandle>,
}

impl App {
    pub fn new(target: String) -> Self {
        let (w, h) = (128, 128);
        // Demo rig: a spikey-circle of 12 spokes on Art-Net is the hero — a
        // Radial effect radiates out every spoke. The others (GRB line on sACN,
        // a small RGBW+dimmer USB-DMX node, a serpentine WLED matrix tucked in a
        // corner) exercise every format/transport. USB-DMX/WLED are no-op stubs
        // until tested on hardware. Replaced by a loaded Patch file at M5.
        let mut controllers = Vec::new();
        if let Ok(a) = ArtNet::new(target) {
            controllers.push(Controller::new(
                Transport::ArtNet(a),
                0,
                patch::spokes(12, 20, PixelFormat::Rgb),
            ));
        }
        if let Ok(s) = Sacn::new(*b"ledbetter-cid-01") {
            let line = Output::line(PixelFormat::Grb, 40, (0.1, 0.95), (0.9, 0.95));
            controllers.push(Controller::new(Transport::Sacn(s), 1, vec![line]));
        }
        controllers.push(Controller::new(
            Transport::UsbDmx,
            2,
            vec![
                Output::matrix(PixelFormat::Rgbw, 6, 6, Wiring::Contiguous)
                    .placed((0.02, 0.02), (0.20, 0.20)),
                Output::line(PixelFormat::Mono, 1, (0.95, 0.5), (0.95, 0.5)),
            ],
        ));
        controllers.push(Controller::new(
            Transport::Wled,
            3,
            vec![Output::matrix(PixelFormat::Rgb, 6, 6, Wiring::Serpentine)
                .placed((0.80, 0.02), (0.98, 0.20))],
        ));
        App {
            clock: BeatClock::new(120.0),
            deck_a: Deck::new(vec![Layer::new(Effect::Plasma)]),
            deck_b: Deck::new(vec![Layer::new(Effect::Gradient)]),
            canvas_a: Canvas::new(w, h),
            canvas_b: Canvas::new(w, h),
            out: Canvas::new(w, h),
            xfade: 0.0,
            fade: FadeType::Multiply,
            focus: Focus::A,
            rig: Rig { controllers },
            view: View::Rig,
            shader_a: DeckShader::new(false),
            shader_b: {
                // deck B showcases the GLSL runner with the feedback example
                let mut s = DeckShader::new(true);
                s.src = shader::FEEDBACK_EXAMPLE.to_string();
                s
            },
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

    fn palette_ui(ui: &mut egui::Ui, params: &mut crate::effect::Params) {
        egui::ComboBox::from_id_salt("palette").selected_text("palette…").show_ui(ui, |ui| {
            for pal in palette::PALETTES {
                if ui.selectable_label(false, pal.name).clicked() {
                    palette::apply(params, pal);
                }
            }
        });
        rslider(ui, &mut params.n_colors, 2..=8, "colors", 3);
        ui.horizontal(|ui| {
            for k in 0..params.n_colors.clamp(2, 8) as usize {
                ui.color_edit_button_srgb(&mut params.colors[k]);
            }
        });
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
                rslider(ui, &mut l.opacity, 0.0..=1.0, "opacity", 1.0);
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
                    rslider(ui, &mut l.params.pitch, 0.5..=16.0, "pitch", 1.0);
                }
                match l.effect {
                    Effect::Wave => {
                        rslider(ui, &mut l.params.width, 0.05..=1.0, "width", 0.5);
                    }
                    Effect::Gradient => Self::palette_ui(ui, &mut l.params),
                    Effect::Radial => {
                        rslider(ui, &mut l.params.pitch, 0.5..=16.0, "rings", 1.0);
                        Self::palette_ui(ui, &mut l.params);
                    }
                    Effect::Sparkle => {
                        rslider(ui, &mut l.params.width, 0.02..=0.6, "density", 0.5);
                        Self::palette_ui(ui, &mut l.params);
                    }
                    Effect::Plasma => {
                        rslider(ui, &mut l.params.pitch, 0.1..=8.0, "zoom", 1.0);
                    }
                    Effect::Color => {}
                }
                ui.collapsing("Map", |ui| {
                    rslider(ui, &mut l.map.offset.0, -1.0..=1.0, "offset x", 0.0);
                    rslider(ui, &mut l.map.offset.1, -1.0..=1.0, "offset y", 0.0);
                    rslider(ui, &mut l.map.scale.0, 0.1..=4.0, "scale x", 1.0);
                    rslider(ui, &mut l.map.scale.1, 0.1..=4.0, "scale y", 1.0);
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
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let beats = self.clock.beats();
        let gl = frame.gl().cloned();
        render_deck(&mut self.shader_a, &self.deck_a.layers, &mut self.canvas_a, beats, gl.as_ref());
        render_deck(&mut self.shader_b, &self.deck_b.layers, &mut self.canvas_b, beats, gl.as_ref());
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
                rslider(ui, &mut bpm, 20.0..=300.0, "BPM", 120.0);
                self.clock.set_bpm(bpm);
                if ui.button("Tap").clicked() {
                    self.clock.tap();
                }
                ui.label(format!("beat {:.2}", beats.rem_euclid(1.0)));
                ui.separator();
                ui.selectable_value(&mut self.view, View::Rig, "Rig");
                ui.selectable_value(&mut self.view, View::Canvas, "Canvas");
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
            {
                let sh = match self.focus {
                    Focus::A => &mut self.shader_a,
                    Focus::B => &mut self.shader_b,
                };
                ui.collapsing("GLSL shader", |ui| {
                    if ui.checkbox(&mut sh.enabled, "enable (overrides layers)").changed() {
                        sh.dirty = true;
                    }
                    if sh.enabled {
                        ui.horizontal(|ui| {
                            ui.label("load:");
                            if ui.button("basic").clicked() {
                                sh.src = shader::EXAMPLE.to_string();
                                sh.dirty = true;
                            }
                            if ui.button("feedback").clicked() {
                                sh.src = shader::FEEDBACK_EXAMPLE.to_string();
                                sh.dirty = true;
                            }
                        });
                        let r = ui.add(
                            egui::TextEdit::multiline(&mut sh.src)
                                .code_editor()
                                .desired_rows(8)
                                .desired_width(f32::INFINITY),
                        );
                        if r.changed() {
                            sh.dirty = true;
                        }
                        if let Some(e) = &sh.error {
                            ui.colored_label(egui::Color32::RED, e);
                        }
                    }
                });
            }
            ui.separator();
            let deck = match self.focus {
                Focus::A => &mut self.deck_a,
                Focus::B => &mut self.deck_b,
            };
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| Self::layer_panel(ui, &mut deck.layers));
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.view {
            View::Canvas => {
                if let Some(tex) = &self.tex {
                    let size = ui.available_size();
                    ui.add(egui::Image::new(tex).fit_to_exact_size(size));
                }
            }
            View::Rig => {
                let (resp, painter) =
                    ui.allocate_painter(ui.available_size(), egui::Sense::hover());
                let rect = resp.rect;
                let side = rect.width().min(rect.height());
                painter.rect_filled(rect, 0.0, egui::Color32::from_gray(12));
                for (u, v, c) in self.rig.preview(&self.out) {
                    let pos = rect.min + egui::vec2(u * side, v * side);
                    painter.circle_filled(pos, 3.0, egui::Color32::from_rgb(c[0], c[1], c[2]));
                }
            }
        });

        ctx.request_repaint();
    }
}
