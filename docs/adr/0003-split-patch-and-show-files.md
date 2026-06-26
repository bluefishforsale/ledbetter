# Split Patch file and Show file; looks are canvas-space only

Persistence splits into two files. A **Patch file** (`*.ledbetterpatch`) holds the
venue/rig state — Controllers, Outputs, Strips, and the camera-mapped `(u,v)`
coordinates. A **Show file** (`*.ledbetter`) holds the creative state — the bank
of Storage Places (looks), crossfader/fade setup, and master clock. The Show file
is portable: it plays on whatever Patch is loaded.

This rests on a hard invariant: **looks live entirely in canvas `[0,1]²` space and
never reference a Controller, Output, or pixel address.** The only bridge between
the two files is per-Pixel `(u,v)` sampling of the canvas.

## Considered Options

- **One bundled Setup file (MADRIX-style).** Rejected — welds hand-crafted looks
  to one venue's address map, which is exactly the coupling that makes lighting
  software miserable to gig with. The webcam-remap workflow only pays off if the
  rig changes while the looks don't.

## Consequences

The serialization boundary is fixed early because it is expensive to retrofit. A
Layer's Map is canvas-space; effects never name pixels. Load-patch and load-show
are independent actions in the UI.
