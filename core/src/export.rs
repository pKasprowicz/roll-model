//! A take written out as a table, for a spreadsheet.

use crate::analysis::{Expected, Hit, Report};
use crate::score::Score;

pub const CSV_HEADER: &str = "measure,beat,instrument,expected_ms,played_ms,offset_ms,midi_note,velocity,grade";

/// One CSV field: quoted, with quotes doubled, when it holds a comma, a quote or a line break.
fn field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// The CSV of a take: a row for each expected note (with the hit that matched it, if any), then a
/// row for each hit that matched no note. `expected` and `report.results` go together.
pub fn take_csv(score: &Score, expected: &[Expected], hits: &[Hit], report: &Report) -> String {
    let mut s = format!("{CSV_HEADER}\n");
    for (e, r) in expected.iter().zip(&report.results) {
        let n = &score.notes[e.note_idx];
        let m = &score.measures[n.measure];
        let beat = (n.q - m.start_q) / m.beat_q() + 1.0;
        let (played, note, vel) = r
            .hit
            .map(|h| {
                let h = &hits[h];
                (format!("{:.1}", h.t * 1000.0), h.note.to_string(), h.velocity.to_string())
            })
            .unwrap_or_default();
        s.push_str(&format!(
            "{},{:.3},{},{:.1},{},{},{},{},{}\n",
            field(&m.number),
            beat,
            field(e.lane.label()),
            e.t * 1000.0,
            played,
            r.dt_ms.map(|d| format!("{d:.1}")).unwrap_or_default(),
            note,
            vel,
            field(r.grade.label())
        ));
    }
    for (hi, h) in hits.iter().enumerate() {
        if report.hit_match[hi].is_none() {
            s.push_str(&format!(",,{},,{:.1},,{},{},Extra\n", field(h.lane.label()), h.t * 1000.0, h.note, h.velocity));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{analyze, expected_notes, Tolerances};
    use crate::drums::Lane;

    #[test]
    fn a_field_with_a_comma_or_a_quote_is_quoted() {
        assert_eq!(field("12"), "12");
        assert_eq!(field("12,a"), "\"12,a\"");
        assert_eq!(field("say \"x\""), "\"say \"\"x\"\"\"");
        assert_eq!(field("a\nb"), "\"a\nb\"");
    }

    #[test]
    fn a_take_becomes_one_row_for_each_note_and_one_for_each_extra_hit() {
        let mut score = Score::click_bar(4, 1);
        score.measures[0].number = "7,b".into(); // a printed number as odd as a file may hold
        let expected = expected_notes(&score, 0, 0, 1.0);
        // The second note is missed, the third is 20 ms late, and an extra crash comes.
        let mut hits: Vec<Hit> = expected.iter().enumerate().filter(|(i, _)| *i != 1).map(|(i, e)| Hit { t: e.t + if i == 2 { 0.02 } else { 0.0 }, note: 38, velocity: 90, lane: e.lane }).collect();
        hits.push(Hit { t: 0.1, note: 49, velocity: 100, lane: Lane::Crash1 });
        let report = analyze(&expected, &hits, &Tolerances::default(), f64::INFINITY);
        let csv = take_csv(&score, &expected, &hits, &report);
        let rows: Vec<&str> = csv.lines().collect();
        assert_eq!(rows[0], CSV_HEADER);
        assert_eq!(rows.len(), 1 + expected.len() + 1, "{csv}");
        assert!(rows[1].starts_with("\"7,b\",1.000,"), "the measure number is quoted: {}", rows[1]);
        assert!(rows[2].ends_with(",,,,Missed"), "{}", rows[2]);
        assert!(rows[3].contains(",20.0,"), "{}", rows[3]);
        assert!(rows.last().unwrap().ends_with(",Extra"));
        for row in &rows[1..] {
            // Without the quoted number every row has as many fields as the header.
            let unquoted = row.replacen("\"7,b\"", "x", 1);
            assert_eq!(unquoted.split(',').count(), CSV_HEADER.split(',').count(), "{row}");
        }
    }
}
