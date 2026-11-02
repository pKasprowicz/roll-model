//! Getting from the bytes of a file to the text of a MusicXML document: the compressed `.mxl`
//! container and the text encodings (UTF-8 and UTF-16).

use std::io::{Cursor, Read};

use super::{ImportError, Limits};

/// The MusicXML text inside `bytes`: a `.mxl` archive (starts with "PK") or a plain document.
pub fn musicxml_text(bytes: &[u8], limits: &Limits) -> Result<String, ImportError> {
    if bytes.starts_with(b"PK") {
        read_mxl(bytes, limits)
    } else {
        Ok(decode_text(bytes))
    }
}

fn read_mxl(bytes: &[u8], limits: &Limits) -> Result<String, ImportError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| ImportError::Container(format!("bad .mxl archive: {e}")))?;
    let names: Vec<String> = zip.file_names().map(String::from).collect();

    let mut root: Option<String> = None;
    if names.iter().any(|n| n == "META-INF/container.xml") {
        let mut s = String::new();
        if let Ok(f) = zip.by_name("META-INF/container.xml") {
            let _ = f.take(limits.max_bytes as u64).read_to_string(&mut s);
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
        .ok_or_else(|| ImportError::Container("no MusicXML document inside the .mxl archive".into()))?;

    // A small archive can hold an enormous document; read no more than the limit allows.
    let mut buf = Vec::new();
    let entry = zip.by_name(&root).map_err(|e| ImportError::Container(e.to_string()))?;
    entry
        .take(limits.max_bytes as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|e| ImportError::Container(e.to_string()))?;
    if buf.len() > limits.max_bytes {
        return Err(ImportError::TooLarge(format!("the document inside the archive is over {} bytes", limits.max_bytes)));
    }
    Ok(decode_text(&buf))
}

fn decode_text(bytes: &[u8]) -> String {
    let utf16 = |be: bool| {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>().0.iter()
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
