//! Fitting a document in a number of pages: the smallest changes to its spacing, margins and type size
//! that get it there. The words are never touched, and nothing goes past limits that keep it readable.

use crate::edit::{self, Sel};
use crate::i18n::{Lang, current, tx};
use crate::tr;

/// Below these, a page is no longer pleasant to read. What's already below them stays as it is.
const LEAST_SIZE: f64 = 9.0;
const LEAST_LEADING: f64 = 0.5;
const LEAST_SPACING: f64 = 0.8;
const LEAST_MARGIN: f64 = 1.5;
/// In the order a reader notices them: spacing first, then margins, and type size last. Each step keeps
/// what the one before it took: a factor for the spacing, one for the margins, and points off the type.
const STEPS: [(f64, f64, f64); 10] = [
    (0.97, 1.0, 0.0),
    (0.94, 1.0, 0.0),
    (0.90, 1.0, 0.0),
    (0.90, 0.95, 0.0),
    (0.90, 0.90, 0.0),
    (0.90, 0.85, 0.0),
    (0.90, 0.85, 0.25),
    (0.90, 0.85, 0.5),
    (0.90, 0.85, 0.75),
    (0.90, 0.85, 1.0),
];
const POINTS_PER_CM: f64 = 72.0 / 2.54;

/// What the document has now, as measured from its layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Current {
    pub size: f64,
    /// In ems.
    pub leading: f64,
    /// In ems.
    pub spacing: f64,
    /// Left, right, top and bottom in centimetres, where they could be measured.
    pub margins: Option<[f64; 4]>,
}

impl Current {
    /// From what the probes reported, in points. Anything senseless is hostile input, and means no.
    pub fn measured(size: f64, leading: f64, spacing: f64, margins: Option<[f32; 4]>) -> Option<Self> {
        let good = |n: f64| n.is_finite() && n > 0.0;
        if !(good(size) && good(leading) && good(spacing)) {
            return None;
        }
        Some(Self { size, leading: leading / size, spacing: spacing / size, margins: margins.map(|m| m.map(|side| f64::from(side) / POINTS_PER_CM)) })
    }
}

/// What changes. What's `None` stays as the document has it.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Plan {
    pub size: Option<f64>,
    pub leading: Option<f64>,
    pub spacing: Option<f64>,
    pub margins: Option<[f64; 4]>,
}

fn rounded(value: f64, unit: f64) -> f64 {
    (value / unit).round() * unit
}

/// A smaller value, not below its limit nor above where it began.
fn reduced(now: f64, wanted: f64, least: f64, unit: f64) -> Option<f64> {
    let value = rounded(wanted.max(least), unit);
    (value < now - unit / 2.0).then_some(value)
}

/// The plans to try, smallest first, without repeats.
pub fn ladder(now: &Current) -> Vec<Plan> {
    let mut plans: Vec<Plan> = Vec::new();
    for (space, margin, type_off) in STEPS {
        let margins = now.margins.and_then(|sides| {
            let new = sides.map(|side| reduced(side, side * margin, LEAST_MARGIN, 0.05).unwrap_or(side));
            (new != sides).then_some(new)
        });
        let plan = Plan {
            size: (type_off > 0.0).then(|| reduced(now.size, now.size - type_off, LEAST_SIZE, 0.25)).flatten(),
            leading: reduced(now.leading, now.leading * space, LEAST_LEADING, 0.01),
            spacing: reduced(now.spacing, now.spacing * space, LEAST_SPACING, 0.01),
            margins,
        };
        if plan != Plan::default() && plans.last() != Some(&plan) {
            plans.push(plan);
        }
    }
    plans
}

fn number(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

impl Plan {
    /// Writes the plan in the source, keeping the selection on the same letters.
    pub fn apply(&self, text: &mut String, mut sel: Sel) -> Sel {
        if let Some(sides) = self.margins {
            let [left, right, top, bottom] = sides.map(number);
            let value = if left == right && right == top && top == bottom {
                format!("{left}cm")
            } else {
                format!("(left: {left}cm, right: {right}cm, top: {top}cm, bottom: {bottom}cm)")
            };
            sel = edit::set_page_margin(text, sel, &value);
        }
        if let Some(size) = self.size {
            sel = edit::set_argument(text, sel, "text", "size", &format!("{}pt", number(size)));
        }
        if let Some(leading) = self.leading {
            sel = edit::set_argument(text, sel, "par", "leading", &format!("{}em", number(leading)));
        }
        if let Some(spacing) = self.spacing {
            sel = edit::set_argument(text, sel, "par", "spacing", &format!("{}em", number(spacing)));
        }
        sel
    }

    pub fn applied_to(&self, text: &str) -> String {
        let mut out = text.to_string();
        self.apply(&mut out, (0, 0));
        out
    }

    /// Each change in words, before and after, in the language on show.
    pub fn describe(&self, now: &Current) -> Vec<String> {
        let shown = |value: f64| {
            let text = number(value);
            if current() == Lang::PtBr { text.replace('.', ",") } else { text }
        };
        let line = |what: &str, before: String, after: String| tr!("fit-change", what = what, before = before, after = after);
        let sides = |all: [f64; 4]| {
            if all.iter().all(|side| (side - all[0]).abs() < 0.005) {
                format!("{} cm", shown(all[0]))
            } else {
                format!("{} / {} / {} / {} cm", shown(all[0]), shown(all[1]), shown(all[2]), shown(all[3]))
            }
        };
        let mut lines = Vec::new();
        if let Some(leading) = self.leading {
            lines.push(line(tx("line-spacing"), format!("{} em", shown(now.leading)), format!("{} em", shown(leading))));
        }
        if let Some(spacing) = self.spacing {
            lines.push(line(tx("fit-paragraph-spacing"), format!("{} em", shown(now.spacing)), format!("{} em", shown(spacing))));
        }
        if let (Some(after), Some(before)) = (self.margins, now.margins) {
            lines.push(line(tx("page-margins"), sides(before), sides(after)));
        }
        if let Some(size) = self.size {
            lines.push(line(tx("fit-type-size"), format!("{} pt", shown(now.size)), format!("{} pt", shown(size))));
        }
        lines
    }
}

/// How a search ended.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ending {
    /// `plan` is the smallest that was found to reach the target.
    Reached,
    /// Even the last plan doesn't; `plan` is it.
    OutOfReach,
    /// It ran out of time; `plan` is the best so far, if any.
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Found {
    pub ending: Ending,
    /// The plan's place in the ladder, and the pages it gives.
    pub plan: Option<(usize, usize)>,
}

/// Looks for the first plan that reaches `target`. More change never gives more pages in any way that
/// matters, so halving saves most of the compiles; what's returned was compiled and counted all the same.
/// `pages_of` compiles the plan at a place in the ladder.
pub fn search<E>(
    plans: usize,
    target: usize,
    mut pages_of: impl FnMut(usize) -> Result<usize, E>,
    mut keep_going: impl FnMut() -> bool,
) -> Result<Found, E> {
    let Some(last) = plans.checked_sub(1) else { return Ok(Found { ending: Ending::OutOfReach, plan: None }) };
    if !keep_going() {
        return Ok(Found { ending: Ending::Stopped, plan: None });
    }
    let furthest = pages_of(last)?;
    if furthest > target {
        return Ok(Found { ending: Ending::OutOfReach, plan: Some((last, furthest)) });
    }
    let mut best = (last, furthest);
    let (mut low, mut high) = (0, last);
    while low < high {
        if !keep_going() {
            return Ok(Found { ending: Ending::Stopped, plan: Some(best) });
        }
        let middle = low + (high - low) / 2;
        let pages = pages_of(middle)?;
        if pages <= target {
            best = (middle, pages);
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Ok(Found { ending: Ending::Reached, plan: Some(best) })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: Current = Current { size: 11.0, leading: 0.65, spacing: 1.2, margins: Some([2.5, 2.5, 2.5, 2.5]) };

    #[test]
    fn the_ladder_goes_from_spacing_to_margins_to_type() {
        let plans = ladder(&NOW);
        assert_eq!(plans.first().map(|p| (p.margins, p.size)), Some((None, None)), "spacing alone first");
        assert!(plans.first().unwrap().leading.unwrap() < 0.65);
        let first_margin = plans.iter().position(|p| p.margins.is_some()).unwrap();
        let first_size = plans.iter().position(|p| p.size.is_some()).unwrap();
        assert!(first_margin > 0 && first_size > first_margin, "{plans:?}");
        assert_eq!(plans.last().unwrap().size, Some(10.0));
    }

    #[test]
    fn nothing_goes_past_its_limit_or_up() {
        let tight = Current { size: 9.0, leading: 0.5, spacing: 0.8, margins: Some([1.0, 1.5, 1.5, 1.5]) };
        assert_eq!(ladder(&tight), [], "already at the limits, there's nothing to take");
        let near = Current { size: 9.5, leading: 0.52, spacing: 0.82, margins: Some([1.55, 1.0, 3.0, 3.0]) };
        for plan in ladder(&near) {
            assert!(plan.size.is_none_or(|s| s >= LEAST_SIZE));
            assert!(plan.leading.is_none_or(|l| l >= LEAST_LEADING));
            assert!(plan.spacing.is_none_or(|s| s >= LEAST_SPACING));
            let sides = plan.margins.unwrap_or(near.margins.unwrap());
            assert_eq!(sides[1], 1.0, "a margin already below the limit stays");
            assert!(sides[0] >= LEAST_MARGIN && sides[2] <= 3.0);
        }
    }

    #[test]
    fn without_its_margins_measured_it_leaves_them_alone() {
        let plans = ladder(&Current { margins: None, ..NOW });
        assert!(!plans.is_empty() && plans.iter().all(|p| p.margins.is_none()));
    }

    #[test]
    fn senseless_measures_are_refused() {
        for bad in [f64::NAN, f64::INFINITY, 0.0, -3.0] {
            assert_eq!(Current::measured(bad, 7.0, 13.0, None), None);
            assert_eq!(Current::measured(11.0, bad, 13.0, None), None);
            assert_eq!(Current::measured(11.0, 7.0, bad, None), None);
        }
        let good = Current::measured(10.0, 6.5, 12.0, None).unwrap();
        assert_eq!((good.leading, good.spacing), (0.65, 1.2));
    }

    #[test]
    fn a_plan_changes_what_the_document_says_and_adds_what_it_doesnt() {
        let plan = Plan { size: Some(10.75), leading: Some(0.6), spacing: Some(1.1), margins: Some([2.25, 2.25, 2.25, 2.25]) };
        let text = "#set page(paper: \"a4\", margin: 2.5cm)\n#set text(lang: \"pt\", size: 11pt)\n= Título\nTexto.\n";
        let done = plan.applied_to(text);
        assert!(done.contains("#set page(paper: \"a4\", margin: 2.25cm)"), "{done}");
        assert!(done.contains("#set text(lang: \"pt\", size: 10.75pt)"), "{done}");
        assert!(done.contains("#set par(leading: 0.6em)") && done.contains("#set par(spacing: 1.1em)"), "{done}");
        assert!(done.ends_with("= Título\nTexto.\n"), "the words stay");
        let uneven = Plan { margins: Some([3.0, 2.0, 3.0, 2.0]), ..Plan::default() };
        assert!(uneven.applied_to("").contains("(left: 3cm, right: 2cm, top: 3cm, bottom: 2cm)"));
    }

    /// Pages by place in a ladder of ten: twelve at the start, down to eight.
    fn pages(place: usize) -> usize {
        12 - (place * 4 / 9)
    }

    #[test]
    fn the_search_finds_the_first_plan_that_reaches_it_in_few_compiles() {
        for target in 8..=12 {
            let mut compiles = 0;
            let found = search(
                10,
                target,
                |place| -> Result<usize, ()> {
                    compiles += 1;
                    Ok(pages(place))
                },
                || true,
            )
            .unwrap();
            let want = (0..10).find(|place| pages(*place) <= target).unwrap();
            assert_eq!(found, Found { ending: Ending::Reached, plan: Some((want, pages(want))) });
            assert!(compiles <= 5, "{compiles}");
        }
    }

    #[test]
    fn out_of_reach_says_how_close_it_got() {
        let found = search(10, 3, |place| -> Result<usize, ()> { Ok(pages(place)) }, || true).unwrap();
        assert_eq!(found, Found { ending: Ending::OutOfReach, plan: Some((9, 8)) });
        assert_eq!(search(0, 3, |_| -> Result<usize, ()> { Ok(1) }, || true).unwrap(), Found { ending: Ending::OutOfReach, plan: None });
    }

    #[test]
    fn running_out_of_time_keeps_the_best_so_far() {
        let mut left = 2;
        let mut asks = move || {
            left -= 1;
            left >= 0
        };
        let found = search(10, 9, |place| -> Result<usize, ()> { Ok(pages(place)) }, &mut asks).unwrap();
        assert_eq!(found.ending, Ending::Stopped);
        assert!(found.plan.is_some_and(|(_, pages)| pages <= 9));
        assert_eq!(search(10, 9, |_| -> Result<usize, ()> { Ok(1) }, || false).unwrap(), Found { ending: Ending::Stopped, plan: None });
    }

    #[test]
    fn a_failed_compile_ends_the_search() {
        assert_eq!(search(10, 9, |_| Err::<usize, _>("broken"), || true), Err("broken"));
    }
}
