//! Pages in dark tones, for the preview in the dark theme. What changes is a copy of the laid-out page,
//! its text and shapes: pictures keep their colours, and the document and its PDF know nothing of this.

use typst::foundations::Smart;
use typst::layout::{Frame, FrameItem};
use typst::visualize::{Color, FixedStroke, Paint};
use typst_layout::Page;

/// Where white ends up: a graphite that doesn't glare, like the sheet in the dark theme.
const PAPER: [u8; 3] = [0x1E, 0x21, 0x27];
/// Where black ends up: light, but short of white.
const INK: [u8; 3] = [0xE6, 0xE8, 0xEC];
/// What text must reach against the paper, as the interface's own text does.
const MIN_CONTRAST: f32 = 4.5;
/// How much of a mid or light fill's brightness stays, so that the light text over it can be read.
const FILL_BRIGHTNESS: f32 = 0.5;

/// Light and dark trade places and the hue stays: dark red becomes light red, black the ink, white the paper.
fn flipped([r, g, b]: [u8; 3]) -> [u8; 3] {
    let (r, g, b) = (i16::from(r), i16::from(g), i16::from(b));
    let shift = 255 - r.max(g).max(b) - r.min(g).min(b);
    let on_paper = |channel: i16, at: usize| {
        let share = f32::from((channel + shift).clamp(0, 255)) / 255.0;
        let (paper, ink) = (f32::from(PAPER.get(at).copied().unwrap_or(0)), f32::from(INK.get(at).copied().unwrap_or(255)));
        (paper + share * (ink - paper)).round() as u8
    };
    [on_paper(r, 0), on_paper(g, 1), on_paper(b, 2)]
}

/// How bright a colour reads, as WCAG measures it.
fn luminance(rgb: [u8; 3]) -> f32 {
    let linear = |channel: u8| {
        let c = f32::from(channel) / 255.0;
        if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let [r, g, b] = rgb;
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

fn contrast_on_paper(rgb: [u8; 3]) -> f32 {
    (luminance(rgb) + 0.05) / (luminance(PAPER) + 0.05)
}

/// Lifted towards the ink until it can be read on the paper: strong blue or red are dark on graphite.
fn readable(rgb: [u8; 3]) -> [u8; 3] {
    const STEPS: u8 = 20;
    let mut out = rgb;
    for step in 0..=STEPS {
        let share = f32::from(step) / f32::from(STEPS);
        let mix = |at: usize| {
            let (from, to) = (f32::from(rgb.get(at).copied().unwrap_or(0)), f32::from(INK.get(at).copied().unwrap_or(255)));
            (from + (to - from) * share).round() as u8
        };
        out = [mix(0), mix(1), mix(2)];
        if contrast_on_paper(out) >= MIN_CONTRAST {
            break;
        }
    }
    out
}

/// A colour for text and lines.
fn ink_colour(colour: &Color) -> Color {
    let [r, g, b, a] = colour.to_vec4_u8();
    let [r, g, b] = readable(flipped([r, g, b]));
    Color::from_u8(r, g, b, a)
}

/// A colour for what text may lie on. Dark fills go light like ink; the others mustn't stay bright,
/// or a yellow highlight would end up with light text on it.
fn fill_colour(colour: &Color) -> Color {
    let [r, g, b, a] = colour.to_vec4_u8();
    let lightness = (u16::from(r.max(g).max(b)) + u16::from(r.min(g).min(b))) / 2;
    let [fr, fg, fb] = flipped([r, g, b]);
    if lightness < 100 {
        return Color::from_u8(fr, fg, fb, a);
    }
    let dimmed = |channel: u8, at: usize| {
        let paper = f32::from(PAPER.get(at).copied().unwrap_or(0));
        (paper + (f32::from(channel) - paper).max(0.0) * FILL_BRIGHTNESS).round() as u8
    };
    Color::from_u8(dimmed(fr, 0), dimmed(fg, 1), dimmed(fb, 2), a)
}

/// Gradients and tilings go as they are: they're closer to pictures than to ink.
fn paint(paint: &Paint, change: fn(&Color) -> Color) -> Paint {
    match paint {
        Paint::Solid(colour) => Paint::Solid(change(colour)),
        other => other.clone(),
    }
}

fn stroke(stroke: &FixedStroke) -> FixedStroke {
    FixedStroke { paint: paint(&stroke.paint, ink_colour), ..stroke.clone() }
}

fn frame(source: &Frame) -> Frame {
    let mut out = Frame::new(source.size(), source.kind());
    for (at, item) in source.items() {
        let item = match item {
            FrameItem::Group(group) => {
                let mut group = group.clone();
                group.frame = frame(&group.frame);
                FrameItem::Group(group)
            }
            FrameItem::Text(text) => {
                let mut text = text.clone();
                text.fill = paint(&text.fill, ink_colour);
                text.stroke = text.stroke.as_ref().map(stroke);
                FrameItem::Text(text)
            }
            FrameItem::Shape(shape, span) => {
                let mut shape = shape.clone();
                shape.fill = shape.fill.as_ref().map(|fill| paint(fill, fill_colour));
                shape.stroke = shape.stroke.as_ref().map(stroke);
                FrameItem::Shape(shape, *span)
            }
            other => other.clone(),
        };
        out.push(*at, item);
    }
    out
}

/// The copy of `page` to show on dark paper.
pub fn page(page: &Page) -> Page {
    let mut dark = page.clone();
    dark.frame = frame(&page.frame);
    let paper = Paint::Solid(Color::from_u8(PAPER[0], PAPER[1], PAPER[2], 255));
    dark.fill = match &page.fill {
        Smart::Custom(Some(fill)) => Smart::Custom(Some(self::paint(fill, fill_colour))),
        _ => Smart::Custom(Some(paper)),
    };
    dark
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_and_black_trade_places_on_the_paper_and_the_ink() {
        assert_eq!(flipped([255, 255, 255]), PAPER);
        assert_eq!(flipped([0, 0, 0]), INK);
    }

    #[test]
    fn a_hue_stays_what_it_was() {
        let [r, g, b] = flipped([128, 0, 0]);
        assert!(r > g + 60 && g.abs_diff(b) < 12, "dark red turns light red: {r} {g} {b}");
        let [r, g, b] = flipped([0, 0, 200]);
        assert!(b > r + 60 && b > g + 60, "blue stays blue: {r} {g} {b}");
    }

    #[test]
    fn every_colour_lands_between_paper_and_ink() {
        for r in (0..=255).step_by(15) {
            for g in (0..=255).step_by(15) {
                for b in (0..=255).step_by(15) {
                    let out = flipped([r, g, b]);
                    for at in 0..3 {
                        assert!(out[at] >= PAPER[at] && out[at] <= INK[at], "{r} {g} {b}: {out:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn coloured_text_can_be_read_and_keeps_its_hue() {
        for rgb in [[0x25, 0x63, 0xEB], [0xC0, 0x39, 0x2B], [255, 0, 0], [0, 0, 255], [128, 128, 128], [0, 128, 0], [255, 255, 255]] {
            let [r, g, b, _] = ink_colour(&Color::from_u8(rgb[0], rgb[1], rgb[2], 255)).to_vec4_u8();
            assert!(contrast_on_paper([r, g, b]) >= MIN_CONTRAST, "{rgb:?} became {r} {g} {b}");
        }
        let [r, g, b, _] = ink_colour(&Color::from_u8(0, 0, 255, 255)).to_vec4_u8();
        assert!(b > r && b > g, "blue stays blue: {r} {g} {b}");
    }

    #[test]
    fn a_bright_fill_is_dimmed_so_light_text_reads_on_it() {
        let [r, g, _, a] = fill_colour(&Color::from_u8(255, 255, 0, 200)).to_vec4_u8();
        assert!(r < 140 && g < 140, "yellow: {r} {g}");
        assert_eq!(a, 200, "the opacity stays");
        // A black rule or box stays visible on the paper.
        assert_eq!(fill_colour(&Color::from_u8(0, 0, 0, 255)).to_vec4_u8(), [INK[0], INK[1], INK[2], 255]);
    }
}
