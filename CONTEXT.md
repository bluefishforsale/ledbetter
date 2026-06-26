# ledbetter

A macOS LED pixel-mapping and content engine: generate visual content on a
virtual canvas, composite it through layers, blend two decks with a crossfader,
and pixel-map the result onto physical LEDs over DMX/Art-Net/sACN/WLED. The
LEDs are a low-resolution screen. Built on COBRA_COMMANDER's infrastructure, but
canvas-centric (MADRIX-style), not fixture-parameter-centric.

## Language

**Canvas**:
A fixed-resolution rectangular RGB framebuffer (`W×H`) that effects render into
and that all compositing operates on. The universal substrate. Not the physical
layout.
_Avoid_: matrix (reserve for the physical grid), framebuffer (implementation term)

**Pixel**:
One physical LED output point. Holds a derived DMX address `(universe, channel)`
and a sampling coordinate `(u,v)` in `[0,1]²`. Its color each frame is the
**Canvas** sampled (bilinear) at its `(u,v)`. Addresses are never hand-authored —
they are derived from **Controller** base + **Output** offset + index.
_Avoid_: LED (use for the hardware), voxel (reserve for future 3D), fixture

**Controller**:
A physical device that drives LEDs (Art-Net/sACN node, WLED box, USB-DMX dongle).
The **transport** lives here — a Controller *is* its connection (an IP, a port).
Carries a base address. ledbetter drives many Controllers of mixed transports
simultaneously.
_Avoid_: node, device, interface (pick Controller)

**Output**:
One physical port on a **Controller**, feeding one **Strip**. Carries color order
(RGB/RGBW/GRB…) and wiring (Contiguous | Serpentine). Often the unit treated as a
"fixture." Its universe/channel range auto-increments from the Controller base,
with optional override.
_Avoid_: port, channel (overloaded), fixture

**Strip**:
The run of N **Pixels** on one **Output**. The compact patch authors a Strip
(count + color order + wiring); the runtime expands it to N addressed **Pixels**.
_Avoid_: run, segment, chain

**Transport**:
How a **Controller** sends data: ArtNet{ip, base_universe} | sACN{base_universe}
| WLED{ip} | UsbDmx{port_path}. A per-Controller property, NOT a global output
mode.

**Sampling coordinate (u,v)**:
A **Pixel**'s normalized position in canvas space. Assigned by camera mapping,
matrix generator, or hand-edit. Camera-derived coordinates live in camera-image
space, normalized to the bounding box of detected pixels.

**Effect**:
A generator that fills (a region of) the **Canvas** each frame, intrinsically
animated as a function of time. Driven by the master clock × the **Storage
Place**'s Pitch. v1 set: Color, Gradient, Wave, Plasma. No external modulation —
movement is built into the effect.
_Avoid_: animation (effects ARE the animation), pattern, generator

**Master clock**:
A tap-tempo beat clock (BPM, set by tap or typed) emitting a beat phase `[0,1)`.
Drives all **Effect** animation so the rig moves with the music — no audio
analysis, just tapped tempo. The bridge to deferred audio-sync.

**Pitch**:
A **Storage Place**'s beat multiplier on the **Master clock** (e.g. ½×, 1×, 2×) —
keeps effects beat-locked at musical ratios. Per-place, not per-effect.

**Layer**:
One **Effect** plus a Map (where on the **Canvas** it draws), a mix mode (how it
composites onto layers below), and opacity. Stacked; last = top.

**Storage Place**:
A recallable look = an owned **Layer** stack + submaster (intensity) + pitch
(speed). Lives in a shared bank. The unit a **Deck** points at.
_Avoid_: cue (reserve for the deferred cue list), preset, scene

**Deck**:
A playhead. Points at one **Storage Place** in the shared bank and renders it to
its own **Canvas**. There are exactly two: A and B. A Deck references content; it
does not own it.
_Avoid_: channel, layer (a Deck is neither)

**Edit focus**:
Which **Deck** the operator is currently editing. Layer edits target the
focused Deck's current **Storage Place**. Exactly one Deck has focus at a time.

**Crossfader**:
Blends the two Decks' rendered **Canvases** (not their layers) into the output
canvas. v1 fade types: Cross / White / Black.

## Relationships

- A **Canvas** is sampled by many **Pixels** at their `(u,v)` coordinates
- Camera mapping assigns each **Pixel** its `(u,v)`; it does not change the
  **Canvas** model (the grid is the substrate regardless of how `(u,v)` is set)
- Two **Decks** (A, B) each point at one **Storage Place** from one shared bank
- A **Storage Place** owns a **Layer** stack; **Decks** reference places, so the
  same place could be shown on both decks
- The **Crossfader** mixes the two Decks' rendered **Canvases**
- Editing targets the **Edit focus** Deck's current **Storage Place**

**Patch file**:
The venue/rig state: **Controllers**, **Outputs**, **Strips**, and the
camera-mapped `(u,v)` coordinates. Specific to one physical install. Recaptured
per venue. (`*.ledbetterpatch`)
_Avoid_: setup, venue file (pick Patch file)

**Show file**:
The creative state: the bank of **Storage Places** (looks), **Crossfader** /
fade setup, **Master clock**. Portable across venues — plays on whatever **Patch**
is loaded. Strictly canvas-space; never references a **Controller**, **Output**,
or pixel address. (`*.ledbetter`)
_Avoid_: project, scene file (pick Show file)

## Invariants

- **Canvas-space / pixel-space separation.** Looks (Show file) live entirely in
  canvas `[0,1]²` space. The patch (Patch file) lives entirely in pixel space. The
  ONLY bridge is per-**Pixel** `(u,v)` sampling of the **Canvas**. A **Layer**'s
  Map is canvas-space; it never names a pixel or address. This is what makes looks
  portable across venues.

## Example dialogue

> **Dev:** "When the operator drags a layer's color knob, which **Storage Place**
> changes — the one on Deck A or Deck B?"
> **LD:** "Whichever deck has **Edit focus**. Usually that's the cued deck, off-air,
> while the other plays out through the **Crossfader**."
> **Dev:** "And if I take that look to a different venue with different bars?"
> **LD:** "It just plays. The look is in **Canvas** space — it never knew about my
> **Controllers**. I load a new **Patch file**, re-run the camera map, done."

## Flagged ambiguities

- "canvas" could mean a rectangular grid (MADRIX) or a continuous field evaluated
  per real-pixel-position — resolved: **rectangular grid framebuffer**. Effects
  render to the grid; **Pixels** bilinear-sample it. Camera mapping only assigns
  `(u,v)`, never replaces the grid.
- "animation engine" (from COBRA, where it modulates fixture *parameters*) — does
  NOT transfer. Resolved: **Effects** self-animate from the **Master clock**;
  there is no separate modulation engine in v1. LFO/param-automation deferred.
- "output" was used as a global fan-out mode AND as a physical port — resolved:
  the port is an **Output**; the send protocol is **Transport**, a per-**Controller**
  property. There is no global output mode.
