use super::*;
use crate::analysis::expected_notes;
use crate::musicxml::{parse_musicxml, ParseOptions};
use crate::synth::SAMPLE_RATE;

fn note(lane: Lane, t: f64) -> Expected {
    Expected { note_idx: 0, t, lane, accent: false, ghost: false, grace: false, hand: None }
}

#[derive(Debug, PartialEq)]
enum Ev {
    AudioClick(ClickLevel, f32),
    MidiClick(ClickLevel),
    AudioDrum(Lane, f32),
    MidiDrum(u8, u8),
}

#[derive(Default)]
struct Fake {
    events: Vec<Ev>,
}

impl Sink for Fake {
    fn audio_click(&mut self, level: ClickLevel, volume: f32) {
        self.events.push(Ev::AudioClick(level, volume));
    }
    fn midi_click(&mut self, level: ClickLevel) {
        self.events.push(Ev::MidiClick(level));
    }
    fn audio_drum(&mut self, lane: Lane, gain: f32) {
        self.events.push(Ev::AudioDrum(lane, gain));
    }
    fn midi_drum(&mut self, note: u8, velocity: u8) {
        self.events.push(Ev::MidiDrum(note, velocity));
    }
}

fn opts() -> PlayOptions {
    PlayOptions {
        audio: true,
        audio_device: String::new(),
        sounds_dir: None,
        click_audio: true,
        midi_port: None,
        midi_drums: true,
        drum_channel: 10,
        click_midi: true,
        click_note: 76,
        click_channel: 10,
        drum_notes: [(Lane::Kick, 36), (Lane::Snare, 38)].into_iter().collect(),
        ramp: Ramp::default(),
    }
}

fn click(t: f64) -> Cue {
    Cue { t, sound: Sound::Click { level: ClickLevel::Beat } }
}

fn drum(lane: Lane, t: f64) -> Cue {
    Cue { t, sound: Sound::Drum { lane, velocity: 90 } }
}

fn play(cue: &Cue, opts: &PlayOptions, control: &Control) -> Vec<Ev> {
    let mut sink = Fake::default();
    dispatch(cue, opts, control, &mut sink);
    sink.events
}

#[test]
fn cues_are_ordered_and_clicks_lead_at_ties() {
    let mut ghost = note(Lane::Snare, 1.0);
    ghost.ghost = true;
    let mut accent = note(Lane::Crash1, 0.0);
    accent.accent = true;
    let c = cues(&[ghost, accent, note(Lane::Kick, 0.0)], &[(-1.0, true), (0.0, true), (1.0, false)]);
    let times: Vec<f64> = c.iter().map(|c| c.t).collect();
    assert_eq!(times, vec![-1.0, 0.0, 0.0, 0.0, 1.0, 1.0]);
    assert_eq!(c[1].sound, Sound::Click { level: ClickLevel::Accent }); // the click, then the two notes
    assert_eq!(c[2].sound, Sound::Drum { lane: Lane::Crash1, velocity: 120 });
    assert_eq!(c[3].sound, Sound::Drum { lane: Lane::Kick, velocity: 90 });
    assert_eq!(c[4].sound, Sound::Click { level: ClickLevel::Beat });
    assert_eq!(c[5].sound, Sound::Drum { lane: Lane::Snare, velocity: 45 });
}

#[test]
fn metronome_bar_has_an_accent_beats_and_soft_subdivisions() {
    let (bar, len) = metronome_cues(120.0, 3, Subdivision::Eighths);
    assert!((len - 1.5).abs() < 1e-9, "three beats at 120 BPM");
    let levels: Vec<ClickLevel> = bar
        .iter()
        .map(|c| match c.sound {
            Sound::Click { level } => level,
            _ => panic!("only clicks"),
        })
        .collect();
    use ClickLevel::*;
    assert_eq!(levels, [Accent, Sub, Beat, Sub, Beat, Sub]);
    let times: Vec<f64> = bar.iter().map(|c| c.t).collect();
    for (got, want) in times.iter().zip([0.0, 0.25, 0.5, 0.75, 1.0, 1.25]) {
        assert!((got - want).abs() < 1e-9, "{got} vs {want}");
    }
    assert_eq!(metronome_cues(60.0, 4, Subdivision::None).0.len(), 4);
    assert_eq!(metronome_cues(60.0, 4, Subdivision::Triplets).0.len(), 12);
    assert_eq!(metronome_cues(60.0, 2, Subdivision::Sixteenths).0.len(), 8);
    assert_eq!(metronome_cues(60.0, 0, Subdivision::None).0.len(), 1, "at least one beat");
    for s in Subdivision::ALL {
        assert_eq!(Subdivision::from_index(s.index()), s);
    }
    assert_eq!(Subdivision::from_index(99), Subdivision::None);
}

#[test]
fn click_switch_mutes_the_music_but_not_the_count_in() {
    let (o, c) = (opts(), Control::new());
    c.set_click_volume(0.4);
    assert_eq!(play(&click(0.0), &o, &c), [Ev::MidiClick(ClickLevel::Beat), Ev::AudioClick(ClickLevel::Beat, 0.4)]);
    c.set_click(false);
    assert!(play(&click(0.0), &o, &c).is_empty(), "silenced during the music");
    assert!(play(&click(2.0), &o, &c).is_empty());
    assert_eq!(play(&click(-0.5), &o, &c).len(), 2, "the count-in still sounds");
    c.set_click(true);
    assert_eq!(play(&click(2.0), &o, &c).len(), 2, "and it comes back");
}

#[test]
fn click_goes_only_where_it_is_routed() {
    let (mut o, c) = (opts(), Control::new());
    o.click_midi = false;
    assert_eq!(play(&click(0.0), &o, &c), [Ev::AudioClick(ClickLevel::Beat, 0.5)]);
    o.click_audio = false;
    assert!(play(&click(0.0), &o, &c).is_empty());
}

#[test]
fn notes_switch_gives_a_click_track() {
    let (o, c) = (opts(), Control::new());
    c.set_notes(false);
    assert!(play(&drum(Lane::Kick, 0.0), &o, &c).is_empty());
    assert_eq!(play(&click(0.0), &o, &c).len(), 2, "the click keeps going");
    c.set_notes(true);
    assert_eq!(play(&drum(Lane::Kick, 0.0), &o, &c).len(), 2);
}

#[test]
fn instrument_gain_scales_the_sound_and_mutes() {
    let (o, c) = (opts(), Control::new());
    c.set_volume(0.5);
    c.set_lane_gain(Lane::Kick, 0.5);
    let full = velocity_gain(90);
    let ev = play(&drum(Lane::Kick, 0.0), &o, &c);
    assert_eq!(ev[0], Ev::MidiDrum(36, 45), "MIDI velocity is scaled by the gain");
    match ev[1] {
        Ev::AudioDrum(Lane::Kick, g) => assert!((g - 0.5 * 0.5 * full).abs() < 1e-6, "{g}"),
        ref other => panic!("{other:?}"),
    }
    // Another instrument is unaffected, and a muted one is silent on both outputs.
    assert_eq!(play(&drum(Lane::Snare, 0.0), &o, &c)[0], Ev::MidiDrum(38, 90));
    c.set_lane_gain(Lane::Snare, 0.0);
    assert!(play(&drum(Lane::Snare, 0.0), &o, &c).is_empty());
    // A boost cannot push the MIDI velocity past 127.
    c.set_lane_gain(Lane::Kick, 3.0);
    assert_eq!(play(&drum(Lane::Kick, 0.0), &o, &c)[0], Ev::MidiDrum(36, 127));
}

#[test]
fn drums_follow_the_output_switches() {
    let (mut o, c) = (opts(), Control::new());
    o.midi_drums = false;
    assert!(matches!(play(&drum(Lane::Kick, 0.0), &o, &c)[..], [Ev::AudioDrum(Lane::Kick, _)]));
    o.audio = false;
    assert!(play(&drum(Lane::Kick, 0.0), &o, &c).is_empty());
    o.midi_drums = true;
    assert_eq!(play(&drum(Lane::Kick, 0.0), &o, &c), [Ev::MidiDrum(36, 90)]);
    // An instrument without a MIDI note is only heard on the sound card.
    o.audio = true;
    assert!(matches!(play(&drum(Lane::Ride, 0.0), &o, &c)[..], [Ev::AudioDrum(Lane::Ride, _)]));
}

#[test]
fn only_the_first_pass_has_the_count_in() {
    let cues = [click(-0.04), click(0.0), drum(Lane::Kick, 0.01)];
    assert_eq!(pass_cues(&cues, 0).count(), 3);
    assert_eq!(pass_cues(&cues, 1).count(), 2);
    assert_eq!(pass_cues(&cues, 7).count(), 2);
}

/// Runs the whole player loop on this thread, a little in the future, with a fake output.
fn run_fake(cues: &[Cue], pass_len: f64, control: &Control, stop: &AtomicBool) -> Vec<Ev> {
    let mut sink = Fake::default();
    let zero = Instant::now() + Duration::from_millis(60);
    run_with(zero, cues, pass_len, &opts(), control, stop, &mut sink, Duration::ZERO);
    sink.events
}

#[test]
fn one_pass_plays_everything_once_and_reports_done() {
    let c = Control::new();
    let cues = [click(-0.02), drum(Lane::Kick, 0.0), click(0.02)];
    let ev = run_fake(&cues, 0.05, &c, &AtomicBool::new(false));
    assert_eq!(ev.len(), 2 + 2 + 2, "count-in click, kick on two outputs, click");
    assert!(c.music_done());
}

#[test]
fn looping_repeats_until_switched_off_and_finishes_the_pass() {
    let c = Control::new();
    c.set_looping(true);
    let cues = [click(-0.02), drum(Lane::Kick, 0.0), click(0.02)];
    let off = {
        let c = c.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(260));
            c.set_looping(false);
        })
    };
    let ev = run_fake(&cues, 0.05, &c, &AtomicBool::new(false));
    off.join().unwrap();
    let count_ins = ev.iter().filter(|e| matches!(e, Ev::MidiClick(_))).count();
    let kicks = ev.iter().filter(|e| matches!(e, Ev::MidiDrum(36, _))).count();
    assert!(kicks >= 3, "several passes: {kicks}");
    // Count-in click + one click per pass, all on MIDI: one more click than kicks.
    assert_eq!(count_ins, kicks + 1, "the count-in is played once, at the start");
    assert!(c.music_done());
}

#[test]
fn stopping_ends_the_loop_at_once() {
    let c = Control::new();
    c.set_looping(true);
    let stop = Arc::new(AtomicBool::new(false));
    let setter = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            stop.store(true, Ordering::Relaxed);
        })
    };
    let started = Instant::now();
    let ev = run_fake(&[drum(Lane::Kick, 0.0)], 0.05, &c, &stop);
    setter.join().unwrap();
    assert!(started.elapsed() < Duration::from_millis(1000), "returned promptly after the stop");
    assert!(!ev.is_empty());
}

#[test]
fn cues_that_are_far_too_late_are_dropped() {
    let c = Control::new();
    let mut sink = Fake::default();
    // Zero is a second in the past, so the early cues are long overdue.
    let zero = Instant::now() - Duration::from_millis(1000);
    let cues = [drum(Lane::Kick, 0.0), drum(Lane::Kick, 0.5), drum(Lane::Snare, 1.05)];
    run_with(zero, &cues, 1.1, &opts(), &c, &AtomicBool::new(false), &mut sink, Duration::ZERO);
    assert!(sink.events.iter().all(|e| !matches!(e, Ev::MidiDrum(36, _))), "the overdue kicks are skipped");
    assert!(sink.events.iter().any(|e| matches!(e, Ev::MidiDrum(38, _))), "the snare, due in 50 ms, plays");
}

/// Mixes the drum cues into a buffer the way the player would, so the
/// sample, the timing and the velocity handling can be checked without a
/// sound card.
fn mix(cues: &[Cue], lead_sec: f64, volume: f32) -> Vec<f32> {
    let (kit, _) = Kit::load(
        cues.iter().filter_map(|c| match c.sound {
            Sound::Drum { lane, .. } => Some(lane),
            _ => None,
        }),
        None,
    );
    let mut out = Vec::new();
    for cue in cues {
        let Sound::Drum { lane, velocity } = cue.sound else { continue };
        let start = ((cue.t + lead_sec) * SAMPLE_RATE as f64).round() as usize;
        let sample = kit.sample(lane).unwrap();
        if out.len() < start + sample.len() {
            out.resize(start + sample.len(), 0.0);
        }
        for (i, s) in sample.iter().enumerate() {
            out[start + i] += s * volume * velocity_gain(velocity);
        }
    }
    out
}

#[test]
fn sample_score_renders_a_note_at_every_onset() {
    let score = parse_musicxml(include_str!("../../examples/basic_rock_beat.musicxml"), &ParseOptions::default()).unwrap();
    let expected = expected_notes(&score, 0, score.measures.len() - 1, 1.0);
    let clicks = score.click_track(0, score.measures.len() - 1, 1.0, 4, true);
    let all = cues(&expected, &clicks);
    let lead = 2.4; // one count-in bar at 100 BPM
    let out = mix(&all, lead, 1.0);

    // Nothing but clicks (which `mix` skips) before the first note.
    let first = (lead * SAMPLE_RATE as f64) as usize;
    assert!(out[..first].iter().all(|s| *s == 0.0));
    assert!(out.iter().all(|s| s.abs() <= 1.0), "the mix clips at full volume");

    // Every note starts audibly within 5 ms of its time.
    let window = (0.005 * SAMPLE_RATE as f64) as usize;
    for e in &expected {
        let at = ((e.t + lead) * SAMPLE_RATE as f64).round() as usize;
        let peak = out[at..at + window].iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.02, "silent at {:.3}s for {:?}", e.t, e.lane);
    }
    // And the take is not far longer than the music plus the last ring-out.
    let last = expected.iter().map(|e| e.t).fold(0.0, f64::max) + lead;
    assert!(out.len() as f64 / (SAMPLE_RATE as f64) < last + 2.5);
}

#[test]
fn accents_are_louder_than_ghost_notes() {
    let mut accent = note(Lane::Snare, 0.0);
    accent.accent = true;
    let mut ghost = note(Lane::Snare, 0.0);
    ghost.ghost = true;
    let loud = mix(&cues(&[accent], &[]), 0.0, 1.0);
    let soft = mix(&cues(&[ghost], &[]), 0.0, 1.0);
    let peak = |v: &[f32]| v.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(peak(&loud) > 2.0 * peak(&soft), "{} vs {}", peak(&loud), peak(&soft));
}

#[test]
fn a_ramp_makes_every_few_passes_faster_up_to_a_limit() {
    let r = Ramp { every: 2, step: 0.5 };
    assert_eq!((r.factor(0), r.factor(1), r.factor(2), r.factor(3), r.factor(4)), (1.0, 1.0, 1.5, 1.5, 2.0));
    assert_eq!(r.factor(1000), Ramp::MAX_FACTOR, "capped");
    assert_eq!(Ramp::default().factor(50), 1.0, "no ramp, no change");
    assert_eq!(Ramp { every: 2, step: 0.0 }.factor(50), 1.0);

    // Passes of 4 s: two at the start tempo, then 4/1.5 s each.
    let (pass, into) = r.locate(4.0, 8.0 + 1.0);
    assert_eq!(pass, 2);
    assert!((into - 1.5).abs() < 1e-9, "1 s real at 1.5 times is 1.5 s of music: {into}");
    assert!((r.base_seconds(4.0, 9.0) - 9.5).abs() < 1e-9);
    assert_eq!(r.base_seconds(4.0, -1.5), -1.5, "the count-in is not squeezed");
    // The limit: passes of 3 s at 3 times are 1 s long, however far on.
    let fast = Ramp { every: 1, step: 2.0 };
    let (pass, into) = fast.locate(3.0, 3.0 + 1.0 + 10.5);
    assert_eq!(pass, 2 + 10);
    assert!((into - 1.5).abs() < 1e-9, "{into}");
    // Without a ramp it is plain division.
    assert_eq!(Ramp::default().locate(4.0, 9.0), (2, 1.0));
    assert_eq!(Ramp::default().base_seconds(4.0, 9.0), 9.0);
}

struct Stamps(Vec<Instant>);

impl Sink for Stamps {
    fn audio_click(&mut self, _: ClickLevel, _: f32) {}
    fn midi_click(&mut self, _: ClickLevel) {}
    fn audio_drum(&mut self, _: Lane, _: f32) {}
    fn midi_drum(&mut self, _: u8, _: u8) {
        self.0.push(Instant::now());
    }
}

#[test]
fn the_player_squeezes_the_later_passes_of_a_ramp() {
    let c = Control::new();
    c.set_looping(true);
    let off = {
        let c = c.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(330));
            c.set_looping(false);
        })
    };
    let mut o = opts();
    // Passes of 100 ms, 50 ms, then 33 ms (the limit of three times).
    o.ramp = Ramp { every: 1, step: 1.0 };
    let mut sink = Stamps(Vec::new());
    let zero = Instant::now() + Duration::from_millis(40);
    run_with(zero, &[drum(Lane::Kick, 0.0)], 0.10, &o, &c, &AtomicBool::new(false), &mut sink, Duration::ZERO);
    off.join().unwrap();
    let gaps: Vec<f64> = sink.0.windows(2).map(|w| (w[1] - w[0]).as_secs_f64()).collect();
    assert!(gaps.len() >= 4, "{gaps:?}");
    assert!(gaps[1] < gaps[0] * 0.75, "the second pass is shorter than the first: {gaps:?}");
    assert!(gaps[2] < gaps[1] * 0.9, "and the third shorter again: {gaps:?}");
    assert!((gaps[3] - gaps[2]).abs() < 0.012, "at the limit the passes stop getting shorter: {gaps:?}");
}
