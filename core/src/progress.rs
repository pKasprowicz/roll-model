//! What the user has achieved, kept apart from the settings: the settings file is rewritten
//! whenever a slider moves, and progress is not something to lose to that (nor to put beside the
//! zoom level). Today it is the best clean tempo of each rudiment; more will follow.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config::{write_atomically, Paths};
use crate::practice_tempo::BPM_RANGE;
use crate::rudiments;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    /// Per rudiment number: the fastest tempo, in BPM, at which a take of it was clean.
    pub best_bpm: BTreeMap<u8, f32>,
}

impl Progress {
    /// A line a rudiment: `best.16 = 96`.
    pub fn to_text(&self) -> String {
        let mut out = String::from("# Roll Model: what you have achieved. One line per rudiment: the best clean tempo in BPM.\n");
        for (n, bpm) in &self.best_bpm {
            out.push_str(&format!("best.{n} = {bpm}\n"));
        }
        out
    }

    /// Reads what `to_text` wrote. What is not understood is left out; numbers are kept in range.
    pub fn from_text(text: &str) -> Progress {
        let mut p = Progress::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let Some(n) = key.trim().strip_prefix("best.").and_then(|n| n.parse::<u8>().ok()) else { continue };
            if rudiments::by_number(n).is_none() {
                continue;
            }
            if let Some(bpm) = value.trim().parse::<f32>().ok().filter(|b| b.is_finite()) {
                p.best_bpm.insert(n, bpm.clamp(BPM_RANGE.0, BPM_RANGE.1));
            }
        }
        p
    }

    /// The file's progress, or, when there is no such file yet, `older` (the best tempos the
    /// settings file used to hold, before progress had a file of its own).
    pub fn load(paths: &Paths, older: &BTreeMap<u8, f32>) -> Progress {
        match paths.progress_file().and_then(|f| std::fs::read(f).ok()) {
            Some(bytes) => Progress::from_text(&String::from_utf8_lossy(&bytes)),
            None => Progress { best_bpm: older.clone() },
        }
    }

    pub fn save(&self, paths: &Paths) -> std::io::Result<PathBuf> {
        let file = paths.progress_file().ok_or_else(|| std::io::Error::other("no settings folder"))?;
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        write_atomically(&file, self.to_text().as_bytes())?;
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_survives_the_text_and_keeps_to_the_known_rudiments_and_the_range() {
        let mut p = Progress::default();
        p.best_bpm.insert(16, 96.5);
        p.best_bpm.insert(1, 120.0);
        assert_eq!(Progress::from_text(&p.to_text()), p);
        let read = Progress::from_text("best.16 = 9999\nbest.99 = 80\nbest.x = 80\nbest.20 = abc\nnonsense\nbest.21 = 88\n");
        assert_eq!(read.best_bpm.get(&16), Some(&BPM_RANGE.1));
        assert_eq!(read.best_bpm.keys().copied().collect::<Vec<_>>(), [16, 21]);
    }

    #[test]
    fn the_best_tempos_of_the_settings_file_are_taken_over_once() {
        let dir = std::env::temp_dir().join(format!("roll-model-progress-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let paths = Paths::in_dir(dir.clone());
        let older = BTreeMap::from([(16u8, 90.0f32)]);
        let first = Progress::load(&paths, &older);
        assert_eq!(first.best_bpm.get(&16), Some(&90.0), "no file yet: the older ones");
        let mut changed = first.clone();
        changed.best_bpm.insert(16, 95.0);
        changed.save(&paths).unwrap();
        assert_eq!(Progress::load(&paths, &older).best_bpm.get(&16), Some(&95.0), "the file wins from then on");
        assert!(!dir.join("progress.txt.tmp").exists(), "nothing is left beside it");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
