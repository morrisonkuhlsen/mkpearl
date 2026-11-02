//! The typography watch: what reads badly on the finished pages, from the layout the compiler already
//! made. Widows and orphans, headings left alone at the end of a page, things past the margin and
//! figures far from their first mention. It runs after each compile, on the compiler's thread.
//!
//! It says nothing when unsure: only the document's own running text counts, so a header, a watermark
//! or anything placed on purpose is left alone. A false alarm costs more than a missed one.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use typst::foundations::{NativeElement, Selector};
use typst::introspection::Introspector;
use typst::layout::{Frame, FrameItem, Point, Transform};
use typst::model::{FigureElem, HeadingElem, RefElem};
use typst::syntax::{Source, Span, SyntaxKind, SyntaxNode};
use typst_layout::PagedDocument;

use crate::tr;

mod accessibility;

/// How far past the margin counts, in points: punctuation hangs a little into it on purpose.
const MARGIN_TOLERANCE: f64 = 4.0;
/// Pages between a figure and its first mention that count as far.
const FAR_PAGES: usize = 2;
/// A heading in the upper half of its page is a title with a page of its own, not one left behind.
const LOW_ON_THE_PAGE: f64 = 0.5;
/// A gap this many times the usual line step is a new paragraph.
const PARAGRAPH_GAP: f64 = 1.4;
/// A horizontal gap in a line this many times its size means columns, which aren't read here.
const COLUMN_GAP: f64 = 1.0;
/// So do lines starting this far apart, as a part of the width they share.
const COLUMN_START: f64 = 0.35;
/// A line this much shorter than the longest on its page ended its paragraph, whatever the source says.
const SHORT_LINE: f64 = 0.6;
const POINTS_PER_MM: f64 = 72.0 / 25.4;
/// More would only hide the first ones.
const MAX_FINDINGS: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A paragraph's last line alone at the top of a page.
    Widow,
    /// A paragraph's first line alone at the bottom of a page.
    Orphan,
    StrandedHeading,
    PastMargin {
        mm: u32,
    },
    /// `mention` is the page of the first mention, counted from zero.
    FarFigure {
        mention: usize,
    },
    NoAltText,
    /// `from` is zero when it's the document's first heading.
    SkippedHeading {
        from: usize,
        to: usize,
    },
    /// In tenths, as they're shown.
    LowContrast {
        tenths: u32,
        least_tenths: u32,
    },
}

impl Kind {
    /// Whether this keeps the document from reaching everyone, rather than reading badly.
    pub fn is_accessibility(&self) -> bool {
        matches!(self, Self::NoAltText | Self::SkippedHeading { .. } | Self::LowContrast { .. })
    }
}

/// Something to look at, on a page, in points from its top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Finding {
    pub kind: Kind,
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Where it is in the source.
    pub byte: usize,
    /// Stays the same while the passage does, wherever it moves, so that ignoring it lasts.
    pub key: u64,
}

impl Finding {
    pub fn message(&self) -> String {
        let page = self.page + 1;
        match self.kind {
            Kind::Widow => tr!("typography-widow", page = page),
            Kind::Orphan => tr!("typography-orphan", page = page),
            Kind::StrandedHeading => tr!("typography-stranded-heading", page = page),
            Kind::PastMargin { mm } => tr!("typography-past-margin", page = page, mm = mm as usize),
            Kind::FarFigure { mention } => tr!("typography-far-figure", page = page, mention = mention + 1),
            Kind::NoAltText => tr!("access-no-alt-text", page = page),
            Kind::SkippedHeading { from: 0, to } => tr!("access-first-heading", page = page, to = to),
            Kind::SkippedHeading { from, to } => tr!("access-skipped-heading", page = page, from = from, to = to),
            Kind::LowContrast { tenths, least_tenths } => {
                let shown = |tenths: u32| {
                    let text = format!("{:.1}", f64::from(tenths) / 10.0);
                    if crate::i18n::current() == crate::i18n::Lang::PtBr { text.replace('.', ",") } else { text }
                };
                tr!("access-low-contrast", page = page, ratio = shown(tenths), least = shown(least_tenths))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Region {
    /// Rules, definitions and what's placed on purpose: it isn't read at all.
    Skip,
    Paragraph,
    Heading,
    /// Tables, figures, equations and the like, which only have to stay inside the margins.
    Block,
}

#[derive(Clone, Copy)]
struct Info {
    region: Region,
    section: usize,
    start: usize,
    end: usize,
}

/// What the source says about each of its spans, read once per compile. Asking the source for one
/// span at a time would walk it for each line of a long document.
struct Outline {
    spans: HashMap<Span, Info>,
    /// Stretches of the source between paragraph breaks and block elements.
    sections: Vec<Range<usize>>,
}

fn read_source(source: &Source) -> Outline {
    let mut outline = Outline { spans: HashMap::new(), sections: Vec::new() };
    let root = source.root();
    outline.spans.insert(root.span(), Info { region: Region::Skip, section: 0, start: 0, end: 0 });
    let (mut offset, mut start) = (0, 0);
    for child in root.children() {
        let end = offset + child.len();
        let own_section = stands_alone(child);
        if own_section || child.kind() == SyntaxKind::Parbreak {
            outline.sections.push(start..offset);
            if own_section {
                outline.sections.push(offset..end);
            }
            start = end;
        }
        // The stretch this child is in is the one just pushed, if it was its own, or else the next.
        let section = outline.sections.len() - usize::from(own_section);
        walk(child, offset, Region::Paragraph, section, &mut outline.spans);
        offset = end;
    }
    outline.sections.push(start..offset);
    outline
}

/// Whether this child of the document is no part of a paragraph around it.
fn stands_alone(node: &SyntaxNode) -> bool {
    match node.kind() {
        SyntaxKind::Heading
        | SyntaxKind::ListItem
        | SyntaxKind::EnumItem
        | SyntaxKind::TermItem
        | SyntaxKind::SetRule
        | SyntaxKind::ShowRule
        | SyntaxKind::LetBinding
        | SyntaxKind::Raw => true,
        SyntaxKind::FuncCall => matches!(
            node.children().next().map(|callee| callee.leaf_text().as_str()),
            Some(
                "table"
                    | "grid"
                    | "figure"
                    | "stack"
                    | "block"
                    | "rect"
                    | "square"
                    | "circle"
                    | "ellipse"
                    | "columns"
                    | "raw"
                    | "image"
                    | "outline"
                    | "bibliography"
                    | "pagebreak"
                    | "colbreak"
                    | "v"
                    | "align"
                    | "pad"
                    | "place"
                    | "heading"
            )
        ),
        _ => false,
    }
}

fn walk(node: &SyntaxNode, offset: usize, outer: Region, section: usize, spans: &mut HashMap<Span, Info>) {
    let region = if outer == Region::Skip { Region::Skip } else { region_of(node).unwrap_or(outer) };
    let end = offset + node.len();
    spans.insert(node.span(), Info { region, section, start: offset, end });
    let mut at = offset;
    for child in node.children() {
        walk(child, at, region, section, spans);
        at += child.len();
    }
}

/// What this node makes of what's inside it, if it changes anything.
fn region_of(node: &SyntaxNode) -> Option<Region> {
    match node.kind() {
        SyntaxKind::SetRule | SyntaxKind::ShowRule | SyntaxKind::LetBinding | SyntaxKind::ModuleImport => Some(Region::Skip),
        SyntaxKind::Heading => Some(Region::Heading),
        SyntaxKind::Raw => Some(Region::Block),
        // `$ x $`, on a line of its own: the space after the dollar says so.
        SyntaxKind::Equation => (node.children().nth(1).map(SyntaxNode::kind) == Some(SyntaxKind::Space)).then_some(Region::Block),
        SyntaxKind::FuncCall => match node.children().next().map(|callee| callee.leaf_text().as_str()) {
            Some("place" | "rotate" | "scale" | "skew" | "move" | "hide" | "metadata") => Some(Region::Skip),
            Some("heading") => Some(Region::Heading),
            Some(
                "table" | "grid" | "figure" | "stack" | "box" | "block" | "rect" | "square" | "circle" | "ellipse" | "columns" | "raw" | "footnote"
                | "image" | "outline" | "bibliography",
            ) => Some(Region::Block),
            _ => None,
        },
        _ => None,
    }
}

/// A run of text or a picture, as it ends up on its page.
#[derive(Clone, Copy)]
struct Item {
    page: usize,
    info: Info,
    byte: usize,
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
    baseline: f64,
    size: f64,
    /// Turned on purpose, so it isn't measured against the margins.
    turned: bool,
    /// A line or a box, such as a table's borders: it has a place, but no words.
    shape: bool,
}

fn collect(frame: &Frame, ts: Transform, page: usize, spans: &HashMap<Span, Info>, items: &mut Vec<Item>) {
    let turned = !ts.kx.is_zero() || !ts.ky.is_zero();
    for (pos, item) in frame.items() {
        match item {
            FrameItem::Group(group) => {
                let inner = ts.pre_concat(Transform::translate(pos.x, pos.y)).pre_concat(group.transform);
                collect(&group.frame, inner, page, spans, items);
            }
            FrameItem::Text(text) => {
                let Some((info, offset)) = text.glyphs.iter().find_map(|g| Some((*spans.get(&g.span.0)?, usize::from(g.span.1)))) else { continue };
                let size = text.size.to_pt();
                let at = pos.transform(ts);
                let width = text.width().to_pt();
                items.push(Item {
                    page,
                    info,
                    byte: info.start + offset,
                    left: at.x.to_pt(),
                    right: at.x.to_pt() + width,
                    top: at.y.to_pt() - size * 0.8,
                    bottom: at.y.to_pt() + size * 0.25,
                    baseline: at.y.to_pt(),
                    size,
                    turned,
                    shape: false,
                });
            }
            FrameItem::Image(_, size, span) => {
                let Some(info) = spans.get(span) else { continue };
                let (a, b) = (pos.transform(ts), (*pos + Point::new(size.x, size.y)).transform(ts));
                items.push(Item {
                    page,
                    info: *info,
                    byte: info.start,
                    left: a.x.to_pt().min(b.x.to_pt()),
                    right: a.x.to_pt().max(b.x.to_pt()),
                    top: a.y.to_pt().min(b.y.to_pt()),
                    bottom: a.y.to_pt().max(b.y.to_pt()),
                    baseline: a.y.to_pt().max(b.y.to_pt()),
                    size: size.y.to_pt(),
                    turned,
                    shape: false,
                });
            }
            FrameItem::Shape(shape, span) => {
                let Some(info) = spans.get(span) else { continue };
                let bounds = shape.bbox(false);
                let (a, b) = ((*pos + bounds.min).transform(ts), (*pos + bounds.max).transform(ts));
                items.push(Item {
                    page,
                    info: *info,
                    byte: info.start,
                    left: a.x.to_pt().min(b.x.to_pt()),
                    right: a.x.to_pt().max(b.x.to_pt()),
                    top: a.y.to_pt().min(b.y.to_pt()),
                    bottom: a.y.to_pt().max(b.y.to_pt()),
                    baseline: a.y.to_pt().max(b.y.to_pt()),
                    size: (bounds.max.y - bounds.min.y).to_pt().abs(),
                    turned,
                    shape: true,
                });
            }
            _ => {}
        }
    }
}

/// FNV-1a: it has to give the same number in every run and version, which the standard hasher doesn't promise.
fn hashed(tag: u8, text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in std::iter::once(tag).chain(text.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

struct Reader<'a> {
    source: &'a Source,
    outline: &'a Outline,
}

impl Reader<'_> {
    fn key(&self, tag: u8, section: usize) -> u64 {
        let text = self.outline.sections.get(section).and_then(|range| self.source.text().get(range.clone())).unwrap_or_default();
        // A call's `#` falls just before its stretch, and what's around a passage isn't part of it.
        hashed(tag, text.trim().trim_end_matches('#').trim_end())
    }
}

/// A box around some items, in the finding's terms.
fn around(kind: Kind, page: usize, byte: usize, key: u64, [left, top, right, bottom]: [f64; 4]) -> Finding {
    Finding { kind, page, x: left as f32, y: top as f32, width: (right - left) as f32, height: (bottom - top) as f32, byte, key }
}

fn union(a: [f64; 4], item: &Item) -> [f64; 4] {
    [a[0].min(item.left), a[1].min(item.top), a[2].max(item.right), a[3].max(item.bottom)]
}

const EMPTY: [f64; 4] = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];

/// What's past the margins, once per passage. A table past the margin has many cells that are.
fn past_margins(doc: &PagedDocument, items: &[Item], margins: &[Option<[f32; 4]>], reader: &Reader, found: &mut Vec<Finding>) {
    for (index, page) in doc.pages().iter().enumerate() {
        let width = page.frame.size().x.to_pt();
        let here = || items.iter().filter(|i| i.page == index && i.info.region != Region::Skip && !i.turned);
        let [left, right] = margins.get(index).copied().flatten().map_or([0.0, 0.0], |m| [f64::from(m[0]), f64::from(m[1])]);
        let over = |i: &Item, left: f64, right: f64| (left - i.left).max(i.right - (width - right));
        let (mut left, mut right) = (left, right);
        // The margins measured are those at the end of the document. When most of this page's running
        // text disagrees, it has others of its own, and only the paper's edges can be trusted. A wide
        // table says nothing about that.
        let running = || here().filter(|i| i.info.region == Region::Paragraph && !i.shape);
        if running().filter(|i| over(i, left, right) > MARGIN_TOLERANCE).count() * 2 > running().count() {
            (left, right) = (0.0, 0.0);
        }
        let mut passages: BTreeMap<usize, (f64, usize, [f64; 4])> = BTreeMap::new();
        for item in here() {
            let amount = over(item, left, right);
            if amount > MARGIN_TOLERANCE {
                let entry = passages.entry(item.info.section).or_insert((0.0, item.byte, EMPTY));
                entry.0 = entry.0.max(amount);
                entry.1 = entry.1.min(item.byte);
                entry.2 = union(entry.2, item);
            }
        }
        for (section, (amount, byte, rect)) in passages {
            let mm = (amount / POINTS_PER_MM).round().max(1.0) as u32;
            found.push(around(Kind::PastMargin { mm }, index, byte, reader.key(1, section), rect));
        }
    }
}

/// A line of running text on a page.
#[derive(Clone, Copy)]
struct Line {
    page: usize,
    section: usize,
    byte: usize,
    baseline: f64,
    rect: [f64; 4],
}

/// The lines of running text, page by page and top to bottom. Pages in columns are left out.
fn lines_of(items: &[Item], pages: usize) -> Vec<Line> {
    let mut lines = Vec::new();
    for page in 0..pages {
        let mut here: Vec<&Item> = items.iter().filter(|i| i.page == page && i.info.region == Region::Paragraph && !i.turned && !i.shape).collect();
        here.sort_by(|a, b| a.baseline.total_cmp(&b.baseline));
        let mut on_page: Vec<(Line, f64, Vec<&Item>)> = Vec::new();
        for item in here {
            match on_page.last_mut() {
                // Superscripts and inline math sit off the baseline, but well within the line.
                Some((line, widest, members)) if item.baseline - line.baseline < item.size.max(1.0) * 0.6 => {
                    line.rect = union(line.rect, item);
                    line.byte = line.byte.min(item.byte);
                    let width = item.right - item.left;
                    if width > *widest {
                        *widest = width;
                        line.section = item.info.section;
                        line.baseline = item.baseline;
                    }
                    members.push(item);
                }
                _ => {
                    let line = Line { page, section: item.info.section, byte: item.byte, baseline: item.baseline, rect: union(EMPTY, item) };
                    on_page.push((line, item.right - item.left, vec![item]));
                }
            }
        }
        let columns = on_page.iter_mut().any(|(_, _, members)| {
            members.sort_by(|a, b| a.left.total_cmp(&b.left));
            members.windows(2).any(|pair| matches!(pair, [a, b] if b.left - a.right > a.size * COLUMN_GAP))
        });
        let left = on_page.iter().map(|(line, _, _)| line.rect[0]).fold(f64::INFINITY, f64::min);
        let right = on_page.iter().map(|(line, _, _)| line.rect[2]).fold(f64::NEG_INFINITY, f64::max);
        let apart = on_page.iter().any(|(line, _, _)| line.rect[0] - left > (right - left) * COLUMN_START);
        if !columns && !apart {
            lines.extend(on_page.into_iter().map(|(line, _, _)| line));
        }
    }
    lines
}

fn widows_and_orphans(lines: &[Line], reader: &Reader, found: &mut Vec<Finding>) {
    let mut steps: Vec<f64> = lines
        .windows(2)
        .filter_map(|p| match p {
            [a, b] if a.page == b.page && a.section == b.section => Some(b.baseline - a.baseline),
            _ => None,
        })
        .collect();
    steps.sort_by(f64::total_cmp);
    let gap = steps.get(steps.len() / 2).map_or(f64::INFINITY, |median| median * PARAGRAPH_GAP);
    let longest = |page: usize| lines.iter().filter(|l| l.page == page).map(|l| l.rect[2] - l.rect[0]).fold(0.0, f64::max);
    let mut paragraph: Vec<Line> = Vec::new();
    for line in lines {
        let continues = paragraph.last().is_some_and(|last| {
            last.section == line.section
                && if last.page == line.page {
                    line.baseline - last.baseline <= gap
                } else {
                    last.page + 1 == line.page && last.rect[2] - last.rect[0] >= longest(last.page) * SHORT_LINE
                }
        });
        if !continues {
            judge(&paragraph, reader, found);
            paragraph.clear();
        }
        paragraph.push(*line);
    }
    judge(&paragraph, reader, found);
}

fn judge(paragraph: &[Line], reader: &Reader, found: &mut Vec<Finding>) {
    let (Some(first), Some(last)) = (paragraph.first(), paragraph.last()) else { return };
    if first.page == last.page {
        return;
    }
    if paragraph.iter().filter(|l| l.page == first.page).count() == 1 {
        found.push(around(Kind::Orphan, first.page, first.byte, reader.key(2, first.section), first.rect));
    } else if paragraph.iter().filter(|l| l.page == last.page).count() == 1 {
        found.push(around(Kind::Widow, last.page, last.byte, reader.key(3, last.section), last.rect));
    }
}

fn stranded_headings(doc: &PagedDocument, items: &[Item], reader: &Reader, found: &mut Vec<Finding>) {
    let introspector = doc.introspector();
    for heading in introspector.query(&Selector::Elem(HeadingElem::ELEM, None)) {
        let Some(info) = reader.outline.spans.get(&heading.span()).filter(|i| i.region != Region::Skip) else { continue };
        let Some(position) = heading.location().and_then(|at| introspector.position(at)) else { continue };
        let page = position.page.get() - 1;
        let y = position.point.y.to_pt();
        let Some(height) = doc.pages().get(page).map(|p| p.frame.size().y.to_pt()) else { continue };
        let body = || items.iter().filter(|i| i.info.region != Region::Skip);
        let outside = |i: &&Item| !(info.start..info.end).contains(&i.byte);
        let nothing_after = !body().filter(outside).any(|i| i.page == page && i.baseline > y);
        if y > height * LOW_ON_THE_PAGE && nothing_after && body().any(|i| i.page > page) {
            let rect = body().filter(|i| i.page == page && !outside(i)).fold(EMPTY, union);
            if rect[0].is_finite() {
                found.push(around(Kind::StrandedHeading, page, info.start, reader.key(4, info.section), rect));
            }
        }
    }
}

fn far_figures(doc: &PagedDocument, items: &[Item], reader: &Reader, found: &mut Vec<Finding>) {
    let introspector = doc.introspector();
    let mut first_mention: HashMap<_, usize> = HashMap::new();
    for reference in introspector.query(&Selector::Elem(RefElem::ELEM, None)) {
        let Some(packed) = reference.to_packed::<RefElem>() else { continue };
        // Only the document's own words count as a mention.
        if !reader.outline.spans.contains_key(&reference.span()) {
            continue;
        }
        let Some(page) = reference.location().and_then(|at| introspector.position(at)).map(|p| p.page.get() - 1) else { continue };
        let entry = first_mention.entry(packed.target).or_insert(page);
        *entry = (*entry).min(page);
    }
    for (label, mention) in first_mention {
        let Ok(figure) = introspector.query_label(label) else { continue };
        if figure.to_packed::<FigureElem>().is_none() {
            continue;
        }
        let Some(info) = reader.outline.spans.get(&figure.span()) else { continue };
        let Some(page) = figure.location().and_then(|at| introspector.position(at)).map(|p| p.page.get() - 1) else { continue };
        if page.abs_diff(mention) < FAR_PAGES {
            continue;
        }
        let rect = items.iter().filter(|i| i.page == page && (info.start..info.end).contains(&i.byte)).fold(EMPTY, union);
        if rect[0].is_finite() {
            let text = reader.source.text().get(info.start..info.end).unwrap_or_default();
            found.push(around(Kind::FarFigure { mention }, page, info.start, hashed(5, text), rect));
        }
    }
}

/// What's worth a look in this document, from top to bottom. `margins` are those of each page, where known.
pub fn analyse(doc: &PagedDocument, source: &Source, margins: &[Option<[f32; 4]>]) -> Vec<Finding> {
    let outline = read_source(source);
    let reader = Reader { source, outline: &outline };
    let mut items = Vec::new();
    for (page, content) in doc.pages().iter().enumerate() {
        collect(&content.frame, Transform::identity(), page, &outline.spans, &mut items);
    }
    let mut found = Vec::new();
    past_margins(doc, &items, margins, &reader, &mut found);
    widows_and_orphans(&lines_of(&items, doc.pages().len()), &reader, &mut found);
    stranded_headings(doc, &items, &reader, &mut found);
    far_figures(doc, &items, &reader, &mut found);
    accessibility::find(doc, &items, &reader, &mut found);
    found.sort_by(|a, b| a.page.cmp(&b.page).then(a.y.total_cmp(&b.y)));
    found.truncate(MAX_FINDINGS);
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{DocWorld, load_fonts};

    const WORDS: &str = "Um texto corrido que ocupa várias linhas para que a página tenha o que quebrar em algum lugar. ";

    /// All that's found in this text, with the page's margins as `margin` points on each side.
    pub(super) fn found(text: &str, margin: f32) -> Vec<Finding> {
        let mut world = DocWorld::new(load_fonts(), None).unwrap();
        world.update(text);
        let doc = typst::compile::<PagedDocument>(&world).output.unwrap_or_else(|e| panic!("{e:?}"));
        analyse(&doc, world.source(), &vec![Some([margin; 4]); doc.pages().len()])
    }

    pub(super) fn kinds(text: &str, margin: f32) -> Vec<Kind> {
        found(text, margin).into_iter().map(|f| f.kind).collect()
    }

    /// A paragraph after a growing space, so that each way of breaking it across two pages comes up.
    fn breaking(rule: &str) -> Vec<Kind> {
        let paragraph = WORDS.repeat(5);
        (0..70)
            .flat_map(|space| kinds(&format!("#set page(width: 10cm, height: 7cm, margin: 1cm)\n{rule}\n#v({}pt)\n{paragraph}\n", space * 2), 28.3))
            .collect()
    }

    #[test]
    fn widows_and_orphans_are_found_only_when_they_are_there() {
        let allowed = breaking("#set text(costs: (widow: 0%, orphan: 0%))");
        assert!(allowed.contains(&Kind::Widow), "{allowed:?}");
        assert!(allowed.contains(&Kind::Orphan), "{allowed:?}");
        let prevented = breaking("");
        assert!(prevented.is_empty(), "Typst keeps them from happening by default: {prevented:?}");
    }

    #[test]
    fn a_heading_alone_at_the_end_of_a_page_is_found() {
        let filler = WORDS.repeat(5);
        let stranded = format!("#set page(width: 10cm, height: 10cm, margin: 1cm)\n{filler}\n= Conclusão\n#pagebreak()\n{WORDS}\n");
        let findings = found(&stranded, 28.3);
        assert_eq!(findings.iter().map(|f| f.kind).collect::<Vec<_>>(), [Kind::StrandedHeading], "{findings:?}");
        assert_eq!(stranded.get(findings[0].byte..findings[0].byte + 1), Some("="));
        assert_eq!(findings[0].page, 0);

        let followed = format!("#set page(width: 10cm, height: 10cm, margin: 1cm)\n{filler}\n= Conclusão\n{WORDS}\n");
        assert_eq!(kinds(&followed, 28.3), []);
        let title_page = format!("#set page(width: 10cm, height: 10cm, margin: 1cm)\n= Título\n#pagebreak()\n{WORDS}\n");
        assert_eq!(kinds(&title_page, 28.3), [], "a title with a page of its own is meant");
        assert_eq!(kinds(&format!("{WORDS}\n#v(1fr)\n= Fim\n"), 70.0), [], "the last page has nothing to go on to");
    }

    #[test]
    fn a_table_past_the_margin_is_found_once() {
        let cells = "[Uma coluna larga demais para a página],".repeat(8);
        let wide = format!("#set page(width: 10cm, height: 10cm, margin: 1cm)\n{WORDS}\n\n#table(columns: (5cm, 5cm, 5cm, 5cm), {cells})\n");
        let findings = found(&wide, 28.3);
        assert_eq!(findings.len(), 1, "{findings:?}");
        let Kind::PastMargin { mm } = findings[0].kind else { panic!("{findings:?}") };
        assert_eq!(mm, 120, "the table's own borders count, not only its words");
        let table = wide.find("table(").unwrap();
        assert!((table..wide.len()).contains(&findings[0].byte), "it points at the table");

        let empty = "#set page(width: 10cm, height: 10cm, margin: 1cm)\n#table(columns: (20cm, 1cm))\n#table(columns: (20cm, 1cm), [], [])\n";
        assert_eq!(found(empty, 28.3).len(), 1, "a table with no words is past the margin as well");

        let narrow = format!("#set page(width: 10cm, height: 10cm, margin: 1cm)\n#table(columns: (3cm, 3cm), [a], [b])\n{WORDS}\n");
        assert_eq!(kinds(&narrow, 28.3), []);
    }

    #[test]
    fn what_decorates_a_page_is_left_alone() {
        let decorated = format!(
            "#set page(width: 10cm, height: 10cm, margin: 1cm, header: text(40pt)[Um cabeçalho largo demais para esta página], \
             background: rotate(-45deg, text(70pt, fill: luma(225), \"RASCUNHO\")))\n\
             #place(dx: -2cm)[Fora de propósito]\n#rotate(30deg)[#box(width: 20cm)[Girado de propósito]]\n{WORDS}\n"
        );
        assert_eq!(kinds(&decorated, 28.3), []);
    }

    #[test]
    fn a_page_with_margins_of_its_own_is_not_all_past_them() {
        let wider = format!("#set page(width: 10cm, height: 10cm, margin: 1cm)\n{}\n", WORDS.repeat(4));
        assert_eq!(kinds(&wider, 90.0), [], "the margins measured belong to another page");
    }

    #[test]
    fn columns_are_not_taken_for_lines() {
        let rule = "#set page(width: 14cm, height: 7cm, margin: 1cm, columns: 2)\n#set text(costs: (widow: 0%, orphan: 0%))\n";
        for space in 0..20 {
            let text = format!("{rule}#v({}pt)\n{}\n", space * 3, WORDS.repeat(8));
            let got = kinds(&text, 28.3);
            assert!(!got.iter().any(|k| matches!(k, Kind::Widow | Kind::Orphan)), "{got:?}");
        }
    }

    fn with_figure(pages_between: usize) -> String {
        let breaks = "#pagebreak()\nMais texto.\n".repeat(pages_between);
        format!("Veja a @fig:um.\n{breaks}#figure(rect(width: 2cm)[Desenho], caption: [Um desenho]) <fig:um>\n")
    }

    #[test]
    fn a_figure_far_from_its_first_mention_is_found() {
        assert_eq!(kinds(&with_figure(0), 70.0), []);
        assert_eq!(kinds(&with_figure(1), 70.0), [], "the next page is close enough");
        let far = with_figure(3);
        let findings = found(&far, 70.0);
        assert_eq!(findings.iter().map(|f| f.kind).collect::<Vec<_>>(), [Kind::FarFigure { mention: 0 }], "{findings:?}");
        assert_eq!(findings[0].page, 3);
        assert!(far.get(findings[0].byte..).unwrap().starts_with("figure("), "{:?}", findings[0]);
        assert!(found("Sem menção.\n#pagebreak()\n#pagebreak()\n#figure(rect())\n", 70.0).is_empty());
    }

    #[test]
    fn a_passage_keeps_its_key_where_it_moves_and_loses_it_when_rewritten() {
        let key = |text: &str| found(text, 28.3).first().map(|f| f.key).unwrap();
        let table = "#table(columns: (9cm, 9cm), [Larga demais], [Larga demais])\n";
        let rule = "#set page(width: 10cm, height: 10cm, margin: 1cm)\n";
        let here = key(&format!("{rule}{table}"));
        assert_eq!(here, key(&format!("{rule}\nUm parágrafo novo antes.\n\n{table}")));
        assert_ne!(here, key(&format!("{rule}{}", table.replace("Larga", "Enorme"))));
    }

    #[test]
    fn the_documents_pearl_starts_from_have_nothing_to_find() {
        for template in &crate::templates::TEMPLATES {
            assert_eq!(kinds(template.source, 70.0), [], "{}", template.name);
        }
        assert_eq!(kinds(crate::sample::SAMPLE, 70.0), []);
    }

    #[test]
    fn hostile_documents_are_survived() {
        for text in [
            "",
            "=",
            "@nada",
            "#figure(rect()) <a> #figure(rect()) <a> @a",
            "$ $",
            "#set page(width: 1pt, height: 1pt, margin: 0pt)\nTexto",
            "#set text(size: 0pt)\nTexto\n\nTexto",
            "#scale(x: 0%)[Texto]",
        ] {
            let mut world = DocWorld::new(load_fonts(), None).unwrap();
            world.update(text);
            if let Ok(doc) = typst::compile::<PagedDocument>(&world).output {
                analyse(&doc, world.source(), &[]);
                analyse(&doc, world.source(), &[Some([f32::MAX; 4])]);
            }
        }
    }

    #[test]
    fn a_long_document_is_read_quickly() {
        let text = format!("{}\n\n", WORDS.repeat(6)).repeat(3000);
        let mut world = DocWorld::new(load_fonts(), None).unwrap();
        world.update(&text);
        let doc = typst::compile::<PagedDocument>(&world).output.unwrap_or_else(|e| panic!("{e:?}"));
        assert!(doc.pages().len() > 100);
        let started = std::time::Instant::now();
        analyse(&doc, world.source(), &vec![Some([70.0; 4]); doc.pages().len()]);
        let spent = started.elapsed();
        // Far above what it takes, so that a busy machine doesn't fail it; a walk per line would.
        assert!(spent.as_millis() < 1500, "{spent:?}");
    }
}
