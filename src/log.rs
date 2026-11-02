//! A log file in the settings folder, for problems that the screen shows once and forgets: what
//! the sound threads report, so that something that went wrong on a stage can be looked at later.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// When the file is longer than this, the old one is kept as `.old` and a new one begins.
const MAX_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, Default)]
pub struct Log {
    path: Option<PathBuf>,
}

impl Log {
    /// A log in the file `path`; with `None` nothing is written.
    pub fn at(path: Option<PathBuf>) -> Log {
        Log { path }
    }

    /// Appends a line with the time (UTC). A log that cannot be written is not a reason to stop.
    pub fn line(&self, text: &str) {
        let Some(path) = self.path.as_deref() else { return };
        let _ = append(path, &format!("{} {text}\n", utc(SystemTime::now())));
    }
}

fn append(path: &Path, text: &str) -> std::io::Result<()> {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(path, path.with_extension("log.old"));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::OpenOptions::new().create(true).append(true).open(path)?.write_all(text.as_bytes())
}

/// "2026-11-02 14:03:09", without a calendar library.
fn utc(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}", rest / 3600, rest % 3600 / 60, rest % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_time_is_written_as_a_date_and_a_clock() {
        assert_eq!(utc(UNIX_EPOCH), "1970-01-01 00:00:00");
        assert_eq!(utc(UNIX_EPOCH + Duration::from_secs(1_000_000_000)), "2001-09-09 01:46:40");
        assert_eq!(utc(UNIX_EPOCH + Duration::from_secs(1_782_950_000)), "2026-07-01 23:53:20");
    }

    #[test]
    fn lines_are_appended_and_a_long_file_is_kept_as_old() {
        let guard = roll_model_core::testing::TempDir::new("log");
        let dir = guard.path().to_path_buf();
        let path = dir.join("roll-model.log");
        let log = Log::at(Some(path.clone()));
        log.line("first");
        log.line("second");
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.lines().next().unwrap().ends_with(" first"));
        std::fs::write(&path, vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        log.line("third");
        assert!(path.with_extension("log.old").exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 1);
        Log::default().line("nowhere");
    }
}
