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

use crate::limiter::{Limiter, LimiterSettings};
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
/// per frame so a turned knob never clicks in the DJ's ears, and a limiter of
/// its own for the PFL sum.
pub struct CueMixer {
    mix: Smoothed,
    level: Smoothed,
    /// Two PFL'd decks at full level sum past 1.0, as two decks into the
    /// master do; without this the headphones get the flat-topped kicks the
    /// master limiter exists to prevent. Driven by the master's
    /// `LimiterSettings`, so one switch and one ceiling cover both buses.
    limiter: Limiter,
}

impl CueMixer {
    /// Starting where the knobs are, so the first buffer is not a fade.
    ///
    /// Allocates (the limiter's delay line), so it is built where the master
    /// limiter is: at the first callback, the earliest point the device rate
    /// is known, which the allocation test treats as warm-up.
    pub fn new(settings: &CueSettings, rate: u32) -> Self {
        Self {
            mix: Smoothed::new(settings.mix(), rate),
            level: Smoothed::new(settings.level(), rate),
            limiter: Limiter::new(rate),
        }
    }

    /// Turns the PFL sum into the headphone feed, in place.
    ///
    /// `cue` arrives holding the summed pre-fader decks and leaves as
    /// `(limited cue × (1 − mix) + master × mix) × level`, clamped to full
    /// scale. `master` is the finished master — after level, limiter and
    /// clamp — so mix at 1 is exactly what the room hears.
    ///
    /// The PFL sum is limited *before* the master joins it, not after: the
    /// master has already been through one lookahead, and limiting the blend
    /// would delay it a second time. A deck both cued and live would then
    /// reach the headphones twice, 1.5 ms apart — a comb filter at any mix
    /// between the ends. Limited first, the two paths carry the same delay.
    pub fn finish(&mut self, cue: &mut [f32], master: &[f32], settings: &CueSettings, limits: &LimiterSettings) {
        self.limiter.process(cue, limits);
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

/// One output range a device offers, as `lanes_for` weighs it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Offer {
    pub format: cpal::SampleFormat,
    pub channels: u16,
    pub min_rate: u32,
    pub max_rate: u32,
}

/// How many output lanes to open, given the device's default and its offers.
///
/// Only when the cue bus is wished for (`StreamWish::cue`): a four-out
/// interface's lanes 3–4 are not necessarily headphones, so widening is the
/// perform host's call, never the default. Wished, a default under four lanes
/// becomes the fewest lanes ≥ 4 the device offers at the same format and
/// rate; a device offering none keeps its default and plays master only.
pub(crate) fn lanes_for(
    wished: bool,
    default: u16,
    format: cpal::SampleFormat,
    rate: u32,
    offers: impl IntoIterator<Item = Offer>,
) -> u16 {
    if !wished || default >= 4 {
        return default;
    }
    offers
        .into_iter()
        .filter(|offer| {
            offer.format == format && offer.channels >= 4 && offer.min_rate <= rate && rate <= offer.max_rate
        })
        .map(|offer| offer.channels)
        .min()
        .unwrap_or(default)
}

/// Whether the render is handed a cue pair: wished for, and lanes 3–4 exist.
/// Unwished, even a native four-lane device gets `None`, as before the bus.
pub(crate) const fn renders_cue(wished: bool, lanes: usize) -> bool {
    wished && lanes >= 4
}

/// `lanes_for` against a real device. Called by `sink::configure`.
pub(crate) fn widen_for_cue(
    device: &cpal::Device,
    format: cpal::SampleFormat,
    config: &mut cpal::StreamConfig,
    wished: bool,
) {
    let default = config.channels;
    if wished && default < 4 {
        match device.supported_output_configs() {
            Ok(ranges) => {
                let offers = ranges.map(|range| Offer {
                    format: range.sample_format(),
                    channels: range.channels(),
                    min_rate: range.min_sample_rate(),
                    max_rate: range.max_sample_rate(),
                });
                config.channels = lanes_for(wished, default, format, config.sample_rate, offers);
            }
            Err(e) => tracing::warn!(error = %e, "cue bus: the device would not list its output configs"),
        }
    }
    let cue = renders_cue(wished, usize::from(config.channels));
    tracing::debug!(lanes = config.channels, wished, cue, "cue bus: {} lanes", config.channels);
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "pure cue and pure master are exact, and that is the assertion")]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    /// A frame count long enough for the smoothing to have arrived.
    const SETTLED: usize = RATE as usize / 5;

    /// The cue's limiter delays the PFL sum by its lookahead whether it is on
    /// or not, so the first frames out are the delay line's silence.
    fn latency() -> usize {
        Limiter::new(RATE).latency_frames()
    }

    /// `frames` of steady cue and master through a fresh mixer, with the
    /// limiter's start-up delay cut off the front.
    fn run(settings: &CueSettings, cue_value: f32, master_value: f32, frames: usize) -> Vec<f32> {
        let skip = latency();
        let mut mixer = CueMixer::new(settings, RATE);
        let mut cue = vec![cue_value; (frames + skip) * 2];
        let master = vec![master_value; (frames + skip) * 2];
        mixer.finish(&mut cue, &master, settings, &LimiterSettings::default());
        cue.split_off(skip * 2)
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
    fn with_the_limiter_off_a_hot_sum_is_still_clamped_to_full_scale() {
        let settings = CueSettings::default();
        let out = run(&settings, 1.6, 0.0, 16);
        assert!(out.iter().all(|s| *s == 1.0), "{out:?}");
    }

    /// Two full-scale decks PFL'd at once: a 100 Hz sine at twice full scale.
    fn two_hot_decks(frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let value = 2.0 * (i as f32 / RATE as f32 * 100.0 * std::f32::consts::TAU).sin();
                [value, value]
            })
            .collect()
    }

    /// The longest run of consecutive frames pinned at full scale: what a
    /// clamp's flat top looks like, and what a limiter must not produce.
    fn longest_flat_top(out: &[f32]) -> usize {
        let (mut longest, mut run) = (0, 0);
        for frame in out.chunks_exact(2) {
            run = if frame[0].abs() >= 0.999 { run + 1 } else { 0 };
            longest = longest.max(run);
        }
        longest
    }

    #[test]
    fn two_full_scale_pfl_decks_are_limited_rather_than_flat_topped() {
        let settings = CueSettings::default();
        let silent_master = vec![0.0_f32; SETTLED * 2];
        let mut clamped = two_hot_decks(SETTLED);
        CueMixer::new(&settings, RATE).finish(&mut clamped, &silent_master, &settings, &LimiterSettings::default());

        let limits = LimiterSettings::default();
        limits.set_enabled(true);
        let mut limited = two_hot_decks(SETTLED);
        CueMixer::new(&settings, RATE).finish(&mut limited, &silent_master, &settings, &limits);

        // The clamp holds every peak at 1.0 for a third of each cycle.
        assert!(longest_flat_top(&clamped) > 50, "the clamp-only reference should be flat-topped");
        let peak = limited.iter().fold(0.0_f32, |worst, s| worst.max(s.abs()));
        assert!(peak <= 1.0, "the limited cue reached {peak}");
        // A sine's own crest stays within 0.1% of its peak for a few frames
        // (six at 100 Hz); a limited one may sit there that long and no more.
        let crest: Vec<f32> = two_hot_decks(SETTLED).iter().map(|s| s / 2.0).collect();
        let natural = longest_flat_top(&crest);
        let flat = longest_flat_top(&limited);
        assert!(flat <= natural + 1, "the limited cue sat at full scale for {flat} frames, a sine's crest is {natural}");
        // And it is still loud: limited, not muted.
        assert!(peak > 0.5, "the limiter crushed the cue to {peak}");
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
        mixer.finish(&mut cue, &master, &settings, &LimiterSettings::default());
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

    const F32: cpal::SampleFormat = cpal::SampleFormat::F32;

    /// A controller sound card as Core Audio describes one: stereo by
    /// default, four lanes on offer at the same format and rates.
    fn four_out_card() -> [Offer; 2] {
        [
            Offer { format: F32, channels: 2, min_rate: 44_100, max_rate: 48_000 },
            Offer { format: F32, channels: 4, min_rate: 44_100, max_rate: 48_000 },
        ]
    }

    #[test]
    fn an_unwished_cue_bus_keeps_a_stereo_default_stereo() {
        // The upstream behaviour, unchanged: no controller, no widening, and
        // a render that is never handed a cue pair.
        assert_eq!(lanes_for(false, 2, F32, 44_100, four_out_card()), 2);
        assert!(!renders_cue(false, 2));
        // Not even on a device whose default is already four lanes: its 3–4
        // are not known to be headphones.
        assert!(!renders_cue(false, 4));
    }

    #[test]
    fn a_wished_cue_bus_opens_lanes_three_and_four_when_offered() {
        assert_eq!(lanes_for(true, 2, F32, 44_100, four_out_card()), 4);
        assert!(renders_cue(true, 4));
    }

    #[test]
    fn a_wished_cue_bus_on_a_stereo_only_device_stays_master_only() {
        let stereo = [Offer { format: F32, channels: 2, min_rate: 44_100, max_rate: 48_000 }];
        assert_eq!(lanes_for(true, 2, F32, 44_100, stereo), 2);
        assert!(!renders_cue(true, 2));
        // Nor is a four-lane range at another rate or format taken.
        assert_eq!(lanes_for(true, 2, F32, 96_000, four_out_card()), 2);
        assert_eq!(lanes_for(true, 2, cpal::SampleFormat::I16, 44_100, four_out_card()), 2);
    }
}
