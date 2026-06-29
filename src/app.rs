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
use std::collections::HashMap;
use std::time::Duration;

use crate::output::{Transport, offline_port};
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
    // DMX settings panel (COBRA-style runtime port selection; ephemeral)
    show_dmx: bool,
    scan_artnet: bool,
    artnet_timeout: String,
    available_ports: Vec<Box<dyn rust_dmx::DmxPort>>,
    /// Selected port in the pool; None = the offline option.
    selected_port: Option<usize>,
    /// Per-(controller, universe) fps text buffers.
    framerate_text: HashMap<(usize, u16), String>,
}

impl App {
    pub fn new(target: String) -> Self {
        let (w, h) = (128, 128);
        // Demo rig: a spikey-circle of 12 spokes on Art-Net is the hero — a
        // Radial effect radiates out every spoke. The others (GRB line on sACN,
        // a small RGBW+dimmer USB-DMX node, a serpentine WLED matrix tucked in a
        // corner) exercise every format/transport. USB-DMX/WLED are no-op stubs
        // until tested on hardware. Replaced by a loaded Patch file at M5.
        // Art-Net target IP from the CLI arg (strip any :port; rust_dmx uses 6454).
        let addr = target
            .split(':')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(std::net::Ipv4Addr::LOCALHOST);
        let controllers = vec![
            // hero: 12 spokes on Art-Net via rust_dmx
            Controller::artnet(addr, 0, patch::spokes(12, 20, PixelFormat::Rgb)),
            // GRB line on sACN (hand-rolled)
            Controller::sacn(
                *b"ledbetter-cid-01",
                1,
                vec![Output::line(PixelFormat::Grb, 40, (0.1, 0.95), (0.9, 0.95))],
            ),
            // offline rust_dmx node: RGBW matrix + a Mono dimmer
            Controller::offline(
                2,
                vec![
                    Output::matrix(PixelFormat::Rgbw, 6, 6, Wiring::Contiguous)
                        .placed((0.02, 0.02), (0.20, 0.20)),
                    Output::line(PixelFormat::Mono, 1, (0.95, 0.5), (0.95, 0.5)),
                ],
            ),
            // WLED stub: serpentine matrix
            Controller::wled(
                3,
                vec![Output::matrix(PixelFormat::Rgb, 6, 6, Wiring::Serpentine)
                    .placed((0.80, 0.02), (0.98, 0.20))],
            ),
        ];
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
            show_dmx: false,
            scan_artnet: false,
            artnet_timeout: "3".to_string(),
            available_ports: Vec::new(),
            selected_port: None,
            framerate_text: HashMap::new(),
        }
    }

    fn canvas_image(&self) -> egui::ColorImage {
        let mut rgba = Vec::with_capacity(self.out.w * self.out.h * 4);
        for [r, g, b] in &self.out.px {
            rgba.extend_from_slice(&[*r, *g, *b, 255]);
        }
        egui::ColorImage::from_rgba_unmultiplied([self.out.w, self.out.h], &rgba)
    }

    /// COBRA-style DMX Ports page: refresh + Scan ArtNet, a selectable port pool
    /// (offline + discovered), Assign per universe, and a per-universe fps field
    /// for framerate-capable ports. Ephemeral — re-select each launch (M5 Patch
    /// file will persist it). Single-threaded: mutates the Rig directly.
    fn dmx_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_dmx;
        let artnet_valid = !self.scan_artnet
            || self.artnet_timeout.parse::<f32>().map(|v| v > 0.0).unwrap_or(false);
        let offline_name = rust_dmx::OfflineDmxPort.to_string();
        let err_color = egui::Color32::from_rgb(180, 60, 60);

        let mut refresh = false;
        let mut assign: Option<(usize, u16)> = None; // assign selected port to (controller, universe)
        let mut set_fps: Option<(usize, u16, u8)> = None;

        egui::Window::new("DMX Ports").open(&mut open).show(ctx, |ui| {
            // Refresh + ArtNet scan options.
            ui.horizontal(|ui| {
                if ui.add_enabled(artnet_valid, egui::Button::new("Refresh")).clicked() {
                    refresh = true;
                }
                ui.checkbox(&mut self.scan_artnet, "Scan ArtNet");
                if self.scan_artnet {
                    ui.label("Timeout:");
                    ui.add(egui::TextEdit::singleline(&mut self.artnet_timeout).desired_width(30.0));
                    ui.label("sec");
                    if !artnet_valid {
                        ui.colored_label(err_color, "must be a positive number");
                    }
                }
            });
            ui.separator();

            // Available ports pool (offline + discovered), single selection.
            ui.label(format!("Available Ports ({})", self.available_ports.len() + 1));
            let mut new_sel: Option<Option<usize>> = None;
            egui::ScrollArea::vertical().id_salt("dmx_pool").max_height(140.0).show(ui, |ui| {
                if ui.selectable_label(self.selected_port.is_none(), &offline_name).clicked() {
                    new_sel = Some(None);
                }
                for (i, p) in self.available_ports.iter().enumerate() {
                    if ui.selectable_label(self.selected_port == Some(i), p.to_string()).clicked() {
                        new_sel = Some(Some(i));
                    }
                }
            });
            if let Some(s) = new_sel {
                self.selected_port = s;
            }
            ui.separator();

            // Universe list across DMX controllers.
            let selected_name = match self.selected_port {
                None => offline_name.clone(),
                Some(i) => {
                    self.available_ports.get(i).map(|p| p.to_string()).unwrap_or(offline_name.clone())
                }
            };
            egui::ScrollArea::vertical().id_salt("dmx_universes").max_height(260.0).show(ui, |ui| {
                egui::Grid::new("dmx_universe_grid").striped(true).show(ui, |ui| {
                    for (ci, c) in self.rig.controllers.iter().enumerate() {
                        let Transport::Dmx(ports) = &c.transport else { continue };
                        for (u, p) in ports {
                            let current = p.to_string();
                            let same = current == selected_name;
                            if ui
                                .add_enabled(!same, egui::Button::new("Assign"))
                                .on_hover_text(format!("Assign {selected_name}"))
                                .clicked()
                            {
                                assign = Some((ci, *u));
                            }
                            ui.label(format!("Ctrl {ci} · Univ {u}"));
                            ui.label(&current);
                            if let Some(fps) = p.get_framerate() {
                                let buf = self
                                    .framerate_text
                                    .entry((ci, *u))
                                    .or_insert_with(|| fps.to_string());
                                let resp =
                                    ui.add(egui::TextEdit::singleline(buf).desired_width(40.0));
                                if resp.lost_focus() {
                                    match buf.parse::<u8>() {
                                        Ok(f) if f > 0 => set_fps = Some((ci, *u, f)),
                                        _ => *buf = fps.to_string(),
                                    }
                                } else if !resp.has_focus() && *buf != fps.to_string() {
                                    *buf = fps.to_string();
                                }
                                ui.label("fps");
                            }
                            ui.end_row();
                        }
                    }
                });
            });
        });
        self.show_dmx = open;

        if refresh {
            let browse = if self.scan_artnet {
                Some(Duration::from_secs_f32(self.artnet_timeout.parse::<f32>().unwrap_or(3.0).max(0.5)))
            } else {
                None
            };
            if let Ok(ports) = rust_dmx::available_ports(browse) {
                self.available_ports = ports;
                self.selected_port = None;
            }
        }
        if let Some((ci, u)) = assign {
            let mut port = match self.selected_port.take() {
                Some(i) if i < self.available_ports.len() => self.available_ports.remove(i),
                _ => offline_port(),
            };
            let _ = port.open();
            if let Transport::Dmx(ports) = &mut self.rig.controllers[ci].transport {
                ports.insert(u, port);
            }
        }
        if let Some((ci, u, f)) = set_fps
            && let Transport::Dmx(ports) = &mut self.rig.controllers[ci].transport
            && let Some(p) = ports.get_mut(&u)
        {
            let _ = p.set_framerate(f);
        }
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
                ui.separator();
                ui.toggle_value(&mut self.show_dmx, "DMX");
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

        if self.show_dmx {
            self.dmx_window(ctx);
        }

        ctx.request_repaint();
    }
}
