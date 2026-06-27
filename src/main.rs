//! ledbetter — a macOS LED pixel-mapping / content engine.
//! M1: four self-animating effects on a Canvas, a tap-tempo beat clock, an
//! egui preview, still feeding Art-Net. See CONTEXT.md and docs/adr/.

mod app;
mod canvas;
mod clock;
mod crossfader;
mod deck;
mod effect;
mod layer;
mod output;
mod palette;
mod patch;

fn main() -> eframe::Result<()> {
    let target = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:6454".to_string());

    eframe::run_native(
        "ledbetter",
        eframe::NativeOptions::default(),
        Box::new(move |_cc| Ok(Box::new(app::App::new(target)))),
    )
}
