//! MusicXML (score-partwise) -> [`RawScore`] for drum parts.

use std::collections::HashMap;

use super::ir::{PartInfo, RawMeasure, RawNote, RawRest, RawScore};
use super::{Format, ImportError, ImportOptions};
use crate::drums::{gm_lane, lane_from_name, lane_from_staff, staff_step, Lane, NoteHead};
use crate::score::{Dynamic, Stem};
use crate::xml::{self, Element};

fn parse_error(reason: String) -> ImportError {
    ImportError::Parse { format: Format::MusicXml, reason }
}

pub fn parse(text: &str, opts: &ImportOptions) -> Result<RawScore, ImportError> {
    let root = xml::parse(text).map_err(|e| parse_error(format!("XML error: {e}")))?;
    match root.name.as_str() {
        "score-partwise" => {}
        "score-timewise" => return Err(parse_error(
            "score-timewise MusicXML is not supported; export as partwise (the default in MuseScore, Sibelius, Dorico, Finale, Guitar Pro)"
                .into(),
        )),
        other => return Err(parse_error(format!("not a MusicXML score (root element <{other}>)"))),
    }

    let mut warnings = Vec::new();
    let title = root
        .path(&["work", "work-title"])
        .map(|e| e.text())
        .filter(|t| !t.is_empty())
        .or_else(|| root.child_text("movement-title").filter(|t| !t.is_empty()))
        .or_else(|| root.children_named("credit").filter_map(|c| c.child_text("credit-words")).find(|t| !t.is_empty()));

    // part id -> (name, instrument id -> lane)
    let mut part_names: HashMap<String, String> = HashMap::new();
    let mut instruments: HashMap<String, Lane> = HashMap::new();
    if let Some(pl) = root.child("part-list") {
        for sp in pl.children_named("score-part") {
            let id = sp.attr("id").unwrap_or_default().to_string();
            part_names.insert(id, sp.child_text("part-name").unwrap_or_default());
            let mut names: HashMap<String, String> = HashMap::new();
            for si in sp.children_named("score-instrument") {
                if let Some(iid) = si.attr("id") {
                    names.insert(iid.to_string(), si.child_text("instrument-name").unwrap_or_default());
                }
            }
            for mi in sp.children_named("midi-instrument") {
                let Some(iid) = mi.attr("id") else { continue };
                let unp = mi.child_text("midi-unpitched").and_then(|t| t.parse::<u8>().ok());
                if let Some(n) = unp.filter(|n| (1..=128).contains(n)) {
                    let lane = gm_lane(n - 1);
                    // GM can't tell e.g. "Tom 1" vs a custom name; prefer the
                    // name when GM says "Other".
                    let lane = if lane == Lane::Other { names.get(iid).and_then(|nm| lane_from_name(nm)).unwrap_or(lane) } else { lane };
                    instruments.insert(iid.to_string(), lane);
                }
            }
            for (iid, nm) in &names {
                if !instruments.contains_key(iid) {
                    if let Some(l) = lane_from_name(nm) {
                        instruments.insert(iid.clone(), l);
                    }
                }
            }
        }
    }

    let elements: Vec<&Element> = root.children_named("part").collect();
    if elements.is_empty() {
        return Err(parse_error("the file contains no parts".into()));
    }
    let name_of = |p: &Element| part_names.get(p.attr("id").unwrap_or_default()).cloned().unwrap_or_default();
    let is_drums =
        |p: &Element| p.find("unpitched").is_some() || p.find("clef").and_then(|c| c.child_text("sign")).is_some_and(|s| s == "percussion");
    let parts: Vec<PartInfo> = elements.iter().map(|p| PartInfo { name: name_of(p), drums: is_drums(p) }).collect();
    // The part that was asked for, else the first with unpitched notes, else the first with a
    // percussion clef, else the first.
    let part_used = opts
        .part
        .filter(|i| *i < elements.len())
        .or_else(|| elements.iter().position(|p| p.find("unpitched").is_some()))
        .or_else(|| elements.iter().position(|p| is_drums(p)))
        .unwrap_or(0);
    let part = elements[part_used];
    if elements.len() > 1 {
        warnings.push(format!("File has {} parts; using \"{}\"", elements.len(), parts[part_used].name));
    }
    let part_name = parts[part_used].name.clone();

    let measures = parse_measures(part, &instruments, &mut warnings);
    Ok(RawScore { title, part_name, parts, part_used, measures, warnings, midi: None })
}

fn parse_f64(s: &str) -> Option<f64> {
    let t: String =
        s.trim().chars().skip_while(|c| !c.is_ascii_digit() && *c != '.').take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    t.parse().ok()
}

fn parse_numbers(s: &str) -> Vec<u32> {
    let mut out = Vec::new();
    for tok in s.split([',', ' ']).map(str::trim).filter(|t| !t.is_empty()) {
        if let Some((a, b)) = tok.split_once('-') {
            if let (Ok(a), Ok(b)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>()) {
                out.extend(a..=b);
            }
        } else if let Ok(n) = tok.parse() {
            out.push(n);
        }
    }
    out
}

fn beat_unit_quarters(unit: &str) -> f64 {
    match unit.trim() {
        "long" => 16.0,
        "breve" => 8.0,
        "whole" => 4.0,
        "half" => 2.0,
        "eighth" => 0.5,
        "16th" => 0.25,
        "32nd" => 0.125,
        _ => 1.0,
    }
}

fn direction_tempo(dir: &Element) -> Option<f64> {
    if let Some(t) = dir.child("sound").and_then(|s| s.attr("tempo")).and_then(parse_f64) {
        return Some(t);
    }
    let met = dir.find("metronome")?;
    let per_min = met.child_text("per-minute").and_then(|t| parse_f64(&t))?;
    let unit = met.child_text("beat-unit").unwrap_or_else(|| "quarter".into());
    let mut mult = beat_unit_quarters(&unit);
    if met.has_child("beat-unit-dot") {
        mult *= 1.5;
    }
    Some(per_min * mult)
}

fn parse_measures(part: &Element, instruments: &HashMap<String, Lane>, warnings: &mut Vec<String>) -> Vec<RawMeasure> {
    let mut out = Vec::new();
    let mut divisions = 1.0f64;
    let mut beats = 4u32;
    let mut beat_type = 4u32;
    let mut active_ending: Vec<u32> = Vec::new();
    let mut unknown_instruments = 0usize;
    let mut voice_up: HashMap<String, bool> = HashMap::new();
    // Inside a measure repeat (the "simile" sign): how many measures back the music is copied from.
    let mut repeating: Option<usize> = None;

    for m in part.children_named("measure") {
        let mut rm = RawMeasure { number: m.attr("number").unwrap_or_default().to_string(), ..Default::default() };
        let mut pos = 0.0f64; // in divisions
        let mut max_pos = 0.0f64;
        let mut last_start = 0.0f64;
        let mut clear_ending = false;

        for el in m.elements() {
            match el.name.as_str() {
                "attributes" => {
                    if let Some(d) = el.child_text("divisions").and_then(|t| parse_f64(&t)) {
                        if d > 0.0 {
                            divisions = d;
                        }
                    }
                    if let Some(t) = el.child("time") {
                        let b: u32 =
                            t.child_text("beats").map(|s| s.split('+').filter_map(|x| x.trim().parse::<u32>().ok()).sum()).unwrap_or(0);
                        let bt = t.child_text("beat-type").and_then(|s| s.trim().parse::<u32>().ok());
                        if b > 0 {
                            beats = b;
                        }
                        if let Some(bt) = bt.filter(|v| *v > 0) {
                            beat_type = bt;
                        }
                    }
                    if let Some(r) = el.child("measure-style").and_then(|s| s.child("measure-repeat")) {
                        match r.attr("type") {
                            Some("start") => repeating = Some(r.text().trim().parse::<usize>().ok().filter(|n| *n > 0).unwrap_or(1)),
                            Some("stop") => repeating = None,
                            _ => {}
                        }
                    }
                }
                "backup" => {
                    let d = el.child_text("duration").and_then(|t| parse_f64(&t)).unwrap_or(0.0);
                    pos = (pos - d).max(0.0);
                }
                "forward" => {
                    let d = el.child_text("duration").and_then(|t| parse_f64(&t)).unwrap_or(0.0);
                    pos += d;
                    max_pos = max_pos.max(pos);
                }
                "direction" => {
                    if let Some(bpm) = direction_tempo(el) {
                        let off = el.child_text("offset").and_then(|t| t.trim().parse::<f64>().ok()).unwrap_or(0.0);
                        rm.tempos.push(((pos + off).max(0.0) / divisions, bpm));
                    }
                }
                "sound" => {
                    if let Some(bpm) = el.attr("tempo").and_then(parse_f64) {
                        rm.tempos.push((pos / divisions, bpm));
                    }
                }
                "barline" => {
                    if let Some(r) = el.child("repeat") {
                        match r.attr("direction") {
                            Some("forward") => rm.forward_repeat = true,
                            Some("backward") => {
                                let times = r.attr("times").and_then(|t| t.trim().parse().ok()).unwrap_or(2u32);
                                rm.backward_repeat = Some(times.max(1));
                            }
                            _ => {}
                        }
                    }
                    if let Some(e) = el.child("ending") {
                        match e.attr("type") {
                            Some("start") => active_ending = parse_numbers(e.attr("number").unwrap_or("1")),
                            Some("stop") | Some("discontinue") => clear_ending = true,
                            _ => {}
                        }
                    }
                }
                "note" => {
                    let is_chord = el.has_child("chord");
                    let is_grace = el.has_child("grace");
                    let dur = el.child_text("duration").and_then(|t| parse_f64(&t)).unwrap_or(0.0);
                    if !is_chord {
                        last_start = pos;
                    }
                    let start = last_start;
                    if !is_chord && !is_grace {
                        pos += dur;
                        max_pos = max_pos.max(pos);
                    }
                    if el.has_child("rest") {
                        // A rest the score hides (print-object="no") is not drawn either.
                        if !is_chord && !is_grace && dur > 0.0 && el.attr("print-object") != Some("no") {
                            rm.rests.push(RawRest {
                                offset_q: start / divisions,
                                dur_q: dur / divisions,
                                voice: voice_of(el),
                                whole_bar: el.child("rest").and_then(|r| r.attr("measure")) == Some("yes"),
                                stem: Stem::Up,
                            });
                        }
                        continue;
                    }
                    if el.has_child("cue") {
                        continue;
                    }
                    let tie_stop = el.children_named("tie").any(|t| t.attr("type") == Some("stop"));
                    if tie_stop {
                        continue;
                    }
                    if let Some(n) = parse_note(el, start / divisions, is_grace, instruments, &mut unknown_instruments) {
                        // A voice with any stem-up note is the hands'.
                        let up = n.stem.map_or(!n.lane.is_foot(), Stem::is_up);
                        *voice_up.entry(voice_of(el)).or_insert(false) |= up;
                        rm.notes.push(n);
                    }
                }
                _ => {}
            }
        }

        let sig_len = beats as f64 * 4.0 / beat_type as f64;
        let content = max_pos / divisions;
        // Pickup bars ("implicit") and overfull bars use their real length.
        let pickup = m.attr("implicit") == Some("yes") && content > 0.0;
        rm.len_q = if pickup || content > sig_len + 1e-6 { content } else { sig_len };
        rm.beats = beats;
        rm.beat_type = beat_type;
        // A repeated measure is written without its notes (some programs put a whole-bar rest there):
        // it is the music of the measure that many back.
        if let Some(back) = repeating.filter(|b| rm.notes.is_empty() && out.len() >= *b) {
            let source: &RawMeasure = &out[out.len() - back];
            rm.notes = source.notes.clone();
            rm.rests = source.rests.clone();
        }
        rm.endings = active_ending.clone();
        if clear_ending {
            active_ending.clear();
        }
        out.push(rm);
    }
    if unknown_instruments > 0 {
        warnings.push(format!("{unknown_instruments} notes had no recognisable instrument; guessed from staff position"));
    }
    // A rest belongs to the hands or the feet by its voice; a voice without notes is the hands' if
    // it is the first.
    for r in out.iter_mut().flat_map(|m| m.rests.iter_mut()) {
        r.stem = Stem::from_up(voice_up.get(&r.voice).copied().unwrap_or(r.voice == "1"));
    }
    out
}

fn voice_of(note: &Element) -> String {
    note.child_text("voice").map(|v| v.trim().to_string()).unwrap_or_else(|| "1".to_string())
}

fn parse_note(el: &Element, offset_q: f64, grace: bool, instruments: &HashMap<String, Lane>, unknown: &mut usize) -> Option<RawNote> {
    let (step, octave) = if let Some(u) = el.child("unpitched") {
        (u.child_text("display-step"), u.child_text("display-octave"))
    } else if let Some(p) = el.child("pitch") {
        (p.child_text("step"), p.child_text("octave"))
    } else {
        (None, None)
    };
    let pos = match (step, octave.and_then(|o| o.trim().parse::<i32>().ok())) {
        (Some(s), Some(o)) => staff_step(&s, o),
        _ => None,
    };

    let nh = el.child("notehead");
    let head = nh.map(|h| NoteHead::from_musicxml(&h.text())).unwrap_or(NoteHead::Normal);
    let ghost = nh.and_then(|h| h.attr("parentheses")) == Some("yes");

    let notations = el.child("notations");
    let accent = notations.map(|n| n.find("accent").is_some() || n.find("strong-accent").is_some()).unwrap_or(false);
    let open = notations.and_then(|n| n.child("technical")).map(|t| t.has_child("open") || t.has_child("open-string")).unwrap_or(false);

    let inst = el.child("instrument").and_then(|i| i.attr("id"));
    let mut lane = match inst.and_then(|id| instruments.get(id)) {
        Some(l) => *l,
        None => {
            if inst.is_some() || pos.is_some() {
                *unknown += 1;
            }
            lane_from_staff(pos.unwrap_or(5), head)
        }
    };
    if lane == Lane::HiHatClosed && open {
        lane = Lane::HiHatOpen;
    }
    let (default_step, default_head) = lane.default_staff();
    let stem = el.child_text("stem").and_then(|s| match s.as_str() {
        "up" => Some(Stem::Up),
        "down" => Some(Stem::Down),
        _ => None,
    });

    Some(RawNote {
        offset_q,
        lane,
        staff_step: pos.unwrap_or(default_step),
        head: if nh.is_some() {
            head
        } else if pos.is_none() {
            default_head
        } else {
            head
        },
        dynamic: if ghost {
            Dynamic::Ghost
        } else if accent {
            Dynamic::Accent
        } else {
            Dynamic::Normal
        },
        grace,
        stem,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{Quarters, Seconds};
    pub const SAMPLE: &str = include_str!("../../../examples/basic_rock_beat.musicxml");

    fn parse_sample(text: &str) -> crate::score::Score {
        let loaded = crate::import::import_bytes(text.as_bytes(), "fixture.musicxml", &ImportOptions::default()).unwrap();
        crate::import::check_invariants(&loaded.score).unwrap();
        loaded.score
    }

    #[test]
    fn parses_sample() {
        let s = parse_sample(SAMPLE);
        assert_eq!(s.title, "Basic Rock Beat");
        assert_eq!(s.measures.len(), 4);
        assert!((s.tempo.bpm_at(Quarters::ZERO) - 100.0).abs() < 1e-9);
        // Each of the 4 bars: 8 hi-hats + 2 kicks + 2 snares (bar 4: crash + fill)
        let hh = s.notes.iter().filter(|n| n.lane == Lane::HiHatClosed).count();
        assert_eq!(hh, 24);
        let kicks: Vec<f64> = s.notes.iter().filter(|n| n.lane == Lane::Kick && n.measure == 0).map(|n| n.q.0).collect();
        assert_eq!(kicks, vec![0.0, 2.0]);
        let snares: Vec<f64> = s.notes.iter().filter(|n| n.lane == Lane::Snare && n.measure == 0).map(|n| n.q.0).collect();
        assert_eq!(snares, vec![1.0, 3.0]);
        assert!(s.notes.iter().any(|n| n.lane == Lane::Crash1 && (n.q - Quarters(12.0)).abs() < Quarters(1e-9)));
        assert!(s.notes.iter().any(|n| n.is_accent()));
        // hi-hat drawn at G5 with x noteheads, kick at F4
        let hhn = s.notes.iter().find(|n| n.lane == Lane::HiHatClosed).unwrap();
        assert_eq!((hhn.staff_step, hhn.head), (9, NoteHead::X));
        assert_eq!(s.staff_for_lane(Lane::Kick).0, 1);
    }

    #[test]
    fn simulated_take_on_sample() {
        use crate::analysis::{analyze, expected_notes, Grade, Hit, Tolerances};
        let s = parse_sample(SAMPLE);
        // Practise bars 2..4 at 80% tempo; play everything 12 ms late except
        // one missed kick and one extra crash.
        let exp = expected_notes(&s, 1, 3, 0.8);
        assert!(exp[0].t.abs() < Seconds(1e-9), "range starts at t=0");
        let mut hits: Vec<Hit> = exp
            .iter()
            .skip(1)
            .map(|e| Hit { t: e.t + Seconds(0.012), note: 0, velocity: if e.dynamic == Dynamic::Accent { 120 } else { 80 }, lane: e.lane })
            .collect();
        hits.push(Hit { t: Seconds(1.1), note: 49, velocity: 100, lane: Lane::Crash1 });
        let r = analyze(&exp, &hits, &Tolerances::default(), Seconds::INFINITY);
        assert_eq!(r.results[0].outcome, crate::analysis::Outcome::Graded(Grade::Missed));
        assert_eq!(r.missed, 1);
        assert_eq!(r.extra, 1);
        assert_eq!(r.perfect, exp.len() - 1);
        assert!((r.mean_dt_ms - 12.0).abs() < 1e-6);
        assert_eq!(r.dynamics_score, Some(1.0));
        assert!(r.overall > 85.0 && r.overall < 100.0, "{}", r.overall);
        // score-time <-> q mapping used to draw the hits
        let t0 = s.tempo.q_to_sec(s.measures[1].start_q, 0.8);
        let q = s.tempo.sec_to_q(hits[0].t - Seconds(0.012) + t0, 0.8);
        assert!((q - s.notes[exp[1].note_idx].q).abs() < Quarters(1e-9));
    }

    #[test]
    fn printed_rests_are_kept_with_their_voice_and_hidden_ones_are_not() {
        let note = |voice: u32, step: &str, stem: &str| {
            format!("<note><unpitched><display-step>{step}</display-step><display-octave>5</display-octave></unpitched><duration>2</duration><voice>{voice}</voice><stem>{stem}</stem></note>")
        };
        let rest =
            |voice: u32, dur: u32, extra: &str| format!("<note><rest{extra}/><duration>{dur}</duration><voice>{voice}</voice></note>");
        let hidden = "<note print-object=\"no\"><rest/><duration>2</duration><voice>2</voice></note>";
        let body = format!(
            "<measure number=\"1\"><attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>             {}{}{}<backup><duration>8</duration></backup>{}{}{hidden}</measure>             <measure number=\"2\">{}</measure>",
            note(1, "G", "up"),
            rest(1, 2, ""),
            rest(1, 4, ""),
            note(2, "F", "down"),
            rest(2, 4, ""),
            rest(1, 8, " measure=\"yes\""),
        );
        let xml = format!("<score-partwise><part-list><score-part id=\"P1\"><part-name>D</part-name></score-part></part-list><part id=\"P1\">{body}</part></score-partwise>");
        let s = parse_sample(&xml);
        let rests: Vec<(usize, f64, f64, bool, bool)> =
            s.rests.iter().map(|r| (r.measure, r.q.0, r.dur_q.0, r.stem.is_up(), r.whole_bar)).collect();
        assert_eq!(
            rests,
            vec![(0, 1.0, 1.0, true, false), (0, 2.0, 2.0, true, false), (0, 1.0, 2.0, false, false), (1, 4.0, 4.0, true, true)]
        );
    }
}
