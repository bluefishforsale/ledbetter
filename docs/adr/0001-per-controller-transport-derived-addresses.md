# Per-Controller transport; pixel addresses derived, not authored

The patch is a tree — **Controller → Output → Strip → Pixels** — and the output
**transport** (Art-Net node IP, sACN, WLED box IP, USB-DMX port) is a property of
each **Controller**, not a global output mode. ledbetter drives many Controllers
of mixed transports simultaneously. Per-pixel DMX addresses `(universe, channel)`
are always *derived* from Controller base + Output offset + index, never
hand-authored; the patch only names controllers, outputs, pixel counts, color
order (Mono/RGB/RGBW/GRB…), and wiring (contiguous/serpentine).

## Considered Options

- **Global output fan-out** (the initial PRD draft): one canvas "drives all
  configured outputs," transport chosen globally. Rejected — real rigs mix an
  Art-Net node, a WLED box, and a USB dongle at once; transport must be per-device.
- **MADRIX flat fixture-patch with hand-assigned addresses.** Rejected — wanted
  the matrix-generator ergonomics (auto-increment from a base) as the default.

## Consequences

Camera mapping enumerates the *derived* Pixel list and only assigns each Pixel its
`(u,v)`; it never touches addressing. Adding a strip never requires computing
universes by hand.
