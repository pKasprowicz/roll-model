//! A small Standard MIDI File writer. The program only reads MIDI files; this exists so that tests
//! and the sample `.mid` are built from data in code, with no binary file of unclear origin in the
//! repository.

/// One event of a track.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ev {
    /// Microseconds per quarter note.
    Tempo(u32),
    /// Numerator and the power of two of the denominator (3 = an eighth).
    TimeSig(u8, u8),
    TrackName(String),
    NoteOn {
        ch: u8,
        key: u8,
        vel: u8,
    },
    NoteOff {
        ch: u8,
        key: u8,
    },
}

fn vlq(mut value: u32, out: &mut Vec<u8>) {
    let mut bytes = vec![(value & 0x7F) as u8];
    value >>= 7;
    while value > 0 {
        bytes.push((value & 0x7F) as u8 | 0x80);
        value >>= 7;
    }
    out.extend(bytes.iter().rev());
}

/// A file of `format` 0 or 1 with the given `ppq`; each track is `(absolute tick, event)` pairs.
pub fn write_smf(format: u16, ppq: u16, tracks: &[Vec<(u64, Ev)>]) -> Vec<u8> {
    let mut out = b"MThd".to_vec();
    out.extend(6u32.to_be_bytes());
    out.extend(format.to_be_bytes());
    out.extend((tracks.len() as u16).to_be_bytes());
    out.extend(ppq.to_be_bytes());
    for track in tracks {
        let mut events: Vec<&(u64, Ev)> = track.iter().collect();
        events.sort_by_key(|(tick, _)| *tick);
        let mut body = Vec::new();
        let mut last = 0u64;
        for (tick, ev) in events {
            vlq((*tick - last) as u32, &mut body);
            last = *tick;
            match ev {
                Ev::Tempo(us) => body.extend([0xFF, 0x51, 3, (us >> 16) as u8, (us >> 8) as u8, *us as u8]),
                Ev::TimeSig(num, den) => body.extend([0xFF, 0x58, 4, *num, *den, 24, 8]),
                Ev::TrackName(name) => {
                    body.extend([0xFF, 0x03]);
                    vlq(name.len() as u32, &mut body);
                    body.extend(name.as_bytes());
                }
                Ev::NoteOn { ch, key, vel } => body.extend([0x90 | (ch & 0x0F), *key, *vel]),
                Ev::NoteOff { ch, key } => body.extend([0x80 | (ch & 0x0F), *key, 0]),
            }
        }
        body.extend([0, 0xFF, 0x2F, 0]);
        out.extend(b"MTrk");
        out.extend((body.len() as u32).to_be_bytes());
        out.extend(body);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_length_numbers() {
        let enc = |v: u32| {
            let mut out = Vec::new();
            vlq(v, &mut out);
            out
        };
        assert_eq!(enc(0), [0x00]);
        assert_eq!(enc(0x7F), [0x7F]);
        assert_eq!(enc(0x80), [0x81, 0x00]);
        assert_eq!(enc(0x3FFF), [0xFF, 0x7F]);
        assert_eq!(enc(0x4000), [0x81, 0x80, 0x00]);
    }

    #[test]
    fn the_header_is_well_formed() {
        let bytes = write_smf(1, 480, &[vec![(0, Ev::Tempo(500_000))]]);
        assert_eq!(&bytes[..4], b"MThd");
        assert_eq!(&bytes[8..14], [0, 1, 0, 1, 1, 0xE0]);
        assert_eq!(&bytes[14..18], b"MTrk");
    }
}
