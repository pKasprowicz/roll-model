//! Opening the score on screen again with other import settings: the "MIDI import" menu of the
//! Score tab. A MIDI file has no bars and no exact positions of its own, so how its notes are laid
//! out is a choice the user can change without opening the file again.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::import::{Format, Grid, ImportError, ImportOptions, Loaded, MidiOptions};
use crate::units::Quarters;
use crate::widgets;

use super::{DrumApp, Source};

/// A file being read on another thread; the result arrives on `rx`.
/// How a score gets on screen: opened as a new file, or read again from the file that is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Opened {
    Fresh,
    Again,
}

pub(super) struct PendingLoad {
    path: PathBuf,
    rx: mpsc::Receiver<Result<Loaded, ImportError>>,
}

impl DrumApp {
    /// The options for the next import.
    pub(super) fn import_options(&self) -> ImportOptions {
        ImportOptions::default().with_repeats(self.cfg.expand_repeats).with_midi(self.score_practice().midi_options.clone())
    }

    /// Whether the score on screen was read from a MIDI file, so that its import can be adjusted.
    pub(super) fn is_midi_score(&self) -> bool {
        self.source == Source::Score
            && self.practice.score_path.is_some()
            && self.practice.score_report.as_ref().is_some_and(|r| r.format == Format::Midi)
    }

    /// Starts reading `path` on another thread; `poll_loading` puts the score on screen when it is
    /// done. A file asked for before that one finishes replaces it.
    pub(super) fn start_loading(&mut self, path: &Path) -> String {
        let (tx, rx) = mpsc::channel();
        let (file, options) = (path.to_path_buf(), self.import_options());
        std::thread::spawn(move || {
            let _ = tx.send(crate::import::load_score(&file, &options));
        });
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.loading = Some(PendingLoad { path: path.to_path_buf(), rx });
        format!("Loading {name}…")
    }

    /// Picks up the result of a background load once it is there.
    pub(super) fn poll_loading(&mut self) {
        let Some(pending) = self.loading.take() else { return };
        // A file that arrives while something is recording or playing waits; it must not stop that.
        if self.take_running() || self.playback.is_listening() {
            self.loading = Some(pending);
            return;
        }
        let result = match pending.rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                self.loading = Some(pending);
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err(ImportError::Read("reading the file stopped unexpectedly".into())),
        };
        match result {
            Ok(loaded) => {
                self.view.status = self.install(loaded, &pending.path, Opened::Fresh);
                self.view.error = None;
            }
            Err(e) => {
                self.view.status.clear();
                self.view.error = Some(format!("Could not load {}: {e}", pending.path.display()));
            }
        }
    }

    /// Reads the score on screen from its file again with the current import settings; the
    /// selected bars, the view and the layout stay.
    pub(super) fn reimport(&mut self) {
        let Some(path) = self.practice.score_path.clone() else { return };
        self.stop_take(super::session::StopReason::Aborted);
        self.stop_listen();
        match crate::import::load_score(&path, &self.import_options()) {
            Ok(loaded) => {
                self.view.status = self.install(loaded, &path, Opened::Again);
                self.view.error = None;
            }
            Err(e) => self.view.error = Some(format!("Could not load {}: {e}", path.display())),
        }
    }

    /// Puts a loaded score on screen. `Opened::Again` is a re-import of the open file: the range of bars,
    /// the tab and the layout are kept. Returns the line for the status bar; showing it is the caller's.
    pub(super) fn install(&mut self, loaded: Loaded, path: &Path, opened: Opened) -> String {
        // The practice fields belong to the Rudiments tab while it shows; the score goes to the Score
        // tab's fields whichever tab the user is on when the file arrives.
        self.activate(Source::Score);
        let Loaded { score, report } = loaded;
        let status = format!(
            "Loaded \"{}\": {} measures, {} notes, {:.0} BPM{}",
            score.title,
            score.measures.len(),
            score.notes.len(),
            score.tempo.bpm_at(Quarters::ZERO),
            report.midi.as_ref().map(|m| format!(" ({})", m.summary())).unwrap_or_default(),
        );
        let last = score.measures.len().saturating_sub(1);
        if opened == Opened::Again {
            self.practice.range_from = self.practice.range_from.min(last);
            self.practice.range_to = self.practice.range_to.min(last);
        } else {
            self.practice.range_from = 0;
            self.practice.range_to = last;
        }
        self.practice.score = Some(score);
        self.practice.score_report = Some(report);
        self.practice.score_path = Some(path.to_path_buf());
        if opened == Opened::Fresh {
            let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
            self.cfg.remember_recent(&absolute.to_string_lossy());
            self.restore_layout();
            self.show_tab(super::Tab::Score);
        }
        self.practice.take = None;
        self.score_gen += 1;
        status
    }

    /// The menu beside Layout; a change re-imports the file at once (it takes milliseconds).
    pub(super) fn midi_import_menu(&mut self, ui: &mut egui::Ui) {
        let before = self.practice.midi_options.clone();
        let summary = self.practice.score_report.as_ref().and_then(|r| r.midi.as_ref()).map(|m| m.summary());
        ui.menu_button("MIDI import", |ui| {
            ui.set_min_width(340.0);
            if let Some(summary) = &summary {
                widgets::hint(ui, summary.clone());
            }
            ui.add_space(4.0);
            ui.label("Grid");
            for (grid, label, tip) in [
                (Grid::Auto, "auto", "Sixteenths, eighths and triplets, whichever fits each beat."),
                (Grid::Eighth, "1/8", "Notes land on eighths at the finest."),
                (Grid::Sixteenth, "1/16", "Notes land on sixteenths at the finest, no triplets."),
                (Grid::Triplet, "triplets", "Eighth and sixteenth triplets only."),
                (Grid::ThirtySecond, "1/32", "As auto, and thirty-seconds too (fast rolls and fills)."),
            ] {
                ui.radio_value(&mut self.practice.midi_options.grid, grid, label).on_hover_text(tip);
            }
            ui.separator();
            ui.checkbox(&mut self.practice.midi_options.remove_offset, "remove a constant offset")
                .on_hover_text("A recording made with a fixed delay is moved as a whole before its notes are put on the grid.");
            widgets::Slider::new("first bar starts at beat", &mut self.practice.midi_options.first_bar_beats, 0..=7).zero_text("1 (as in the file)").show(ui)
                .on_hover_text("Moves the bar lines, not the notes: the music before the first bar becomes a short bar (a pickup or a count-in). Not possible when the time signature changes later in the file.");
            ui.separator();
            widgets::Slider::new("accent from", &mut self.practice.midi_options.accent_ratio, 1.1..=2.0).suffix("× median").step(0.05).show(ui)
                .on_hover_text("A note this much louder than the median of its instrument is an accent.");
            widgets::Slider::new("ghost up to", &mut self.practice.midi_options.ghost_ratio, 0.2..=0.9).suffix("× median").step(0.05).show(ui)
                .on_hover_text("A note this much quieter than the median of its instrument is a ghost note.");
            if ui.button("Defaults").clicked() {
                self.practice.midi_options = MidiOptions::default();
            }
        });
        if self.practice.midi_options != before {
            self.reimport();
        }
    }
}
