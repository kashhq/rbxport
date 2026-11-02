//! The headphone (cue) bus: a second stereo mix for the DJ's ears.
//!
//! Fork addition (`docs/fork.md`). A DJ controller's sound card has four
//! output lanes — the DDJ-FLX4's 1–2 are master, 3–4 headphones — and cueing
//! is hearing a deck there before the room does. Each deck's channel CUE
//! button (PFL, pre-fader listen) adds its post-EQ, pre-fader signal to the
//! cue sum, tapped inside `Channel::process`; the HEADPHONES MIX knob then
//! blends that sum with the finished master.
//!
//! `CueSettings` is the control side, atomics like `MixerSettings`;
//! `CueMixer` is the callback side and owns the smoothing. Nothing here runs
//! unless the sink has lanes 3–4 to put it in.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use cpal::traits::DeviceTrait;

use crate::smooth::Smoothed;

/// HEADPHONES MIX fully left: the cue sum only, nothing of the master.
const DEFAULT_MIX: f32 = 0.0;
/// Software headphone level at full. **[ASSUME]** the FLX4's own
/// HEADPHONES LEVEL knob is analog and after this, so unity here leaves the
/// level to the hardware; not yet checked on the physical unit.
const DEFAULT_LEVEL: f32 = 1.0;

/// What the controller has asked of the cue bus.
///
/// Written by the control side (the MIDI fast path), read once a callback.
/// Each value is its own atomic and none is read as a group, like the mixer.
#[derive(Debug)]
pub struct CueSettings {
    /// Per deck, A then B: whether its CUE (PFL) button is lit.
    pfl: [AtomicBool; 2],
    /// `f32` bits, 0 = cue only … 1 = master only, 0.5 = equal.
    mix: AtomicU32,
    /// `f32` bits, 0 to 1.
    level: AtomicU32,
}

impl Default for CueSettings {
    fn default() -> Self {
        Self {
            pfl: [AtomicBool::new(false), AtomicBool::new(false)],
            mix: AtomicU32::new(DEFAULT_MIX.to_bits()),
            level: AtomicU32::new(DEFAULT_LEVEL.to_bits()),
        }
    }
}

impl CueSettings {
    /// Sends deck `index` (0 = A, 1 = B) to the headphones, or stops.
    /// An index past the second deck is ignored.
    pub fn set_pfl(&self, index: usize, on: bool) {
        if let Some(slot) = self.pfl.get(index) {
            slot.store(on, Ordering::Relaxed);
            tracing::debug!(deck = index, on, "cue: PFL");
        }
    }

    pub fn pfl(&self, index: usize) -> bool {
        self.pfl.get(index).is_some_and(|slot| slot.load(Ordering::Relaxed))
    }

    /// The HEADPHONES MIX knob, 0 (cue) to 1 (master). Clamped rather than
    /// refused; a value that is not a number leaves the knob at cue.
    pub fn set_mix(&self, mix: f32) {
        let safe = if mix.is_finite() { mix.clamp(0.0, 1.0) } else { DEFAULT_MIX };
        self.mix.store(safe.to_bits(), Ordering::Relaxed);
    }

    pub fn mix(&self) -> f32 {
        f32::from_bits(self.mix.load(Ordering::Relaxed))
    }

    /// The software headphone level, 0 to 1.
    pub fn set_level(&self, level: f32) {
        let safe = if level.is_finite() { level.clamp(0.0, 1.0) } else { DEFAULT_LEVEL };
        self.level.store(safe.to_bits(), Ordering::Relaxed);
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }
}

/// The cue bus as the audio callback owns it: the mix and level, smoothed
/// per frame so a turned knob never clicks in the DJ's ears.
#[derive(Debug)]
pub struct CueMixer {
    mix: Smoothed,
    level: Smoothed,
}

impl CueMixer {
    /// Starting where the knobs are, so the first buffer is not a fade.
    pub fn new(settings: &CueSettings, rate: u32) -> Self {
        Self { mix: Smoothed::new(settings.mix(), rate), level: Smoothed::new(settings.level(), rate) }
    }

    /// Turns the PFL sum into the headphone feed, in place.
    ///
    /// `cue` arrives holding the summed pre-fader decks and leaves as
    /// `(cue × (1 − mix) + master × mix) × level`, clamped to full scale.
    /// `master` is the finished master — after level, limiter and clamp — so
    /// mix at 1 is exactly what the room hears.
    pub fn finish(&mut self, cue: &mut [f32], master: &[f32], settings: &CueSettings) {
        let (mix_target, level_target) = (settings.mix(), settings.level());
        for (frame, main) in cue.chunks_exact_mut(2).zip(master.chunks_exact(2)) {
            let mix = self.mix.step(mix_target);
            let level = self.level.step(level_target);
            for (sample, main) in frame.iter_mut().zip(main) {
                *sample = ((*sample * (1.0 - mix) + main * mix) * level).clamp(-1.0, 1.0);
            }
        }
    }
}

/// How many output lanes to open for a device whose default is `config`.
///
/// Called by `sink::configure`. A default under four lanes is widened to the
/// fewest the device offers at four or more, at the same format and rate, so
/// lanes 3–4 exist for the cue bus; a device that offers no such range keeps
/// its default and plays master only.
pub(crate) fn widen_for_cue(device: &cpal::Device, format: cpal::SampleFormat, config: &mut cpal::StreamConfig) {
    if config.channels < 4 {
        let rate = config.sample_rate;
        let wider = device.supported_output_configs().ok().and_then(|ranges| {
            ranges
                .filter(|range| {
                    range.sample_format() == format
                        && range.channels() >= 4
                        && range.min_sample_rate() <= rate
                        && rate <= range.max_sample_rate()
                })
                .map(|range| range.channels())
                .min()
        });
        if let Some(lanes) = wider {
            config.channels = lanes;
        }
    }
    tracing::debug!(lanes = config.channels, "cue bus: {} lanes", config.channels);
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "pure cue and pure master are exact, and that is the assertion")]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    /// A frame count long enough for the smoothing to have arrived.
    const SETTLED: usize = RATE as usize / 5;

    fn run(settings: &CueSettings, cue_value: f32, master_value: f32, frames: usize) -> Vec<f32> {
        let mut mixer = CueMixer::new(settings, RATE);
        let mut cue = vec![cue_value; frames * 2];
        let master = vec![master_value; frames * 2];
        mixer.finish(&mut cue, &master, settings);
        cue
    }

    #[test]
    fn mix_at_zero_is_the_cue_alone() {
        let settings = CueSettings::default();
        let out = run(&settings, 0.25, 0.75, 64);
        assert!(out.iter().all(|s| *s == 0.25), "the master leaked into the cue: {out:?}");
    }

    #[test]
    fn mix_at_one_is_the_master_alone() {
        let settings = CueSettings::default();
        settings.set_mix(1.0);
        let out = run(&settings, 0.25, 0.75, 64);
        assert!(out.iter().all(|s| *s == 0.75), "the cue leaked into the master: {out:?}");
    }

    #[test]
    fn the_centre_of_the_mix_is_half_of_each() {
        let settings = CueSettings::default();
        settings.set_mix(0.5);
        let out = run(&settings, 0.2, 0.6, 64);
        assert!(out.iter().all(|s| (s - 0.4).abs() < 1e-6), "{out:?}");
    }

    #[test]
    fn the_level_scales_the_whole_feed() {
        let settings = CueSettings::default();
        settings.set_level(0.5);
        let out = run(&settings, 0.8, 0.0, 64);
        assert!(out.iter().all(|s| (s - 0.4).abs() < 1e-6), "{out:?}");
    }

    #[test]
    fn a_hot_sum_is_clamped_to_full_scale() {
        // Two PFL'd decks at full sum past 1.0, and the cue has no limiter.
        let settings = CueSettings::default();
        let out = run(&settings, 1.6, 0.0, 16);
        assert!(out.iter().all(|s| *s == 1.0), "{out:?}");
    }

    #[test]
    fn a_turned_mix_knob_moves_gradually_rather_than_clicking() {
        // Cue silent, master steady: slamming the knob from cue to master
        // must bring the master in over the smoothing, never in one step.
        let settings = CueSettings::default();
        let mut mixer = CueMixer::new(&settings, RATE);
        settings.set_mix(1.0);
        let mut cue = vec![0.0_f32; SETTLED * 2];
        let master = vec![1.0_f32; SETTLED * 2];
        mixer.finish(&mut cue, &master, &settings);
        let first = cue.first().copied().unwrap_or(1.0);
        assert!(first < 0.01, "the first frame jumped to {first}");
        let mut worst = 0.0_f32;
        let mut last = 0.0_f32;
        for frame in cue.chunks_exact(2) {
            worst = worst.max((frame[0] - last).abs());
            last = frame[0];
        }
        assert!(worst < 0.01, "the largest step was {worst}");
        assert!(last > 0.99, "the mix never arrived: {last}");
    }

    #[test]
    fn a_knob_past_its_end_is_taken_at_its_end() {
        let settings = CueSettings::default();
        settings.set_mix(3.0);
        assert_eq!(settings.mix(), 1.0);
        settings.set_mix(f32::NAN);
        assert_eq!(settings.mix(), DEFAULT_MIX);
        settings.set_level(-1.0);
        assert_eq!(settings.level(), 0.0);
        settings.set_pfl(5, true);
        assert!(!settings.pfl(0) && !settings.pfl(1));
    }
}
