//! Tap-tempo beat clock. Emits a beat phase in [0,1) so effects move with the
//! music without audio analysis (CONTEXT.md "Master clock").

use std::time::Instant;

pub struct BeatClock {
    bpm: f32,
    start: Instant,
    taps: Vec<Instant>,
}

impl BeatClock {
    pub fn new(bpm: f32) -> Self {
        BeatClock { bpm, start: Instant::now(), taps: Vec::new() }
    }

    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    pub fn set_bpm(&mut self, bpm: f32) {
        self.bpm = bpm.clamp(20.0, 300.0);
    }

    /// Total elapsed beats, monotonically increasing. Effects divide this by a
    /// per-deck beats-per-cycle to run slower than one loop per beat.
    /// ponytail: computed from start, so changing BPM rescales the timeline.
    /// A per-frame phase accumulator is the upgrade if live re-tapping jumps.
    pub fn beats(&self) -> f32 {
        self.start.elapsed().as_secs_f32() * self.bpm / 60.0
    }

    /// Register a tap. Two or more taps within ~2s set the BPM from the mean
    /// inter-tap interval. ponytail: averaging window, not a PLL — fine for live.
    pub fn tap(&mut self) {
        self.tap_at(Instant::now());
    }

    fn tap_at(&mut self, now: Instant) {
        if let Some(&last) = self.taps.last()
            && now.duration_since(last).as_secs_f32() > 2.0
        {
            self.taps.clear();
        }
        self.taps.push(now);
        if self.taps.len() >= 2 {
            let span = now.duration_since(self.taps[0]).as_secs_f32();
            let intervals = (self.taps.len() - 1) as f32;
            if span > 0.0 {
                self.set_bpm(60.0 * intervals / span);
            }
        }
        if self.taps.len() > 8 {
            self.taps.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn taps_at_120bpm_derive_120() {
        let mut clk = BeatClock::new(60.0);
        let t = Instant::now();
        // four taps 500ms apart == 120 BPM
        for i in 0..4 {
            clk.tap_at(t + Duration::from_millis(500 * i));
        }
        assert!((clk.bpm() - 120.0).abs() < 0.5, "bpm was {}", clk.bpm());
    }
}
