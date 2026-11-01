//! Standard MIDI File -> [`RawScore`]: the drum notes of a file laid out on bars.
//!
//! Reading ([`read`]) finds the drum notes, the tempo changes and the time signatures, all in
//! ticks. Building ([`build`]) lays bars over the ticks and places each onset in its bar.

use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};

use std::collections::HashMap;

use super::ir::{PartInfo, RawMeasure, RawNote, RawScore};
use super::quantise::{self, MidiOptions};
use super::{Format, ImportError, ImportOptions, Limits};
use crate::drums::{gm_lane, Lane};

/// Where a drum note starts, in ticks from the start of the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Onset {
    pub tick: u64,
    pub key: u8,
    pub velocity: u8,
}

/// What the file says, before bars are laid over it.
#[derive(Clone, Debug, Default)]
pub struct MidiData {
    pub ppq: u64,
    /// `(tick, microseconds per quarter note)`, sorted, one per tick.
    pub tempos: Vec<(u64, u32)>,
    /// `(tick, beats, beat type)`, sorted, one per tick.
    pub signatures: Vec<(u64, u32, u32)>,
    /// Sorted by tick.
    pub onsets: Vec<Onset>,
    pub warnings: Vec<String>,
}

fn parse_error(reason: impl Into<String>) -> ImportError {
    ImportError::Parse { format: Format::Midi, reason: reason.into() }
}

struct Note {
    track: usize,
    channel: u8,
    onset: Onset,
}

pub fn read(bytes: &[u8], limits: &Limits) -> Result<MidiData, ImportError> {
    let smf = Smf::parse(bytes).map_err(|e| parse_error(format!("MIDI error: {e}")))?;
    if smf.header.format == midly::Format::Sequential {
        return Err(parse_error("MIDI format 2 (independent sequences) is not supported; export the drums as one track"));
    }
    let ppq = match smf.header.timing {
        Timing::Metrical(t) if t.as_int() > 0 => u64::from(t.as_int()),
        Timing::Metrical(_) => return Err(parse_error("the file's time division is zero")),
        Timing::Timecode(..) => return Err(parse_error("a file timed in SMPTE frames, not in beats, is not supported")),
    };

    let mut data = MidiData { ppq, ..Default::default() };
    let mut notes: Vec<Note> = Vec::new();
    let mut track_names: Vec<String> = vec![String::new(); smf.tracks.len()];
    let mut events = 0usize;
    for (ti, track) in smf.tracks.iter().enumerate() {
        let mut tick = 0u64;
        for ev in track {
            events += 1;
            if events > limits.max_events {
                return Err(ImportError::TooLarge(format!("more than {} MIDI events", limits.max_events)));
            }
            tick += u64::from(ev.delta.as_int());
            match ev.kind {
                TrackEventKind::Meta(MetaMessage::Tempo(us)) if us.as_int() > 0 => data.tempos.push((tick, us.as_int())),
                TrackEventKind::Meta(MetaMessage::TimeSignature(num, den_pow, _, _)) if num > 0 && den_pow <= 6 => {
                    data.signatures.push((tick, u32::from(num), 1u32 << den_pow));
                }
                TrackEventKind::Meta(MetaMessage::TrackName(name)) => {
                    let name = String::from_utf8_lossy(name).trim().to_string();
                    if track_names[ti].is_empty() {
                        track_names[ti] = name;
                    }
                }
                TrackEventKind::Midi { channel, message: MidiMessage::NoteOn { key, vel } } if vel.as_int() > 0 => {
                    if notes.len() >= limits.max_notes {
                        return Err(ImportError::TooLarge(format!("more than {} notes", limits.max_notes)));
                    }
                    notes.push(Note {
                        track: ti,
                        channel: channel.as_int(),
                        onset: Onset { tick, key: key.as_int(), velocity: vel.as_int() },
                    });
                }
                _ => {}
            }
        }
    }
    data.tempos.sort_by_key(|(t, _)| *t);
    data.tempos.reverse();
    data.tempos.dedup_by_key(|(t, _)| *t); // the later event of a tick wins
    data.tempos.reverse();
    data.signatures.sort_by_key(|(t, _, _)| *t);
    data.signatures.reverse();
    data.signatures.dedup_by_key(|(t, _, _)| *t);
    data.signatures.reverse();

    let (chosen, how) = choose_drums(&notes, &track_names).ok_or(ImportError::NoDrumPart)?;
    data.onsets = chosen;
    data.onsets.sort_by_key(|o| (o.tick, o.key));
    if let Some(how) = how {
        data.warnings.push(how);
    }
    Ok(data)
}

/// The drum notes of the file. Channel 10 of any track; else the tracks named like a drum part;
/// else the one channel that looks like drums (few keys, nearly all in the General MIDI drum range).
fn choose_drums(notes: &[Note], track_names: &[String]) -> Option<(Vec<Onset>, Option<String>)> {
    let pick = |f: &dyn Fn(&Note) -> bool| -> Vec<Onset> { notes.iter().filter(|n| f(n)).map(|n| n.onset).collect() };

    let channel_ten = pick(&|n| n.channel == 9);
    if !channel_ten.is_empty() {
        return Some((channel_ten, None));
    }
    let drum_named = |name: &str| {
        let n = name.to_ascii_lowercase();
        ["drum", "perc", "kit", "bateri"].iter().any(|w| n.contains(w))
    };
    let named = pick(&|n| drum_named(&track_names[n.track]));
    if !named.is_empty() {
        return Some((named, Some("No drum channel (10) in the file; using the track named like a drum part".into())));
    }
    let mut best: Option<(usize, u8)> = None;
    for channel in 0..16u8 {
        let on: Vec<&Note> = notes.iter().filter(|n| n.channel == channel).collect();
        let in_range = on.iter().filter(|n| (35..=81).contains(&n.onset.key)).count();
        let mut keys: Vec<u8> = on.iter().map(|n| n.onset.key).collect();
        keys.sort_unstable();
        keys.dedup();
        if !on.is_empty() && in_range * 10 >= on.len() * 9 && keys.len() <= 12 && best.is_none_or(|(c, _)| in_range > c) {
            best = Some((in_range, channel));
        }
    }
    let (_, channel) = best?;
    Some((pick(&|n| n.channel == channel), Some(format!("No drum channel (10) in the file; using channel {} because it looks like drums", channel + 1))))
}

/// One bar laid over the ticks.
#[derive(Clone, Copy, Debug)]
struct Bar {
    start: u64,
    len: u64,
    beats: u32,
    beat_type: u32,
}

fn lay_bars(data: &MidiData, last_tick: u64, limits: &Limits, first_bar_beats: u32, warnings: &mut Vec<String>) -> Result<Vec<Bar>, ImportError> {
    let mut signatures = data.signatures.clone();
    if signatures.first().is_none_or(|s| s.0 > 0) {
        signatures.insert(0, (0, 4, 4));
        warnings.push("No time signature at the start of the file; using 4/4".into());
    }
    let mut bars = Vec::new();
    let mut start = 0u64;
    if first_bar_beats > 0 {
        // The music starts that many beats before its first bar line: a short first bar, like a pickup.
        if signatures.len() > 1 {
            warnings.push("The first bar cannot start later: the time signature changes later in the file".into());
        } else {
            let (_, beats, beat_type) = signatures[0];
            let beat = data.ppq * 4 / u64::from(beat_type);
            let len = u64::from(first_bar_beats.min(beats - 1)) * beat;
            if len > 0 {
                bars.push(Bar { start: 0, len, beats, beat_type });
                start = len;
            }
        }
    }
    let mut moved = false;
    let mut inexact = false;
    loop {
        let (_, beats, beat_type) = *signatures.iter().rev().find(|s| s.0 <= start).unwrap_or(&signatures[0]);
        let exact = data.ppq * 4 * u64::from(beats);
        if exact % u64::from(beat_type) != 0 {
            inexact = true;
        }
        let len = (exact / u64::from(beat_type)).max(1);
        moved |= signatures.iter().any(|s| s.0 > start && s.0 < start + len);
        bars.push(Bar { start, len, beats, beat_type });
        // One bar past the last onset, for a note that rounds up to the next bar line.
        let past_the_end = start > last_tick;
        start += len;
        if past_the_end {
            break;
        }
        if bars.len() >= limits.max_measures {
            return Err(ImportError::TooLarge(format!("more than {} measures", limits.max_measures)));
        }
    }
    if moved {
        warnings.push("A time signature changes inside a bar; it takes effect at the next bar line".into());
    }
    if inexact {
        warnings.push("A bar is not a whole number of ticks long; its length was rounded down".into());
    }
    Ok(bars)
}

/// General MIDI, plus three keys Roland modules (the TD series, the V31) send for the edge of the hi-hat
/// and for a tom rim, which recordings from them are full of.
fn lane_of(key: u8) -> Lane {
    match key {
        22 => Lane::HiHatClosed,
        26 => Lane::HiHatOpen,
        58 => Lane::Tom3,
        _ => gm_lane(key),
    }
}

pub fn parse(bytes: &[u8], opts: &ImportOptions) -> Result<RawScore, ImportError> {
    build(read(bytes, &opts.limits)?, opts)
}

/// Accent and ghost flags from velocity, relative to the median of each instrument: a file played
/// quietly has no accents under a fixed threshold. An instrument with too few notes, or whose notes
/// are all one velocity (a programmed loop), has none.
fn dynamics(onsets: &[Onset], lanes: &[Lane], opts: &MidiOptions) -> Vec<(bool, bool)> {
    let mut by_lane: HashMap<Lane, Vec<u8>> = HashMap::new();
    for (o, lane) in onsets.iter().zip(lanes) {
        by_lane.entry(*lane).or_default().push(o.velocity);
    }
    let medians: HashMap<Lane, Option<f64>> = by_lane
        .into_iter()
        .map(|(lane, mut v)| {
            v.sort_unstable();
            let spread = v[v.len() - 1] - v[0];
            let median = (f64::from(v[(v.len() - 1) / 2]) + f64::from(v[v.len() / 2])) / 2.0;
            (lane, (v.len() >= 4 && spread >= 8).then_some(median))
        })
        .collect();
    onsets
        .iter()
        .zip(lanes)
        .map(|(o, lane)| match medians[lane] {
            Some(m) => (f64::from(o.velocity) >= m * opts.accent_ratio, f64::from(o.velocity) <= m * opts.ghost_ratio),
            None => (false, false),
        })
        .collect()
}

pub fn build(data: MidiData, opts: &ImportOptions) -> Result<RawScore, ImportError> {
    let mut warnings = data.warnings.clone();
    let last_tick = data.onsets.last().map_or(0, |o| o.tick);
    let bars = lay_bars(&data, last_tick, &opts.limits, opts.midi.first_bar_beats, &mut warnings)?;
    let ppq = data.ppq as f64;

    let lanes: Vec<Lane> = data.onsets.iter().map(|o| lane_of(o.key)).collect();
    let quantiser_bars: Vec<quantise::Bar> = bars.iter().map(|b| quantise::Bar { start: b.start, len: b.len, beat: data.ppq * 4 / u64::from(b.beat_type) }).collect();
    let ticks: Vec<(u64, Lane)> = data.onsets.iter().zip(&lanes).map(|(o, l)| (o.tick, *l)).collect();
    let (placed, report) = quantise::quantise(data.ppq, &quantiser_bars, &data.tempos, &ticks, &opts.midi);
    let flags = dynamics(&data.onsets, &lanes, &opts.midi);

    let mut measures: Vec<RawMeasure> = bars
        .iter()
        .enumerate()
        .map(|(i, b)| RawMeasure {
            number: (i + 1).to_string(),
            len_q: b.len as f64 / ppq,
            beats: b.beats,
            beat_type: b.beat_type,
            ..Default::default()
        })
        .collect();
    for &(tick, us) in &data.tempos {
        let bar = bars.partition_point(|b| b.start <= tick).saturating_sub(1);
        if let Some(m) = measures.get_mut(bar) {
            m.tempos.push(((tick - bars[bar].start) as f64 / ppq, 60_000_000.0 / f64::from(us)));
        }
    }

    let mut unknown_keys: Vec<u8> = Vec::new();
    let mut last_bar = 0;
    for p in &placed {
        let lane = lanes[p.index];
        if lane == Lane::Other {
            unknown_keys.push(data.onsets[p.index].key);
        }
        let (staff_step, head) = lane.default_staff();
        let (accent, ghost) = flags[p.index];
        measures[p.bar].notes.push(RawNote { offset_q: p.offset_q, lane, staff_step, head, accent, ghost, grace: false, stem_up: None });
        last_bar = last_bar.max(p.bar);
    }
    measures.truncate(last_bar + 1);
    if !unknown_keys.is_empty() {
        let count = unknown_keys.len();
        unknown_keys.sort_unstable();
        unknown_keys.dedup();
        let keys: Vec<String> = unknown_keys.iter().map(u8::to_string).collect();
        warnings.push(format!("{count} notes are on keys with no known drum instrument (keys {}); they are drawn on one line", keys.join(", ")));
    }
    if report.free_time {
        warnings.push("Many notes fall between the points of any grid: the file was probably played without a click, so some notes may sit in the wrong place".into());
    }
    if report.merged > 0 {
        warnings.push(format!("{} notes of the same instrument fell on one place (flams, double hits) and were merged", report.merged));
    }
    Ok(RawScore {
        title: "Untitled".into(),
        part_name: "Drums".into(),
        parts: vec![PartInfo { name: "Drums".into(), drums: true }],
        part_used: 0,
        measures,
        warnings,
        midi: Some(report),
    })
}
