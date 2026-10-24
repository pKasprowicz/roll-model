//! Drum kit model: lanes (pads/instruments), General MIDI and Roland V31
//! note mappings, and standard drum-notation staff positions.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Lane {
    Kick,
    Snare,
    CrossStick,
    HiHatClosed,
    HiHatOpen,
    HiHatPedal,
    Tom1,
    Tom2,
    Tom3,
    Tom4,
    Crash1,
    Crash2,
    Ride,
    RideBell,
    China,
    Splash,
    Cowbell,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteHead {
    Normal,
    X,
    CircleX,
    Diamond,
    Triangle,
}

impl NoteHead {
    pub fn from_musicxml(s: &str) -> NoteHead {
        match s.trim() {
            "x" | "cross" => NoteHead::X,
            "circle-x" | "circled" | "circle dot" => NoteHead::CircleX,
            "diamond" => NoteHead::Diamond,
            "triangle" | "inverted triangle" => NoteHead::Triangle,
            _ => NoteHead::Normal,
        }
    }
}

impl Lane {
    pub const ALL: [Lane; 18] = [
        Lane::Kick,
        Lane::Snare,
        Lane::CrossStick,
        Lane::HiHatClosed,
        Lane::HiHatOpen,
        Lane::HiHatPedal,
        Lane::Tom1,
        Lane::Tom2,
        Lane::Tom3,
        Lane::Tom4,
        Lane::Crash1,
        Lane::Crash2,
        Lane::Ride,
        Lane::RideBell,
        Lane::China,
        Lane::Splash,
        Lane::Cowbell,
        Lane::Other,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Lane::Kick => "Kick",
            Lane::Snare => "Snare",
            Lane::CrossStick => "CrossStick",
            Lane::HiHatClosed => "HiHatClosed",
            Lane::HiHatOpen => "HiHatOpen",
            Lane::HiHatPedal => "HiHatPedal",
            Lane::Tom1 => "Tom1",
            Lane::Tom2 => "Tom2",
            Lane::Tom3 => "Tom3",
            Lane::Tom4 => "Tom4",
            Lane::Crash1 => "Crash1",
            Lane::Crash2 => "Crash2",
            Lane::Ride => "Ride",
            Lane::RideBell => "RideBell",
            Lane::China => "China",
            Lane::Splash => "Splash",
            Lane::Cowbell => "Cowbell",
            Lane::Other => "Other",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Lane::Kick => "Kick",
            Lane::Snare => "Snare",
            Lane::CrossStick => "Cross-stick",
            Lane::HiHatClosed => "Hi-hat closed",
            Lane::HiHatOpen => "Hi-hat open",
            Lane::HiHatPedal => "Hi-hat pedal",
            Lane::Tom1 => "Tom 1 (high)",
            Lane::Tom2 => "Tom 2 (mid)",
            Lane::Tom3 => "Tom 3 (floor)",
            Lane::Tom4 => "Tom 4 (low floor)",
            Lane::Crash1 => "Crash 1",
            Lane::Crash2 => "Crash 2",
            Lane::Ride => "Ride",
            Lane::RideBell => "Ride bell",
            Lane::China => "China",
            Lane::Splash => "Splash",
            Lane::Cowbell => "Cowbell",
            Lane::Other => "Other",
        }
    }

    pub fn from_name(s: &str) -> Option<Lane> {
        Lane::ALL.iter().copied().find(|l| l.name().eq_ignore_ascii_case(s.trim()))
    }

    /// Instruments that are "nearly right" when confused with each other
    /// (correct pad, wrong articulation). They earn partial credit.
    pub fn family(self) -> u8 {
        match self {
            Lane::Snare | Lane::CrossStick => 1,
            Lane::HiHatClosed | Lane::HiHatOpen => 2,
            Lane::Ride | Lane::RideBell => 3,
            Lane::Crash1 | Lane::Crash2 | Lane::China | Lane::Splash => 4,
            _ => 100 + self as u8,
        }
    }

    pub fn related(self, other: Lane) -> bool {
        self != other && self.family() == other.family()
    }

    pub fn is_foot(self) -> bool {
        matches!(self, Lane::Kick | Lane::HiHatPedal)
    }

    /// Default (staff step, notehead). Step 0 is the bottom staff line (E4),
    /// each step is one line/space, so the top line (F5) is step 8.
    pub fn default_staff(self) -> (i32, NoteHead) {
        match self {
            Lane::Kick => (1, NoteHead::Normal),
            Lane::Snare => (5, NoteHead::Normal),
            Lane::CrossStick => (5, NoteHead::CircleX),
            Lane::HiHatClosed => (9, NoteHead::X),
            Lane::HiHatOpen => (9, NoteHead::CircleX),
            Lane::HiHatPedal => (-1, NoteHead::X),
            Lane::Tom1 => (7, NoteHead::Normal),
            Lane::Tom2 => (6, NoteHead::Normal),
            Lane::Tom3 => (3, NoteHead::Normal),
            Lane::Tom4 => (2, NoteHead::Normal),
            Lane::Crash1 => (10, NoteHead::X),
            Lane::Crash2 => (11, NoteHead::X),
            Lane::Ride => (8, NoteHead::X),
            Lane::RideBell => (8, NoteHead::Diamond),
            Lane::China => (12, NoteHead::X),
            Lane::Splash => (10, NoteHead::Triangle),
            Lane::Cowbell => (7, NoteHead::Triangle),
            Lane::Other => (4, NoteHead::Diamond),
        }
    }
}

/// Staff step for a MusicXML display-step/display-octave pair (E4 -> 0).
pub fn staff_step(step: &str, octave: i32) -> Option<i32> {
    let idx = match step.trim().to_ascii_uppercase().as_str() {
        "C" => 0,
        "D" => 1,
        "E" => 2,
        "F" => 3,
        "G" => 4,
        "A" => 5,
        "B" => 6,
        _ => return None,
    };
    Some(idx + octave * 7 - (2 + 4 * 7))
}

/// General MIDI percussion key -> lane (used for MusicXML `midi-unpitched`).
pub fn gm_lane(note: u8) -> Lane {
    match note {
        35 | 36 => Lane::Kick,
        37 => Lane::CrossStick,
        38 | 40 => Lane::Snare,
        42 => Lane::HiHatClosed,
        44 => Lane::HiHatPedal,
        46 => Lane::HiHatOpen,
        41 => Lane::Tom4,
        43 => Lane::Tom3,
        45 | 47 => Lane::Tom2,
        48 | 50 => Lane::Tom1,
        49 => Lane::Crash1,
        57 => Lane::Crash2,
        51 | 59 => Lane::Ride,
        53 => Lane::RideBell,
        52 => Lane::China,
        55 => Lane::Splash,
        56 => Lane::Cowbell,
        _ => Lane::Other,
    }
}

/// Guess a lane from a MusicXML instrument name such as "Closed Hi-Hat".
pub fn lane_from_name(name: &str) -> Option<Lane> {
    let n = name.to_ascii_lowercase().replace(['-', '_'], " ");
    let has = |w: &str| n.contains(w);
    let hihat = has("hi hat") || has("hihat") || has("hi-hat") || has("high hat") || n.split_whitespace().any(|w| w == "hh");
    Some(if has("kick") || has("bass drum") {
        Lane::Kick
    } else if has("side stick") || has("cross stick") || has("rim click") || has("sidestick") {
        Lane::CrossStick
    } else if has("snare") || has("rimshot") {
        Lane::Snare
    } else if hihat && (has("pedal") || has("foot")) {
        Lane::HiHatPedal
    } else if hihat && (has("open") || has("half")) {
        Lane::HiHatOpen
    } else if hihat {
        Lane::HiHatClosed
    } else if has("ride") && has("bell") {
        Lane::RideBell
    } else if has("ride") {
        Lane::Ride
    } else if has("china") {
        Lane::China
    } else if has("splash") {
        Lane::Splash
    } else if has("crash") {
        if has("2") || has("ii") || has("right") { Lane::Crash2 } else { Lane::Crash1 }
    } else if has("cowbell") {
        Lane::Cowbell
    } else if has("tom") {
        // GM: 41 low floor, 43 high floor, 45 low, 47 low-mid, 48 hi-mid, 50 high
        if has("low floor") || has("4") {
            Lane::Tom4
        } else if has("floor") || has("3") {
            Lane::Tom3
        } else if has("low") || has("2") {
            Lane::Tom2
        } else if has("high") || n.starts_with("hi ") || has(" hi ") || has("1") {
            Lane::Tom1
        } else {
            Lane::Tom2
        }
    } else {
        return None;
    })
}

/// Last-resort guess from the staff position and notehead.
pub fn lane_from_staff(step: i32, head: NoteHead) -> Lane {
    let cymbal = matches!(head, NoteHead::X | NoteHead::CircleX);
    match step {
        s if s <= -1 && cymbal => Lane::HiHatPedal,
        s if s <= 1 => Lane::Kick,
        2 => Lane::Tom4,
        3 => Lane::Tom3,
        5 if cymbal => Lane::CrossStick,
        4 | 5 => Lane::Snare,
        6 => Lane::Tom2,
        7 => Lane::Tom1,
        8 if head == NoteHead::Diamond => Lane::RideBell,
        8 => Lane::Ride,
        9 if head == NoteHead::CircleX => Lane::HiHatOpen,
        9 => Lane::HiHatClosed,
        10 => Lane::Crash1,
        11 => Lane::Crash2,
        _ => Lane::China,
    }
}

/// MIDI note -> lane mapping for the input drum module.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteMap {
    pub map: BTreeMap<u8, Lane>,
}

impl NoteMap {
    /// Factory defaults of Roland V-Drums modules (TD-17/27/50, V51/V71/V31).
    /// Head and rim/edge zones send different notes; both map to the same lane.
    /// Verify against your kit's "Trigger > MIDI note" settings and use
    /// MIDI-learn in Settings if anything differs.
    pub fn roland_v31() -> NoteMap {
        let pairs: &[(u8, Lane)] = &[
            (36, Lane::Kick),
            (35, Lane::Kick),
            (38, Lane::Snare),       // head
            (40, Lane::Snare),       // rim shot
            (37, Lane::CrossStick),  // cross stick
            (48, Lane::Tom1),
            (50, Lane::Tom1),        // rim
            (45, Lane::Tom2),
            (47, Lane::Tom2),        // rim
            (43, Lane::Tom3),
            (58, Lane::Tom3),        // rim
            (41, Lane::Tom4),
            (39, Lane::Tom4),        // rim
            (42, Lane::HiHatClosed), // closed bow
            (22, Lane::HiHatClosed), // closed edge
            (46, Lane::HiHatOpen),   // open bow
            (26, Lane::HiHatOpen),   // open edge
            (44, Lane::HiHatPedal),
            (49, Lane::Crash1),
            (55, Lane::Crash1),      // edge
            (57, Lane::Crash2),
            (52, Lane::Crash2),      // edge
            (51, Lane::Ride),        // bow
            (59, Lane::Ride),        // edge
            (53, Lane::RideBell),
        ];
        NoteMap {
            map: pairs.iter().copied().collect(),
        }
    }

    pub fn lane(&self, note: u8) -> Option<Lane> {
        self.map.get(&note).copied()
    }

    pub fn notes_for(&self, lane: Lane) -> Vec<u8> {
        self.map
            .iter()
            .filter(|(_, l)| **l == lane)
            .map(|(n, _)| *n)
            .collect()
    }

    pub fn set_lane_notes(&mut self, lane: Lane, notes: &[u8]) {
        self.map.retain(|_, l| *l != lane);
        for n in notes {
            self.map.insert(*n, lane);
        }
    }

    pub fn assign(&mut self, note: u8, lane: Lane) {
        self.map.insert(note, lane);
    }
}

/// Parses "36, 35 40" into note numbers (ignores garbage).
pub fn parse_note_list(s: &str) -> Vec<u8> {
    s.split(|c: char| c == ',' || c.is_whitespace())
        .filter_map(|t| t.trim().parse::<u8>().ok())
        .filter(|n| *n < 128)
        .collect()
}

pub fn format_note_list(notes: &[u8]) -> String {
    notes.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staff_steps() {
        assert_eq!(staff_step("E", 4), Some(0));
        assert_eq!(staff_step("F", 5), Some(8));
        assert_eq!(staff_step("C", 5), Some(5));
        assert_eq!(staff_step("G", 5), Some(9));
        assert_eq!(staff_step("D", 4), Some(-1));
    }

    #[test]
    fn names() {
        assert_eq!(lane_from_name("Acoustic Bass Drum"), Some(Lane::Kick));
        assert_eq!(lane_from_name("Closed Hi-Hat"), Some(Lane::HiHatClosed));
        assert_eq!(lane_from_name("Open Hi-Hat"), Some(Lane::HiHatOpen));
        assert_eq!(lane_from_name("Pedal Hi-Hat"), Some(Lane::HiHatPedal));
        assert_eq!(lane_from_name("Side Stick"), Some(Lane::CrossStick));
        assert_eq!(lane_from_name("Low Floor Tom"), Some(Lane::Tom4));
        assert_eq!(lane_from_name("High Floor Tom"), Some(Lane::Tom3));
        assert_eq!(lane_from_name("Hi-Mid Tom"), Some(Lane::Tom1));
        assert_eq!(lane_from_name("Low-Mid Tom"), Some(Lane::Tom2));
        assert_eq!(lane_from_name("Low Tom"), Some(Lane::Tom2));
        assert_eq!(lane_from_name("High Tom"), Some(Lane::Tom1));
        assert_eq!(lane_from_name("Ride Bell"), Some(Lane::RideBell));
        assert_eq!(lane_from_name("Crash Cymbal 2"), Some(Lane::Crash2));
        assert_eq!(lane_from_name("Tambourine"), None);
    }

    #[test]
    fn note_lists() {
        assert_eq!(parse_note_list("36, 35 x 200 40"), vec![36, 35, 40]);
        let mut m = NoteMap::roland_v31();
        m.set_lane_notes(Lane::Kick, &[30]);
        assert_eq!(m.notes_for(Lane::Kick), vec![30]);
        assert_eq!(m.lane(36), None);
    }
}
