//! What keeps a document from reaching everyone: pictures with no alternative text, heading levels
//! skipped and text too faint to read. It reads as the rest of the watch does, and says nothing when
//! unsure: text over a gradient, a see-through colour or a picture isn't judged.

use std::collections::BTreeMap;
use std::ops::Range;

use typst::foundations::{Content, NativeElement, Selector, Smart, StyleChain};
use typst::introspection::Introspector;
use typst::layout::{Frame, FrameItem, Point, Transform};
use typst::model::{FigureElem, HeadingElem};
use typst::visualize::{Color, Paint};
use typst_layout::PagedDocument;

use super::{EMPTY, Finding, Info, Item, Kind, Reader, Region, around, hashed, union};

/// WCAG 2's minimum contrast for text, and for large text, from this size in points.
const LEAST_CONTRAST: f64 = 4.5;
const LEAST_CONTRAST_LARGE: f64 = 3.0;
const LARGE_TEXT: f64 = 18.0;

/// The contrast between two colours, from 1 to 21, by WCAG 2's formula.
fn contrast(a: [f64; 3], b: [f64; 3]) -> f64 {
    let luminance = |rgb: [f64; 3]| {
        let [r, g, b] = rgb.map(|c| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) });
        0.2126f64.mul_add(r, 0.7152f64.mul_add(g, 0.0722 * b))
    };
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// A colour fully solid, if it's that.
fn solid(paint: &Paint) -> Option<[f64; 3]> {
    let Paint::Solid(color) = paint else { return None };
    if color.alpha().is_some_and(|alpha| alpha < 1.0) {
        return None;
    }
    Some(rgb_of(color))
}

fn rgb_of(color: &Color) -> [f64; 3] {
    let rgb = color.to_rgb();
    [f64::from(rgb.red), f64::from(rgb.green), f64::from(rgb.blue)]
}

/// Something painted under what comes after it. `None` for what can't be judged.
struct Under {
    rect: [f64; 4],
    color: Option<[f64; 3]>,
}

fn contains(rect: &[f64; 4], x: f64, y: f64) -> bool {
    (rect[0]..=rect[2]).contains(&x) && (rect[1]..=rect[3]).contains(&y)
}

/// What's found on a page: pictures, and the faintest text of each passage.
struct Page<'a> {
    reader: &'a Reader<'a>,
    /// Figures that describe their pictures themselves.
    described: &'a [Range<usize>],
    index: usize,
    paper: Option<[f64; 3]>,
    under: Vec<Under>,
    faint: BTreeMap<usize, (f64, f64, usize, [f64; 4])>,
    found: Vec<Finding>,
}

impl Page<'_> {
    fn walk(&mut self, frame: &Frame, ts: Transform) {
        for (pos, item) in frame.items() {
            match item {
                FrameItem::Group(group) => {
                    let inner = ts.pre_concat(Transform::translate(pos.x, pos.y)).pre_concat(group.transform);
                    self.walk(&group.frame, inner);
                }
                FrameItem::Shape(shape, _) => {
                    if let Some(fill) = &shape.fill {
                        let bounds = shape.bbox(false);
                        let (a, b) = ((*pos + bounds.min).transform(ts), (*pos + bounds.max).transform(ts));
                        let rect =
                            [a.x.to_pt().min(b.x.to_pt()), a.y.to_pt().min(b.y.to_pt()), a.x.to_pt().max(b.x.to_pt()), a.y.to_pt().max(b.y.to_pt())];
                        self.under.push(Under { rect, color: solid(fill) });
                    }
                }
                FrameItem::Image(image, size, span) => {
                    let (a, b) = (pos.transform(ts), (*pos + Point::new(size.x, size.y)).transform(ts));
                    let rect =
                        [a.x.to_pt().min(b.x.to_pt()), a.y.to_pt().min(b.y.to_pt()), a.x.to_pt().max(b.x.to_pt()), a.y.to_pt().max(b.y.to_pt())];
                    self.under.push(Under { rect, color: None });
                    if let Some(info) = self.reader.outline.spans.get(span) {
                        self.picture(image.alt().is_some(), *info, rect);
                    }
                }
                FrameItem::Text(text) => {
                    let Some((info, offset)) =
                        text.glyphs.iter().find_map(|g| Some((*self.reader.outline.spans.get(&g.span.0)?, usize::from(g.span.1))))
                    else {
                        continue;
                    };
                    let (size, width) = (text.size.to_pt(), text.width().to_pt());
                    let at = pos.transform(ts);
                    let rect = [at.x.to_pt(), at.y.to_pt() - size * 0.8, at.x.to_pt() + width, at.y.to_pt() + size * 0.25];
                    if info.region != Region::Skip && text.glyphs.iter().any(|g| g.x_advance.get() > 0.0) {
                        self.text(&text.fill, size, info, info.start + offset, rect);
                    }
                }
                _ => {}
            }
        }
    }

    fn picture(&mut self, has_alt: bool, info: Info, rect: [f64; 4]) {
        let inside_described = self.described.iter().any(|range| range.contains(&info.start));
        if has_alt || inside_described || info.region == Region::Skip {
            return;
        }
        let text = self.reader.source.text().get(info.start..info.end).unwrap_or_default();
        self.found.push(around(Kind::NoAltText, self.index, info.start, hashed(6, text), rect));
    }

    fn text(&mut self, fill: &Paint, size: f64, info: Info, byte: usize, rect: [f64; 4]) {
        let (x, y) = (f64::midpoint(rect[0], rect[2]), f64::midpoint(rect[1], rect[3]));
        let background = self.under.iter().rev().find(|under| contains(&under.rect, x, y)).map_or(self.paper, |under| under.color);
        let (Some(ink), Some(background)) = (solid(fill), background) else { return };
        let ratio = contrast(ink, background);
        let least = if size >= LARGE_TEXT { LEAST_CONTRAST_LARGE } else { LEAST_CONTRAST };
        if ratio >= least {
            return;
        }
        let entry = self.faint.entry(info.section).or_insert((ratio, least, byte, EMPTY));
        if ratio < entry.0 {
            (entry.0, entry.1) = (ratio, least);
        }
        entry.2 = entry.2.min(byte);
        entry.3 = union(
            entry.3,
            &Item {
                page: self.index,
                info,
                byte,
                left: rect[0],
                right: rect[2],
                top: rect[1],
                bottom: rect[3],
                baseline: rect[3],
                size,
                turned: false,
                shape: false,
            },
        );
    }
}

fn level_of(heading: &Content) -> Option<usize> {
    Some(heading.to_packed::<HeadingElem>()?.resolve_level(StyleChain::default()).get())
}

fn skipped_headings(doc: &PagedDocument, items: &[Item], reader: &Reader, found: &mut Vec<Finding>) {
    let introspector = doc.introspector();
    let mut last = 0;
    for heading in introspector.query(&Selector::Elem(HeadingElem::ELEM, None)) {
        let Some(info) = reader.outline.spans.get(&heading.span()).filter(|i| i.region != Region::Skip) else { continue };
        let Some(level) = level_of(&heading) else { continue };
        if level > last + 1 {
            let Some(page) = heading.location().and_then(|at| introspector.position(at)).map(|p| p.page.get() - 1) else { continue };
            let rect = items.iter().filter(|i| i.page == page && (info.start..info.end).contains(&i.byte)).fold(EMPTY, union);
            if rect[0].is_finite() {
                found.push(around(Kind::SkippedHeading { from: last, to: level }, page, info.start, reader.key(7, info.section), rect));
            }
        }
        last = level;
    }
}

pub(super) fn find(doc: &PagedDocument, items: &[Item], reader: &Reader, found: &mut Vec<Finding>) {
    let described: Vec<Range<usize>> = doc
        .introspector()
        .query(&Selector::Elem(FigureElem::ELEM, None))
        .iter()
        .filter(|figure| figure.to_packed::<FigureElem>().is_some_and(|f| f.alt.get_cloned(StyleChain::default()).is_some()))
        .filter_map(|figure| reader.outline.spans.get(&figure.span()).map(|info| info.start..info.end))
        .collect();
    for (index, content) in doc.pages().iter().enumerate() {
        // A page with no fill of its own is printed on white, and read on it.
        let paper = match &content.fill {
            Smart::Custom(Some(paint)) => solid(paint),
            _ => Some([1.0, 1.0, 1.0]),
        };
        let mut page = Page { reader, described: &described, index, paper, under: Vec::new(), faint: BTreeMap::new(), found: Vec::new() };
        page.walk(&content.frame, Transform::identity());
        let Page { faint, found: pictures, .. } = page;
        found.extend(pictures);
        for (section, (ratio, least, byte, rect)) in faint {
            // Tenths of a point of contrast, which is all that's shown.
            let kind = Kind::LowContrast { tenths: (ratio * 10.0) as u32, least_tenths: (least * 10.0) as u32 };
            found.push(around(kind, index, byte, reader.key(8, section), rect));
        }
    }
    skipped_headings(doc, items, reader, found);
}

#[cfg(test)]
mod tests {
    use super::super::tests::{found, kinds};
    use super::*;

    const PICTURE: &str =
        "#let picture = bytes(\"<svg xmlns='http://www.w3.org/2000/svg' width='4' height='2'><rect width='4' height='2' fill='teal'/></svg>\")\n";

    fn just(kinds: &[Kind], wanted: fn(&Kind) -> bool) -> usize {
        kinds.iter().filter(|k| wanted(k)).count()
    }

    #[test]
    fn a_picture_with_no_description_is_found() {
        let no_alt = |k: &Kind| matches!(k, Kind::NoAltText);
        assert_eq!(just(&kinds(&format!("{PICTURE}#image(picture, width: 2cm)\n"), 70.0), no_alt), 1);
        assert_eq!(just(&kinds(&format!("{PICTURE}#image(picture, width: 2cm, alt: \"Um ponto\")\n"), 70.0), no_alt), 0);
        assert_eq!(
            just(&kinds(&format!("{PICTURE}#figure(image(picture, width: 2cm), alt: \"Um ponto\")\n"), 70.0), no_alt),
            0,
            "the figure describes it"
        );
        assert_eq!(
            just(&kinds(&format!("{PICTURE}#set page(background: image(picture, width: 2cm))\nTexto.\n"), 70.0), no_alt),
            0,
            "a background is decoration"
        );
    }

    #[test]
    fn a_heading_level_skipped_is_found() {
        let at = |text: &str| kinds(text, 70.0).into_iter().filter(|k| matches!(k, Kind::SkippedHeading { .. })).collect::<Vec<_>>();
        assert_eq!(at("= Um\n== Dois\n=== Três\n== Dois\n= Um\n"), []);
        assert_eq!(at("= Um\n=== Três\n"), [Kind::SkippedHeading { from: 1, to: 3 }]);
        assert_eq!(at("== Dois\n"), [Kind::SkippedHeading { from: 0, to: 2 }], "the first has to be the top one");
        assert_eq!(at("= Um\n=== Três\n=== Três\n"), [Kind::SkippedHeading { from: 1, to: 3 }], "once, where it happens");
        let findings = found("= Um\nTexto.\n=== Três\n", 70.0);
        assert_eq!("= Um\nTexto.\n=== Três\n".get(findings[0].byte..findings[0].byte + 3), Some("==="));
    }

    fn faint(text: &str) -> Vec<Kind> {
        kinds(text, 70.0).into_iter().filter(|k| matches!(k, Kind::LowContrast { .. })).collect()
    }

    #[test]
    fn text_too_faint_is_found_against_what_is_really_under_it() {
        assert_eq!(faint("Texto preto.\n"), []);
        assert_eq!(faint("#text(fill: luma(235))[Quase invisível.]\n"), [Kind::LowContrast { tenths: 11, least_tenths: 45 }]);
        assert_eq!(faint("#text(fill: luma(100))[Cinza médio.]\n").len(), 0, "just over the line");
        assert_eq!(faint("#text(fill: luma(130))[Cinza claro.]\n").len(), 1);
        assert_eq!(faint("#text(fill: luma(130), size: 24pt)[Cinza grande.]\n").len(), 0, "large text may be fainter");
        assert_eq!(faint("#text(fill: white)[Branco no branco.]\n").len(), 1);
        assert_eq!(faint("#block(fill: black, inset: 4pt)[#text(fill: white)[Branco no preto.]]\n"), [], "the box is what it's on");
        assert_eq!(faint("#block(fill: white, inset: 4pt)[#text(fill: white)[Branco no branco.]]\n").len(), 1);
        assert_eq!(faint("#set page(fill: black)\n#text(fill: white)[Branco no preto.]\n"), [], "the paper is what it's on");
        assert_eq!(faint("#set page(fill: black)\nPreto no preto.\n").len(), 1);
    }

    #[test]
    fn what_cant_be_judged_is_left_alone() {
        assert_eq!(faint("#text(fill: gradient.linear(white, black))[Em gradiente.]\n"), []);
        assert_eq!(faint("#text(fill: luma(235).transparentize(50%))[Translúcido.]\n"), []);
        assert_eq!(faint("#block(fill: gradient.linear(white, black))[#text(fill: white)[Sobre gradiente.]]\n"), []);
        assert_eq!(faint("#block(fill: black.transparentize(50%))[#text(fill: white)[Sobre translúcido.]]\n"), []);
        assert_eq!(
            faint(&format!("{PICTURE}#box(width: 3cm, height: 2cm)[#place(image(picture, width: 3cm)) #text(fill: white)[Sobre imagem.]]\n")),
            []
        );
        assert_eq!(faint("#set page(background: text(70pt, fill: luma(240))[RASCUNHO])\nTexto.\n"), [], "a watermark is meant to be faint");
    }

    #[test]
    fn contrast_follows_the_formula() {
        assert!((contrast([0.0; 3], [1.0; 3]) - 21.0).abs() < 0.01);
        assert!((contrast([1.0; 3], [1.0; 3]) - 1.0).abs() < 0.01);
    }
}
