//! Score tab drawing: for every system (line) the expected part is drawn on
//! a percussion staff and the captured hits on a second staff right under
//! it, on the same time axis, so early/late hits are visibly offset.

use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, Ui};

use crate::analysis::Grade;
use crate::drums::{Lane, NoteHead};
use crate::notation::{self, Beam, Notation};
use crate::score::{Hand, Score, ScoreNote};

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
const BEAM_W: f32 = 3.0;
const BEAM_GAP: f32 = 4.5;
const FLAG_GAP: f32 = 4.5;
const STUB: f32 = 7.0;

/// Where the stem of a note at `x` stands: right of the head going up, left of it going down.
fn stem_x(x: f32, up: bool, scale: f32) -> f32 {
    let rx = 5.0 * scale;
    if up {
        x + rx - 0.5
    } else {
        x - rx + 0.5
    }
}

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

    /// Where the playhead is at `q` in measure `mi`. It is where a note at `q` is drawn, except
    /// at the end of a bar that is followed by another in the same row: a bar has a padding
    /// at both ends, so the first note of the next bar is two paddings further on than the end
    /// of this one. Instead of jumping over them at the barline, the playhead speeds up during
    /// the last stretch of the bar (about two paddings long) and arrives at that note on time.
    fn playhead_x(&self, score: &Score, mi: usize, q: f64, next_in_row: bool) -> f32 {
        let aligned = self.x_of(score, mi, q);
        if !next_in_row {
            return aligned;
        }
        let (mp, m) = (&self.measures[mi], &score.measures[mi]);
        // The stretch, in quarters; never more than a quarter of the bar.
        let stretch = ((2.0 * mp.pad / mp.px_per_q) as f64).min(m.len_q / 4.0).max(1e-6);
        let from_q = m.end_q() - stretch;
        if q <= from_q {
            return aligned;
        }
        let start = self.x_of(score, mi, from_q);
        let arrival = self.measures[mi + 1].x0 + self.measures[mi + 1].pad;
        let t = ((q - from_q) / stretch).clamp(0.0, 1.0) as f32;
        start + t * (arrival - start)
    }
}

fn y_of(step: i32, staff_bottom: f32) -> f32 {
    staff_bottom - step as f32 * STEP
}

/// How far left of its main note a grace note is drawn.
fn grace_dx(n: &ScoreNote) -> f32 {
    if n.grace {
        -9.0 * f32::from(n.grace_order.max(1))
    } else {
        0.0
    }
}

/// A single staff that scrolls past a fixed "now" line, for playing along
/// with a click: the practice range repeats endlessly.
pub struct RollingView<'a> {
    pub score: &'a Score,
    /// Inclusive measure range that repeats.
    pub range: (usize, usize),
    /// Where "now" is, in quarters from the start of the score, within the range.
    pub pos_q: f64,
}

/// Height of the rolling strip.
pub const ROLL_H: f32 = EXP_BOT + STEM + 22.0;
/// Pixels per quarter note in the rolling strip.
const ROLL_PPQ: f32 = 56.0;

/// Which repeats of a loop `[start, end)` have any part inside `[left_q, right_q]`.
fn loop_repeats(start: f64, end: f64, left_q: f64, right_q: f64) -> std::ops::RangeInclusive<i64> {
    let len = (end - start).max(1e-6);
    (((left_q - end) / len).ceil() as i64)..=(((right_q - start) / len).floor() as i64)
}

pub fn rolling(ui: &mut Ui, view: &RollingView) {
    let score = view.score;
    let last = score.measures.len().saturating_sub(1);
    if score.measures.is_empty() {
        return;
    }
    let (a, b) = (view.range.0.min(last), view.range.1.clamp(view.range.0.min(last), last));
    let (start, end) = (score.measures[a].start_q, score.measures[b].end_q());
    let len = end - start;
    let width = ui.available_width().max(300.0);
    let (response, painter) = ui.allocate_painter(vec2(width, ROLL_H), Sense::hover());
    let rect = response.rect;
    let text = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    let line_col = ui.visuals().widgets.noninteractive.fg_stroke.color.gamma_multiply(0.7);
    let grid_col = weak.gamma_multiply(0.3);
    if len <= 0.0 {
        return;
    }

    let (x_min, x_max) = (rect.left() + LEFT, rect.right() - 6.0);
    let now_x = (rect.left() + width * 0.22).max(x_min + 24.0);
    let x_of = |q: f64| now_x + ((q - view.pos_q) as f32) * ROLL_PPQ;
    let (left_q, right_q) = (view.pos_q - f64::from((now_x - x_min) / ROLL_PPQ), view.pos_q + f64::from((x_max - now_x) / ROLL_PPQ));
    let top = rect.top() + EXP_TOP;
    let bot = rect.top() + EXP_BOT;

    let canvas = Canvas { painter: &painter, text, weak, line: line_col, grid: grid_col };
    canvas.staff(rect.left() + 4.0, rect.right(), top);
    canvas.clef(rect.left() + 8.0, top, text);
    // Notes are only drawn right of the clef.
    let clipped = painter.with_clip_rect(Rect::from_min_max(pos2(x_min, rect.top()), pos2(x_max, rect.bottom())));
    let notes = Canvas { painter: &clipped, text, weak, line: line_col, grid: grid_col };

    for k in loop_repeats(start, end, left_q, right_q) {
        let shift = k as f64 * len;
        for mi in a..=b {
            let m = &score.measures[mi];
            let mut bq = 0.0;
            while bq < m.len_q - 1e-6 {
                let x = x_of(m.start_q + bq + shift);
                let first = bq == 0.0;
                clipped.line_segment([pos2(x, top), pos2(x, bot)], Stroke::new(if first { 1.4_f32 } else { 1.0 }, if first { line_col } else { grid_col }));
                if first {
                    clipped.text(pos2(x + 3.0, top - 14.0), Align2::LEFT_TOP, &m.number, FontId::proportional(9.0), weak);
                }
                bq += m.beat_q();
            }
        }
        for n in score.notes.iter().filter(|n| n.measure >= a && n.measure <= b) {
            let x = x_of(n.q + shift) + grace_dx(n);
            if x < x_min - 12.0 || x > x_max + 12.0 {
                continue;
            }
            let col = if x < now_x - 4.0 { weak } else { text };
            notes.written_note(n, x, bot, rect.top() + 28.0, Ink { note: col, hand: col, hand_dy: 16.0 }, StemEnd::default());
        }
    }
    painter.line_segment([pos2(now_x, rect.top() + 12.0), pos2(now_x, bot + STEM + 4.0)], Stroke::new(2.0_f32, ACCENT));
}

/// The painter and the colours a score is drawn with.
struct Canvas<'a> {
    painter: &'a Painter,
    text: Color32,
    weak: Color32,
    line: Color32,
    grid: Color32,
}

/// How the stem of a written note ends: at a beam (`tip`, the y of the far end of the stem) or
/// with `flags` flags; both none for a plain stem.
#[derive(Clone, Copy, Default)]
struct StemEnd {
    tip: Option<f32>,
    flags: u8,
}

/// What colours a written note is drawn in, and how far under the staff its hand letter goes.
struct Ink {
    note: Color32,
    hand: Color32,
    hand_dy: f32,
}

impl Canvas<'_> {
    fn head(&self, c: Pos2, head: NoteHead, color: Color32, scale: f32) {
        let p = self.painter;
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

    /// Ledger lines for a head at `step` of the staff that ends at `staff_bottom`.
    fn ledgers(&self, x: f32, step: i32, staff_bottom: f32) {
        let p = self.painter;
        let stroke = Stroke::new(1.0_f32, self.line);
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

    fn staff(&self, x0: f32, x1: f32, top: f32) {
        let (p, color) = (self.painter, self.line);
        for k in 0..5 {
            let y = top + k as f32 * LS;
            p.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(1.0_f32, color));
        }
    }

    fn clef(&self, x: f32, top: f32, color: Color32) {
        let p = self.painter;
        let r1 = Rect::from_min_max(pos2(x, top + LS), pos2(x + 3.0, top + 3.0 * LS));
        let r2 = Rect::from_min_max(pos2(x + 6.0, top + LS), pos2(x + 9.0, top + 3.0 * LS));
        p.rect_filled(r1, 0.0, color);
        p.rect_filled(r2, 0.0, color);
    }

    /// The letter of the hand that plays a note, under it (the grace notes' ones are small): right
    /// in the text colour, left in the accent colour.
    fn hand(&self, x: f32, y: f32, n: &ScoreNote, text: Color32) {
        let p = self.painter;
        let Some(hand) = n.hand else { return };
        let (size, color) = match (n.grace, hand) {
            (true, _) => (8.0, text.gamma_multiply(0.6)),
            (false, Hand::Right) => (11.0, text),
            (false, Hand::Left) => (11.0, ACCENT),
        };
        let letter = if n.grace { hand.letter().to_lowercase() } else { hand.letter().to_string() };
        p.text(pos2(x, y), Align2::CENTER_CENTER, letter, FontId::proportional(size), color);
    }

    /// The beams of a group of stems, level at `tip`; further beams go towards the heads. A beam
    /// that only one note has is a stub, to the right for the first note and to the left for the others.
    fn beam(&self, beam: &Beam, xs: &[f32], tip: f32, color: Color32) {
        let dir = if beam.up { 1.0 } else { -1.0 };
        let levels: Vec<u8> = beam.onsets.iter().map(|o| o.level).collect();
        for k in 1..=levels.iter().copied().max().unwrap_or(1) {
            let y = tip + dir * f32::from(k - 1) * BEAM_GAP;
            let mut i = 0;
            while i < levels.len() {
                if levels[i] < k {
                    i += 1;
                    continue;
                }
                let mut j = i;
                while j + 1 < levels.len() && levels[j + 1] >= k {
                    j += 1;
                }
                let (x0, x1) = match (i == j, i == 0) {
                    (false, _) => (xs[i], xs[j]),
                    (true, true) => (xs[i], xs[i] + STUB),
                    (true, false) => (xs[i] - STUB, xs[i]),
                };
                self.beam_bar(x0, x1, y, beam.up, color);
                i = j + 1;
            }
        }
    }

    /// One thick bar of a beam from `x0` to `x1`, its outer edge at `y`, growing towards the heads.
    fn beam_bar(&self, x0: f32, x1: f32, y: f32, up: bool, color: Color32) {
        let (a, b) = if up { (y, y + BEAM_W) } else { (y - BEAM_W, y) };
        self.painter.add(Shape::convex_polygon(vec![pos2(x0, a), pos2(x1, a), pos2(x1, b), pos2(x0, b)], color, Stroke::NONE));
    }

    /// The bracket of a tuplet over stems that end at `tip`, with the number in a gap in the middle.
    fn bracket(&self, x0: f32, x1: f32, tip: f32, up: bool, number: u8) {
        let p = self.painter;
        let stroke = Stroke::new(1.0_f32, self.weak);
        let (y, hook) = if up { (tip - 8.0, 4.0) } else { (tip + 8.0, -4.0) };
        let (mid, gap) = ((x0 + x1) / 2.0, 7.0);
        p.line_segment([pos2(x0, y + hook), pos2(x0, y)], stroke);
        p.line_segment([pos2(x0, y), pos2(mid - gap, y)], stroke);
        p.line_segment([pos2(mid + gap, y), pos2(x1, y)], stroke);
        p.line_segment([pos2(x1, y), pos2(x1, y + hook)], stroke);
        p.text(pos2(mid, y), Align2::CENTER_CENTER, number.to_string(), FontId::proportional(10.0), self.weak);
    }

    /// A written note: head, ledger lines, hand letter, stem, ghost brackets and accent. `x` already
    /// includes the grace offset; `accent_up_y` is where the accent goes above a stem that points up.
    fn written_note(&self, n: &ScoreNote, x: f32, staff_bottom: f32, accent_up_y: f32, ink: Ink, end: StemEnd) {
        let p = self.painter;
        let (col, scale) = (ink.note, if n.grace { 0.65 } else { 1.0 });
        let y = y_of(n.staff_step, staff_bottom);
        self.ledgers(x, n.staff_step, staff_bottom);
        self.head(pos2(x, y), n.head, col, scale);
        self.hand(x, staff_bottom + ink.hand_dy, n, ink.hand);
        let up = n.stem_is_up();
        let sx = stem_x(x, up, scale);
        let sy = end.tip.unwrap_or(if up { y - STEM * scale } else { y + STEM * scale });
        p.line_segment([pos2(sx, y), pos2(sx, sy)], Stroke::new(1.0_f32, col));
        for k in 0..end.flags {
            // A flag curls to the right of the stem, towards the heads.
            let dy = if up { 1.0 } else { -1.0 };
            let y0 = sy + dy * f32::from(k) * FLAG_GAP;
            let curl = vec![pos2(sx, y0), pos2(sx + 3.5, y0 + 3.0 * dy), pos2(sx + 6.5, y0 + 8.0 * dy), pos2(sx + 4.5, y0 + 12.0 * dy)];
            p.add(Shape::line(curl, Stroke::new(1.6_f32, col)));
        }
        if n.ghost {
            p.text(pos2(x - 8.0, y), Align2::CENTER_CENTER, "(", FontId::proportional(11.0), col);
            p.text(pos2(x + 8.0, y), Align2::CENTER_CENTER, ")", FontId::proportional(11.0), col);
        }
        if n.accent {
            let ay = if up { accent_up_y } else { staff_bottom + STEM + 10.0 };
            let s = Stroke::new(1.4_f32, col);
            p.line_segment([pos2(x - 4.0, ay - 3.0), pos2(x + 4.0, ay)], s);
            p.line_segment([pos2(x + 4.0, ay), pos2(x - 4.0, ay + 3.0)], s);
        }
    }
}

/// What the drawing of the systems needs to know about the score being shown.
struct Frame<'a> {
    view: &'a ScoreView<'a>,
    lay: &'a Layout,
    cv: Canvas<'a>,
    /// Top left corner of the first system.
    origin: Pos2,
    hover: Option<Pos2>,
    /// Staff position and head of each instrument, in the order of `Lane::ALL`: where the played
    /// notes go, the same as the score draws each lane.
    lane_staff: Vec<(i32, NoteHead)>,
    notes_by_measure: Vec<Vec<usize>>,
    played_by_measure: Vec<Vec<usize>>,
    notation: Notation,
    /// Per note: where its stem ends when a beam holds it, as a y relative to the bottom line of its
    /// staff (so it holds for every row).
    stem_tips: Vec<Option<f32>>,
    beams_by_measure: Vec<Vec<usize>>,
    tuplets_by_measure: Vec<Vec<usize>>,
}

impl Frame<'_> {
    fn x_of(&self, measure: usize, q: f64) -> f32 {
        self.origin.x + self.lay.x_of(self.view.score, measure, q)
    }
}

/// The vertical positions of the rows of the system that starts at `y0`.
struct Rows {
    y0: f32,
    exp_top: f32,
    exp_bot: f32,
    play_top: f32,
    play_bot: f32,
    vel_base: f32,
}

impl Rows {
    fn at(y0: f32) -> Rows {
        Rows { y0, exp_top: y0 + EXP_TOP, exp_bot: y0 + EXP_BOT, play_top: y0 + PLAY_TOP, play_bot: y0 + PLAY_BOT, vel_base: y0 + VEL_BASE }
    }
}

/// The played notes of each measure (one played before the start belongs to the first, one long
/// after the end to none).
fn played_by_measure(score: &Score, played: &[PlayedNote]) -> Vec<Vec<usize>> {
    let mut by_measure: Vec<Vec<usize>> = vec![Vec::new(); score.measures.len()];
    for (i, pn) in played.iter().enumerate() {
        if let Some(mi) = score.measure_at(pn.q) {
            let m = &score.measures[mi];
            if pn.q < m.end_q() + m.len_q * 0.5 || mi + 1 < score.measures.len() {
                by_measure[mi].push(i);
            }
        } else if pn.q < 0.0 {
            by_measure[0].push(i);
        }
    }
    by_measure
}

/// For each measure, the positions in a list whose measures are `measures`.
fn indexed_by_measure(score: &Score, measures: impl Iterator<Item = usize>) -> Vec<Vec<usize>> {
    let mut by_measure = vec![Vec::new(); score.measures.len()];
    for (i, m) in measures.enumerate() {
        if let Some(slot) = by_measure.get_mut(m) {
            slot.push(i);
        }
    }
    by_measure
}

/// The y of the end of each beamed note's stem, relative to the bottom line of the staff: a beam is
/// level, at the end of the longest stem of its group, and the other stems grow to meet it.
fn stem_tips(score: &Score, notation: &Notation) -> Vec<Option<f32>> {
    let mut tips = vec![None; score.notes.len()];
    for beam in &notation.beams {
        let notes: Vec<usize> = beam.onsets.iter().flat_map(|o| o.notes.iter().copied()).collect();
        let ends = notes.iter().map(|&i| y_of(score.notes[i].staff_step, 0.0) + if beam.up { -STEM } else { STEM });
        let tip = if beam.up { ends.fold(f32::MAX, f32::min) } else { ends.fold(f32::MIN, f32::max) };
        for i in notes {
            tips[i] = Some(tip);
        }
    }
    tips
}

fn notes_by_measure(score: &Score) -> Vec<Vec<usize>> {
    let mut by_measure: Vec<Vec<usize>> = vec![Vec::new(); score.measures.len()];
    for (i, n) in score.notes.iter().enumerate() {
        if n.measure < by_measure.len() {
            by_measure[n.measure].push(i);
        }
    }
    by_measure
}

/// Whether the playhead is in the system of measures `first..end` (the first system also holds it
/// during the count-in, the last one a little after the end).
fn holds_playhead(view: &ScoreView, system: usize, first: usize, end: usize) -> bool {
    let Some(q) = view.playhead_q else { return false };
    let score = view.score;
    let (first, last) = (&score.measures[first], &score.measures[end - 1]);
    let inside = q >= first.start_q && (q < last.end_q() || end == score.measures.len() && q < last.end_q() + 8.0);
    inside || (system == 0 && q < first.start_q)
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
    let weak = ui.visuals().weak_text_color();
    let notation = notation::analyse(score);
    let stem_tips = stem_tips(score, &notation);
    let beams_by_measure = indexed_by_measure(score, notation.beams.iter().map(|b| b.measure));
    let tuplets_by_measure = indexed_by_measure(score, notation.tuplets.iter().map(|t| t.measure));
    let frame = Frame {
        view,
        lay: &lay,
        cv: Canvas {
            painter: &painter,
            text: ui.visuals().text_color(),
            weak,
            line: ui.visuals().widgets.noninteractive.fg_stroke.color.gamma_multiply(0.7),
            grid: weak.gamma_multiply(0.25),
        },
        origin: response.rect.min,
        hover: response.hover_pos(),
        lane_staff: Lane::ALL.iter().map(|l| score.staff_for_lane(*l)).collect(),
        notes_by_measure: notes_by_measure(score),
        played_by_measure: played_by_measure(score, view.played),
        notation,
        stem_tips,
        beams_by_measure,
        tuplets_by_measure,
    };
    let clip = painter.clip_rect();
    let mut hover_text: Option<String> = None;
    let mut playhead_rect: Option<Rect> = None;

    for (si, &(a, b)) in lay.systems.iter().enumerate() {
        let rows = Rows::at(frame.origin.y + si as f32 * SYS_H);
        let sys_rect = Rect::from_min_max(pos2(frame.origin.x, rows.y0), pos2(frame.origin.x + width, rows.y0 + SYS_H));
        if holds_playhead(view, si, a, b) {
            playhead_rect = Some(sys_rect);
        }
        if !sys_rect.intersects(clip) {
            continue;
        }
        draw_system(&frame, &rows, (a, b), &mut hover_text);
        if response.clicked() {
            if let Some(pp) = response.interact_pointer_pos() {
                if pp.y >= rows.y0 && pp.y < rows.y0 + SYS_H {
                    let at = (a..b).find(|&mi| pp.x >= frame.origin.x + lay.measures[mi].x0 && pp.x < frame.origin.x + lay.measures[mi].x0 + lay.measures[mi].w);
                    if let Some(mi) = at {
                        out.clicked_measure = Some((mi, ui.input(|i| i.modifiers.shift)));
                    }
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

/// One system: the range highlight, both staves, the measures `bars.0..bars.1` and the playhead.
fn draw_system(f: &Frame, rows: &Rows, bars: (usize, usize), hover_text: &mut Option<String>) {
    let (a, b) = bars;
    let (cv, view, lay) = (&f.cv, f.view, f.lay);
    let painter = cv.painter;
    let x_end = f.origin.x + lay.measures[b - 1].x0 + lay.measures[b - 1].w;

    // Practice range highlight
    for mi in a..b {
        if mi >= view.range.0 && mi <= view.range.1 {
            let mp = &lay.measures[mi];
            let r = Rect::from_min_max(pos2(f.origin.x + mp.x0, rows.y0 + 14.0), pos2(f.origin.x + mp.x0 + mp.w, rows.vel_base + 4.0));
            painter.rect_filled(r, 0.0, ACCENT.gamma_multiply(0.08));
        }
    }

    cv.staff(f.origin.x + LEFT - 22.0, x_end, rows.exp_top);
    cv.staff(f.origin.x + LEFT - 22.0, x_end, rows.play_top);
    cv.clef(f.origin.x + LEFT - 18.0, rows.exp_top, cv.text);
    cv.clef(f.origin.x + LEFT - 18.0, rows.play_top, cv.weak);
    painter.text(pos2(f.origin.x + 2.0, rows.exp_top + 2.0 * LS), Align2::LEFT_CENTER, "Score", FontId::proportional(11.0), cv.weak);
    painter.text(pos2(f.origin.x + 2.0, rows.play_top + 2.0 * LS), Align2::LEFT_CENTER, "You", FontId::proportional(11.0), ACCENT);
    painter.text(pos2(f.origin.x + 2.0, rows.vel_base - 6.0), Align2::LEFT_CENTER, "vel", FontId::proportional(9.0), cv.weak);

    for mi in a..b {
        draw_measure_marks(f, rows, mi);
        for &ni in &f.notes_by_measure[mi] {
            draw_score_note(f, rows, mi, ni);
        }
        for &bi in &f.beams_by_measure[mi] {
            draw_beam(f, rows, &f.notation.beams[bi]);
        }
        for &ti in &f.tuplets_by_measure[mi] {
            draw_tuplet(f, rows, &f.notation.tuplets[ti]);
        }
        for &pi in &f.played_by_measure[mi] {
            draw_played_note(f, rows, mi, pi, hover_text);
        }
    }

    // playhead
    if let Some(q) = view.playhead_q {
        if let Some(mi) = view.score.measure_at(q.max(0.0)) {
            if mi >= a && mi < b && q >= 0.0 {
                let x = f.origin.x + lay.playhead_x(view.score, mi, q, mi + 1 < b);
                painter.line_segment([pos2(x, rows.y0 + 12.0), pos2(x, rows.vel_base + 2.0)], Stroke::new(2.0_f32, ACCENT));
            }
        }
    }
}

/// The measure number, a changed time signature, the barline and the beat grid of measure `mi`.
fn draw_measure_marks(f: &Frame, rows: &Rows, mi: usize) {
    let (cv, score) = (&f.cv, f.view.score);
    let painter = cv.painter;
    let m = &score.measures[mi];
    let mp = &f.lay.measures[mi];
    let mx0 = f.origin.x + mp.x0;
    let mx1 = mx0 + mp.w;

    painter.text(pos2(mx0 + 2.0, rows.y0 + 14.0), Align2::LEFT_TOP, &m.number, FontId::proportional(9.0), cv.weak);
    if mi == 0 || score.measures[mi - 1].beats != m.beats || score.measures[mi - 1].beat_type != m.beat_type {
        painter.text(pos2(mx0 + 14.0, rows.y0 + 14.0), Align2::LEFT_TOP, format!("{}/{}", m.beats, m.beat_type), FontId::proportional(9.0), cv.weak);
    }
    // barlines
    let thick = mi + 1 == score.measures.len();
    for (top, bot) in [(rows.exp_top, rows.exp_bot), (rows.play_top, rows.play_bot)] {
        painter.line_segment([pos2(mx1, top), pos2(mx1, bot)], Stroke::new(if thick { 3.0_f32 } else { 1.0 }, cv.line));
    }
    // beat grid under the played staff (where timing is judged)
    let beat_q = m.beat_q();
    let mut bq = 0.0;
    while bq < m.len_q - 1e-6 {
        let x = f.x_of(mi, m.start_q + bq);
        painter.line_segment([pos2(x, rows.play_top - 6.0), pos2(x, rows.vel_base)], Stroke::new(1.0_f32, cv.grid));
        bq += beat_q;
    }
}

/// An expected note (`score.notes[ni]` of measure `mi`): head, ledger lines, hand letter, stem,
/// ghost brackets and accent, in the colour of its grade.
fn draw_score_note(f: &Frame, rows: &Rows, mi: usize, ni: usize) {
    let (cv, view) = (&f.cv, f.view);
    let n = &view.score.notes[ni];
    let x = f.x_of(mi, n.q) + grace_dx(n);
    let col = view.grades.get(ni).copied().flatten().map(|g| grade_color(g, cv.text)).unwrap_or(cv.text);
    let end = StemEnd { tip: f.stem_tips[ni].map(|t| rows.exp_bot + t), flags: f.notation.flags[ni] };
    cv.written_note(n, x, rows.exp_bot, rows.y0 + 28.0, Ink { note: col, hand: cv.text, hand_dy: 18.0 }, end);
}

/// The beams over a group of stems (the stems are drawn with their notes, up to the beam).
fn draw_beam(f: &Frame, rows: &Rows, beam: &Beam) {
    let Some(tip) = beam.onsets.first().and_then(|o| f.stem_tips[o.notes[0]]) else { return };
    let xs: Vec<f32> = beam.onsets.iter().map(|o| stem_x(f.x_of(beam.measure, o.q), beam.up, 1.0)).collect();
    f.cv.beam(beam, &xs, rows.exp_bot + tip, f.cv.text);
}

/// The bracket and number over a tuplet, above (or under) the ends of its stems.
fn draw_tuplet(f: &Frame, rows: &Rows, t: &notation::Tuplet) {
    let notes = &f.view.score.notes;
    let tips = t.notes.iter().map(|&i| f.stem_tips[i].unwrap_or_else(|| y_of(notes[i].staff_step, 0.0) + if t.up { -STEM } else { STEM }));
    let tip = if t.up { tips.fold(f32::MAX, f32::min) } else { tips.fold(f32::MIN, f32::max) };
    let (x0, x1) = (f.x_of(t.measure, t.first_q), f.x_of(t.measure, t.last_q));
    f.cv.bracket(x0 - 5.0, x1 + 5.0, rows.exp_bot + tip, t.up, t.number);
}

/// A played note (`view.played[pi]` of measure `mi`) on the lower staff, with its velocity bar;
/// the text for the tooltip goes into `hover_text` when the pointer is on it.
fn draw_played_note(f: &Frame, rows: &Rows, mi: usize, pi: usize, hover_text: &mut Option<String>) {
    let (cv, view) = (&f.cv, f.view);
    let painter = cv.painter;
    let pn = &view.played[pi];
    let x = f.x_of(mi, pn.q);
    if x < f.origin.x + LEFT - 20.0 {
        return;
    }
    let lane = Lane::ALL.iter().position(|x| *x == pn.lane).unwrap_or(0);
    let (step, head) = f.lane_staff[lane];
    let y = y_of(step, rows.play_bot);
    let col = status_color(pn.status, ACCENT);
    cv.ledgers(x, step, rows.play_bot);
    cv.head(pos2(x, y), head, col, 1.0);
    let h = 16.0 * pn.velocity as f32 / 127.0;
    painter.rect_filled(Rect::from_min_max(pos2(x - 1.5, rows.vel_base - h), pos2(x + 1.5, rows.vel_base)), 0.0, col);
    if let Some(hp) = f.hover {
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
            *hover_text = Some(format!("{} · velocity {} · {}", pn.lane.label(), pn.velocity, timing));
        }
    }
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
    fn the_playhead_crosses_a_barline_without_a_jump() {
        let score = score_of(&[4.0, 4.0, 4.0]);
        let lay = layout(&score, W, 1.0, FIXED(0, 0));
        assert_eq!(lay.systems, vec![(0, 3)], "one row");
        let x = |mi: usize, q: f64, next: bool| lay.playhead_x(&score, mi, q, next);
        // In most of the bar it is where the notes are drawn.
        assert_eq!(x(0, 1.0, true), lay.x_of(&score, 0, 1.0));
        // At the end of the bar it has arrived where the next bar's first note is.
        let arrival = lay.x_of(&score, 1, 4.0);
        assert!((x(0, 4.0 - 1e-9, true) - arrival).abs() < 0.01, "{} {arrival}", x(0, 4.0 - 1e-9, true));
        // And it never goes backwards, or further than a few pixels per step.
        let mut last = x(0, 0.0, true);
        let mut biggest = 0.0f32;
        let mut q = 0.0;
        while q < 3.99 {
            q += 0.01;
            let now = x(0, q, true);
            assert!(now >= last - 1e-4, "moved back at {q}");
            biggest = biggest.max(now - last);
            last = now;
        }
        assert!(biggest < 1.5, "the biggest step is {biggest} px for 0.01 of a quarter");
        // The last bar of a row (or of the score) has no next bar to arrive at.
        assert_eq!(x(2, 3.99, false), lay.x_of(&score, 2, 3.99));
    }

    #[test]
    fn the_rolling_strip_repeats_a_loop_across_its_whole_width() {
        // A 4-quarter loop starting at 8: a window of 20 quarters needs six copies (-1 to 4).
        let copies: Vec<i64> = loop_repeats(8.0, 12.0, 6.0, 26.0).collect();
        assert_eq!(copies.first(), Some(&-1));
        assert_eq!(copies.last(), Some(&4));
        // Every quarter of the window is covered by some copy.
        for q in [6.0, 7.9, 8.0, 15.5, 26.0] {
            assert!(
                copies.iter().any(|k| {
                    let (s, e) = (8.0 + 4.0 * *k as f64, 12.0 + 4.0 * *k as f64);
                    q >= s && q <= e
                }),
                "{q} is covered"
            );
        }
        // Looking at the loop itself needs that copy.
        assert!(loop_repeats(0.0, 16.0, 2.0, 5.0).contains(&0));
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
