# ledbetter

A macOS desktop application written in Rust. DMX/OSC/MIDI lighting control,
built on the same stack as [COBRA_COMMANDER](https://github.com/generalelectrix/COBRA_COMMANDER):
`eframe`/`egui` for the GUI, `rust_dmx`/`rosc` for I/O, and the
`generalelectrix/tunnels` crates.

## Build

```sh
cargo run
```

Requires the Rust toolchain (stable, edition 2024). Install via
[rustup](https://rustup.rs).
