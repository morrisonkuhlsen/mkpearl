//! The rulers around the pages, in centimetres, drawn as in MK Matrix's page layout view: the margins
//! shaded, the writing area light, and the count starting where the writing area starts.

use egui::{Align2, Painter, Rect, Stroke, pos2};

use crate::theme::{self, Tokens};

/// How thick a ruler is on screen.
pub const RULER: f32 = 18.0;
const POINTS_PER_CM: f32 = 72.0 / 2.54;
/// Below this many screen points per centimetre the marks would merge into a block.
const FINEST_CM: f32 = 2.0;
/// Half-centimetre marks are left out when they would be closer than this.
const FINEST_MARK: f32 = 3.5;
/// The room a number needs, which sets how many centimetres go between numbers.
const LABEL_ROOM: f32 = 22.0;

/// One page along a ruler, in screen coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub from: f32,
    pub to: f32,
    pub margin_from: f32,
    pub margin_to: f32,
}

impl Span {
    /// `margins` are the page's distances to the writing area at each end, in points.
    pub fn new(from: f32, to: f32, margins: Option<(f32, f32)>, zoom: f32) -> Self {
        let (before, after) = margins.unwrap_or((0.0, 0.0));
        Self { from, to, margin_from: from + before * zoom, margin_to: to - after * zoom }
    }
}

/// A mark on a ruler: a number, or a line of the given length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    Number(f32, usize),
    Line(f32, f32),
}

/// The marks between `start` and `end`, which are screen coordinates, at this zoom.
pub fn marks(start: f32, end: f32, zoom: f32) -> Vec<Mark> {
    let cm = POINTS_PER_CM * zoom;
    if cm < FINEST_CM || end <= start {
        return Vec::new();
    }
    let every = ((LABEL_ROOM / cm).ceil() as usize).max(1);
    let halves = (((end - start) / cm * 2.0).ceil() as usize).min(4_000);
    let mut out = Vec::new();
    for half in 0..=halves {
        let at = start + half as f32 * cm / 2.0;
        if at > end + 0.5 {
            break;
        }
        let n = half / 2;
        if half % 2 == 1 {
            if cm / 2.0 >= FINEST_MARK {
                out.push(Mark::Line(at, 2.5));
            }
        } else if n > 0 && n.is_multiple_of(every) {
            out.push(Mark::Number(at, n));
        } else {
            out.push(Mark::Line(at, 6.0));
        }
    }
    out
}

/// Paints one ruler. `area` runs along the pages, and the strip keeps to it when they scroll.
pub fn paint(p: &Painter, area: Rect, horizontal: bool, spans: &[Span], zoom: f32, t: &Tokens) {
    let p = p.with_clip_rect(area);
    p.rect_filled(area, 0.0, t.window);
    let band = |a: f32, b: f32| {
        if horizontal {
            Rect::from_min_max(pos2(a, area.top() + 2.0), pos2(b, area.bottom() - 2.0))
        } else {
            Rect::from_min_max(pos2(area.left() + 2.0, a), pos2(area.right() - 2.0, b))
        }
    };
    let font = theme::ui_font(9.5);
    let ink = Stroke::new(1.0, t.header_text.gamma_multiply(0.6));
    for span in spans {
        p.rect_filled(band(span.from, span.to), 0.0, t.header_sel_bg);
        p.rect_filled(band(span.margin_from, span.margin_to), 0.0, t.header_bg);
        for mark in marks(span.margin_from, span.margin_to, zoom) {
            match mark {
                Mark::Number(at, n) => {
                    let centre = if horizontal { pos2(at, area.center().y) } else { pos2(area.center().x, at) };
                    p.text(centre, Align2::CENTER_CENTER, n.to_string(), font.clone(), t.header_text);
                }
                Mark::Line(at, len) => {
                    let ends = if horizontal {
                        [pos2(at, area.bottom() - 2.0 - len), pos2(at, area.bottom() - 2.0)]
                    } else {
                        [pos2(area.right() - 2.0 - len, at), pos2(area.right() - 2.0, at)]
                    };
                    p.line_segment(ends, ink);
                }
            }
        }
    }
    let edge = if horizontal {
        [pos2(area.left(), area.bottom() - 0.5), pos2(area.right(), area.bottom() - 0.5)]
    } else {
        [pos2(area.right() - 0.5, area.top()), pos2(area.right() - 0.5, area.bottom())]
    };
    p.line_segment(edge, Stroke::new(1.0, t.header_line));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(marks: &[Mark]) -> Vec<usize> {
        marks.iter().filter_map(|m| if let Mark::Number(_, n) = m { Some(*n) } else { None }).collect()
    }

    #[test]
    fn a_centimetre_on_the_ruler_is_a_centimetre_on_the_page() {
        let zoom = 1.5;
        let marks = marks(100.0, 100.0 + 5.0 * POINTS_PER_CM * zoom, zoom);
        let Some(Mark::Number(at, 3)) = marks.iter().find(|m| matches!(m, Mark::Number(_, 3))) else { panic!("{marks:?}") };
        assert!((at - (100.0 + 3.0 * POINTS_PER_CM * zoom)).abs() < 0.01);
        assert_eq!(numbers(&marks), [1, 2, 3, 4, 5]);
    }

    #[test]
    fn the_count_starts_at_the_margin_and_stops_at_the_other() {
        let span = Span::new(0.0, 500.0, Some((50.0, 100.0)), 1.0);
        assert_eq!((span.margin_from, span.margin_to), (50.0, 400.0));
        let inside = marks(span.margin_from, span.margin_to, 1.0);
        assert_eq!(inside.first(), Some(&Mark::Line(50.0, 6.0)));
        assert!(inside.iter().all(|m| match m {
            Mark::Number(at, _) | Mark::Line(at, _) => *at >= 50.0 && *at <= 400.5,
        }));
        let unknown = Span::new(0.0, 500.0, None, 1.0);
        assert_eq!((unknown.margin_from, unknown.margin_to), (0.0, 500.0));
    }

    #[test]
    fn small_zooms_thin_the_marks_out() {
        let far = marks(0.0, 3000.0, 0.1);
        let shown = numbers(&far);
        assert!(shown.len() > 1 && shown.windows(2).all(|w| (w[1] - w[0]) as f32 * POINTS_PER_CM * 0.1 >= LABEL_ROOM), "{shown:?}");
        assert!(far.iter().all(|m| !matches!(m, Mark::Line(_, 2.5))), "half marks would merge");
        assert!(marks(0.0, 3000.0, 0.01).is_empty());
        assert!(marks(10.0, 5.0, 1.0).is_empty(), "margins that overlap");
    }
}
