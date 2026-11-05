//! MK Matrix's icon set (MIT OR Apache-2.0; see NOTICE), drawn in code on a 16×16 design grid (original artwork; see
//! ATTRIBUTION.md). Large ribbon icons use the same drawings at 32×32 with accent colours.
//! Accent and paper colours come from the theme tokens, so icons follow light and dark mode;
//! icons placed on the sheet ([`paint_on_sheet`]) keep the light colours, as the sheet does.

use egui::epaint::{PathShape, PathStroke};
use egui::{Color32, Painter, Pos2, Rect, Stroke, pos2, vec2};

use crate::theme::Tokens;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Paste,
    Cut,
    Copy,
    Brush,
    Bold,
    Italic,
    Underline,
    Strike,
    Borders,
    BorderAll,
    BorderNone,
    BorderOutside,
    BorderThick,
    Fill,
    FontColor,
    AlignTop,
    AlignMiddle,
    AlignBottom,
    AlignLeft,
    AlignCenter,
    AlignRight,
    Orientation,
    IndentDec,
    IndentInc,
    Wrap,
    Merge,
    Currency,
    Percent,
    Comma,
    DecInc,
    DecDec,
    CondFormat,
    FormatTable,
    CellStyles,
    Insert,
    Delete,
    Format,
    Sum,
    FillDown,
    Eraser,
    SortFilter,
    Find,
    Replace,
    Undo,
    Redo,
    Save,
    Home,
    More,
    Chevron,
    ChevronUp,
    Close,
    Check,
    Fx,
    Plus,
    Left,
    Right,
    Normal,
    PageLayout,
    PageBreak,
    Minus,
    Table,
    PivotTable,
    Chart,
    ChartBar,
    ChartLine,
    ChartPie,
    ChartScatter,
    Picture,
    Shapes,
    Link,
    Comment,
    Note,
    TextBox,
    Symbol,
    Sparkline,
    Checkbox,
    Function,
    Name,
    Trace,
    Calc,
    Filter,
    SortAsc,
    SortDesc,
    Duplicates,
    TextColumns,
    Validation,
    Group,
    Ungroup,
    Subtotal,
    Lock,
    Spell,
    Freeze,
    Zoom,
    Gridlines,
    Theme,
    Margins,
    Orient,
    /// A sheet of paper standing (Portrait).
    Portrait,
    /// A sheet of paper lying on its side (Landscape).
    Landscape,
    SheetRtl,
    Paper,
    Ruler,
    PrintArea,
    Script,
    Pen,
    Share,
    Search,
    Settings,
    /// Light/dark appearance: a circle, its right half filled.
    Appearance,
    Sheet,
    /// All Sheets navigator next to the sheet tabs: a list of rows with a bullet each.
    SheetList,
    Book,
    Folder,
    /// Header & Footer: a page with its top / bottom band marked.
    Header,
    Footer,
    PageNumber,
    PageCount,
    Calendar,
    Clock,
    FileName,
    /// File › Backstage navigation (line art, drawn in the navigation colour).
    Back,
    NewFile,
    OpenFolder,
    Info,
    Print,
    /// Formulas › Linear Algebra: λ, the eigenvalue.
    Lambda,
    /// Formulas › Math & Trig: π.
    Pi,
}

impl Icon {
    /// Every icon, for galleries and tests.
    pub const ALL: &[Icon] = &[
        Icon::Paste,
        Icon::Cut,
        Icon::Copy,
        Icon::Brush,
        Icon::Bold,
        Icon::Italic,
        Icon::Underline,
        Icon::Strike,
        Icon::Borders,
        Icon::BorderAll,
        Icon::BorderNone,
        Icon::BorderOutside,
        Icon::BorderThick,
        Icon::Fill,
        Icon::FontColor,
        Icon::AlignTop,
        Icon::AlignMiddle,
        Icon::AlignBottom,
        Icon::AlignLeft,
        Icon::AlignCenter,
        Icon::AlignRight,
        Icon::Orientation,
        Icon::IndentDec,
        Icon::IndentInc,
        Icon::Wrap,
        Icon::Merge,
        Icon::Currency,
        Icon::Percent,
        Icon::Comma,
        Icon::DecInc,
        Icon::DecDec,
        Icon::CondFormat,
        Icon::FormatTable,
        Icon::CellStyles,
        Icon::Insert,
        Icon::Delete,
        Icon::Format,
        Icon::Sum,
        Icon::FillDown,
        Icon::Eraser,
        Icon::SortFilter,
        Icon::Find,
        Icon::Replace,
        Icon::Undo,
        Icon::Redo,
        Icon::Save,
        Icon::Home,
        Icon::More,
        Icon::Chevron,
        Icon::ChevronUp,
        Icon::Close,
        Icon::Check,
        Icon::Fx,
        Icon::Plus,
        Icon::Left,
        Icon::Right,
        Icon::Normal,
        Icon::PageLayout,
        Icon::PageBreak,
        Icon::Minus,
        Icon::Table,
        Icon::PivotTable,
        Icon::Chart,
        Icon::ChartBar,
        Icon::ChartLine,
        Icon::ChartPie,
        Icon::ChartScatter,
        Icon::Picture,
        Icon::Shapes,
        Icon::Link,
        Icon::Comment,
        Icon::Note,
        Icon::TextBox,
        Icon::Symbol,
        Icon::Sparkline,
        Icon::Checkbox,
        Icon::Function,
        Icon::Name,
        Icon::Trace,
        Icon::Calc,
        Icon::Filter,
        Icon::SortAsc,
        Icon::SortDesc,
        Icon::Duplicates,
        Icon::TextColumns,
        Icon::Validation,
        Icon::Group,
        Icon::Ungroup,
        Icon::Subtotal,
        Icon::Lock,
        Icon::Spell,
        Icon::Freeze,
        Icon::Zoom,
        Icon::Gridlines,
        Icon::Theme,
        Icon::Margins,
        Icon::Orient,
        Icon::Portrait,
        Icon::Landscape,
        Icon::SheetRtl,
        Icon::Paper,
        Icon::Ruler,
        Icon::PrintArea,
        Icon::Script,
        Icon::Pen,
        Icon::Share,
        Icon::Search,
        Icon::Settings,
        Icon::Appearance,
        Icon::Sheet,
        Icon::SheetList,
        Icon::Book,
        Icon::Folder,
        Icon::Header,
        Icon::Footer,
        Icon::PageNumber,
        Icon::PageCount,
        Icon::Calendar,
        Icon::Clock,
        Icon::FileName,
        Icon::Back,
        Icon::NewFile,
        Icon::OpenFolder,
        Icon::Info,
        Icon::Print,
        Icon::Lambda,
        Icon::Pi,
    ];
}

/// The stroke every line in every icon is drawn with, in design-grid units: one pixel at 16
/// points, two at 32, so the whole set reads as one weight at any size.
const W: f32 = 1.0;

/// A painter scaled to the 16×16 design grid. Lines sit on half units, so a 1-unit stroke covers
/// whole pixels at 16 and 32 points.
struct G<'a> {
    p: &'a Painter,
    r: Rect,
    s: f32,
}

impl G<'_> {
    fn pt(&self, x: f32, y: f32) -> Pos2 {
        pos2(self.r.left() + x * self.s, self.r.top() + y * self.s)
    }
    fn width(&self, w: f32) -> f32 {
        (w * self.s).max(1.0)
    }
    /// An open polyline of `w` units.
    fn line(&self, pts: &[(f32, f32)], c: Color32, w: f32) {
        let v: Vec<Pos2> = pts.iter().map(|(x, y)| self.pt(*x, *y)).collect();
        self.p.add(PathShape::line(v, PathStroke::new(self.width(w), c)));
    }
    /// An open polyline at the set's stroke.
    fn stroke(&self, pts: &[(f32, f32)], c: Color32) {
        self.line(pts, c, W);
    }
    /// A closed outline at the set's stroke.
    fn closed(&self, pts: &[(f32, f32)], c: Color32) {
        let v: Vec<Pos2> = pts.iter().map(|(x, y)| self.pt(*x, *y)).collect();
        self.p.add(PathShape::closed_line(v, PathStroke::new(self.width(W), c)));
    }
    /// A rectangle from (x0, y0) to (x1, y1), its outline centred on those lines.
    fn frame(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32) {
        self.closed(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], c);
    }
    /// A filled rectangle from (x0, y0) to (x1, y1).
    fn block(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32) {
        self.p.rect_filled(Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1)), 0.0, c);
    }
    /// A filled convex polygon.
    fn poly(&self, pts: &[(f32, f32)], fill: Color32) {
        let v: Vec<Pos2> = pts.iter().map(|(x, y)| self.pt(*x, *y)).collect();
        self.p.add(PathShape::convex_polygon(v, fill, Stroke::NONE));
    }
    fn ring(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.p.circle_stroke(self.pt(x, y), r * self.s, Stroke::new(self.width(W), c));
    }
    fn dot(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.p.circle_filled(self.pt(x, y), r * self.s, c);
    }
    /// An arc around (x, y) from angle `a0` to `a1` (radians, clockwise from the x axis).
    fn arc(&self, x: f32, y: f32, r: f32, a0: f32, a1: f32, c: Color32) -> (f32, f32) {
        let n = 16;
        let pts: Vec<(f32, f32)> = (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                (x + r * a.cos(), y + r * a.sin())
            })
            .collect();
        self.stroke(&pts, c);
        pts.last().copied().unwrap_or((x, y))
    }
    /// A cubic Bézier curve from `p0` to `p3`, `w` units wide.
    fn curve(&self, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), c: Color32, w: f32) {
        let pts: Vec<(f32, f32)> = (0..=20)
            .map(|i| {
                let t = i as f32 / 20.0;
                let u = 1.0 - t;
                let (a, b, d, e) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                (a * p0.0 + b * p1.0 + d * p2.0 + e * p3.0, a * p0.1 + b * p1.1 + d * p2.1 + e * p3.1)
            })
            .collect();
        self.line(&pts, c, w);
    }
    /// A solid arrowhead with its tip at (x, y), pointing along angle `a` (radians).
    fn head(&self, x: f32, y: f32, a: f32, c: Color32) {
        let (len, half) = (3.0, 2.0);
        let (dx, dy) = (a.cos(), a.sin());
        let bx = x - dx * len;
        let by = y - dy * len;
        self.poly(&[(x, y), (bx - dy * half, by + dx * half), (bx + dy * half, by - dx * half)], c);
    }
    // The drawing helpers take plain coordinates and styles; a struct for this one would stand alone.
    #[allow(clippy::too_many_arguments)]
    fn text(&self, x: f32, y: f32, t: &str, size: f32, c: Color32, bold: bool, italic: bool) {
        let fam = if bold {
            crate::theme::CELL_BOLD
        } else if italic {
            crate::theme::CELL_ITALIC
        } else {
            crate::theme::CELL
        };
        let font = egui::FontId::new(size * self.s, egui::FontFamily::Name(fam.into()));
        self.p.text(self.pt(x, y), egui::Align2::CENTER_CENTER, t, font, c);
    }
}

/// The colours an icon draws with besides its main colour: MK Matrix's accent, for the part of
/// the icon that is its action, and the paper behind documents and cells.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Palette {
    accent: Color32,
    paper: Color32,
    dark: bool,
    /// The action part in the main colour too: one colour throughout.
    mono: bool,
}

impl Palette {
    fn of(t: &Tokens) -> Palette {
        Palette { accent: t.accent, paper: t.paper, dark: t.dark, mono: false }
    }
}

/// The vintage inks the icons' action parts are drawn in, one per family of commands, as
/// (light theme, dark theme).
mod ink {
    use egui::Color32;
    pub(super) const TERRACOTTA: (Color32, Color32) = (Color32::from_rgb(0xA0, 0x52, 0x2D), Color32::from_rgb(0xE0, 0x8A, 0x5F));
    pub(super) const SLATE: (Color32, Color32) = (Color32::from_rgb(0x3E, 0x5C, 0x76), Color32::from_rgb(0x8F, 0xB0, 0xCC));
    pub(super) const MUSTARD: (Color32, Color32) = (Color32::from_rgb(0x9A, 0x6B, 0x12), Color32::from_rgb(0xE0, 0xB0, 0x4F));
    pub(super) const OLIVE: (Color32, Color32) = (Color32::from_rgb(0x6B, 0x7A, 0x3A), Color32::from_rgb(0xAE, 0xBB, 0x7A));
    pub(super) const OXBLOOD: (Color32, Color32) = (Color32::from_rgb(0x7B, 0x2D, 0x35), Color32::from_rgb(0xD9, 0x82, 0x8B));
}

impl Icon {
    /// The vintage ink of this icon's family: clipboard and file in terracotta, type, alignment,
    /// borders, view and page in slate blue, number, styles and data in mustard, insert and
    /// review in olive, formulas and matrices in oxblood. `None` for interface chrome (arrows,
    /// close, check, plus), which keeps the theme's accent.
    fn ink(self, dark: bool) -> Option<Color32> {
        use Icon::*;
        let (light, night) = match self {
            Paste | Cut | Copy | Brush | Undo | Redo | Save | Eraser | FillDown | Find | Replace | Search | NewFile | OpenFolder | Print | Share
            | Folder | Book | Home | Info => ink::TERRACOTTA,
            Bold | Italic | Underline | Strike | Borders | BorderAll | BorderNone | BorderOutside | BorderThick | Fill | FontColor | AlignTop
            | AlignMiddle | AlignBottom | AlignLeft | AlignCenter | AlignRight | Orientation | IndentDec | IndentInc | Wrap | Merge | Freeze
            | Zoom | Gridlines | Normal | PageLayout | PageBreak | Margins | Orient | Portrait | Landscape | SheetRtl | Paper | Ruler | PrintArea
            | Header | Footer | Theme => ink::SLATE,
            Currency | Percent | Comma | DecInc | DecDec | CondFormat | FormatTable | CellStyles | Insert | Delete | Format | SortFilter | Filter
            | SortAsc | SortDesc | Duplicates | TextColumns | Validation | Group | Ungroup | Subtotal => ink::MUSTARD,
            Table | PivotTable | Chart | ChartBar | ChartLine | ChartPie | ChartScatter | Picture | Shapes | Link | Comment | Note | TextBox
            | Symbol | Sparkline | Checkbox | Spell | Lock | Pen | Script | Calendar | Clock | PageNumber | PageCount | FileName => ink::OLIVE,
            Sum | Fx | Function | Name | Trace | Calc => ink::OXBLOOD,
            _ => return None,
        };
        Some(if dark { night } else { light })
    }
}

/// The code-drawn icon that stands in for a character the current fonts cannot draw (`✕` and
/// `✓` have no bundled egui face). Interface chrome uses this instead of painting a tofu box; a
/// workbook cell still needs a real glyph because its characters are user data.
pub fn fallback_for_symbol(c: char) -> Option<Icon> {
    match c {
        '✕' => Some(Icon::Close),
        '✓' => Some(Icon::Check),
        _ => None,
    }
}

/// Draws `icon` into `rect` (square-ish), in colour `c`, with the current theme's palette.
pub fn paint(p: &Painter, rect: Rect, icon: Icon, c: Color32) {
    paint_with(p, rect, icon, c, &Palette::of(&Tokens::get(p.ctx())));
}

/// Draws `icon` in colour `c` alone, its action part included (the title bar's icons).
pub fn paint_mono(p: &Painter, rect: Rect, icon: Icon, c: Color32) {
    paint_with(p, rect, icon, c, &Palette { mono: true, ..Palette::of(&Tokens::get(p.ctx())) });
}

/// Draws `icon` on the sheet, which stays white in both themes, so with the light palette.
pub fn paint_on_sheet(p: &Painter, rect: Rect, icon: Icon, c: Color32) {
    paint_with(p, rect, icon, c, &Palette::of(&Tokens::light()));
}

/// Draws `icon` into `rect` (square-ish), in colour `c`, with palette `pal`.
///
/// One system for the whole set: line art at a single stroke weight ([`W`]) on a 16×16 grid whose
/// live area runs from 1.5 to 14.5, square corners and ends, in the text colour; the part of the
/// icon that is its action (the arrow, the new cell, the selected border) in MK Matrix's accent;
/// secondary lines (the cells around, the text on a page) muted.
fn paint_with(p: &Painter, rect: Rect, icon: Icon, c: Color32, pal: &Palette) {
    use std::f32::consts::{FRAC_PI_2, PI};
    let side = rect.width().min(rect.height());
    let r = Rect::from_center_size(rect.center(), vec2(side, side));
    let g = G { p, r, s: side / 16.0 };
    let muted = c.gamma_multiply(0.45);
    let a = if pal.mono { c } else { icon.ink(pal.dark).unwrap_or(pal.accent) };
    let wash = a.gamma_multiply(0.35);
    let (down, up, left, right) = (FRAC_PI_2, -FRAC_PI_2, PI, 0.0);
    // A page with a folded corner, the shape documents share.
    let page = |g: &G, c: Color32| {
        g.closed(&[(3.5, 1.5), (9.5, 1.5), (12.5, 4.5), (12.5, 14.5), (3.5, 14.5)], c);
        g.stroke(&[(9.5, 1.5), (9.5, 4.5), (12.5, 4.5)], c);
    };
    // A 3×3 grid of cells, the sheet.
    let grid = |g: &G, c: Color32, inner: Color32| {
        g.frame(1.5, 2.5, 14.5, 13.5, c);
        g.stroke(&[(5.83, 2.5), (5.83, 13.5)], inner);
        g.stroke(&[(10.17, 2.5), (10.17, 13.5)], inner);
        g.stroke(&[(1.5, 6.17), (14.5, 6.17)], inner);
        g.stroke(&[(1.5, 9.83), (14.5, 9.83)], inner);
    };
    match icon {
        Icon::Paste => {
            g.closed(&[(5.5, 2.5), (2.5, 2.5), (2.5, 14.5), (13.5, 14.5), (13.5, 2.5), (10.5, 2.5)], c);
            g.block(5.5, 1.0, 10.5, 4.0, a);
            g.stroke(&[(5.5, 7.5), (10.5, 7.5)], muted);
            g.stroke(&[(5.5, 10.5), (10.5, 10.5)], muted);
        }
        Icon::Cut => {
            g.ring(4.5, 12.0, 2.0, c);
            g.ring(11.5, 12.0, 2.0, c);
            g.stroke(&[(5.7, 10.4), (11.0, 1.5)], c);
            g.stroke(&[(10.3, 10.4), (5.0, 1.5)], c);
            g.dot(8.0, 7.0, 0.9, a);
        }
        Icon::Copy => {
            g.stroke(&[(2.5, 10.5), (2.5, 1.5), (10.5, 1.5)], muted);
            g.frame(5.5, 4.5, 13.5, 14.5, c);
        }
        Icon::Brush => {
            g.block(2.5, 1.5, 11.5, 5.5, a);
            g.stroke(&[(11.5, 3.5), (13.5, 3.5), (13.5, 7.5), (7.5, 7.5), (7.5, 9.5)], c);
            g.block(6.5, 9.5, 8.5, 14.5, c);
        }
        Icon::Bold => g.text(8.0, 8.5, "B", 12.0, c, true, false),
        Icon::Italic => g.text(8.0, 8.5, "I", 12.0, c, false, true),
        Icon::Underline => {
            g.text(8.0, 7.5, "U", 11.0, c, false, false);
            g.stroke(&[(3.5, 14.5), (12.5, 14.5)], a);
        }
        Icon::Strike => {
            g.text(8.0, 8.0, "ab", 10.0, c, false, false);
            g.stroke(&[(1.5, 8.5), (14.5, 8.5)], a);
        }
        Icon::Borders | Icon::BorderAll => {
            grid(&g, if icon == Icon::BorderAll { c } else { muted }, if icon == Icon::BorderAll { c } else { muted });
            if icon == Icon::Borders {
                g.line(&[(1.5, 13.5), (14.5, 13.5)], a, 2.0);
            }
        }
        Icon::BorderNone => {
            for i in 0..7 {
                let t = 1.5 + i as f32 * 13.0 / 6.0;
                for (x, y) in [(t, 2.5), (t, 13.5), (1.5, 2.5 + i as f32 * 11.0 / 6.0), (14.5, 2.5 + i as f32 * 11.0 / 6.0)] {
                    g.dot(x, y, 0.5, muted);
                }
            }
        }
        Icon::BorderOutside | Icon::BorderThick => {
            g.stroke(&[(8.0, 2.5), (8.0, 13.5)], muted);
            g.stroke(&[(1.5, 8.0), (14.5, 8.0)], muted);
            if icon == Icon::BorderThick {
                g.line(&[(2.0, 3.0), (14.0, 3.0), (14.0, 13.0), (2.0, 13.0), (2.0, 3.0)], a, 2.0);
            } else {
                g.frame(1.5, 2.5, 14.5, 13.5, a);
            }
        }
        Icon::Fill => {
            g.closed(&[(2.0, 7.5), (7.0, 2.5), (12.0, 7.5), (7.0, 12.5)], c);
            g.stroke(&[(4.5, 5.0), (2.5, 3.0)], c);
            g.poly(&[(13.5, 9.0), (14.8, 11.6), (13.5, 12.6), (12.2, 11.6)], a);
        }
        Icon::FontColor => g.text(8.0, 7.0, "A", 12.0, c, false, false),
        Icon::AlignTop | Icon::AlignMiddle | Icon::AlignBottom => {
            // The edge the text aligns to, in the accent; the text's lines against it.
            let (edge, ys) = match icon {
                Icon::AlignTop => (1.5, [4.5, 7.5]),
                Icon::AlignMiddle => (8.0, [4.5, 11.5]),
                _ => (14.5, [8.5, 11.5]),
            };
            g.stroke(&[(1.5, edge), (14.5, edge)], a);
            g.stroke(&[(3.5, ys[0]), (12.5, ys[0])], c);
            g.stroke(&[(5.5, ys[1]), (10.5, ys[1])], c);
        }
        Icon::AlignLeft | Icon::AlignCenter | Icon::AlignRight | Icon::IndentDec | Icon::IndentInc => {
            for (i, y) in [2.5f32, 6.0, 9.5, 13.0].iter().enumerate() {
                let w = if i % 2 == 0 { 13.0 } else { 8.0 };
                let (x0, x1) = match icon {
                    Icon::AlignLeft => (1.5, 1.5 + w),
                    Icon::AlignRight => (14.5 - w, 14.5),
                    Icon::AlignCenter => (8.0 - w / 2.0, 8.0 + w / 2.0),
                    _ => (if i == 0 || i == 3 { 1.5 } else { 7.5 }, 14.5),
                };
                g.stroke(&[(x0, *y), (x1, *y)], c);
            }
            if icon == Icon::IndentInc {
                g.head(5.5, 7.75, right, a);
            }
            if icon == Icon::IndentDec {
                g.head(1.5, 7.75, left, a);
            }
        }
        Icon::Orientation => {
            g.text(5.5, 10.0, "ab", 8.5, c, false, false);
            g.stroke(&[(9.5, 12.5), (13.0, 3.5)], a);
            g.head(13.6, 2.0, -1.2, a);
        }
        Icon::Wrap => {
            g.stroke(&[(1.5, 3.5), (14.5, 3.5)], c);
            g.stroke(&[(1.5, 12.5), (6.5, 12.5)], c);
            g.stroke(&[(1.5, 8.0), (11.5, 8.0)], c);
            let end = g.arc(11.5, 10.25, 2.25, -FRAC_PI_2, FRAC_PI_2, a);
            g.stroke(&[end, (10.0, end.1)], a);
            g.head(8.5, 12.5, left, a);
        }
        Icon::Merge => {
            g.frame(1.5, 3.5, 14.5, 12.5, c);
            g.stroke(&[(4.5, 8.0), (11.5, 8.0)], a);
            g.head(3.5, 8.0, left, a);
            g.head(12.5, 8.0, right, a);
        }
        Icon::Currency => g.text(8.0, 8.5, "$", 12.0, c, false, false),
        Icon::Percent => {
            g.ring(4.5, 4.5, 2.0, c);
            g.ring(11.5, 11.5, 2.0, c);
            g.stroke(&[(12.5, 2.5), (3.5, 13.5)], c);
        }
        Icon::Comma => {
            g.dot(8.0, 7.5, 1.6, c);
            g.line(&[(9.3, 8.0), (8.6, 11.0), (7.0, 13.0)], c, 1.4);
        }
        Icon::DecInc | Icon::DecDec => {
            let (top, bottom) = if icon == Icon::DecInc { (".0", ".00") } else { (".00", ".0") };
            g.text(8.5, 4.5, top, 7.0, c, false, false);
            g.text(9.5, 11.0, bottom, 7.0, c, false, false);
            g.stroke(&[(1.5, 14.5), (5.5, 14.5)], a);
            if icon == Icon::DecInc {
                g.head(1.0, 14.5, left, a);
            } else {
                g.head(6.0, 14.5, right, a);
            }
        }
        Icon::CondFormat => {
            g.frame(1.5, 1.5, 14.5, 14.5, muted);
            g.block(3.5, 3.5, 7.5, 5.5, c);
            g.block(3.5, 7.0, 10.5, 9.0, c);
            g.block(3.5, 10.5, 12.5, 12.5, a);
        }
        Icon::FormatTable => {
            g.block(1.5, 2.5, 14.5, 5.5, a);
            g.block(1.5, 8.5, 14.5, 11.0, wash);
            g.frame(1.5, 2.5, 14.5, 13.5, c);
            g.stroke(&[(6.0, 5.5), (6.0, 13.5)], muted);
            g.stroke(&[(10.0, 5.5), (10.0, 13.5)], muted);
        }
        Icon::CellStyles => {
            g.block(1.5, 2.5, 14.5, 7.0, wash);
            g.frame(1.5, 2.5, 14.5, 7.0, c);
            g.frame(1.5, 9.0, 14.5, 13.5, c);
            g.stroke(&[(3.5, 4.75), (9.5, 4.75)], a);
            g.stroke(&[(3.5, 11.25), (9.5, 11.25)], muted);
        }
        Icon::Insert | Icon::Delete | Icon::Format => {
            g.frame(5.5, 2.5, 14.5, 13.5, c);
            g.stroke(&[(5.5, 6.17), (14.5, 6.17)], muted);
            g.stroke(&[(5.5, 9.83), (14.5, 9.83)], muted);
            g.stroke(&[(10.0, 2.5), (10.0, 13.5)], muted);
            match icon {
                Icon::Insert => {
                    g.stroke(&[(0.5, 8.0), (4.5, 8.0)], a);
                    g.stroke(&[(2.5, 6.0), (2.5, 10.0)], a);
                }
                Icon::Delete => {
                    g.stroke(&[(0.8, 6.3), (4.2, 9.7)], a);
                    g.stroke(&[(4.2, 6.3), (0.8, 9.7)], a);
                }
                _ => g.block(5.5, 6.17, 14.5, 9.83, wash),
            }
        }
        Icon::Sum => g.line(&[(12.5, 4.0), (12.5, 2.5), (3.5, 2.5), (8.5, 8.0), (3.5, 13.5), (12.5, 13.5), (12.5, 12.0)], c, 1.2),
        Icon::FillDown => {
            g.frame(4.5, 1.5, 11.5, 14.5, muted);
            g.stroke(&[(8.0, 3.5), (8.0, 10.5)], a);
            g.head(8.0, 13.0, down, a);
        }
        Icon::Eraser => {
            g.closed(&[(1.5, 9.5), (8.5, 2.5), (14.0, 8.0), (8.5, 13.5), (5.5, 13.5)], c);
            g.poly(&[(1.5, 9.5), (5.0, 6.0), (10.5, 11.5), (8.5, 13.5), (5.5, 13.5)], a);
            g.stroke(&[(8.5, 14.5), (14.5, 14.5)], muted);
        }
        Icon::SortFilter => {
            g.text(4.0, 4.0, "A", 6.0, c, false, false);
            g.text(4.0, 11.5, "Z", 6.0, c, false, false);
            g.stroke(&[(8.0, 2.0), (8.0, 11.5)], a);
            g.head(8.0, 14.5, down, a);
            g.closed(&[(10.0, 2.5), (14.5, 2.5), (12.75, 6.0), (12.75, 10.0), (11.75, 10.0), (11.75, 6.0)], muted);
        }
        Icon::Replace => {
            // One letter becoming another, the arrow turning from the first down to the second.
            g.text(4.5, 5.5, "a", 10.0, c, true, false);
            g.text(10.5, 11.0, "b", 10.0, c, true, false);
            g.stroke(&[(8.0, 2.5), (13.5, 2.5), (13.5, 5.0)], a);
            g.head(13.5, 7.0, down, a);
        }
        Icon::Find | Icon::Search | Icon::Zoom => {
            g.ring(6.5, 6.5, 4.5, c);
            g.line(&[(9.8, 9.8), (14.5, 14.5)], c, 1.6);
            if icon == Icon::Zoom {
                g.stroke(&[(4.5, 6.5), (8.5, 6.5)], a);
                g.stroke(&[(6.5, 4.5), (6.5, 8.5)], a);
            }
        }
        Icon::Undo | Icon::Redo => {
            // An arrow back along the top, turning down and round to the bottom.
            let flip = icon == Icon::Redo;
            let fx = |v: f32| if flip { 16.0 - v } else { v };
            let arc: Vec<(f32, f32)> = (0..=16)
                .map(|i| {
                    let t = -FRAC_PI_2 + PI * i as f32 / 16.0;
                    (fx(10.0 + 4.0 * t.cos()), 9.5 + 4.0 * t.sin())
                })
                .collect();
            let mut path = vec![(fx(4.5), 5.5)];
            path.extend(arc);
            path.push((fx(5.5), 13.5));
            g.stroke(&path, c);
            g.head(fx(1.5), 5.5, if flip { right } else { left }, a);
        }
        Icon::Save => {
            g.closed(&[(1.5, 1.5), (11.5, 1.5), (14.5, 4.5), (14.5, 14.5), (1.5, 14.5)], c);
            g.frame(4.5, 1.5, 10.5, 5.5, c);
            g.block(4.0, 9.0, 12.0, 14.5, a);
        }
        Icon::Home => {
            g.stroke(&[(1.0, 8.0), (8.0, 1.5), (15.0, 8.0)], c);
            g.stroke(&[(3.5, 6.0), (3.5, 14.5), (12.5, 14.5), (12.5, 6.0)], c);
            g.block(6.5, 10.0, 9.5, 14.5, a);
        }
        Icon::SheetList => {
            for y in [3.5, 8.0, 12.5] {
                g.block(1.5, y - 1.0, 3.5, y + 1.0, a);
                g.stroke(&[(5.5, y), (14.5, y)], c);
            }
        }
        Icon::More => {
            for x in [3.5, 8.0, 12.5] {
                g.dot(x, 8.0, 1.1, c);
            }
        }
        Icon::Chevron => g.line(&[(4.0, 6.0), (8.0, 10.0), (12.0, 6.0)], c, 1.2),
        Icon::ChevronUp => g.line(&[(4.0, 10.0), (8.0, 6.0), (12.0, 10.0)], c, 1.2),
        Icon::Left => g.line(&[(10.0, 3.5), (5.5, 8.0), (10.0, 12.5)], c, 1.2),
        Icon::Right => g.line(&[(6.0, 3.5), (10.5, 8.0), (6.0, 12.5)], c, 1.2),
        Icon::Close => {
            g.line(&[(3.5, 3.5), (12.5, 12.5)], c, 1.2);
            g.line(&[(12.5, 3.5), (3.5, 12.5)], c, 1.2);
        }
        Icon::Check => g.line(&[(2.5, 8.5), (6.0, 12.0), (13.5, 4.0)], c, 1.3),
        Icon::Checkbox => {
            g.frame(1.5, 1.5, 14.5, 14.5, c);
            g.line(&[(4.0, 8.0), (6.75, 10.75), (12.0, 5.25)], a, 1.4);
        }
        Icon::Fx | Icon::Function => {
            // f(x) as LaTeX sets it: a math-italic f, hooked at the top and tailing below the
            // baseline, crossed; curved parentheses; an italic x.
            let w = 1.1;
            g.curve((6.4, 2.4), (5.9, 1.1), (4.5, 1.2), (4.0, 4.4), c, w);
            g.curve((4.0, 4.4), (3.5, 8.5), (3.3, 12.4), (2.3, 13.8), c, w);
            g.curve((2.3, 13.8), (1.8, 14.6), (0.9, 14.3), (0.8, 13.4), c, w);
            g.line(&[(2.1, 6.0), (5.8, 6.0)], c, 0.9);
            g.curve((8.6, 2.6), (7.1, 5.3), (7.1, 10.9), (8.6, 13.6), c, 0.9);
            g.curve((9.2, 7.0), (10.2, 7.0), (10.9, 11.6), (12.3, 12.2), c, 1.0);
            g.curve((12.4, 7.0), (11.4, 7.6), (10.4, 11.6), (9.1, 12.2), c, 1.0);
            g.curve((13.3, 2.6), (14.8, 5.3), (14.8, 10.9), (13.3, 13.6), c, 0.9);
        }
        Icon::Plus => {
            g.line(&[(8.0, 2.5), (8.0, 13.5)], c, 1.2);
            g.line(&[(2.5, 8.0), (13.5, 8.0)], c, 1.2);
        }
        Icon::Minus => g.line(&[(2.5, 8.0), (13.5, 8.0)], c, 1.2),
        Icon::Normal | Icon::Gridlines | Icon::Sheet => grid(&g, c, if icon == Icon::Gridlines { a } else { c }),
        Icon::PageLayout | Icon::Paper => {
            page(&g, c);
            for y in [7.5, 10.0, 12.5] {
                g.stroke(&[(5.5, y), (10.5, y)], muted);
            }
        }
        Icon::Ruler => {
            g.frame(1.5, 5.0, 14.5, 11.0, c);
            for (k, x) in [4.0, 6.5, 9.0, 11.5].into_iter().enumerate() {
                g.stroke(&[(x, 5.0), (x, if k % 2 == 0 { 8.5 } else { 7.5 })], muted);
            }
        }
        Icon::PageBreak => {
            g.frame(2.5, 1.5, 13.5, 14.5, c);
            for x in [1.0, 5.0, 9.0, 13.0] {
                g.stroke(&[(x, 8.0), (x + 2.5, 8.0)], a);
            }
        }
        Icon::Table | Icon::PivotTable => {
            g.block(1.5, 2.5, 14.5, 5.5, a);
            g.frame(1.5, 2.5, 14.5, 13.5, c);
            g.stroke(&[(5.5, 5.5), (5.5, 13.5)], muted);
            g.stroke(&[(1.5, 9.5), (14.5, 9.5)], muted);
            if icon == Icon::PivotTable {
                g.stroke(&[(8.0, 11.5), (11.5, 11.5), (11.5, 8.0)], c);
                g.head(11.5, 7.0, up, c);
            }
        }
        Icon::Chart | Icon::ChartBar => {
            g.stroke(&[(1.5, 14.5), (14.5, 14.5)], c);
            g.frame(2.5, 8.5, 5.5, 14.5, c);
            g.block(6.5, 3.0, 9.5, 14.5, a);
            g.frame(10.5, 6.5, 13.5, 14.5, c);
        }
        Icon::ChartLine | Icon::Sparkline => {
            g.stroke(&[(1.5, 1.5), (1.5, 14.5), (14.5, 14.5)], muted);
            g.line(&[(3.5, 11.5), (6.5, 7.5), (9.5, 9.5), (13.5, 3.5)], a, 1.4);
        }
        Icon::ChartPie => {
            g.ring(8.0, 8.5, 6.0, c);
            g.poly(&[(8.0, 8.5), (8.0, 2.5), (10.3, 2.96), (12.24, 4.26), (13.54, 6.2), (14.0, 8.5)], a);
        }
        Icon::ChartScatter => {
            g.stroke(&[(1.5, 1.5), (1.5, 14.5), (14.5, 14.5)], c);
            for (x, y) in [(4.5, 11.5), (6.5, 8.5), (9.0, 10.0), (10.5, 5.5)] {
                g.dot(x, y, 1.1, c);
            }
            g.dot(13.0, 3.5, 1.3, a);
        }
        Icon::Picture => {
            g.frame(1.5, 2.5, 14.5, 13.5, c);
            g.stroke(&[(1.5, 12.0), (6.0, 7.0), (9.5, 10.5), (11.0, 9.0), (14.5, 12.5)], c);
            g.dot(11.0, 5.5, 1.4, a);
        }
        Icon::Shapes => {
            g.ring(5.5, 5.5, 4.0, c);
            g.block(7.5, 7.5, 14.5, 14.5, a);
        }
        Icon::Link => {
            g.closed(&[(2.4, 9.3), (5.3, 6.4), (7.7, 8.8), (4.8, 11.7)], c);
            g.closed(&[(8.3, 7.2), (11.2, 4.3), (13.6, 6.7), (10.7, 9.6)], c);
            g.stroke(&[(6.0, 10.0), (10.0, 6.0)], a);
        }
        Icon::Comment | Icon::Note => {
            if icon == Icon::Note {
                g.poly(&[(1.5, 2.5), (14.5, 2.5), (14.5, 10.5), (1.5, 10.5)], wash);
            }
            g.closed(&[(1.5, 2.5), (14.5, 2.5), (14.5, 10.5), (7.5, 10.5), (4.5, 13.5), (4.5, 10.5), (1.5, 10.5)], c);
            g.stroke(&[(4.5, 5.5), (11.5, 5.5)], a);
            g.stroke(&[(4.5, 7.5), (9.0, 7.5)], muted);
        }
        Icon::TextBox => {
            g.frame(1.5, 2.5, 14.5, 13.5, c);
            g.text(8.0, 8.0, "A", 8.5, a, false, false);
        }
        Icon::Symbol => g.text(8.0, 8.0, "Ω", 12.0, c, false, false),
        Icon::Name => {
            g.closed(&[(1.5, 4.5), (11.5, 4.5), (14.5, 8.0), (11.5, 11.5), (1.5, 11.5)], c);
            g.dot(11.0, 8.0, 1.0, a);
            g.stroke(&[(4.0, 8.0), (8.0, 8.0)], muted);
        }
        Icon::Trace => {
            g.frame(1.5, 10.5, 5.5, 14.5, c);
            g.frame(10.5, 1.5, 14.5, 5.5, c);
            g.stroke(&[(5.5, 10.5), (9.0, 7.0)], a);
            g.head(10.5, 5.5, -FRAC_PI_2 / 2.0, a);
        }
        Icon::Calc => {
            g.frame(2.5, 1.5, 13.5, 14.5, c);
            g.block(4.5, 3.5, 11.5, 6.0, a);
            for (x, y) in [(5.0, 8.75), (8.0, 8.75), (11.0, 8.75), (5.0, 12.0), (8.0, 12.0), (11.0, 12.0)] {
                g.block(x - 0.75, y - 0.75, x + 0.75, y + 0.75, c);
            }
        }
        Icon::Filter => g.closed(&[(1.5, 2.5), (14.5, 2.5), (9.5, 8.5), (9.5, 14.0), (6.5, 12.5), (6.5, 8.5)], c),
        Icon::SortAsc | Icon::SortDesc => {
            let (top, bottom) = if icon == Icon::SortAsc { ("A", "Z") } else { ("Z", "A") };
            g.text(5.0, 4.0, top, 6.5, c, false, false);
            g.text(5.0, 11.5, bottom, 6.5, c, false, false);
            g.stroke(&[(11.5, 1.5), (11.5, 11.5)], a);
            g.head(11.5, 14.5, down, a);
        }
        Icon::Duplicates => {
            g.frame(1.5, 1.5, 10.5, 5.5, c);
            g.frame(1.5, 7.5, 10.5, 11.5, c);
            g.stroke(&[(11.0, 10.5), (14.5, 14.0)], a);
            g.stroke(&[(14.5, 10.5), (11.0, 14.0)], a);
        }
        Icon::TextColumns => {
            g.frame(1.5, 2.5, 6.5, 13.5, c);
            g.frame(9.5, 2.5, 14.5, 13.5, c);
            g.head(9.0, 8.0, right, a);
        }
        Icon::Validation => {
            g.frame(1.5, 2.5, 14.5, 13.5, c);
            g.line(&[(4.0, 8.0), (6.75, 10.75), (12.0, 5.25)], a, 1.4);
        }
        Icon::Group | Icon::Ungroup | Icon::Subtotal => {
            g.stroke(&[(3.5, 5.5), (3.5, 14.5), (5.5, 14.5)], c);
            for y in [6.5, 9.5, 12.5] {
                g.stroke(&[(7.5, y), (14.5, y)], muted);
            }
            match icon {
                Icon::Group => {
                    g.stroke(&[(1.5, 3.0), (5.5, 3.0)], a);
                    g.stroke(&[(3.5, 1.0), (3.5, 5.0)], a);
                }
                Icon::Ungroup => g.stroke(&[(1.5, 3.0), (5.5, 3.0)], a),
                _ => g.line(&[(14.0, 1.5), (9.0, 1.5), (11.5, 3.25), (9.0, 5.0), (14.0, 5.0)], a, 1.0),
            }
        }
        Icon::Lock => {
            g.frame(2.5, 7.5, 13.5, 14.5, c);
            g.arc(8.0, 5.5, 3.0, PI, 2.0 * PI, c);
            g.stroke(&[(5.0, 5.5), (5.0, 7.5)], c);
            g.stroke(&[(11.0, 5.5), (11.0, 7.5)], c);
            g.block(7.25, 10.0, 8.75, 12.5, a);
        }
        Icon::Spell => {
            g.text(7.5, 5.5, "abc", 7.5, c, false, false);
            g.line(&[(4.0, 11.0), (6.75, 13.75), (12.5, 8.0)], a, 1.4);
        }
        Icon::Freeze => {
            g.block(1.5, 2.5, 14.5, 6.17, wash);
            g.block(1.5, 6.17, 5.83, 13.5, wash);
            grid(&g, c, muted);
            g.stroke(&[(1.5, 6.17), (14.5, 6.17)], a);
            g.stroke(&[(5.83, 2.5), (5.83, 13.5)], a);
        }
        Icon::Theme => {
            g.frame(1.5, 1.5, 7.0, 7.0, c);
            g.block(9.0, 1.5, 14.5, 7.0, a);
            g.block(1.5, 9.0, 7.0, 14.5, wash);
            g.frame(9.0, 9.0, 14.5, 14.5, c);
        }
        Icon::Margins => {
            g.frame(2.5, 1.5, 13.5, 14.5, c);
            g.closed(&[(5.0, 4.0), (11.0, 4.0), (11.0, 12.0), (5.0, 12.0)], a);
        }
        Icon::Portrait => {
            g.frame(3.5, 1.5, 12.5, 14.5, c);
            for y in [5.0, 7.5, 10.0] {
                g.stroke(&[(5.5, y), (10.5, y)], muted);
            }
        }
        Icon::Landscape => {
            g.frame(1.5, 3.5, 14.5, 12.5, c);
            for y in [6.0, 8.0, 10.0] {
                g.stroke(&[(3.5, y), (12.5, y)], muted);
            }
        }
        Icon::Orient => {
            g.frame(1.5, 4.5, 8.5, 14.5, c);
            g.block(6.0, 1.5, 14.5, 7.5, wash);
            g.frame(6.0, 1.5, 14.5, 7.5, a);
        }
        Icon::SheetRtl => {
            g.frame(1.5, 1.5, 14.5, 9.5, c);
            g.stroke(&[(5.83, 1.5), (5.83, 9.5)], muted);
            g.stroke(&[(10.17, 1.5), (10.17, 9.5)], muted);
            g.stroke(&[(4.5, 13.0), (14.5, 13.0)], a);
            g.head(1.5, 13.0, left, a);
        }
        Icon::PrintArea => {
            for (x0, y0, x1, y1) in [(1.5, 1.5, 4.5, 1.5), (11.5, 1.5, 14.5, 1.5), (1.5, 14.5, 4.5, 14.5), (11.5, 14.5, 14.5, 14.5)] {
                g.stroke(&[(x0, y0), (x1, y1)], muted);
            }
            for (x0, y0, x1, y1) in [(1.5, 1.5, 1.5, 4.5), (14.5, 1.5, 14.5, 4.5), (1.5, 11.5, 1.5, 14.5), (14.5, 11.5, 14.5, 14.5)] {
                g.stroke(&[(x0, y0), (x1, y1)], muted);
            }
            g.frame(4.5, 4.5, 11.5, 11.5, a);
        }
        Icon::Script => {
            g.frame(1.5, 2.5, 14.5, 13.5, c);
            g.line(&[(4.0, 6.0), (6.5, 8.0), (4.0, 10.0)], a, 1.2);
            g.stroke(&[(8.0, 10.5), (12.0, 10.5)], c);
        }
        Icon::Pen => {
            g.closed(&[(2.0, 14.0), (2.5, 10.5), (10.5, 2.5), (13.5, 5.5), (5.5, 13.5)], c);
            g.stroke(&[(9.0, 4.0), (12.0, 7.0)], c);
            g.poly(&[(2.0, 14.0), (2.5, 10.5), (5.5, 13.5)], a);
        }
        Icon::Share => {
            g.stroke(&[(5.5, 6.5), (2.5, 6.5), (2.5, 14.5), (13.5, 14.5), (13.5, 6.5), (10.5, 6.5)], c);
            g.stroke(&[(8.0, 11.0), (8.0, 4.0)], a);
            g.head(8.0, 1.0, up, a);
        }
        Icon::Settings => {
            for i in 0..8 {
                let t = i as f32 * PI / 4.0;
                g.line(&[(8.0 + 4.5 * t.cos(), 8.0 + 4.5 * t.sin()), (8.0 + 6.5 * t.cos(), 8.0 + 6.5 * t.sin())], c, 2.0);
            }
            g.ring(8.0, 8.0, 4.5, c);
            g.ring(8.0, 8.0, 1.75, a);
        }
        Icon::Appearance => {
            // A crescent moon in line art, and a four-pointed star in the bite it leaves: the
            // outer circle's arc from one horn round to the other, and the bite's arc between them.
            let (outer, bite) = (((7.0, 9.0), 6.0), ((10.6, 5.6), 4.9));
            let arc = |((cx, cy), r): ((f32, f32), f32), a0: f32, a1: f32| {
                (0..=24).map(move |i| {
                    let t = a0 + (a1 - a0) * i as f32 / 24.0;
                    (cx + r * t.cos(), cy + r * t.sin())
                })
            };
            // Two open strokes meeting at the horns: a closed path would mitre the sharp tips out.
            g.stroke(&arc(outer, 0.1523, 4.6172).collect::<Vec<_>>(), c);
            g.stroke(&arc(bite, 3.6944, 1.0751).collect::<Vec<_>>(), c);
            g.poly(&[(13.0, 0.75), (13.55, 2.25), (15.0, 2.8), (13.55, 3.35), (13.0, 4.85), (12.45, 3.35), (11.0, 2.8), (12.45, 2.25)], a);
        }
        Icon::Book => {
            g.block(1.5, 1.5, 14.5, 5.0, a);
            g.frame(1.5, 1.5, 14.5, 14.5, c);
            g.stroke(&[(8.0, 5.0), (8.0, 14.5)], muted);
            g.stroke(&[(1.5, 9.75), (14.5, 9.75)], muted);
        }
        Icon::Folder => {
            g.closed(&[(1.5, 2.5), (6.0, 2.5), (7.5, 4.5), (14.5, 4.5), (14.5, 13.5), (1.5, 13.5)], c);
            g.stroke(&[(1.5, 6.5), (14.5, 6.5)], a);
        }
        Icon::Header | Icon::Footer => {
            g.frame(3.5, 1.5, 12.5, 14.5, c);
            let (band, lines) = if icon == Icon::Header { ((5.0, 3.5, 11.0, 5.5), [8.5, 11.0]) } else { ((5.0, 10.5, 11.0, 12.5), [4.5, 7.0]) };
            g.block(band.0, band.1, band.2, band.3, a);
            for y in lines {
                g.stroke(&[(5.5, y), (10.5, y)], muted);
            }
        }
        Icon::PageNumber | Icon::PageCount => {
            if icon == Icon::PageCount {
                g.stroke(&[(5.5, 0.5), (14.5, 0.5), (14.5, 11.5)], muted);
            }
            g.frame(2.5, 2.5, 12.5, 15.0, c);
            g.stroke(&[(6.3, 6.0), (5.8, 12.0)], a);
            g.stroke(&[(9.3, 6.0), (8.8, 12.0)], a);
            g.stroke(&[(4.5, 8.0), (10.8, 8.0)], a);
            g.stroke(&[(4.2, 10.3), (10.5, 10.3)], a);
        }
        Icon::Calendar => {
            g.block(1.5, 2.5, 14.5, 5.5, a);
            g.frame(1.5, 2.5, 14.5, 14.5, c);
            g.stroke(&[(4.5, 1.0), (4.5, 3.5)], c);
            g.stroke(&[(11.5, 1.0), (11.5, 3.5)], c);
            for (x, y) in [(4.5, 8.5), (8.0, 8.5), (11.5, 8.5), (4.5, 11.75), (8.0, 11.75)] {
                g.block(x - 0.75, y - 0.75, x + 0.75, y + 0.75, c);
            }
        }
        Icon::Clock => {
            g.ring(8.0, 8.0, 6.5, c);
            g.stroke(&[(8.0, 4.0), (8.0, 8.0), (11.0, 10.0)], a);
        }
        Icon::FileName => {
            page(&g, c);
            g.stroke(&[(5.5, 9.0), (10.5, 9.0)], a);
            g.stroke(&[(5.5, 11.5), (8.5, 11.5)], muted);
        }
        Icon::Back => {
            g.ring(8.0, 8.0, 6.5, c);
            g.stroke(&[(11.5, 8.0), (6.0, 8.0)], c);
            g.head(4.5, 8.0, left, c);
        }
        Icon::NewFile => {
            page(&g, c);
            g.stroke(&[(8.0, 7.5), (8.0, 12.5)], a);
            g.stroke(&[(5.5, 10.0), (10.5, 10.0)], a);
        }
        Icon::OpenFolder => {
            g.stroke(&[(12.5, 6.5), (12.5, 4.5), (7.5, 4.5), (6.0, 2.5), (1.5, 2.5), (1.5, 13.5)], c);
            g.closed(&[(1.5, 13.5), (4.0, 7.5), (15.0, 7.5), (12.5, 13.5)], c);
            g.stroke(&[(4.5, 10.5), (12.0, 10.5)], a);
        }
        Icon::Info => {
            g.ring(8.0, 8.0, 6.5, c);
            g.line(&[(8.0, 7.0), (8.0, 11.5)], c, 1.3);
            g.dot(8.0, 4.75, 0.9, a);
        }
        Icon::Lambda => {
            g.stroke(&[(3.5, 1.5), (5.5, 1.5), (12.5, 14.5)], c);
            g.stroke(&[(8.25, 6.75), (3.5, 14.5)], a);
        }
        Icon::Pi => {
            g.stroke(&[(2.0, 4.0), (14.0, 4.0)], a);
            g.stroke(&[(5.75, 4.0), (5.0, 14.5)], c);
            g.stroke(&[(10.5, 4.0), (10.5, 13.0), (12.5, 14.5)], c);
        }
        Icon::Print => {
            g.stroke(&[(4.5, 5.5), (4.5, 1.5), (11.5, 1.5), (11.5, 5.5)], c);
            g.stroke(&[(4.5, 11.5), (1.5, 11.5), (1.5, 5.5), (14.5, 5.5), (14.5, 11.5), (11.5, 11.5)], c);
            g.frame(4.5, 9.0, 11.5, 14.5, c);
            g.dot(12.25, 7.75, 0.75, a);
        }
    }
}

/// Icons offered by Insert › Icons, by name.
pub const LIBRARY: &[(&str, Icon)] = &[
    ("check", Icon::Check),
    ("close", Icon::Close),
    ("plus", Icon::Plus),
    ("minus", Icon::Minus),
    ("chart", Icon::Chart),
    ("line chart", Icon::ChartLine),
    ("pie chart", Icon::ChartPie),
    ("scatter", Icon::ChartScatter),
    ("table", Icon::Table),
    ("sum", Icon::Sum),
    ("filter", Icon::Filter),
    ("lock", Icon::Lock),
    ("comment", Icon::Comment),
    ("note", Icon::Note),
    ("picture", Icon::Picture),
    ("book", Icon::Book),
    ("folder", Icon::Folder),
    ("search", Icon::Search),
    ("calculator", Icon::Calc),
    ("link", Icon::Link),
    ("pen", Icon::Pen),
    ("share", Icon::Share),
    ("settings", Icon::Settings),
    ("home", Icon::Home),
    ("save", Icon::Save),
    ("theme", Icon::Theme),
    ("shapes", Icon::Shapes),
    ("text", Icon::TextBox),
    ("trace", Icon::Trace),
    ("validation", Icon::Validation),
    ("lambda", Icon::Lambda),
    ("pi", Icon::Pi),
];

pub fn from_name(name: &str) -> Option<Icon> {
    LIBRARY.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, i)| *i)
}
