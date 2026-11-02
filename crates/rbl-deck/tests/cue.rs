//! The headphone (cue) bus end to end, on a sink with no device behind it.
//!
//! Fork addition (`docs/fork.md`). Drives the same render callback the device
//! does, handing it a cue pair the way `sink::build` does when the device has
//! lanes 3–4, and checks what a DJ hears in each ear against the room.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::cast_possible_truncation)]

use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use rbl_deck::{Deck, DeckEvent, Engine, Render, Sink, StreamWish};

const RATE: u32 = 44_100;

/// The steady level of the fixture, as `tests/engine.rs`'s `flat` is.
const FLAT: f32 = 0.8;

/// Below this nothing is audible: −80 dB, as in `tests/engine.rs`.
const INAUDIBLE: f32 = 1e-4;

/// Frames pulled before anything is measured: the deck's fade in, the
/// strip's filters settling, the limiter's lookahead and the cue smoothing
/// (10 ms) are all well inside this.
const SETTLE: usize = 8 * 512;

/// A 16-bit stereo WAV holding one steady value: any change in the output
/// came from the mixer, not from the music.
fn flat_wav(path: &Path, frames: usize) {
    let data = u32::try_from(frames * 2 * 2).unwrap();
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&2_u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 4).to_le_bytes());
    out.extend_from_slice(&4_u16.to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    let sample = ((FLAT * 32_767.0) as i16).to_le_bytes();
    for _ in 0..frames * 2 {
        out.extend_from_slice(&sample);
    }
    std::fs::write(path, out).unwrap();
}

/// A controller sound card: stereo by default, four lanes on offer. Opened
/// as `CpalSink` opens it — with `StreamWish::cue` it has lanes 3–4 and the
/// render gets a cue pair; without, it stays stereo and gets `None`.
struct Card {
    render: Mutex<Render>,
    wish: StreamWish,
}

impl Card {
    /// `frames` of master and, when the bus is open, of cue.
    fn pull(&self, frames: usize) -> (Vec<f32>, Option<Vec<f32>>) {
        let mut master = vec![0.0_f32; frames * 2];
        let mut cue = self.wish.cue.then(|| vec![0.0_f32; frames * 2]);
        let mut render = self.render.lock().unwrap();
        render(&mut master, cue.as_deref_mut());
        drop(render);
        (master, cue)
    }
}

impl Sink for Card {
    fn sample_rate(&self) -> u32 {
        RATE
    }
    fn start(&self) -> rbl_deck::Result<()> {
        Ok(())
    }
    fn stop(&self) -> rbl_deck::Result<()> {
        Ok(())
    }
}

/// An engine on the card, opened with `wish`, with deck A loaded and playing.
fn playing_with(wish: StreamWish) -> (Engine, Arc<Card>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flat.wav");
    flat_wav(&path, RATE as usize * 4);

    let events: rbl_deck::EventSink = Arc::new(|_: DeckEvent| {});
    let (sink_tx, sink_rx) = mpsc::channel();
    let engine = Engine::with_sink(
        move |render| {
            let sink = Arc::new(Card { render: Mutex::new(render), wish });
            sink_tx.send(Arc::clone(&sink)).unwrap();
            Ok(sink as Arc<dyn Sink>)
        },
        &events,
    )
    .expect("engine");
    let sink = sink_rx.recv().unwrap();
    // Opened and off: the master is the decks' sum delayed by the lookahead,
    // and the cue's limiter, sharing these settings, is the same delay.
    engine.master().set_gain(1.0);
    engine.limiter().set_enabled(false);

    engine.load(Deck::A, &path);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !engine.snapshot().a.loaded {
        assert!(Instant::now() < deadline, "the deck never loaded");
        std::thread::sleep(Duration::from_millis(2));
    }
    engine.play(Deck::A);
    (engine, sink, dir)
}

/// The perform host's open: the cue bus wished for.
fn playing() -> (Engine, Arc<Card>, tempfile::TempDir) {
    playing_with(StreamWish { cue: true, ..StreamWish::default() })
}

/// Pulls until the deck has audio flowing, then returns `frames` more of it.
fn settled(engine: &Engine, sink: &Card, frames: usize) -> (Vec<f32>, Option<Vec<f32>>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut pulled = 0;
    while pulled < SETTLE || engine.snapshot().a.position_frames < SETTLE as u64 {
        assert!(Instant::now() < deadline, "the deck never started");
        sink.pull(512);
        pulled += 512;
        std::thread::sleep(Duration::from_millis(1));
    }
    sink.pull(frames)
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |worst, s| worst.max(s.abs()))
}

#[test]
fn a_deck_faded_out_of_the_room_is_still_in_the_headphones() {
    let (engine, sink, _dir) = playing();
    // Crossfader hard to B: deck A is gone from the master.
    engine.mixer().set_crossfade(1.0);
    engine.cue().set_pfl(0, true);
    engine.cue().set_mix(0.0);
    let (master, cue) = settled(&engine, &sink, 1_024);
    let cue = cue.expect("a cue pair");
    assert!(peak(&master) < INAUDIBLE, "deck A reached the room at {}", peak(&master));
    // Pre-fader, post-EQ: a flat strip passes the file's own level.
    let quiet = cue.iter().fold(f32::MAX, |least, s| least.min(s.abs()));
    assert!((quiet - FLAT).abs() < 0.02, "the cue carried {quiet}, not the deck's {FLAT}");
}

#[test]
fn a_closed_channel_fader_silences_the_room_but_not_the_headphones() {
    // The channel fader (`ChannelSettings::set_level`, the controller's) sits
    // after the PFL tap, as on a DJM: a deck cued with its fader down is heard
    // in the headphones and nowhere else.
    let (engine, sink, _dir) = playing();
    engine.mixer().set_crossfade(0.0);
    engine.mixer().channels[0].set_level(0.0);
    engine.cue().set_pfl(0, true);
    engine.cue().set_mix(0.0);
    let (master, cue) = settled(&engine, &sink, 1_024);
    let cue = cue.expect("a cue pair");
    assert!(peak(&master) < INAUDIBLE, "a closed fader reached the room at {}", peak(&master));
    let quiet = cue.iter().fold(f32::MAX, |least, s| least.min(s.abs()));
    assert!((quiet - FLAT).abs() < 0.02, "the cue carried {quiet}, not the deck's {FLAT}");
}

#[test]
#[allow(clippy::float_cmp, reason = "the fader's ends and its fallback are exact constants")]
fn a_channel_fader_is_clamped_and_a_nan_is_the_fader_up() {
    let mixer = rbl_deck::MixerSettings::default();
    let channel = &mixer.channels[1];
    assert_eq!(channel.level(), 1.0, "a fresh channel is at full");
    channel.set_level(-0.5);
    assert_eq!(channel.level(), 0.0);
    channel.set_level(3.0);
    assert_eq!(channel.level(), 1.0);
    channel.set_level(0.25);
    channel.set_level(f32::NAN);
    assert_eq!(channel.level(), 1.0);
}

#[test]
fn toggling_pfl_ramps_the_cue_rather_than_stepping_it() {
    // A steady level through a flat strip, so any jump in the cue pair is
    // the PFL gain's and not the music's.
    let mixer = rbl_deck::MixerSettings::default();
    let mut channel = rbl_deck::Channel::new(RATE);
    let flat = |frames: usize| vec![FLAT; frames * 2];
    let mut cue = vec![0.0_f32; SETTLE * 2];
    channel.process(&mut flat(SETTLE), &mixer.channels[0], rbl_deck::Curve::Eq, 1.0, Some((&mut cue, false)));
    assert!(peak(&cue) < INAUDIBLE, "an un-PFL'd deck reached the cue at {}", peak(&cue));
    // On for six buffers, then off for six: each toggle starts its own ramp.
    let mut heard = Vec::new();
    for on in [[true; 6], [false; 6]].concat() {
        let mut chunk = vec![0.0_f32; 512 * 2];
        channel.process(&mut flat(512), &mixer.channels[0], rbl_deck::Curve::Eq, 1.0, Some((&mut chunk, on)));
        heard.extend(chunk.chunks_exact(2).map(|frame| frame[0]));
    }
    // A 10 ms time constant moves at most FLAT / 441 a frame at 44.1 kHz; a
    // step would be the whole FLAT in one.
    let worst = heard.windows(2).fold(0.0_f32, |worst, pair| worst.max((pair[1] - pair[0]).abs()));
    assert!(worst < FLAT / 200.0, "the cue jumped {worst} between two frames");
    let top = heard.iter().fold(0.0_f32, |top, s| top.max(*s));
    assert!((top - FLAT).abs() < 0.02, "PFL never reached the deck's level: {top}");
    assert!(heard.last().is_some_and(|s| s.abs() < 0.01), "PFL off never faded the cue out");
}

#[test]
fn mix_at_one_puts_the_master_in_the_headphones() {
    let (engine, sink, _dir) = playing();
    // Crossfader in the middle, so the master is not silence and the
    // comparison means something.
    engine.mixer().set_crossfade(0.5);
    engine.cue().set_pfl(0, true);
    engine.cue().set_mix(1.0);
    let (master, cue) = settled(&engine, &sink, 1_024);
    let cue = cue.expect("a cue pair");
    assert!(peak(&master) > 0.5, "the master is too quiet to compare: {}", peak(&master));
    let worst = master.iter().zip(&cue).fold(0.0_f32, |worst, (m, c)| worst.max((m - c).abs()));
    assert!(worst < 1e-6, "the cue differed from the master by {worst}");
}

#[test]
fn no_pfl_and_mix_at_cue_is_silence() {
    let (engine, sink, _dir) = playing();
    engine.cue().set_mix(0.0);
    let (master, cue) = settled(&engine, &sink, 1_024);
    let cue = cue.expect("a cue pair");
    assert!(peak(&master) > 0.5, "the deck should be playing in the room");
    assert!(peak(&cue) < INAUDIBLE, "nothing is cued, yet the headphones carried {}", peak(&cue));
}

#[test]
fn a_stereo_open_without_the_wish_renders_the_master_as_before() {
    // No `cue` in the wish: the card stays at its stereo default and the
    // render is handed `None`, exactly as upstream's was. PFL is ignored and
    // the master is what a cue-bus open makes of the same deck.
    let (engine, sink, _dir) = playing_with(StreamWish::default());
    engine.cue().set_pfl(0, true);
    let (master, cue) = settled(&engine, &sink, 1_024);
    assert!(cue.is_none(), "an unwished open was handed a cue pair");

    let (wished, wished_sink, _wished_dir) = playing();
    wished.cue().set_pfl(0, true);
    let (reference, _) = settled(&wished, &wished_sink, 1_024);
    // A steady fixture, so the two settled masters agree sample for sample.
    let level = |out: &[f32]| out.iter().fold(f32::MAX, |least, s| least.min(*s));
    assert!((level(&master) - FLAT).abs() < 0.02, "the master carried {}", level(&master));
    let worst = master.iter().zip(&reference).fold(0.0_f32, |worst, (a, b)| worst.max((a - b).abs()));
    assert!(worst < 1e-4, "the stereo master differed from the cue-bus one by {worst}");
}
