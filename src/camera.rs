//! Camera-assisted pixel mapping — the verifiable core (CONTEXT.md / PRD M7).
//!
//! The live path (deferred to hardware): drive ONE pixel white at a time across
//! the whole rig (globally sequential → exactly one blob), capture a webcam
//! frame per lit pixel via `nokhwa`, and feed each frame to `detect_centroid`.
//! For straight strips we light only each Output's endpoints and `interpolate`
//! the pixels between; `normalize` then maps all camera-space points into canvas
//! `(u,v)` preserving aspect ratio. This module is that pure logic; the nokhwa
//! capture loop and the exclusive scan mode (ADR-0002) wire in on real hardware.
#![allow(dead_code)] // wired when the nokhwa capture layer lands (M7 hardware)

/// A single-channel (luminance) camera frame.
pub struct Frame {
    pub w: usize,
    pub h: usize,
    pub lum: Vec<u8>,
}

/// Brightness-weighted centroid of the pixels that exceed `baseline + threshold`.
/// `None` if nothing is bright enough (occluded / off-camera pixel). Subtracting
/// a dark `baseline` frame rejects ambient light.
pub fn detect_centroid(frame: &Frame, baseline: &Frame, threshold: u8) -> Option<(f32, f32)> {
    let (mut sx, mut sy, mut sw) = (0.0f32, 0.0f32, 0.0f32);
    for y in 0..frame.h {
        for x in 0..frame.w {
            let i = y * frame.w + x;
            let lit = frame.lum[i].saturating_sub(baseline.lum[i]);
            if lit > threshold {
                let w = lit as f32;
                sx += x as f32 * w;
                sy += y as f32 * w;
                sw += w;
            }
        }
    }
    (sw > 0.0).then(|| (sx / sw, sy / sw))
}

/// `n` positions from `start` to `end` inclusive (linear). For a straight strip
/// this fills the pixels between the two detected endpoints.
pub fn interpolate(start: (f32, f32), end: (f32, f32), n: usize) -> Vec<(f32, f32)> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![start];
    }
    (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1) as f32;
            (start.0 + (end.0 - start.0) * t, start.1 + (end.1 - start.1) * t)
        })
        .collect()
}

/// Normalize camera-space points into canvas `(u,v)`, preserving aspect ratio:
/// the longer axis maps to `[0,1]`, the shorter to `[0, shorter/longer]`, so a
/// wide bar array isn't stretched. Degenerate spans collapse to 0.5.
pub fn normalize(points: &[(f32, f32)]) -> Vec<(f32, f32)> {
    if points.is_empty() {
        return Vec::new();
    }
    let (mut minx, mut miny) = (f32::MAX, f32::MAX);
    let (mut maxx, mut maxy) = (f32::MIN, f32::MIN);
    for &(x, y) in points {
        minx = minx.min(x);
        maxx = maxx.max(x);
        miny = miny.min(y);
        maxy = maxy.max(y);
    }
    let span = (maxx - minx).max(maxy - miny);
    if span <= 0.0 {
        return points.iter().map(|_| (0.5, 0.5)).collect();
    }
    points.iter().map(|&(x, y)| ((x - minx) / span, (y - miny) / span)).collect()
}

/// One Output's endpoint scan: (start_centroid, end_centroid, pixel_count) in
/// camera space.
pub type EndpointScan = ((f32, f32), (f32, f32), usize);

/// Assemble a full canvas map from per-Output endpoint scans → one `(u,v)` per
/// pixel, in scan order, normalized together across the whole rig.
pub fn build_map(outputs: &[EndpointScan]) -> Vec<(f32, f32)> {
    let mut cam = Vec::new();
    for &(start, end, n) in outputs {
        cam.extend(interpolate(start, end, n));
    }
    normalize(&cam)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: usize, h: usize, fill: u8) -> Frame {
        Frame { w, h, lum: vec![fill; w * h] }
    }

    #[test]
    fn centroid_finds_bright_pixel_over_ambient() {
        let base = frame(5, 5, 20); // ambient
        let mut f = frame(5, 5, 20);
        let (x, y) = (3usize, 1usize); // one bright pixel in a 5-wide frame
        f.lum[y * 5 + x] = 240;
        assert_eq!(detect_centroid(&f, &base, 50), Some((3.0, 1.0)));
    }

    #[test]
    fn centroid_none_when_below_threshold() {
        let base = frame(4, 4, 20);
        let mut f = frame(4, 4, 20);
        f.lum[0] = 40; // only +20 over ambient, threshold 50
        assert_eq!(detect_centroid(&f, &base, 50), None);
    }

    #[test]
    fn interpolate_endpoints_inclusive() {
        assert_eq!(interpolate((0.0, 0.0), (10.0, 0.0), 3), vec![(0.0, 0.0), (5.0, 0.0), (10.0, 0.0)]);
        assert_eq!(interpolate((2.0, 2.0), (9.0, 9.0), 1), vec![(2.0, 2.0)]);
    }

    #[test]
    fn normalize_preserves_aspect() {
        // wide-short: width span 10, height span 2 -> v stays in [0, 0.2]
        let n = normalize(&[(0.0, 0.0), (10.0, 0.0), (0.0, 2.0)]);
        assert_eq!(n[0], (0.0, 0.0));
        assert_eq!(n[1], (1.0, 0.0));
        assert_eq!(n[2], (0.0, 0.2));
    }

    #[test]
    fn build_map_assembles_two_strips() {
        // two 3-pixel strips laid end to end horizontally in camera space
        let map = build_map(&[((0.0, 0.0), (2.0, 0.0), 3), ((0.0, 1.0), (2.0, 1.0), 3)]);
        assert_eq!(map.len(), 6);
        assert_eq!(map[0], (0.0, 0.0)); // normalized to bbox (span 2)
        assert_eq!(map[2], (1.0, 0.0));
        assert_eq!(map[5], (1.0, 0.5));
    }
}
