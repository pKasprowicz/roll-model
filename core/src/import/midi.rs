//! Standard MIDI File -> [`RawScore`]: the drum notes of a file laid out on bars.
//!
//! Reading ([`read`]) finds the drum notes, the tempo changes and the time signatures, all in
//! ticks. Building ([`build`]) lays bars over the ticks and places each onset in its bar.

use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};

use super::ir::{PartInfo, RawMeasure, RawNote, RawScore};
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

fn lay_bars(data: &MidiData, last_tick: u64, limits: &Limits, warnings: &mut Vec<String>) -> Result<Vec<Bar>, ImportError> {
    let mut signatures = data.signatures.clone();
    if signatures.first().is_none_or(|s| s.0 > 0) {
        signatures.insert(0, (0, 4, 4));
        warnings.push("No time signature at the start of the file; using 4/4".into());
    }
    let mut bars = Vec::new();
    let mut start = 0u64;
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
        start += len;
        if start > last_tick {
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

pub fn build(data: MidiData, opts: &ImportOptions) -> Result<RawScore, ImportError> {
    let mut warnings = data.warnings.clone();
    let last_tick = data.onsets.last().map_or(0, |o| o.tick);
    let bars = lay_bars(&data, last_tick, &opts.limits, &mut warnings)?;
    let ppq = data.ppq as f64;

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
    for onset in &data.onsets {
        let bar = bars.partition_point(|b| b.start <= onset.tick).saturating_sub(1);
        // Interim placement: the nearest sixteenth (the quantiser replaces it).
        let step = (data.ppq / 4).max(1);
        let tick_in_bar = onset.tick - bars[bar].start;
        let snapped = ((tick_in_bar + step / 2) / step * step).min(bars[bar].len);
        let lane = lane_of(onset.key);
        if lane == Lane::Other {
            unknown_keys.push(onset.key);
        }
        let (staff_step, head) = lane.default_staff();
        measures[bar].notes.push(RawNote {
            offset_q: snapped as f64 / ppq,
            lane,
            staff_step,
            head,
            accent: false,
            ghost: false,
            grace: false,
            stem_up: None,
        });
    }
    if !unknown_keys.is_empty() {
        let count = unknown_keys.len();
        unknown_keys.sort_unstable();
        unknown_keys.dedup();
        let keys: Vec<String> = unknown_keys.iter().map(u8::to_string).collect();
        warnings.push(format!("{count} notes are on keys with no known drum instrument (keys {}); they are drawn on one line", keys.join(", ")));
    }
    Ok(RawScore {
        title: "Untitled".into(),
        part_name: "Drums".into(),
        parts: vec![PartInfo { name: "Drums".into(), drums: true }],
        part_used: 0,
        measures,
        warnings,
    })
}
