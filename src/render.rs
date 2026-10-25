//! Score tab drawing: for every system (line) the expected part is drawn on
//! a percussion staff and the captured hits on a second staff right under
//! it, on the same time axis, so early/late hits are visibly offset.

use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, Ui};

use crate::analysis::Grade;
use crate::drums::{Lane, NoteHead};
use crate::score::Score;

const LS: f32 = 8.0; // staff line spacing
const STEP: f32 = LS / 2.0;
const EXP_TOP: f32 = 40.0;
const EXP_BOT: f32 = EXP_TOP + 4.0 * LS;
const PLAY_TOP: f32 = EXP_BOT + 50.0;
const PLAY_BOT: f32 = PLAY_TOP + 4.0 * LS;
const VEL_BASE: f32 = PLAY_BOT + 30.0;
const SYS_H: f32 = VEL_BASE + 16.0;
const LEFT: f32 = 58.0;
const MEASURE_PAD: f32 = 12.0;
const STEM: f32 = 3.5 * LS;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HitStatus {
    Perfect,
    Good,
    Ok,
    WrongPad,
    Extra,
    /// Not graded (outside range / not yet evaluated).
    Neutral,
}

#[derive(Clone, Debug)]
pub struct PlayedNote {
    pub q: f64,
    pub lane: Lane,
    pub velocity: u8,
    pub status: HitStatus,
    pub dt_ms: Option<f64>,
}

pub struct ScoreView<'a> {
    pub score: &'a Score,
    /// Per `score.notes` index.
    pub grades: &'a [Option<Grade>],
    pub played: &'a [PlayedNote],
    pub playhead_q: Option<f64>,
    /// Inclusive measure range being practised.
    pub range: (usize, usize),
    pub zoom: f32,
    pub follow: bool,
    pub layout: LayoutOpts,
}

pub struct ViewOutput {
    /// Clicked measure and whether shift was held.
    pub clicked_measure: Option<(usize, bool)>,
    /// A fixed number of bars per row was asked for but does not fit the
    /// width: this many are shown per row instead.
    pub fit_bars: Option<usize>,
}

/// How the bars are split into rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutOpts {
    /// Bars per row; 0 = as many as fit the width at the current zoom.
    pub bars_per_row: usize,
    /// Position (1-based, in the score as shown) of a bar that must start a row; 0 = none.
    pub align_bar: usize,
}

pub const GREEN: Color32 = Color32::from_rgb(0x2e, 0xb8, 0x5c);
pub const LIGHT_GREEN: Color32 = Color32::from_rgb(0x8b, 0xc3, 0x4a);
pub const AMBER: Color32 = Color32::from_rgb(0xf2, 0xb1, 0x34);
pub const ORANGE: Color32 = Color32::from_rgb(0xf2, 0x7a, 0x1d);
pub const RED: Color32 = Color32::from_rgb(0xe0, 0x3e, 0x3e);
pub const ACCENT: Color32 = Color32::from_rgb(0x3d, 0x8b, 0xfd);

pub fn grade_color(g: Grade, neutral: Color32) -> Color32 {
    match g {
        Grade::Perfect => GREEN,
        Grade::Good => LIGHT_GREEN,
        Grade::Ok => AMBER,
        Grade::WrongPad => ORANGE,
        Grade::Missed => RED,
        Grade::Pending => neutral,
    }
}

pub fn status_color(s: HitStatus, neutral: Color32) -> Color32 {
    match s {
        HitStatus::Perfect => GREEN,
        HitStatus::Good => LIGHT_GREEN,
        HitStatus::Ok => AMBER,
        HitStatus::WrongPad => ORANGE,
        HitStatus::Extra => RED,
        HitStatus::Neutral => neutral,
    }
}

struct MeasurePos {
    x0: f32,
    pad: f32,
    w: f32,
    px_per_q: f32,
}

struct Layout {
    systems: Vec<(usize, usize)>, // measure index range [a, b)
    measures: Vec<MeasurePos>,
    /// See `ViewOutput::fit_bars`.
    fit_bars: Option<usize>,
}

/// Narrowest a quarter note is drawn when a fixed number of bars per row is asked for.
const MIN_PPQ: f32 = 28.0;

/// First bar of every row when each row holds `per_row` bars: rows start at
/// the `align` bar (0-based) and every `per_row` bars from it, and the bars
/// before it fill the earlier rows, so the first row may be shorter.
fn row_starts(n: usize, per_row: usize, align: Option<usize>) -> Vec<usize> {
    let per_row = per_row.max(1);
    let anchor = align.filter(|a| *a < n).unwrap_or(0);
    let first = anchor % per_row;
    let mut starts = vec![0];
    let mut s = if first == 0 { per_row } else { first };
    while s < n {
        starts.push(s);
        s += per_row;
    }
    starts
}

/// The bar length (in quarters) most bars of the score have; a pickup bar or
/// a single odd bar does not decide how wide a bar is drawn.
fn typical_len_q(score: &Score) -> f64 {
    let mut counts: Vec<(f64, usize)> = Vec::new();
    for m in &score.measures {
        match counts.iter_mut().find(|(len, _)| (*len - m.len_q).abs() < 1e-6) {
            Some(c) => c.1 += 1,
            None => counts.push((m.len_q, 1)),
        }
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(a.0.total_cmp(&b.0)))
        .map(|c| c.0)
        .unwrap_or(4.0)
        .max(0.25)
}

fn layout(score: &Score, width: f32, zoom: f32, opts: LayoutOpts) -> Layout {
    let n = score.measures.len();
    let line_w = (width - LEFT - 8.0).max(200.0);
    let align = (opts.align_bar >= 1 && opts.align_bar <= n).then(|| opts.align_bar - 1);
    let fixed = opts.bars_per_row > 0;
    let mut fit_bars = None;
    let mut systems: Vec<(usize, usize)> = Vec::new();
    let ppq;
    if n == 0 {
        ppq = 56.0 * zoom;
    } else if fixed {
        // Every bar is drawn at the same width, so barlines line up from row to row.
        let l = typical_len_q(score) as f32;
        let per_quarter = |k: usize| (line_w / k as f32 - 2.0 * MEASURE_PAD) / l;
        let mut k = opts.bars_per_row;
        while k > 1 && per_quarter(k) < MIN_PPQ {
            k -= 1;
        }
        if k < opts.bars_per_row {
            fit_bars = Some(k);
        }
        ppq = per_quarter(k).max(4.0);
        let starts = row_starts(n, k, align);
        for (i, s) in starts.iter().enumerate() {
            systems.push((*s, starts.get(i + 1).copied().unwrap_or(n)));
        }
    } else {
        ppq = 56.0 * zoom;
        let mut start = 0usize;
        let mut w = 0.0f32;
        for (i, m) in score.measures.iter().enumerate() {
            let mw = m.len_q as f32 * ppq + 2.0 * MEASURE_PAD;
            if i > start && (w + mw > line_w || align == Some(i)) {
                systems.push((start, i));
                start = i;
                w = 0.0;
            }
            w += mw;
        }
        systems.push((start, n));
    }
    let mut measures = Vec::with_capacity(n);
    let last = systems.len().saturating_sub(1);
    for (si, (a, b)) in systems.iter().enumerate() {
        let natural: f32 = (*a..*b).map(|i| score.measures[i].len_q as f32 * ppq + 2.0 * MEASURE_PAD).sum();
        let stretch = if fixed {
            // Same width as the other rows; only squeeze a row that is too wide.
            if natural > line_w { line_w / natural } else { 1.0 }
        } else if si < last || natural > 0.75 * line_w {
            // Justify full lines; leave a short last line at natural width.
            line_w / natural
        } else {
            1.0
        };
        let mut x = LEFT;
        for i in *a..*b {
            let m = &score.measures[i];
            let mw = (m.len_q as f32 * ppq + 2.0 * MEASURE_PAD) * stretch;
            measures.push(MeasurePos { x0: x, pad: MEASURE_PAD * stretch, w: mw, px_per_q: ppq * stretch });
            x += mw;
        }
    }
    Layout { systems, measures, fit_bars }
}

impl Layout {
    fn x_of(&self, score: &Score, mi: usize, q: f64) -> f32 {
        let mp = &self.measures[mi];
        mp.x0 + mp.pad + ((q - score.measures[mi].start_q) as f32) * mp.px_per_q
    }
}

fn y_of(step: i32, staff_bottom: f32) -> f32 {
    staff_bottom - step as f32 * STEP
}

fn draw_head(p: &Painter, c: Pos2, head: NoteHead, color: Color32, scale: f32) {
    let rx = 5.0 * scale;
    let ry = 3.6 * scale;
    let stroke = Stroke::new(1.6 * scale.max(0.8), color);
    match head {
        NoteHead::Normal => {
            let (s, co) = (-0.35f32).sin_cos();
            let pts: Vec<Pos2> = (0..16)
                .map(|k| {
                    let a = k as f32 / 16.0 * std::f32::consts::TAU;
                    let (x, y) = (rx * a.cos(), ry * a.sin());
                    pos2(c.x + x * co - y * s, c.y + x * s + y * co)
                })
                .collect();
            p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        NoteHead::X => {
            p.line_segment([c + vec2(-rx * 0.8, -ry), c + vec2(rx * 0.8, ry)], stroke);
            p.line_segment([c + vec2(-rx * 0.8, ry), c + vec2(rx * 0.8, -ry)], stroke);
        }
        NoteHead::CircleX => {
            p.circle_stroke(c, rx * 0.95, Stroke::new(1.2 * scale.max(0.8), color));
            let d = rx * 0.6;
            p.line_segment([c + vec2(-d, -d), c + vec2(d, d)], stroke);
            p.line_segment([c + vec2(-d, d), c + vec2(d, -d)], stroke);
        }
        NoteHead::Diamond => {
            let pts = vec![c + vec2(0.0, -ry * 1.3), c + vec2(rx, 0.0), c + vec2(0.0, ry * 1.3), c + vec2(-rx, 0.0)];
            p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        NoteHead::Triangle => {
            let pts = vec![c + vec2(0.0, -ry * 1.3), c + vec2(rx, ry), c + vec2(-rx, ry)];
            p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
    }
}

fn draw_ledgers(p: &Painter, x: f32, step: i32, staff_bottom: f32, color: Color32) {
    let stroke = Stroke::new(1.0_f32, color);
    let mut s = 10;
    while s <= step {
        let y = y_of(s, staff_bottom);
        p.line_segment([pos2(x - 8.0, y), pos2(x + 8.0, y)], stroke);
        s += 2;
    }
    let mut s = -2;
    while s >= step {
        let y = y_of(s, staff_bottom);
        p.line_segment([pos2(x - 8.0, y), pos2(x + 8.0, y)], stroke);
        s -= 2;
    }
}

fn draw_staff(p: &Painter, x0: f32, x1: f32, top: f32, color: Color32) {
    for k in 0..5 {
        let y = top + k as f32 * LS;
        p.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(1.0_f32, color));
    }
}

fn draw_clef(p: &Painter, x: f32, top: f32, color: Color32) {
    let r1 = Rect::from_min_max(pos2(x, top + LS), pos2(x + 3.0, top + 3.0 * LS));
    let r2 = Rect::from_min_max(pos2(x + 6.0, top + LS), pos2(x + 9.0, top + 3.0 * LS));
    p.rect_filled(r1, 0.0, color);
    p.rect_filled(r2, 0.0, color);
}

pub fn show(ui: &mut Ui, view: &ScoreView) -> ViewOutput {
    let score = view.score;
    let mut out = ViewOutput { clicked_measure: None, fit_bars: None };
    if score.measures.is_empty() {
        ui.label("The score has no measures.");
        return out;
    }
    let width = ui.available_width().max(420.0);
    let lay = layout(score, width, view.zoom, view.layout);
    out.fit_bars = lay.fit_bars;
    let total_h = lay.systems.len() as f32 * SYS_H + 10.0;
    let (response, painter) = ui.allocate_painter(vec2(width, total_h), Sense::click());
    let origin = response.rect.min;
    let clip = painter.clip_rect();

    let text = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    let line_col = ui.visuals().widgets.noninteractive.fg_stroke.color.gamma_multiply(0.7);
    let grid_col = weak.gamma_multiply(0.25);
    let range_bg = ACCENT.gamma_multiply(0.08);

    // Staff positions for played notes: same as the score draws each lane.
    let lane_staff: Vec<(i32, NoteHead)> = Lane::ALL.iter().map(|l| score.staff_for_lane(*l)).collect();
    let lane_idx = |l: Lane| Lane::ALL.iter().position(|x| *x == l).unwrap_or(0);

    let hover = response.hover_pos();
    let mut hover_text: Option<String> = None;
    let mut playhead_rect: Option<Rect> = None;

    // Played notes bucketed by measure.
    let mut played_by_measure: Vec<Vec<usize>> = vec![Vec::new(); score.measures.len()];
    for (i, pn) in view.played.iter().enumerate() {
        if let Some(mi) = score.measure_at(pn.q) {
            let m = &score.measures[mi];
            if pn.q < m.end_q() + m.len_q * 0.5 || mi + 1 < score.measures.len() {
                played_by_measure[mi].push(i);
            }
        } else if pn.q < 0.0 {
            played_by_measure[0].push(i);
        }
    }
    // Notes bucketed by measure.
    let mut notes_by_measure: Vec<Vec<usize>> = vec![Vec::new(); score.measures.len()];
    for (i, n) in score.notes.iter().enumerate() {
        if n.measure < notes_by_measure.len() {
            notes_by_measure[n.measure].push(i);
        }
    }

    for (si, &(a, b)) in lay.systems.iter().enumerate() {
        let y0 = origin.y + si as f32 * SYS_H;
        let sys_rect = Rect::from_min_max(pos2(origin.x, y0), pos2(origin.x + width, y0 + SYS_H));
        let x_end = origin.x + lay.measures[b - 1].x0 + lay.measures[b - 1].w;
        if let Some(q) = view.playhead_q {
            let first = &score.measures[a];
            let last = &score.measures[b - 1];
            if q >= first.start_q && (q < last.end_q() || b == score.measures.len() && q < last.end_q() + 8.0) {
                playhead_rect = Some(sys_rect);
            }
            if si == 0 && q < first.start_q {
                playhead_rect = Some(sys_rect);
            }
        }
        if !sys_rect.intersects(clip) {
            continue;
        }
        let exp_top = y0 + EXP_TOP;
        let exp_bot = y0 + EXP_BOT;
        let play_top = y0 + PLAY_TOP;
        let play_bot = y0 + PLAY_BOT;
        let vel_base = y0 + VEL_BASE;

        // Practice range highlight
        for mi in a..b {
            if mi >= view.range.0 && mi <= view.range.1 {
                let mp = &lay.measures[mi];
                let r = Rect::from_min_max(
                    pos2(origin.x + mp.x0, y0 + 14.0),
                    pos2(origin.x + mp.x0 + mp.w, vel_base + 4.0),
                );
                painter.rect_filled(r, 0.0, range_bg);
            }
        }

        draw_staff(&painter, origin.x + LEFT - 22.0, x_end, exp_top, line_col);
        draw_staff(&painter, origin.x + LEFT - 22.0, x_end, play_top, line_col);
        draw_clef(&painter, origin.x + LEFT - 18.0, exp_top, text);
        draw_clef(&painter, origin.x + LEFT - 18.0, play_top, weak);
        painter.text(pos2(origin.x + 2.0, exp_top + 2.0 * LS), Align2::LEFT_CENTER, "Score", FontId::proportional(11.0), weak);
        painter.text(pos2(origin.x + 2.0, play_top + 2.0 * LS), Align2::LEFT_CENTER, "You", FontId::proportional(11.0), ACCENT);
        painter.text(pos2(origin.x + 2.0, vel_base - 6.0), Align2::LEFT_CENTER, "vel", FontId::proportional(9.0), weak);

        for mi in a..b {
            let m = &score.measures[mi];
            let mp = &lay.measures[mi];
            let mx0 = origin.x + mp.x0;
            let mx1 = mx0 + mp.w;
            let note_x = |q: f64| origin.x + lay.x_of(score, mi, q);

            // measure number
            painter.text(pos2(mx0 + 2.0, y0 + 14.0), Align2::LEFT_TOP, &m.number, FontId::proportional(9.0), weak);
            if mi == 0 || score.measures[mi - 1].beats != m.beats || score.measures[mi - 1].beat_type != m.beat_type {
                painter.text(
                    pos2(mx0 + 14.0, y0 + 14.0),
                    Align2::LEFT_TOP,
                    format!("{}/{}", m.beats, m.beat_type),
                    FontId::proportional(9.0),
                    weak,
                );
            }
            // barlines
            let thick = mi + 1 == score.measures.len();
            for (top, bot) in [(exp_top, exp_bot), (play_top, play_bot)] {
                painter.line_segment([pos2(mx1, top), pos2(mx1, bot)], Stroke::new(if thick { 3.0_f32 } else { 1.0 }, line_col));
            }
            // beat grid under the played staff (where timing is judged)
            let beat_q = m.beat_q();
            let mut bq = 0.0;
            while bq < m.len_q - 1e-6 {
                let x = note_x(m.start_q + bq);
                painter.line_segment([pos2(x, play_top - 6.0), pos2(x, vel_base)], Stroke::new(1.0_f32, grid_col));
                bq += beat_q;
            }

            // expected notes
            for &ni in &notes_by_measure[mi] {
                let n = &score.notes[ni];
                let x = note_x(n.q) - if n.grace { 9.0 } else { 0.0 };
                let y = y_of(n.staff_step, exp_bot);
                let col = view.grades.get(ni).copied().flatten().map(|g| grade_color(g, text)).unwrap_or(text);
                let scale = if n.grace { 0.65 } else { 1.0 };
                draw_ledgers(&painter, x, n.staff_step, exp_bot, line_col);
                draw_head(&painter, pos2(x, y), n.head, col, scale);
                let up = n.stem_up.unwrap_or(!n.lane.is_foot());
                let rx = 5.0 * scale;
                if up {
                    painter.line_segment([pos2(x + rx - 0.5, y), pos2(x + rx - 0.5, y - STEM * scale)], Stroke::new(1.0_f32, col));
                } else {
                    painter.line_segment([pos2(x - rx + 0.5, y), pos2(x - rx + 0.5, y + STEM * scale)], Stroke::new(1.0_f32, col));
                }
                if n.ghost {
                    painter.text(pos2(x - 8.0, y), Align2::CENTER_CENTER, "(", FontId::proportional(11.0), col);
                    painter.text(pos2(x + 8.0, y), Align2::CENTER_CENTER, ")", FontId::proportional(11.0), col);
                }
                if n.accent {
                    let ay = if up { y0 + 28.0 } else { exp_bot + STEM + 10.0 };
                    let s = Stroke::new(1.4_f32, col);
                    painter.line_segment([pos2(x - 4.0, ay - 3.0), pos2(x + 4.0, ay)], s);
                    painter.line_segment([pos2(x + 4.0, ay), pos2(x - 4.0, ay + 3.0)], s);
                }
            }

            // played notes
            for &pi in &played_by_measure[mi] {
                let pn = &view.played[pi];
                let x = note_x(pn.q);
                if x < origin.x + LEFT - 20.0 {
                    continue;
                }
                let (step, head) = lane_staff[lane_idx(pn.lane)];
                let y = y_of(step, play_bot);
                let col = status_color(pn.status, ACCENT);
                draw_ledgers(&painter, x, step, play_bot, line_col);
                draw_head(&painter, pos2(x, y), head, col, 1.0);
                let h = 16.0 * pn.velocity as f32 / 127.0;
                painter.rect_filled(
                    Rect::from_min_max(pos2(x - 1.5, vel_base - h), pos2(x + 1.5, vel_base)),
                    0.0,
                    col,
                );
                if let Some(hp) = hover {
                    if (hp - pos2(x, y)).length() < 7.0 {
                        let timing = match pn.dt_ms {
                            Some(d) if d >= 0.5 => format!("{d:.0} ms late"),
                            Some(d) if d <= -0.5 => format!("{:.0} ms early", -d),
                            Some(_) => "on time".to_string(),
                            None => match pn.status {
                                HitStatus::Extra => "extra hit".to_string(),
                                _ => String::new(),
                            },
                        };
                        hover_text = Some(format!("{} · velocity {} · {}", pn.lane.label(), pn.velocity, timing));
                    }
                }
            }

            if response.clicked() {
                if let Some(pp) = response.interact_pointer_pos() {
                    if pp.x >= mx0 && pp.x < mx1 && pp.y >= y0 && pp.y < y0 + SYS_H {
                        let shift = ui.input(|i| i.modifiers.shift);
                        out.clicked_measure = Some((mi, shift));
                    }
                }
            }
        }

        // playhead
        if let Some(q) = view.playhead_q {
            if let Some(mi) = score.measure_at(q.max(0.0)) {
                if mi >= a && mi < b && q >= 0.0 {
                    let x = origin.x + lay.x_of(score, mi, q);
                    painter.line_segment([pos2(x, y0 + 12.0), pos2(x, vel_base + 2.0)], Stroke::new(2.0_f32, ACCENT));
                }
            }
        }
    }

    if let Some(t) = hover_text {
        let _ = response.on_hover_text_at_pointer(t);
    }
    if view.follow {
        if let Some(r) = playhead_rect {
            ui.scroll_to_rect(r, Some(egui::Align::Center));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::Measure;

    /// A score whose bars have the given lengths in quarter notes.
    fn score_of(lens: &[f64]) -> Score {
        let mut q = 0.0;
        let measures = lens
            .iter()
            .enumerate()
            .map(|(i, len)| {
                let m = Measure { number: (i + 1).to_string(), start_q: q, len_q: *len, beats: *len as u32, beat_type: 4 };
                q += len;
                m
            })
            .collect();
        Score { measures, ..Default::default() }
    }

    const W: f32 = 1300.0;
    const FIXED: fn(usize, usize) -> LayoutOpts = |bars_per_row, align_bar| LayoutOpts { bars_per_row, align_bar };

    fn width_of(lay: &Layout, bar: usize) -> f32 {
        lay.measures[bar].w
    }

    #[test]
    fn row_starts_follow_the_alignment_bar() {
        assert_eq!(row_starts(10, 4, None), [0, 4, 8]);
        assert_eq!(row_starts(10, 4, Some(0)), [0, 4, 8]);
        // Align at the second bar: a one-bar first row, then rows of four from there.
        assert_eq!(row_starts(10, 4, Some(1)), [0, 1, 5, 9]);
        // The same grid whichever bar of it is named, also a later one.
        assert_eq!(row_starts(10, 4, Some(5)), [0, 1, 5, 9]);
        assert_eq!(row_starts(10, 4, Some(2)), [0, 2, 6]);
        assert_eq!(row_starts(10, 1, Some(3)), (0..10).collect::<Vec<_>>(), "one bar per row");
        assert_eq!(row_starts(6, 4, Some(9)), [0, 4], "an alignment bar past the end is ignored");
        assert_eq!(row_starts(3, 4, None), [0], "a score shorter than a row");
        assert_eq!(row_starts(3, 4, Some(2)), [0, 2]);
        assert_eq!(row_starts(5, 0, None), (0..5).collect::<Vec<_>>(), "zero is treated as one");
    }

    #[test]
    fn fixed_bars_per_row_fill_the_width_and_line_up() {
        let score = score_of(&[4.0; 10]);
        let lay = layout(&score, W, 1.0, FIXED(4, 0));
        assert_eq!(lay.systems, [(0, 4), (4, 8), (8, 10)]);
        assert_eq!(lay.fit_bars, None);
        let line_w = W - LEFT - 8.0;
        let row_end = lay.measures[3].x0 + lay.measures[3].w;
        assert!((row_end - (LEFT + line_w)).abs() < 0.01, "a full row fills the line: {row_end}");
        // The short last row has the same bar width and the same barlines as the first.
        assert!((width_of(&lay, 8) - width_of(&lay, 0)).abs() < 0.01);
        assert!((lay.measures[8].x0 - lay.measures[0].x0).abs() < 0.01);
        assert!((lay.measures[9].x0 - lay.measures[1].x0).abs() < 0.01);
        // Window size changes the width of the bars, not how many there are per row.
        let narrow = layout(&score, 900.0, 1.0, FIXED(4, 0));
        assert_eq!(narrow.systems, lay.systems);
        assert!(width_of(&narrow, 0) < width_of(&lay, 0));
    }

    #[test]
    fn zoom_does_not_matter_with_a_fixed_count() {
        let score = score_of(&[4.0; 8]);
        let a = layout(&score, W, 0.5, FIXED(4, 0));
        let b = layout(&score, W, 3.0, FIXED(4, 0));
        assert_eq!(a.systems, b.systems);
        assert!((width_of(&a, 0) - width_of(&b, 0)).abs() < 0.01);
    }

    #[test]
    fn the_alignment_bar_starts_a_row() {
        let score = score_of(&[4.0; 10]);
        let lay = layout(&score, W, 1.0, FIXED(4, 2)); // align at bar 2
        assert_eq!(lay.systems, [(0, 1), (1, 5), (5, 9), (9, 10)]);
        // The lone first bar is not stretched across the row.
        assert!((width_of(&lay, 0) - width_of(&lay, 1)).abs() < 0.01);
        // Aligning at bar 1 is the same as no alignment.
        assert_eq!(layout(&score, W, 1.0, FIXED(4, 1)).systems, layout(&score, W, 1.0, FIXED(4, 0)).systems);
        // Past the last bar nothing changes either.
        assert_eq!(layout(&score, W, 1.0, FIXED(4, 11)).systems, layout(&score, W, 1.0, FIXED(4, 0)).systems);
        // One bar per row ignores the alignment.
        assert_eq!(layout(&score, W, 1.0, FIXED(1, 3)).systems.len(), 10);
    }

    #[test]
    fn automatic_rows_still_break_at_the_alignment_bar() {
        // 12 bars at zoom 1 fit 4 per row in this width (each is 4 * 56 + 24 = 248 px).
        let score = score_of(&[4.0; 12]);
        let auto = layout(&score, 1100.0, 1.0, FIXED(0, 0));
        assert_eq!(auto.systems, [(0, 4), (4, 8), (8, 12)]);
        let aligned = layout(&score, 1100.0, 1.0, FIXED(0, 3));
        assert_eq!(aligned.systems, [(0, 2), (2, 6), (6, 10), (10, 12)]);
        // Rows are still justified to the full width (except a short last one).
        let first_end = aligned.measures[5].x0 + aligned.measures[5].w;
        assert!((first_end - (LEFT + 1100.0 - LEFT - 8.0)).abs() < 0.01);
    }

    #[test]
    fn a_short_last_row_keeps_its_natural_width_in_automatic_mode() {
        let score = score_of(&[4.0; 5]);
        let lay = layout(&score, 1100.0, 1.0, FIXED(0, 0));
        assert_eq!(lay.systems, [(0, 4), (4, 5)]);
        assert!((width_of(&lay, 4) - 248.0).abs() < 0.01, "{}", width_of(&lay, 4));
    }

    #[test]
    fn bars_of_other_lengths_keep_their_proportions() {
        // One 3/4 bar among 4/4 bars: it is drawn narrower, and its row is not stretched to fit.
        let score = score_of(&[4.0, 3.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0]);
        let lay = layout(&score, W, 1.0, FIXED(4, 0));
        assert_eq!(lay.systems, [(0, 4), (4, 8)]);
        assert!(width_of(&lay, 1) < width_of(&lay, 0));
        let first_row: f32 = (0..4).map(|i| width_of(&lay, i)).sum();
        let second_row: f32 = (4..8).map(|i| width_of(&lay, i)).sum();
        assert!(first_row < second_row, "{first_row} vs {second_row}");
        // A pickup bar does not make the typical bar short.
        assert_eq!(typical_len_q(&score_of(&[1.0, 4.0, 4.0, 4.0])), 4.0);
        // A row that would be too wide because of a long bar is squeezed to the line.
        let long = score_of(&[4.0, 4.0, 4.0, 8.0, 4.0, 4.0, 4.0, 4.0]);
        let lay = layout(&long, W, 1.0, FIXED(4, 0));
        let row: f32 = (0..4).map(|i| width_of(&lay, i)).sum();
        assert!((row - (W - LEFT - 8.0)).abs() < 0.01, "{row}");
    }

    #[test]
    fn too_many_bars_for_the_width_fall_back_to_fewer_and_say_so() {
        let score = score_of(&[4.0; 16]);
        let lay = layout(&score, 700.0, 1.0, FIXED(8, 0));
        let fit = lay.fit_bars.expect("8 bars do not fit 700 px");
        assert!(fit < 8 && fit >= 1);
        assert!(lay.systems.iter().all(|(a, b)| b - a <= fit));
        // Every quarter note keeps at least the minimum width.
        assert!(lay.measures[0].px_per_q >= MIN_PPQ - 0.01, "{}", lay.measures[0].px_per_q);
        // Plenty of room: what was asked for is shown.
        assert_eq!(layout(&score, 2400.0, 1.0, FIXED(8, 0)).fit_bars, None);
    }

    #[test]
    fn degenerate_scores_do_not_panic() {
        let empty = Score::default();
        for opts in [FIXED(0, 0), FIXED(4, 0), FIXED(4, 2)] {
            let lay = layout(&empty, W, 1.0, opts);
            assert!(lay.systems.is_empty() && lay.measures.is_empty());
        }
        let one = score_of(&[4.0]);
        for opts in [FIXED(0, 0), FIXED(4, 1), FIXED(1, 1), FIXED(0, 5)] {
            let lay = layout(&one, 300.0, 1.0, opts);
            assert_eq!(lay.systems, [(0, 1)]);
            assert_eq!(lay.measures.len(), 1);
        }
    }

    #[test]
    fn every_bar_is_in_exactly_one_row() {
        let score = score_of(&[4.0, 3.0, 4.0, 2.0, 4.0, 4.0, 5.0, 4.0, 4.0, 4.0, 4.0]);
        for per_row in [0, 1, 2, 3, 4, 7] {
            for align in [0, 1, 2, 5, 11, 12] {
                let lay = layout(&score, 1000.0, 1.0, FIXED(per_row, align));
                let mut next = 0;
                for (a, b) in &lay.systems {
                    assert_eq!(*a, next, "rows are consecutive ({per_row}, {align})");
                    assert!(b > a);
                    next = *b;
                }
                assert_eq!(next, score.measures.len());
                assert_eq!(lay.measures.len(), score.measures.len());
            }
        }
    }
}
