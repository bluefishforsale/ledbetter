# Single render thread at one rate; WLED async

One render thread runs the beat clock, renders both deck canvases, crossfades,
and samples every pixel at a single fixed rate (~44 Hz). Real-time Controllers
(Art-Net, sACN, USB-DMX) are packetized and sent inline on that same tick
(building those packets is microseconds). WLED Controllers get their own async
worker (HTTP is slow), fed from the lock-free published frame. The GUI reads a
canvas snapshot lock-free and never blocks the render thread.

## Considered Options

- **MADRIX's split: decouple Main Mixing FPS from per-device output FPS.**
  Rejected for v1 — adds a per-controller scheduler for no benefit when all
  real-time transports run at one rate and WLED self-throttles. Revisit only if a
  real controller forces a different send rate.

## Consequences

No per-device FPS scheduler exists in v1. Camera mapping is an exclusive mode that
takes over output (no live show during calibration). Adopts COBRA's
`worker::spawn` discipline and no-panic rule on the render path.
