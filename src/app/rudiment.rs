//! The Rudiments tab: one of the 40 PAS rudiments, repeated, drawn on the staff like a score,
//! played along with the click and graded like a take. The rudiment is built into a `Score` (see
//! `crate::rudiments`) that lives in the practice fields of `DrumApp` while the tab is shown, so
//! Listen, takes, the Results tab and the controller work on it as on a loaded file.

use egui::RichText;

use super::{DrumApp, Source, Tab};
use crate::render;
use crate::rudiments::{self, Family, Lead, Rudiment, MAX_REPEATS};
use crate::widgets;

/// The practice tempo of a rudiment, in BPM. (The rudiment scores are written at 100 BPM, so
/// the practice tempo in percent is the BPM.)
pub(super) const BPM_RANGE: (f32, f32) = (30.0, 240.0);
/// How much the tempo goes up after a clean take when the ladder is on.
const LADDER_STEP: f32 = 5.0;

/// What the Rudiments tab remembers while the program runs (the chosen rudiment and its
/// repetitions are settings, kept in `Config`).
#[derive(Default)]
pub(super) struct State {
    /// A lead other than the rudiment's own.
    pub lead: Option<Lead>,
    /// What is typed into the search box of the list.
    filter: String,
    /// What the score was built from: rudiment, repetitions, lead.
    built: Option<(u8, usize, Lead)>,
}

impl DrumApp {
    pub(super) fn rudiment(&self) -> &'static Rudiment {
        rudiments::by_number(self.cfg.rudiment).unwrap_or(&rudiments::all()[0])
    }

    pub(super) fn rudiment_lead(&self) -> Lead {
        self.rud.lead.unwrap_or_else(|| self.rudiment().default_lead())
    }

    /// Puts the right practice source in place for the tab that is showing, and keeps the
    /// rudiment's score and tempo up to date.
    pub(super) fn sync_source(&mut self) {
        match self.tab {
            Tab::Score => self.activate(Source::Score),
            Tab::Rudiments => {
                self.activate(Source::Rudiments);
                self.ensure_rudiment();
            }
            _ => {}
        }
        self.remember_tempo();
    }

    /// Builds the score of the chosen rudiment unless that is what is there already. A take
    /// that is recording keeps the score it began with.
    pub(super) fn ensure_rudiment(&mut self) {
        if self.source != Source::Rudiments || self.take.as_ref().is_some_and(|t| t.running) {
            return;
        }
        let r = self.rudiment();
        let params = (r.number, self.cfg.rudiment_repeats, self.rudiment_lead());
        if self.rud.built == Some(params) && self.score.is_some() {
            return;
        }
        let another = self.rud.built.map_or(true, |b| b.0 != params.0);
        let score = r.build(params.1, params.2);
        self.range_from = 0;
        self.range_to = score.measures.len() - 1;
        self.score = Some(score);
        self.score_path = None;
        self.take = None; // it was played on the old score
        self.score_gen += 1;
        self.rud.built = Some(params);
        if another {
            self.tempo_pct = self.cfg.rudiment_bpm.get(&r.number).copied().unwrap_or(r.default_bpm() as f32);
            self.status = format!("Rudiment {}: {} — {}", r.number, r.name, r.sticking());
        }
    }

    /// Keeps the tempo of the rudiment in the settings, so it comes back next time; nothing is
    /// stored for a rudiment that is still at its starting tempo.
    fn remember_tempo(&mut self) {
        let (Source::Rudiments, Some((number, ..))) = (self.source, self.rud.built) else { return };
        let Some(r) = rudiments::by_number(number) else { return };
        let bpm = self.tempo_pct.round();
        match self.cfg.rudiment_bpm.get(&number) {
            Some(stored) if *stored == bpm => {}
            None if bpm == r.default_bpm() as f32 => {}
            _ => {
                self.cfg.rudiment_bpm.insert(number, bpm);
            }
        }
    }

    /// The rudiments the list offers (and the buttons step through).
    fn listed_rudiments(&self) -> Vec<&'static Rudiment> {
        rudiments::all().iter().filter(|r| !self.cfg.rudiment_nard_only || r.nard).collect()
    }

    pub(super) fn select_rudiment(&mut self, number: u8) {
        if rudiments::by_number(number).is_none() || number == self.cfg.rudiment {
            return;
        }
        self.cfg.rudiment = number;
        self.rud.lead = None;
        self.ensure_rudiment();
    }

    /// Moves `steps` rudiments on in the list (backwards if negative), stopping at its ends.
    pub(super) fn step_rudiment(&mut self, steps: i32) {
        let list = self.listed_rudiments();
        if list.is_empty() {
            return;
        }
        let current = self.cfg.rudiment;
        let at = match list.iter().position(|r| r.number == current) {
            Some(i) => i as i32 + steps,
            // The current one is not listed (the list is cut down to the NARD 26): start from where it would be.
            None => list.partition_point(|r| r.number < current) as i32 + steps - i32::from(steps > 0),
        };
        if let Some(r) = list.get(at.clamp(0, list.len() as i32 - 1) as usize) {
            self.select_rudiment(r.number);
        }
    }

    pub(super) fn step_repeats(&mut self, steps: i32) {
        let n = (self.cfg.rudiment_repeats as i32 + steps).clamp(1, MAX_REPEATS as i32) as usize;
        self.cfg.rudiment_repeats = n;
        self.ensure_rudiment();
    }

    pub(super) fn step_lead(&mut self, steps: i32) {
        let at = Lead::ALL.iter().position(|l| *l == self.rudiment_lead()).unwrap_or(0) as i32;
        let n = Lead::ALL.len() as i32;
        self.rud.lead = Some(Lead::ALL[(at + steps).rem_euclid(n) as usize]);
        self.ensure_rudiment();
    }

    pub(super) fn reset_rudiment_tempo(&mut self) {
        self.tempo_pct = self.rudiment().default_bpm() as f32;
        self.status = format!("Tempo {:.0} BPM", self.tempo_pct);
    }

    /// Called when a take of the rudiment was clean: remembers the tempo if it is the best so
    /// far, and with the ladder on, goes on to a faster one.
    pub(super) fn note_clean_rudiment(&mut self, bpm: f64) {
        let number = self.cfg.rudiment;
        let bpm = bpm.round() as f32;
        let best = self.cfg.rudiment_best.entry(number).or_insert(0.0);
        let new_best = bpm > *best;
        if new_best {
            *best = bpm;
        }
        let mut message = format!("Clean take at {bpm:.0} BPM");
        if new_best {
            message.push_str(": a new best for this rudiment");
        }
        if self.cfg.rudiment_ladder && self.tempo_pct.round() <= bpm {
            self.tempo_pct = (bpm + LADDER_STEP).min(BPM_RANGE.1);
            message.push_str(&format!(" · up to {:.0} BPM", self.tempo_pct));
        }
        self.status = format!("{} · {message}", self.status);
    }

    pub(super) fn rudiments_tab(&mut self, ui: &mut egui::Ui) {
        let running = self.take.as_ref().is_some_and(|t| t.running);
        ui.scope(|ui| {
            widgets::roomy(ui);
            self.rudiment_picker(ui, running);
        });
        self.ensure_rudiment();
        let Some(score) = self.score.as_ref() else { return };
        let n_measures = score.measures.len();
        let part = score.part_name.clone();
        let r = self.rudiment();

        ui.horizontal_wrapped(|ui| {
            ui.heading(format!("{}. {}", r.number, r.name));
            ui.label(RichText::new(part).weak());
        });
        ui.label(RichText::new(r.about).small().weak());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Sticking").small().weak());
            ui.label(RichText::new(r.sticking()).monospace().strong());
            ui.label(RichText::new("(right lead; flams and drags: the small letter is the grace note)").small().weak());
            if let Some(best) = self.cfg.rudiment_best.get(&r.number) {
                ui.label(RichText::new(format!("· best clean tempo {best:.0} BPM")).color(render::GREEN));
            }
            ui.add_enabled_ui(!running, |ui| {
                if ui.small_button("Starting tempo").on_hover_text("Go back to the tempo this rudiment starts at").clicked() {
                    self.reset_rudiment_tempo();
                }
                ui.checkbox(&mut self.cfg.rudiment_ladder, "ladder").on_hover_text(
                    "After a clean take (85 or better, the whole thing played) the tempo goes up by 5 BPM, so you climb as long as you play it cleanly",
                );
            });
        });
        ui.scope(|ui| {
            widgets::roomy(ui);
            self.transport_row(ui, running, n_measures);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!running, |ui| {
                    widgets::slider(ui, "tempo", &mut self.tempo_pct, BPM_RANGE.0..=BPM_RANGE.1, " BPM", 1.0);
                });
                self.playback_switches(ui, running);
            });
        });
        self.score_body(ui, running, n_measures);
    }

    /// Choosing the rudiment, how often it repeats and which hand starts.
    fn rudiment_picker(&mut self, ui: &mut egui::Ui, running: bool) {
        let mut chosen: Option<u8> = None;
        let mut steps = 0;
        let mut lead = self.rudiment_lead();
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!running, |ui| {
                if ui.button("Prev").on_hover_text("The previous rudiment").clicked() {
                    steps = -1;
                }
                let current = self.rudiment();
                let title = format!("{}. {}", current.number, current.name);
                ui.menu_button(RichText::new(title).strong(), |ui| {
                    ui.set_min_width(340.0);
                    chosen = self.rudiment_list(ui);
                });
                if ui.button("Next").on_hover_text("The next rudiment").clicked() {
                    steps = 1;
                }
                let mut repeats = self.cfg.rudiment_repeats;
                widgets::slider(ui, "repeats", &mut repeats, 1..=MAX_REPEATS, "", 1.0);
                self.cfg.rudiment_repeats = repeats;
                ui.label("lead");
                for l in Lead::ALL {
                    ui.selectable_value(&mut lead, l, l.label());
                }
            });
        });
        if let Some(n) = chosen {
            self.select_rudiment(n);
        }
        if steps != 0 {
            self.step_rudiment(steps);
        }
        if lead != self.rudiment_lead() {
            self.rud.lead = Some(lead);
        }
    }

    /// The menu of all rudiments by family, with a search box; returns the one clicked.
    fn rudiment_list(&mut self, ui: &mut egui::Ui) -> Option<u8> {
        let mut chosen = None;
        ui.horizontal(|ui| {
            ui.label("Search");
            ui.add(egui::TextEdit::singleline(&mut self.rud.filter).desired_width(180.0));
        });
        ui.checkbox(&mut self.cfg.rudiment_nard_only, "Only the original 26 (NARD)");
        ui.label(RichText::new("* = one of the original NARD Standard 26").small().weak());
        let current = self.cfg.rudiment;
        egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
            for family in Family::ALL {
                let items: Vec<&Rudiment> = self
                    .listed_rudiments()
                    .into_iter()
                    .filter(|r| r.family == family && r.matches(&self.rud.filter))
                    .collect();
                if items.is_empty() {
                    continue;
                }
                ui.label(RichText::new(family.label()).strong());
                for r in items {
                    let mut label = format!("{:>2}  {}{}", r.number, r.name, if r.nard { " *" } else { "" });
                    if let Some(best) = self.cfg.rudiment_best.get(&r.number) {
                        label.push_str(&format!("   ·  best {best:.0} BPM"));
                    }
                    if ui.selectable_label(r.number == current, label).on_hover_text(r.about).clicked() {
                        chosen = Some(r.number);
                        ui.close_menu();
                    }
                }
            }
        });
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::TakeSummary;
    use crate::config::Config;

    fn app() -> DrumApp {
        DrumApp::with_config(Config::default())
    }

    /// An app that has the Rudiments tab open.
    fn on_rudiments() -> DrumApp {
        let mut app = app();
        app.tab = Tab::Rudiments;
        app.sync_source();
        app
    }

    fn sample() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join("basic_rock_beat.musicxml")
    }

    #[test]
    fn opening_the_tab_builds_the_chosen_rudiment_at_its_own_tempo() {
        let mut app = app();
        app.cfg.rudiment = 16;
        app.tab = Tab::Rudiments;
        app.sync_source();
        let score = app.score.as_ref().expect("the rudiment is the score");
        assert_eq!(score.title, "Single paradiddle");
        assert_eq!(score.measures.len(), app.cfg.rudiment_repeats);
        assert_eq!((app.range_from, app.range_to), (0, app.cfg.rudiment_repeats - 1), "the whole thing is practised");
        assert_eq!(app.tempo_pct, 90.0, "the tempo is in BPM, starting where the rudiment suggests");
        let plan = app.plan_run(false).unwrap();
        assert!((plan.bpm - 90.0).abs() < 1e-9, "the practice tempo in percent is the BPM: {}", plan.bpm);
        assert!(app.cfg.rudiment_bpm.is_empty(), "a tempo that was not changed is not stored");
    }

    #[test]
    fn repeats_lead_and_rudiment_rebuild_the_score() {
        let mut app = on_rudiments();
        app.select_rudiment(7); // five stroke roll: alternate leads by default
        assert_eq!(app.rudiment_lead(), Lead::Alternate);
        let notes = |app: &DrumApp| app.score.as_ref().unwrap().notes.len();
        assert_eq!(notes(&app), 5 * app.cfg.rudiment_repeats);
        app.step_repeats(2);
        assert_eq!(app.score.as_ref().unwrap().measures.len(), app.cfg.rudiment_repeats);
        assert_eq!(app.range_to, app.cfg.rudiment_repeats - 1);
        let gen = app.score_gen;
        app.step_lead(1); // alternate -> right
        assert_eq!(app.rudiment_lead(), Lead::Right);
        assert!(app.score_gen > gen, "Listen is told that the score changed");
        // Another rudiment goes back to its own lead.
        app.select_rudiment(16);
        assert_eq!(app.rudiment_lead(), Lead::Right);
        assert_eq!(app.score.as_ref().unwrap().title, "Single paradiddle");
        // Repeats stay within their limits.
        app.step_repeats(-1000);
        assert_eq!(app.cfg.rudiment_repeats, 1);
        app.step_repeats(1000);
        assert_eq!(app.cfg.rudiment_repeats, MAX_REPEATS);
    }

    #[test]
    fn stepping_through_the_list_stops_at_the_ends_and_follows_the_nard_filter() {
        let mut app = on_rudiments();
        app.step_rudiment(-1);
        assert_eq!(app.cfg.rudiment, 1);
        app.step_rudiment(3);
        assert_eq!(app.cfg.rudiment, 4);
        app.step_rudiment(1000);
        assert_eq!(app.cfg.rudiment, 40);
        // With only the original 26 listed, the others are skipped.
        app.cfg.rudiment_nard_only = true;
        app.select_rudiment(2); // not a NARD rudiment
        app.step_rudiment(1);
        assert_eq!(app.cfg.rudiment, 6, "the next listed one after 2 is the double stroke open roll");
        app.select_rudiment(3);
        app.step_rudiment(-1);
        assert_eq!(app.cfg.rudiment, 1, "and the one before 3 is the single stroke roll");
    }

    #[test]
    fn each_tab_keeps_its_own_score_tempo_and_takes() {
        let mut app = app();
        app.open_file(&sample());
        app.tempo_pct = 80.0;
        app.range_from = 1;
        assert!(app.tab == Tab::Score && app.source == Source::Score);

        app.tab = Tab::Rudiments;
        app.sync_source();
        assert!(app.source == Source::Rudiments);
        assert_eq!(app.score.as_ref().unwrap().title, "Single stroke roll");
        assert!(app.has_main_score(), "the file is still there, parked");
        app.history.push(TakeSummary {
            number: 1,
            range: "x".into(),
            bpm: 90.0,
            overall: 90.0,
            letter: "A",
            accuracy: 1.0,
            mean_dt: 0.0,
            std_dt: 0.0,
            extra: 0,
        });

        app.tab = Tab::Score;
        app.sync_source();
        assert!(app.source == Source::Score);
        assert_eq!(app.score.as_ref().unwrap().title, "Basic Rock Beat", "the file is back with its own settings");
        assert_eq!((app.tempo_pct, app.range_from), (80.0, 1));
        assert!(app.history.is_empty(), "the rudiment's history stays with the rudiment");
        assert_eq!(app.parked.history.len(), 1);

        // Opening a file from the Rudiments tab goes to the Score tab and does not touch the rudiment.
        app.tab = Tab::Rudiments;
        app.sync_source();
        app.open_file(&sample());
        assert!(app.tab == Tab::Score && app.source == Source::Score);
        assert_eq!(app.parked.score.as_ref().unwrap().title, "Single stroke roll");
    }

    #[test]
    fn the_file_is_found_for_reload_from_either_tab() {
        let mut app = app();
        assert!(app.main_path().is_none());
        app.open_file(&sample());
        let path = app.main_path().expect("loaded");
        app.tab = Tab::Rudiments;
        app.sync_source();
        assert!(app.score_path.is_none(), "the rudiment has no file");
        assert_eq!(app.main_path(), Some(path));
    }

    #[test]
    fn a_change_of_the_rudiment_is_noticed_by_listen() {
        let mut app = on_rudiments();
        let before = app.listen_signature().unwrap();
        app.select_rudiment(16);
        assert_ne!(app.listen_signature().unwrap(), before);
        let sig = app.listen_signature().unwrap();
        app.step_lead(1);
        assert_ne!(app.listen_signature().unwrap(), sig, "also when only the lead changed");
    }

    #[test]
    fn the_tempo_is_remembered_per_rudiment() {
        let mut app = on_rudiments();
        app.select_rudiment(16);
        app.tempo_pct = 104.0;
        app.sync_source();
        assert_eq!(app.cfg.rudiment_bpm.get(&16), Some(&104.0));
        app.select_rudiment(24);
        app.sync_source();
        assert_eq!(app.tempo_pct, 90.0, "the other rudiment starts at its own tempo");
        assert!(!app.cfg.rudiment_bpm.contains_key(&24));
        app.select_rudiment(16);
        assert_eq!(app.tempo_pct, 104.0, "and the first comes back at the tempo it was left at");
        // Back at the starting tempo, the stored one is replaced, not kept.
        app.reset_rudiment_tempo();
        app.sync_source();
        assert_eq!(app.cfg.rudiment_bpm.get(&16), Some(&90.0));
    }

    #[test]
    fn a_clean_take_sets_the_best_tempo_and_the_ladder_climbs() {
        let mut app = on_rudiments();
        app.select_rudiment(16);
        app.tempo_pct = 90.0;
        app.status.clear();
        app.note_clean_rudiment(90.0);
        assert_eq!(app.cfg.rudiment_best.get(&16), Some(&90.0));
        assert!(app.status.contains("new best"), "{}", app.status);
        assert_eq!(app.tempo_pct, 90.0, "the ladder is off");
        app.cfg.rudiment_ladder = true;
        app.note_clean_rudiment(85.0);
        assert_eq!(app.cfg.rudiment_best.get(&16), Some(&90.0), "a slower clean take is not a best");
        assert_eq!(app.tempo_pct, 90.0, "and the ladder does not step down or sideways from a slower tempo");
        app.note_clean_rudiment(90.0);
        assert_eq!(app.tempo_pct, 95.0, "the ladder goes up by 5 BPM");
        app.tempo_pct = 238.0;
        app.note_clean_rudiment(238.0);
        assert_eq!(app.tempo_pct, BPM_RANGE.1);
    }

    #[test]
    fn a_closed_roll_cannot_be_recorded_but_can_be_listened_to() {
        let mut app = on_rudiments();
        app.select_rudiment(4);
        app.start_take();
        assert!(app.take.is_none(), "no take");
        assert!(app.status.contains("cannot be graded"), "{}", app.status);
        app.select_rudiment(16);
        assert!(app.rudiment().gradable());
    }

    #[test]
    fn a_take_of_a_rudiment_is_graded_with_hands_and_remembers_what_it_was() {
        use crate::analysis::Hit;
        use crate::drums::Lane;
        let mut app = on_rudiments();
        app.select_rudiment(16);
        app.cfg.rudiment_repeats = 4;
        app.cfg.count_in_bars = 0;
        app.ensure_rudiment();
        // Play it perfectly on the snare, the left hand a little softer (silently: no click).
        app.cfg.click_audio = false;
        app.cfg.click_midi = false;
        app.start_take();
        let take = app.take.as_mut().expect("recording");
        assert!(take.adaptive && take.label == "Single paradiddle ×4", "{}", take.label);
        let hits: Vec<Hit> = take
            .expected
            .iter()
            .map(|e| Hit { t: e.t, note: 38, velocity: if e.hand == Some(crate::score::Hand::Left) { 70 } else { 90 }, lane: Lane::Snare })
            .collect();
        take.hits = hits;
        app.stop_take(true);
        let take = app.take.as_ref().unwrap();
        assert_eq!(take.report.perfect, 32, "four bars of eight strokes");
        assert_eq!(take.report.hands.len(), 2);
        assert!(app.cfg.rudiment_best.contains_key(&16), "a perfect take is a clean one");
        assert_eq!(app.history.len(), 1);
        assert_eq!(app.history[0].range, "Single paradiddle ×4");
    }

    #[test]
    fn the_tab_draws_for_every_rudiment() {
        let ctx = egui::Context::default();
        let mut app = on_rudiments();
        for n in 1..=40u8 {
            app.select_rudiment(n);
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.rudiments_tab(ui));
            });
        }
        // And on the Home tab with the new button.
        app.tab = Tab::Home;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.welcome_panel(ui));
        });
    }
}
