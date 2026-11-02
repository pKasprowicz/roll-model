//! Small UI helpers shared by the tabs.

use std::ops::RangeInclusive;

use egui::emath::Numeric;
use egui::{vec2, Color32, Response, RichText, Ui, Vec2};

/// Whether a text field (also a slider's value box) is being typed in. Not the same as
/// `ctx.wants_keyboard_input()`, which is also true while a button or slider has the focus (after
/// Tab or a click): the shortcuts must still work then.
pub fn typing(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.focused()).is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some())
}

/// A line of grey helper text (the "small" style): how every page explains itself.
pub fn hint(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.label(RichText::new(text).small().weak())
}

/// A line of small text in `color`: a warning (`theme::AMBER`) or a state under a control.
pub fn note(ui: &mut Ui, text: impl Into<String>, color: Color32) -> Response {
    ui.label(RichText::new(text).small().color(color))
}

/// A slider with its `caption` in front of it:
///
/// ```ignore
/// Slider::new("zoom", &mut zoom, limits::ZOOM.range()).step(0.05).show(ui);
/// ```
///
/// The control also follows the mouse wheel while the pointer is over the caption, the slider or
/// the value box: one notch moves the value by `step` (1 unless set; it snaps the value to the
/// `step` grid that starts at the low end of the range). The wheel is then not passed on, so a
/// scrolling page stays put while a slider is being tuned. `zero_text` shows a value of zero as
/// a word ("auto", "off"). Every slider of the program is one of these, not a bare `egui::Slider`.
pub struct Slider<'a, N: Numeric> {
    caption: &'a str,
    value: &'a mut N,
    range: RangeInclusive<N>,
    suffix: &'a str,
    step: f64,
    zero_text: Option<&'a str>,
}

impl<'a, N: Numeric> Slider<'a, N> {
    pub fn new(caption: &'a str, value: &'a mut N, range: RangeInclusive<N>) -> Self {
        Slider { caption, value, range, suffix: "", step: 1.0, zero_text: None }
    }

    /// Text after the number (" ms", "%").
    pub fn suffix(mut self, suffix: &'a str) -> Self {
        self.suffix = suffix;
        self
    }

    /// How far one notch of the wheel moves the value.
    pub fn step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }

    pub fn zero_text(mut self, text: &'a str) -> Self {
        self.zero_text = Some(text);
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let Slider { caption, value, range, suffix, step, zero_text } = self;
        let (lo, hi) = (range.start().to_f64(), range.end().to_f64());
        // Show only as many decimals as the step needs ("100%", not "100.0%").
        let decimals = if step >= 1.0 { 0 } else { (-step.log10()).ceil().clamp(0.0, 6.0) as usize };
        let group = ui.horizontal(|ui| {
            if !caption.is_empty() {
                ui.label(caption);
            }
            let mut slider = egui::Slider::new(value, range).suffix(suffix).max_decimals(decimals);
            if let Some(zero) = zero_text {
                let zero = zero.to_string();
                slider = slider.custom_formatter(move |n, _| if n == 0.0 { zero.clone() } else { format!("{n}") });
            }
            ui.add(slider)
        });
        let mut response = group.inner;

        if ui.is_enabled() && step > 0.0 && ui.rect_contains_pointer(group.response.rect) {
            let dy = ui.input(|i| i.raw_scroll_delta.y);
            if dy != 0.0 {
                ui.input_mut(|i| {
                    i.raw_scroll_delta = Vec2::ZERO;
                    i.smooth_scroll_delta = Vec2::ZERO;
                });
                // A smooth wheel or touchpad sends many small deltas: collect them into notches
                // (egui turns one wheel "line" into `line_scroll_speed` points).
                let notch = ui.ctx().options(|o| o.line_scroll_speed).max(1.0);
                let id = response.id.with("wheel");
                let notches = ui.data_mut(|d| {
                    let acc = d.get_temp_mut_or_default::<f32>(id);
                    *acc += dy;
                    let n = (*acc / notch).trunc();
                    *acc -= n * notch;
                    n
                });
                if notches != 0.0 {
                    let on_grid = lo + ((value.to_f64() - lo) / step).round() * step;
                    *value = N::from_f64((on_grid + notches as f64 * step).clamp(lo, hi));
                    response.mark_changed();
                }
            }
        }
        response
    }
}

/// Bigger controls for the Score tab, which is used with the mouse while the
/// drummer is sitting some distance from the screen. Applies to the rest of `ui`
/// (call it inside a `ui.scope`).
pub fn roomy(ui: &mut Ui) {
    let style = ui.style_mut();
    for font in style.text_styles.values_mut() {
        font.size = (font.size * 1.25).round();
    }
    let s = &mut style.spacing;
    s.interact_size.y = 30.0;
    s.slider_width = 150.0;
    s.slider_rail_height = 10.0;
    s.icon_width = 20.0;
    s.icon_width_inner = 11.0;
    s.button_padding = vec2(14.0, 6.0);
    s.item_spacing = vec2(12.0, 8.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, MouseWheelUnit, Pos2, RawInput, Rect};

    /// Runs one egui frame with `events`; the same context is reused so layer
    /// and pointer state carry over between frames, as in the app.
    fn frame(ctx: &egui::Context, events: Vec<Event>, ui_fn: impl FnOnce(&mut Ui)) {
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0))), events, ..Default::default() };
        let mut ui_fn = Some(ui_fn);
        let _ = ctx.run(input, |ctx| {
            if let Some(f) = ui_fn.take() {
                egui::CentralPanel::default().show(ctx, f);
            }
        });
    }

    fn wheel_at(pos: Pos2, notches: f32) -> Vec<Event> {
        vec![
            Event::PointerMoved(pos),
            Event::MouseWheel { unit: MouseWheelUnit::Line, delta: vec2(0.0, notches), modifiers: Modifiers::NONE },
        ]
    }

    #[test]
    fn wheel_tunes_a_slider_by_steps_and_stays_in_range() {
        let ctx = egui::Context::default();
        let mut tempo = 100.0_f32;
        let mut centre = Pos2::ZERO;
        frame(&ctx, vec![], |ui| centre = Slider::new("tempo", &mut tempo, 25.0..=150.0).suffix("%").show(ui).rect.center());
        assert_eq!(tempo, 100.0, "no wheel, no change");

        let mut changed = false;
        frame(&ctx, wheel_at(centre, 1.0), |ui| changed = Slider::new("tempo", &mut tempo, 25.0..=150.0).suffix("%").show(ui).changed());
        assert_eq!(tempo, 101.0, "wheel up raises the value");
        assert!(changed, "the response reports the change");
        frame(&ctx, wheel_at(centre, -3.0), |ui| _ = Slider::new("tempo", &mut tempo, 25.0..=150.0).suffix("%").show(ui));
        assert_eq!(tempo, 98.0, "three notches down");
        frame(&ctx, wheel_at(centre, 500.0), |ui| _ = Slider::new("tempo", &mut tempo, 25.0..=150.0).suffix("%").show(ui));
        assert_eq!(tempo, 150.0, "clamped at the top");
    }

    #[test]
    fn wheel_works_for_integer_ranges_and_fractional_steps() {
        let ctx = egui::Context::default();
        let (mut bars, mut volume) = (1_u32, 0.7_f32);
        let (mut bars_at, mut volume_at) = (Pos2::ZERO, Pos2::ZERO);
        let draw = |ui: &mut Ui, bars: &mut u32, volume: &mut f32| {
            let a = Slider::new("count-in bars", bars, 0..=2).show(ui).rect.center();
            let b = Slider::new("volume", volume, 0.0..=1.0).step(0.02).show(ui).rect.center();
            (a, b)
        };
        frame(&ctx, vec![], |ui| (bars_at, volume_at) = draw(ui, &mut bars, &mut volume));
        frame(&ctx, wheel_at(bars_at, 5.0), |ui| _ = draw(ui, &mut bars, &mut volume));
        assert_eq!((bars, volume), (2, 0.7), "only the slider under the pointer changes");
        frame(&ctx, wheel_at(volume_at, -2.0), |ui| _ = draw(ui, &mut bars, &mut volume));
        assert!((volume - 0.66).abs() < 1e-6, "{volume}");
        assert_eq!(bars, 2);
    }

    #[test]
    fn a_zero_label_does_not_change_how_the_wheel_works() {
        let ctx = egui::Context::default();
        let mut bars = 0_usize;
        let mut at = Pos2::ZERO;
        let draw = |ui: &mut Ui, bars: &mut usize| Slider::new("bars per row", bars, 0..=8).zero_text("auto").show(ui).rect.center();
        frame(&ctx, vec![], |ui| at = draw(ui, &mut bars));
        frame(&ctx, wheel_at(at, 3.0), |ui| _ = draw(ui, &mut bars));
        assert_eq!(bars, 3);
        frame(&ctx, wheel_at(at, -9.0), |ui| _ = draw(ui, &mut bars));
        assert_eq!(bars, 0, "back to the zero value, which is shown as \"auto\"");
    }

    #[test]
    fn disabled_sliders_ignore_the_wheel() {
        let ctx = egui::Context::default();
        let mut v = 5_u8;
        let mut at = Pos2::ZERO;
        frame(&ctx, vec![], |ui| ui.add_enabled_ui(false, |ui| at = Slider::new("x", &mut v, 0..=10).show(ui).rect.center()).inner);
        frame(&ctx, wheel_at(at, 3.0), |ui| _ = ui.add_enabled_ui(false, |ui| Slider::new("x", &mut v, 0..=10).show(ui)));
        assert_eq!(v, 5);
    }

    #[test]
    fn a_slider_keeps_the_wheel_from_scrolling_the_page() {
        let ctx = egui::Context::default();
        let mut v = 10_i32;
        let mut at = Pos2::ZERO;
        let page = |ui: &mut Ui, v: &mut i32| -> (Pos2, f32) {
            let out = egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let at = Slider::new("level", v, 0..=100).show(ui).rect.center();
                ui.add_space(3000.0);
                at
            });
            (out.inner, out.state.offset.y)
        };
        frame(&ctx, vec![], |ui| at = page(ui, &mut v).0);

        let mut offset = f32::NAN;
        frame(&ctx, wheel_at(at, -1.0), |ui| offset = page(ui, &mut v).1);
        assert_eq!((v, offset), (9, 0.0), "the slider took the wheel and the page stayed");

        // Away from any slider the same wheel scrolls the page.
        frame(&ctx, wheel_at(Pos2::new(450.0, 400.0), -1.0), |ui| offset = page(ui, &mut v).1);
        assert!(offset > 0.0, "page offset {offset}");
        assert_eq!(v, 9);
    }

    /// Runs one frame with a text box and a button, giving the focus to the one in `focus`.
    fn typing_with_focus_on(focus: &str) -> bool {
        let ctx = egui::Context::default();
        let mut text = String::new();
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let edit = ui.text_edit_singleline(&mut text);
                    let button = ui.button("Record");
                    match focus {
                        "text" => edit.request_focus(),
                        "button" => button.request_focus(),
                        _ => {}
                    }
                });
            });
        }
        typing(&ctx)
    }

    #[test]
    fn only_a_text_box_counts_as_typing() {
        assert!(typing_with_focus_on("text"));
        assert!(!typing_with_focus_on("button"), "a button with the focus does not stop the shortcuts");
        assert!(!typing_with_focus_on("nothing"));
    }

    /// Conventions that a rule of prose did not keep: every slider is a `Slider`, so that the
    /// caption and the wheel behave the same everywhere, and small coloured text is a `note`.
    #[test]
    fn what_this_file_makes_is_not_made_by_hand_elsewhere() {
        const BY_HAND: [(&str, &str); 2] = [("egui::Slider::new", "widgets::Slider"), (".small().color(", "widgets::note")];
        fn walk(dir: &std::path::Path, found: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, found);
                } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("widgets.rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    for (by_hand, instead) in BY_HAND {
                        if text.contains(by_hand) {
                            found.push(format!("{}: use {instead}, not {by_hand}", path.display()));
                        }
                    }
                }
            }
        }
        let mut found = Vec::new();
        walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut found);
        assert!(found.is_empty(), "{found:#?}");
    }
}
