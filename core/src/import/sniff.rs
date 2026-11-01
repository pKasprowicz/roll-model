//! Which format a file is: by what is inside it first, by its name second.

/// A format this program reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    MusicXml,
    Midi,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::MusicXml => "MusicXML",
            Format::Midi => "MIDI",
        }
    }
}

/// What [`sniff`] made of a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detected {
    Known(Format),
    /// A format the program knows by name but does not read; the score has to be exported as one
    /// it reads.
    Recognised { format: &'static str },
    Unknown,
}

/// Formats known by their extension.
const RECOGNISED_BY_EXTENSION: &[(&str, &str)] = &[
    ("mscz", "MuseScore"),
    ("mscx", "MuseScore"),
    ("sib", "Sibelius"),
    ("gp", "Guitar Pro"),
    ("gpx", "Guitar Pro"),
    ("gp3", "Guitar Pro"),
    ("gp4", "Guitar Pro"),
    ("gp5", "Guitar Pro"),
    ("mus", "Finale"),
    ("musx", "Finale"),
    ("dorico", "Dorico"),
];

pub fn sniff(bytes: &[u8], name: &str) -> Detected {
    let ext = std::path::Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    // Guitar Pro 3 to 5 begin with a length byte and this text; version 6 with "BCFZ" or "BCFS".
    if bytes.get(1..).is_some_and(|b| b.starts_with(b"FICHIER GUITAR PRO")) || bytes.starts_with(b"BCFZ") || bytes.starts_with(b"BCFS") {
        return Detected::Recognised { format: "Guitar Pro" };
    }
    if let Some((_, format)) = RECOGNISED_BY_EXTENSION.iter().find(|(e, _)| *e == ext) {
        return Detected::Recognised { format };
    }
    if bytes.starts_with(b"MThd") {
        return Detected::Known(Format::Midi);
    }
    // A zip is a compressed MusicXML file (.mxl); the container module says what is wrong with it.
    if bytes.starts_with(b"PK") || looks_like_text(bytes) || ["musicxml", "xml", "mxl"].contains(&ext.as_str()) {
        return Detected::Known(Format::MusicXml);
    }
    if ext == "mid" || ext == "midi" {
        return Detected::Known(Format::Midi);
    }
    Detected::Unknown
}

/// Starts with a UTF-16 byte-order mark, or with `<` after an optional UTF-8 mark and white space.
fn looks_like_text(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return true;
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'<')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_decides_before_the_name() {
        let known = Detected::Known(Format::MusicXml);
        assert_eq!(sniff(b"<?xml version=\"1.0\"?><score-partwise/>", "x.bin"), known);
        assert_eq!(sniff(b"  \n<score-partwise/>", ""), known);
        assert_eq!(sniff(b"\xEF\xBB\xBF<score-partwise/>", "x"), known);
        assert_eq!(sniff(b"\xFF\xFE<\0", "x"), known);
        assert_eq!(sniff(b"PK\x03\x04", "x.dat"), known);
    }

    #[test]
    fn a_known_extension_keeps_the_old_error_texts_for_files_that_are_not_text() {
        assert_eq!(sniff(b"\x00\x01", "broken.MusicXML"), Detected::Known(Format::MusicXml));
        assert_eq!(sniff(b"\x00\x01", "broken.txt"), Detected::Unknown);
        assert_eq!(sniff(b"", "empty"), Detected::Unknown);
    }

    #[test]
    fn programs_that_have_to_export_are_named() {
        let name = |bytes: &[u8], file: &str| match sniff(bytes, file) {
            Detected::Recognised { format } => format,
            other => panic!("{other:?}"),
        };
        assert_eq!(name(b"PK\x03\x04", "song.mscz"), "MuseScore");
        assert_eq!(name(b"<museScore/>", "song.MSCX"), "MuseScore");
        assert_eq!(name(b"PK", "song.sib"), "Sibelius");
        assert_eq!(name(b"PK", "song.gp"), "Guitar Pro");
        assert_eq!(name(b"\x18FICHIER GUITAR PRO v3.00", "song.bin"), "Guitar Pro");
        assert_eq!(name(b"BCFZ....", "song"), "Guitar Pro");
        assert_eq!(name(b"x", "song.musx"), "Finale");
        assert_eq!(name(b"x", "song.dorico"), "Dorico");
    }
}
