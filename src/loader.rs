//! Reads `.musicxml` / `.xml` (UTF-8 or UTF-16) and compressed `.mxl` files.

use std::io::{Cursor, Read};
use std::path::Path;

use crate::musicxml::{parse_musicxml, ParseOptions, ScoreError};
use crate::score::Score;

/// Why a score file cannot be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    Read(String),
    BadArchive(String),
    NoDocument,
    Entry(String),
    Score(ScoreError),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Read(e) | LoadError::Entry(e) => write!(f, "{e}"),
            LoadError::BadArchive(e) => write!(f, "bad .mxl archive: {e}"),
            LoadError::NoDocument => write!(f, "no MusicXML document inside the .mxl archive"),
            LoadError::Score(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<ScoreError> for LoadError {
    fn from(e: ScoreError) -> Self {
        LoadError::Score(e)
    }
}

pub fn load_score(path: &Path, opts: &ParseOptions) -> Result<Score, LoadError> {
    let bytes = std::fs::read(path).map_err(|e| LoadError::Read(e.to_string()))?;
    let text = if bytes.starts_with(b"PK") { read_mxl(&bytes)? } else { decode_text(&bytes) };
    let mut score = parse_musicxml(&text, opts)?;
    if score.title == "Untitled" {
        if let Some(stem) = path.file_stem() {
            score.title = stem.to_string_lossy().into_owned();
        }
    }
    Ok(score)
}

fn read_mxl(bytes: &[u8]) -> Result<String, LoadError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| LoadError::BadArchive(e.to_string()))?;
    let names: Vec<String> = zip.file_names().map(String::from).collect();

    let mut root: Option<String> = None;
    if names.iter().any(|n| n == "META-INF/container.xml") {
        let mut s = String::new();
        if let Ok(mut f) = zip.by_name("META-INF/container.xml") {
            let _ = f.read_to_string(&mut s);
        }
        if let Ok(c) = crate::xml::parse(&s) {
            root = c.find("rootfile").and_then(|r| r.attr("full-path")).map(String::from);
        }
    }
    let root = root
        .filter(|r| names.contains(r))
        .or_else(|| {
            names
                .iter()
                .find(|n| !n.starts_with("META-INF") && (n.ends_with(".xml") || n.ends_with(".musicxml")))
                .cloned()
        })
        .ok_or(LoadError::NoDocument)?;

    let mut buf = Vec::new();
    zip.by_name(&root)
        .map_err(|e| LoadError::Entry(e.to_string()))?
        .read_to_end(&mut buf)
        .map_err(|e| LoadError::Entry(e.to_string()))?;
    Ok(decode_text(&buf))
}

fn decode_text(bytes: &[u8]) -> String {
    let utf16 = |be: bool| {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| if be { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) })
            .collect();
        String::from_utf16_lossy(&units)
    };
    if bytes.starts_with(&[0xFF, 0xFE]) {
        utf16(false)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        utf16(true)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}
