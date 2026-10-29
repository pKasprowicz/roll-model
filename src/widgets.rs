//! Small UI helpers shared by the tabs.

use std::ops::RangeInclusive;

use egui::emath::Numeric;
use egui::{vec2, Response, RichText, Ui, Vec2};

/// A line of grey helper text (the "small" style): how every page explains itself.
pub fn hint(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.label(RichText::new(text).small().weak())
}

/// A slider with its `caption` in front of it. The control also follows the
/// mouse wheel while the pointer is over the caption, the slider or the value
/// box: one notch moves the value by `step` (and snaps it to the `step` grid
/// that starts at the low end of the range). The wheel is then not passed on,
/// so a scrolling page stays put while a slider is being tuned.
pub fn slider<N: Numeric>(ui: &mut Ui, caption: &str, value: &mut N, range: RangeInclusive<N>, suffix: &str, step: f64) -> Response {
    slider_with_zero(ui, caption, value, range, suffix, step, None)
}

/// Like [`slider`], but a value of zero is shown as `zero_text` ("auto", "off").
pub fn slider_with_zero<N: Numeric>(
    ui: &mut Ui,
    caption: &str,
    value: &mut N,
    range: RangeInclusive<N>,
    suffix: &str,
    step: f64,
    zero_text: Option<&str>,
) -> Response {
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
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0))),
            events,
            ..Default::default()
        };
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
        frame(&ctx, vec![], |ui| centre = slider(ui, "tempo", &mut tempo, 25.0..=150.0, "%", 1.0).rect.center());
        assert_eq!(tempo, 100.0, "no wheel, no change");

        let mut changed = false;
        frame(&ctx, wheel_at(centre, 1.0), |ui| changed = slider(ui, "tempo", &mut tempo, 25.0..=150.0, "%", 1.0).changed());
        assert_eq!(tempo, 101.0, "wheel up raises the value");
        assert!(changed, "the response reports the change");
        frame(&ctx, wheel_at(centre, -3.0), |ui| _ = slider(ui, "tempo", &mut tempo, 25.0..=150.0, "%", 1.0));
        assert_eq!(tempo, 98.0, "three notches down");
        frame(&ctx, wheel_at(centre, 500.0), |ui| _ = slider(ui, "tempo", &mut tempo, 25.0..=150.0, "%", 1.0));
        assert_eq!(tempo, 150.0, "clamped at the top");
    }

    #[test]
    fn wheel_works_for_integer_ranges_and_fractional_steps() {
        let ctx = egui::Context::default();
        let (mut bars, mut volume) = (1_u32, 0.7_f32);
        let (mut bars_at, mut volume_at) = (Pos2::ZERO, Pos2::ZERO);
        let draw = |ui: &mut Ui, bars: &mut u32, volume: &mut f32| {
            let a = slider(ui, "count-in bars", bars, 0..=2, "", 1.0).rect.center();
            let b = slider(ui, "volume", volume, 0.0..=1.0, "", 0.02).rect.center();
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
        let draw = |ui: &mut Ui, bars: &mut usize| slider_with_zero(ui, "bars per row", bars, 0..=8, "", 1.0, Some("auto")).rect.center();
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
        frame(&ctx, vec![], |ui| ui.add_enabled_ui(false, |ui| at = slider(ui, "x", &mut v, 0..=10, "", 1.0).rect.center()).inner);
        frame(&ctx, wheel_at(at, 3.0), |ui| _ = ui.add_enabled_ui(false, |ui| slider(ui, "x", &mut v, 0..=10, "", 1.0)));
        assert_eq!(v, 5);
    }

    #[test]
    fn a_slider_keeps_the_wheel_from_scrolling_the_page() {
        let ctx = egui::Context::default();
        let mut v = 10_i32;
        let mut at = Pos2::ZERO;
        let page = |ui: &mut Ui, v: &mut i32| -> (Pos2, f32) {
            let out = egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let at = slider(ui, "level", v, 0..=100, "", 1.0).rect.center();
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
}
