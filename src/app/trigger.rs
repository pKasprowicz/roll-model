//! The start trigger in the application: two drum hits in a row start a recording on the
//! Score and Rudiments tabs. The detection is `crate::trigger`; this decides when it may
//! fire, starts the take, and has the settings and the hint.

use std::time::Instant;

use egui::RichText;

use super::{DrumApp, Tab};
use crate::drums::Lane;
use crate::render;
use crate::trigger::{Sequence, Slot, WINDOW};

impl DrumApp {
    fn trigger_sequence(&self) -> Sequence {
        Sequence { first: self.cfg.trigger_first, second: self.cfg.trigger_second }
    }

    /// Only where a recording can be started by hand: on the Score or Rudiments tab with
    /// something to play, and not while a take records or the latency is being measured.
    fn trigger_allowed(&self) -> bool {
        self.cfg.trigger_enabled
            && matches!(self.view.tab, Tab::Score | Tab::Rudiments)
            && self.score.is_some()
            && !self.take.as_ref().is_some_and(|t| t.running)
            && self.calibration.is_none()
    }

    /// A hit of `lane` at `at`, from `handle_event`. True when the hit was used up by the
    /// trigger (assigned to it, or it started the recording) and is no part of a take.
    pub(super) fn handle_trigger(&mut self, lane: Lane, at: Instant) -> bool {
        if let Some(slot) = self.trigger_learn.take() {
            match slot {
                Slot::First => self.cfg.trigger_first = lane,
                Slot::Second => self.cfg.trigger_second = lane,
            }
            self.trigger.reset();
            self.view.status = format!("{} trigger hit: {}", slot.label(), lane.label());
            return true;
        }
        if !self.trigger_allowed() {
            self.trigger.reset();
            return false;
        }
        if !self.trigger.feed(self.trigger_sequence(), lane, at) {
            return false;
        }
        self.start_take();
        if self.take.as_ref().is_some_and(|t| t.running) {
            self.view.status = format!("{} — recording started by the trigger", self.view.status);
        }
        true
    }

    /// For the legend line of the Score and Rudiments tabs: what the trigger is and whether it
    /// is waiting for its second hit.
    pub(super) fn trigger_hint(&self) -> Option<(String, bool)> {
        if !self.cfg.trigger_enabled {
            return None;
        }
        let s = self.trigger_sequence();
        let text = format!("· start trigger: {}, then {} within {:.1} s", s.first.label(), s.second.label(), WINDOW.as_secs_f32());
        Some((text, self.trigger.armed(Instant::now())))
    }

    /// The Start trigger section of Settings -> MIDI input.
    pub(super) fn settings_trigger(&mut self, ui: &mut egui::Ui) {
        ui.heading("Start trigger");
        ui.label(
            RichText::new(
                "Start a recording with two hits in a row, so you can do it with sticks in your hands: the first instrument, then the second within half a second, with nothing played in between. \
                 It works on the Score and Rudiments tabs when no take is recording. The recording then begins with the count-in. Pick an instrument you do not play in the middle of a piece, such as a crash followed by a tom.",
            )
            .small()
            .weak(),
        );
        ui.checkbox(&mut self.cfg.trigger_enabled, "Start recording with the two hits");
        ui.add_enabled_ui(self.cfg.trigger_enabled, |ui| {
            for slot in [Slot::First, Slot::Second] {
                ui.horizontal(|ui| {
                    ui.label(format!("{} hit", slot.label()));
                    let lane = match slot {
                        Slot::First => &mut self.cfg.trigger_first,
                        Slot::Second => &mut self.cfg.trigger_second,
                    };
                    egui::ComboBox::from_id_salt(("trigger", slot as u8)).selected_text(lane.label()).width(180.0).show_ui(ui, |ui| {
                        for l in Lane::ALL.into_iter().filter(|l| *l != Lane::Other) {
                            ui.selectable_value(lane, l, l.label());
                        }
                    });
                    let learning = self.trigger_learn == Some(slot);
                    if ui.selectable_label(learning, if learning { "hit a pad…" } else { "Learn" }).clicked() {
                        self.trigger_learn = if learning { None } else { Some(slot) };
                    }
                });
            }
        });
        if let Some((text, armed)) = self.trigger_hint() {
            let color = if armed { render::GREEN } else { ui.visuals().weak_text_color() };
            ui.label(RichText::new(text.trim_start_matches("· ")).small().color(color));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::midi::RawEvent;
    use std::time::Duration;

    fn sample() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join("basic_rock_beat.musicxml")
    }

    /// An app on the Score tab with the trigger on (crash 1, then tom 1) and no sound.
    fn app() -> DrumApp {
        let mut app = DrumApp::with_config(Config::default());
        app.cfg.trigger_enabled = true;
        app.cfg.click_audio = false;
        app.cfg.click_midi = false;
        app.open_file(&sample());
        app
    }

    /// A hit of `lane` at `ms` after `t0`, as the drum module would send it.
    fn hit(app: &mut DrumApp, t0: Instant, lane: Lane, ms: u64) {
        let note = app.cfg.note_map.notes_for(lane)[0];
        app.handle_event(RawEvent { at: t0 + Duration::from_millis(ms), note, velocity: 100, channel: 10, hihat: None });
    }

    fn recording(app: &DrumApp) -> bool {
        app.take.as_ref().is_some_and(|t| t.running)
    }

    #[test]
    fn it_is_off_until_it_is_switched_on() {
        assert!(!Config::default().trigger_enabled);
        let mut app = app();
        app.cfg.trigger_enabled = false;
        let t0 = Instant::now();
        hit(&mut app, t0, Lane::Crash1, 0);
        hit(&mut app, t0, Lane::Tom1, 100);
        assert!(!recording(&app));
        assert!(app.trigger_hint().is_none());
    }

    #[test]
    fn two_hits_start_a_recording_on_the_score_tab() {
        let mut app = app();
        let t0 = Instant::now();
        hit(&mut app, t0, Lane::Crash1, 0);
        assert!(!recording(&app) && app.trigger_hint().unwrap().1, "armed after the first hit");
        hit(&mut app, t0, Lane::Tom1, 300);
        assert!(recording(&app));
        assert!(app.view.status.contains("started by the trigger"), "{}", app.view.status);
        assert!(app.take.as_ref().unwrap().hits.is_empty(), "the trigger hits are not part of the take");
    }

    #[test]
    fn a_late_second_hit_does_not_start_it() {
        let mut app = app();
        let t0 = Instant::now();
        hit(&mut app, t0, Lane::Crash1, 0);
        hit(&mut app, t0, Lane::Tom1, 700);
        assert!(!recording(&app));
    }

    #[test]
    fn it_works_on_the_rudiments_tab_and_not_on_the_others() {
        let mut app = app();
        let t0 = Instant::now();
        app.view.tab = Tab::Metronome;
        hit(&mut app, t0, Lane::Crash1, 0);
        hit(&mut app, t0, Lane::Tom1, 100);
        assert!(!recording(&app), "not on the Metronome tab");
        app.view.tab = Tab::Settings;
        hit(&mut app, t0, Lane::Crash1, 200);
        hit(&mut app, t0, Lane::Tom1, 300);
        assert!(!recording(&app), "not on Settings");

        app.view.tab = Tab::Rudiments;
        app.sync_source();
        hit(&mut app, t0, Lane::Crash1, 400);
        hit(&mut app, t0, Lane::Tom1, 500);
        assert!(recording(&app));
        assert_eq!(app.take.as_ref().unwrap().label, "Single stroke roll ×4");
    }

    #[test]
    fn a_hit_on_another_tab_does_not_leave_the_trigger_half_armed() {
        let mut app = app();
        let t0 = Instant::now();
        app.view.tab = Tab::Settings;
        hit(&mut app, t0, Lane::Crash1, 0);
        app.view.tab = Tab::Score;
        hit(&mut app, t0, Lane::Tom1, 100);
        assert!(!recording(&app));
    }

    #[test]
    fn nothing_starts_while_a_take_records_and_the_hits_count_as_hits() {
        let mut app = app();
        let t0 = Instant::now();
        hit(&mut app, t0, Lane::Crash1, 0);
        hit(&mut app, t0, Lane::Tom1, 100);
        assert!(recording(&app));
        let number = app.take.as_ref().unwrap().number;
        // A hit that is due in the take: the first note is 0.5 s lead plus the count-in away.
        let zero = app.take.as_ref().unwrap().zero;
        let hits_before = app.take.as_ref().unwrap().hits.len();
        let note = app.cfg.note_map.notes_for(Lane::Crash1)[0];
        app.handle_event(RawEvent { at: zero, note, velocity: 100, channel: 10, hihat: None });
        let note = app.cfg.note_map.notes_for(Lane::Tom1)[0];
        app.handle_event(RawEvent { at: zero + Duration::from_millis(100), note, velocity: 100, channel: 10, hihat: None });
        let take = app.take.as_ref().unwrap();
        assert_eq!(take.number, number, "no second take was started");
        assert_eq!(take.hits.len(), hits_before + 2, "the same two hits are played notes now");
    }

    #[test]
    fn a_closed_roll_is_not_started_and_says_why() {
        let mut app = app();
        app.view.tab = Tab::Rudiments;
        app.sync_source();
        app.select_rudiment(4);
        let t0 = Instant::now();
        hit(&mut app, t0, Lane::Crash1, 0);
        hit(&mut app, t0, Lane::Tom1, 100);
        assert!(!recording(&app));
        assert!(app.view.status.contains("cannot be graded"), "{}", app.view.status);
    }

    #[test]
    fn learn_assigns_the_next_hit_and_is_not_a_trigger_hit() {
        let mut app = app();
        let t0 = Instant::now();
        app.trigger_learn = Some(Slot::First);
        hit(&mut app, t0, Lane::Ride, 0);
        assert_eq!(app.cfg.trigger_first, Lane::Ride);
        assert!(app.trigger_learn.is_none() && app.view.status.contains("First trigger hit: Ride"), "{}", app.view.status);
        app.trigger_learn = Some(Slot::Second);
        hit(&mut app, t0, Lane::Snare, 100);
        assert_eq!(app.cfg.trigger_second, Lane::Snare);
        assert!(!recording(&app), "the learn hits did not trigger anything");
        // The new pair works.
        hit(&mut app, t0, Lane::Ride, 1000);
        hit(&mut app, t0, Lane::Snare, 1200);
        assert!(recording(&app));
    }

    #[test]
    fn the_settings_section_draws() {
        let ctx = egui::Context::default();
        let mut app = app();
        for enabled in [false, true] {
            app.cfg.trigger_enabled = enabled;
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.settings_trigger(ui));
            });
        }
    }
}
