//! The window: tab strip, ribbon, source editor, live pages and status bar.

use crate::i18n::tx;
use crate::tr;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use egui::text::{CCursor, CCursorRange};
use egui::{Align, Color32, FontId, Id, Key, KeyboardShortcut, Modifiers, Rect, Sense, Stroke, TextureHandle, TextureOptions, Ui, pos2, vec2};

use self::backstage::Page as FilePage;
use crate::assistant::api::{self, Connection, Credential, Usage};
use crate::assistant::{Assistant, Update};
use crate::bib::Shelf;
use crate::collab::{Change, Session};
use crate::compiler::{Compiler, Diag, Event, Job, Page, Suggestion};
use crate::doi::{Servers, Verdict};
use crate::edit::{self, Sel};
use crate::highlight;
use crate::history::{self, Store, Version};
use crate::icons::Icon;
use crate::images;
use crate::recovery::{self, Orphan, Recovery};
use crate::ruler::{self, Edge, RULER};
use crate::sample::SAMPLE;
use crate::spell::{self, Speller};
use crate::theme::{self, ThemeChoice, Tokens};
use crate::widgets::{field_menu, glyphs, group_begin, group_end, icon_button_mono, labelled, sep, toggle, tool, tool_menu};

mod backstage;
mod docs;
mod figures;
mod formulas;
mod references;

/// The tabs, by the ids of their names.
const TABS: [&str; 6] = ["home-2", "insert", "page-layout", "references", "math", "view"];
/// Seconds between copies of unsaved text while it keeps changing.
const RECOVERY_EVERY: f64 = 3.0;
/// Holds "off" when spell checking is off.
const SPELL_FILE: &str = "spell.txt";
/// The words added to the dictionary, one per line.
const PERSONAL_FILE: &str = "dictionary.txt";
/// Seconds after the last change before the new words are checked.
const SPELL_DELAY: f64 = 0.4;
/// Holds "off" when the rulers are hidden.
const RULER_FILE: &str = "ruler.txt";
/// Where a saved API key is kept, readable by this user alone.
const KEY_FILE: &str = "anthropic-key";
/// Seconds between the versions autosave keeps.
const AUTO_VERSION_EVERY: f64 = 300.0;
/// Most changed lines listed under a version.
const MAX_CHANGES_SHOWN: usize = 200;
/// Most search matches highlighted at a time.
const MAX_MATCHES_SHOWN: usize = 2000;
const LIST_BULLETS: &str = "bulleted-list";
const LIST_NUMBERED: &str = "numbered-list";
const LIST_TERMS: &str = "term-list";
const RAIL_WIDTH: f32 = 44.0;
const SIDE_WIDTH: f32 = 320.0;
/// How long a message in the status bar is safe from the next compile's time.
const NOTICE_SECONDS: f64 = 5.0;
const SIDE_STRIP: f32 = 30.0;
const TITLE_BAR_H: f32 = 38.0;
const TITLE_CONTROL_H: f32 = 26.0;
/// Windows has no system title bar above Pearl's, which takes its buttons and resizing edges.
pub const WINDOW_CONTROLS: bool = cfg!(target_os = "windows");
/// Seconds after the last change before autosave writes.
const AUTOSAVE_DELAY: f64 = 1.0;
const EDITOR: &str = "source-editor";
const STATUS_H: f32 = 24.0;
const ZOOMS: [f32; 7] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 2.5];
const MIN_ZOOM: f32 = 0.5;
/// The pages are rasterised at 2.5 pixels per point: past this they'd go soft.
const MAX_ZOOM: f32 = 2.5;
/// The most "fit to width" enlarges a page: a wide window shouldn't blow it up to fill it.
const MAX_FIT_ZOOM: f32 = 1.5;

/// The zoom at which a page `widest` points wide fits `room` points.
fn fit_zoom(room: f32, widest: f32) -> f32 {
    (room / widest.max(1.0)).clamp(0.2, MAX_FIT_ZOOM)
}

/// Every fixed snippet the ribbon inserts; a test compiles them all. `|` marks the caret.
pub const SNIPPETS: &[&str] = &[
    EQ_INLINE,
    EQ_BLOCK,
    IMAGE_FIG,
    LINK,
    FOOTNOTE,
    PAGEBREAK,
    OUTLINE,
    LIST_OF_FIGURES,
    LIST_OF_TABLES,
    QUOTE,
    CODE,
    COLUMNS,
    LABEL,
    LINE,
    FRACTION,
    ROOT,
    SUM,
    INTEGRAL,
    LIMIT,
    MATRIX,
    CASES,
    VECTOR,
    BINOM,
];
const EQ_INLINE: &str = "$|x$";
const EQ_BLOCK: &str = "$ |x^2 $\n";
const IMAGE_FIG: &str = "#figure(\n  rect(width: 60%, height: 3cm),\n  caption: [|Legenda],\n)\n";
const LINK: &str = "#link(\"https://typst.app\")[|texto]";
const FOOTNOTE: &str = "#footnote[|Nota]";
const PAGEBREAK: &str = "#pagebreak()\n|";
const OUTLINE: &str = "#outline(title: [Sumário])\n|";
const LIST_OF_FIGURES: &str = "#outline(title: [Lista de figuras], target: figure.where(kind: image))\n|";
const LIST_OF_TABLES: &str = "#outline(title: [Lista de tabelas], target: figure.where(kind: table))\n|";
const QUOTE: &str = "#quote(block: true)[|Citação]\n";
const CODE: &str = "```\n|código\n```\n";
const COLUMNS: &str = "#columns(2)[\n  |Texto\n]\n";
const LABEL: &str = "#figure(rect(), caption: [Figura]) <|rotulo>\n";
const LINE: &str = "#line(length: 100%)\n|";
const FRACTION: &str = "$ (|a) / (b) $";
const ROOT: &str = "$ sqrt(|x) $";
const SUM: &str = "$ sum_(i=1)^n |i $";
const INTEGRAL: &str = "$ integral_a^b |x dif x $";
const LIMIT: &str = "$ lim_(x -> 0) |x $";
const MATRIX: &str = "$ mat(|1, 0; 0, 1) $";
const CASES: &str = "$ f(x) = cases(|1 \"se\" x > 0, 0 \"caso contrário\") $";
const VECTOR: &str = "$ vec(|a, b, c) $";
const BINOM: &str = "$ binom(|n, k) $";

#[derive(Clone, Copy, PartialEq)]
enum Layout {
    Both,
    Source,
    Pages,
}

/// What waits for the answer to "save the changes?".
#[derive(Clone, Copy, PartialEq)]
enum Pending {
    New,
    Open,
    /// A document from the recent list, by its place in it.
    Recent(usize),
    /// A new document from a template, by its place in the list.
    Template(usize),
    /// Joining the session behind a link.
    Join,
    Quit,
}

/// The panes the right-hand strip opens.
#[derive(Clone, Copy, PartialEq)]
enum Side {
    Details,
    Outline,
    Files,
    References,
    Symbols,
    Problems,
    Versions,
    Claude,
    Docs,
}

/// A past version looked at in the Versions pane.
struct Viewed {
    version: Version,
    text: String,
    /// What differs from the document, and the text it was worked out against.
    changes: Vec<history::Change>,
    compared_with: u64,
}

/// A line of the conversation with Claude.
enum Said {
    Person(String),
    Claude(String),
    Action(String),
    Problem(String),
}

/// The Symbols pane: groups of the glyph shown and the Typst math code it inserts.
pub const SYMBOL_GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "greek-letters",
        &[
            ("α", "alpha"),
            ("β", "beta"),
            ("γ", "gamma"),
            ("δ", "delta"),
            ("ε", "epsilon"),
            ("ζ", "zeta"),
            ("η", "eta"),
            ("θ", "theta"),
            ("λ", "lambda"),
            ("μ", "mu"),
            ("ν", "nu"),
            ("ξ", "xi"),
            ("π", "pi"),
            ("ρ", "rho"),
            ("σ", "sigma"),
            ("τ", "tau"),
            ("φ", "phi"),
            ("χ", "chi"),
            ("ψ", "psi"),
            ("ω", "omega"),
            ("Γ", "Gamma"),
            ("Δ", "Delta"),
            ("Θ", "Theta"),
            ("Λ", "Lambda"),
            ("Σ", "Sigma"),
            ("Φ", "Phi"),
            ("Ω", "Omega"),
        ],
    ),
    (
        "operators",
        &[
            ("±", "plus.minus"),
            ("×", "times"),
            ("÷", "div"),
            ("·", "dot.op"),
            ("∑", "sum"),
            ("∏", "product"),
            ("∫", "integral"),
            ("∂", "partial"),
            ("∇", "nabla"),
            ("∞", "infinity"),
            ("√", "sqrt(x)"),
        ],
    ),
    ("relations", &[("≤", "<="), ("≥", ">="), ("≠", "!="), ("≈", "approx"), ("≡", "equiv"), ("∝", "prop")]),
    ("arrows", &[("→", "->"), ("←", "<-"), ("↔", "<->"), ("⇒", "=>"), ("⇔", "<=>"), ("↦", "|->")]),
    (
        "sets-and-logic",
        &[
            ("∈", "in"),
            ("∉", "in.not"),
            ("⊂", "subset"),
            ("⊃", "supset"),
            ("∪", "union"),
            ("∩", "inter"),
            ("∅", "nothing"),
            ("∀", "forall"),
            ("∃", "exists"),
            ("∧", "and"),
            ("∨", "or"),
            ("¬", "not"),
        ],
    ),
];

/// The session window: starting one, joining one, or the one running.
enum ShareDialog {
    Start,
    Join { link: String },
    Running { copied: bool },
}

/// Caret colours for the people in a session: strong enough for white names on them.
const PEOPLE_COLORS: [Color32; 6] = [
    Color32::from_rgb(0x7B, 0x3F, 0xE4),
    Color32::from_rgb(0x0E, 0x8F, 0x80),
    Color32::from_rgb(0x1F, 0x6F, 0xD6),
    Color32::from_rgb(0xC7, 0x5A, 0x12),
    Color32::from_rgb(0xB8, 0x32, 0x7F),
    Color32::from_rgb(0x2E, 0x85, 0x40),
];

/// The suggestions on show under the caret.
struct Completions {
    /// Replacing the text from here to the caret.
    from: usize,
    /// The caret they were asked for; once it moves they no longer apply.
    caret: usize,
    suggestions: Vec<Suggestion>,
    selected: usize,
}

/// The pointer resting on the source, and what's shown for it.
#[derive(Default)]
struct Hover {
    /// The byte under the pointer, and since when.
    byte: Option<usize>,
    since: f64,
    asked: bool,
    request: u64,
    /// The words on show, whether they're code, and the byte they're for.
    tip: Option<(String, bool, usize)>,
}

struct TableDialog {
    rows: usize,
    cols: usize,
    header: bool,
    figure: bool,
}

pub struct App {
    compiler: Compiler,
    text: String,
    path: Option<PathBuf>,
    dirty: bool,
    tab: String,
    layout: Layout,
    /// `None` fits the page to the pane's width.
    zoom: Option<f32>,
    fit_zoom: f32,
    pages: Vec<Page>,
    textures: HashMap<u128, TextureHandle>,
    diags: Vec<Diag>,
    millis: Option<u128>,
    status: String,
    setup: bool,
    want_focus: bool,
    pending_caret: Option<usize>,
    scroll_caret: bool,
    scroll_page: Option<(usize, f32)>,
    marker: Option<(usize, egui::Pos2, f64)>,
    table_dialog: Option<TableDialog>,
    confirm: Option<Pending>,
    quitting: bool,
    colored: Option<(u64, Color32, egui::text::LayoutJob)>,
    title: String,
    autosave: bool,
    changed_at: f64,
    ribbon_collapsed: bool,
    find: Option<String>,
    find_focus: bool,
    about: bool,
    spell: Option<Speller>,
    spell_on: bool,
    /// Each word checked so far, and whether it's right.
    spell_known: std::collections::HashMap<String, bool>,
    spell_personal: std::collections::HashSet<String>,
    /// Goes up when `spell_known` changes, so the colours are worked out anew.
    spell_generation: u64,
    /// The hash of the text whose words have been sent.
    spell_sent: Option<u64>,
    spell_menu: Option<SpellMenu>,
    licenses: bool,
    recovery: Option<Recovery>,
    references: Shelf,
    reference_search: String,
    docs: docs::Docs,
    doi_input: String,
    doi_lookup: Option<Receiver<Result<String, &'static str>>>,
    servers: Servers,
    verdicts: HashMap<String, Verdict>,
    checking: Option<Receiver<(String, Verdict)>>,
    /// Whether a picture brought into an unsaved document asks where to save it.
    asks_to_save: bool,
    recovery_written_at: f64,
    orphans: Vec<Orphan>,
    orphans_later: bool,
    side: Option<Side>,
    line_numbers: bool,
    ruler: bool,
    /// Where the rulers were last drawn: along the top, then down the side.
    ruler_strips: Option<(Rect, Rect)>,
    /// Where each margin can be grabbed, on the rulers.
    margin_handles: Vec<(Edge, egui::Pos2)>,
    /// The margin being dragged and the value it would get, in points.
    margin_drag: Option<(Edge, f32)>,
    /// The caret on the pages: page, x, baseline and text size, in points.
    page_caret: Option<(usize, f32, f32, f32)>,
    /// What the caret on the pages was last asked for: the byte, and of which compile.
    caret_asked: Option<(usize, u64)>,
    compiles: u64,
    /// The last click on a page reached the source, so the keyboard writes there.
    page_typing: bool,
    /// The caret has moved in the pages, which should follow it.
    follow_caret: bool,
    figures: figures::Figures,
    formulas: formulas::FormulaPane,
    /// The status bar's last compile time, which anything else may replace at any moment.
    compile_status: String,
    /// The status as of the last frame, and since when, to give a message time to be read.
    status_seen: (String, f64),
    /// Where the pages are on screen, for the tests' pointer.
    page_rects: Vec<Rect>,
    /// The Files pane's listing, read when the pane opens or on Refresh.
    files: Vec<String>,
    /// The File view, when open, on this page.
    backstage: Option<FilePage>,
    theme: ThemeChoice,
    hover: Hover,
    kept_selection: Option<Sel>,
    completions: Option<Completions>,
    /// The newest question asked of the compiler; older answers are dropped.
    completion_request: u64,
    /// Scroll the list to the selected suggestion on the next frame.
    completion_scroll: bool,
    /// The document's versions, newest first, as last listed.
    versions: Vec<Version>,
    viewed: Option<Viewed>,
    /// The pages are showing the viewed version, not the document.
    previewing: bool,
    /// Keep a version when this time comes (the last of Claude's edits has to arrive first).
    version_due: Option<(f64, history::Kind)>,
    last_auto_version: Option<f64>,
    autosaving: bool,
    pending_version_action: Option<bool>,
    restore_requested: bool,
    /// This frame's time, for what has no context to ask.
    time_now: f64,
    confirm_clear_history: bool,
    /// Claude, once asked for something; it stays in the session until sent away.
    assistant: Option<Assistant>,
    assistant_busy: bool,
    transcript: Vec<Said>,
    assistant_input: String,
    assistant_usage: Usage,
    key_input: String,
    /// The session exists only for Claude, on this machine, and goes when Claude does.
    session_private: bool,
    /// The session being edited together, if any.
    session: Option<Session>,
    share_dialog: Option<ShareDialog>,
    user_name: String,
    recent: Vec<PathBuf>,
    /// Seconds since each recent document changed (`None`: it wasn't found), filled in by a thread.
    recent_ages: std::sync::Arc<std::sync::Mutex<HashMap<PathBuf, Option<u64>>>>,
}

impl App {
    /// Opens `path`, or the sample document without one.
    pub fn new(path: Option<PathBuf>) -> Result<Self, String> {
        let text = match &path {
            Some(p) if p.exists() => std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
            Some(_) => String::new(),
            None => SAMPLE.to_string(),
        };
        let app = Self {
            compiler: Compiler::start()?,
            text,
            path,
            dirty: false,
            tab: "home-2".into(),
            layout: Layout::Both,
            zoom: None,
            fit_zoom: 1.0,
            pages: Vec::new(),
            textures: HashMap::new(),
            diags: Vec::new(),
            millis: None,
            status: tx("loading-fonts").into(),
            setup: false,
            want_focus: true,
            pending_caret: None,
            scroll_caret: false,
            scroll_page: None,
            marker: None,
            table_dialog: None,
            confirm: None,
            quitting: false,
            colored: None,
            title: String::new(),
            autosave: true,
            changed_at: 0.0,
            ribbon_collapsed: false,
            find: None,
            find_focus: false,
            about: false,
            spell: Some(Speller::start()),
            spell_on: crate::recent::read_setting(SPELL_FILE).is_none_or(|v| v.trim() != "off"),
            spell_known: spell_personal_words().iter().map(|w| (w.clone(), true)).collect(),
            spell_personal: spell_personal_words(),
            spell_generation: 0,
            spell_sent: None,
            spell_menu: None,
            licenses: false,
            recovery: None,
            references: Shelf::default(),
            reference_search: String::new(),
            docs: docs::Docs::load(),
            doi_input: String::new(),
            doi_lookup: None,
            servers: Servers::default(),
            verdicts: HashMap::new(),
            checking: None,
            asks_to_save: false,
            recovery_written_at: f64::NEG_INFINITY,
            orphans: Vec::new(),
            orphans_later: false,
            side: None,
            line_numbers: true,
            ruler: crate::recent::read_setting(RULER_FILE).is_none_or(|v| v.trim() != "off"),
            ruler_strips: None,
            margin_handles: Vec::new(),
            margin_drag: None,
            page_caret: None,
            caret_asked: None,
            compiles: 0,
            page_typing: false,
            follow_caret: false,
            figures: figures::Figures::default(),
            formulas: formulas::FormulaPane::default(),
            // What it says at the start gives way to the first compile.
            compile_status: tx("loading-fonts").to_string(),
            status_seen: (String::new(), 0.0),
            page_rects: Vec::new(),
            files: Vec::new(),
            backstage: None,
            theme: ThemeChoice::load(),
            hover: Hover::default(),
            kept_selection: None,
            versions: Vec::new(),
            viewed: None,
            previewing: false,
            version_due: None,
            last_auto_version: None,
            autosaving: false,
            pending_version_action: None,
            restore_requested: false,
            time_now: 0.0,
            confirm_clear_history: false,
            assistant: None,
            assistant_busy: false,
            transcript: Vec::new(),
            assistant_input: String::new(),
            assistant_usage: Usage::default(),
            key_input: String::new(),
            session_private: false,
            completions: None,
            completion_request: 0,
            completion_scroll: false,
            session: None,
            share_dialog: None,
            user_name: crate::recent::read_setting("name.txt")
                .or_else(|| std::env::var("USER").ok())
                .or_else(|| std::env::var("USERNAME").ok())
                .unwrap_or_default()
                .trim()
                .to_string(),
            recent: crate::recent::load(),
            recent_ages: std::sync::Arc::default(),
        };
        let mut app = app;
        app.remember_path();
        // What's in the file now, before anything can write over it.
        app.keep_version(history::Kind::Opened);
        app.compile();
        Ok(app)
    }

    pub fn set_tab(&mut self, tab: &str) {
        // By id, or by its name in the language on show.
        self.tab = TABS.iter().find(|id| **id == tab || tx(id) == tab).map_or_else(|| tab.to_string(), |id| (*id).to_string());
    }

    /// Opens a side pane by its place on the strip.
    pub fn open_side(&mut self, index: usize) {
        self.side =
            [Side::Details, Side::Outline, Side::Files, Side::Symbols, Side::Problems, Side::Claude, Side::Versions, Side::References, Side::Docs]
                .get(index)
                .copied();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn error_count(&self) -> usize {
        self.diags.iter().filter(|d| d.error).count()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Opens the search, as Ctrl+F does.
    pub fn open_search(&mut self) {
        self.open_find();
    }

    /// The text selected in the editor.
    pub fn selected_text(&self, ctx: &egui::Context) -> String {
        edit::selected(&self.text, self.selection(ctx)).to_string()
    }

    /// Selects a char range in the editor, and focuses it.
    pub fn select(&mut self, ctx: &egui::Context, a: usize, b: usize) {
        self.set_selection(ctx, (a, b));
        self.want_focus = true;
    }

    /// The char range selected in the editor.
    pub fn selected_range(&self, ctx: &egui::Context) -> (usize, usize) {
        self.selection(ctx)
    }

    /// The words of the tip on show for the pointer's resting place.
    pub fn hover_words(&self) -> Option<&str> {
        self.hover.tip.as_ref().map(|t| t.0.as_str())
    }

    /// The labels of the suggestions on show.
    pub fn completion_labels(&self) -> Vec<String> {
        self.completions.as_ref().map_or_else(Vec::new, |c| c.suggestions.iter().map(|s| s.label.clone()).collect())
    }

    /// The people here besides this one, when in a session.
    pub fn session_others(&self) -> Vec<String> {
        self.session.as_ref().map_or_else(Vec::new, Session::others)
    }

    pub fn session_carets(&self) -> Vec<crate::collab::RemoteCursor> {
        self.session.as_ref().map_or_else(Vec::new, Session::cursors)
    }

    /// Opens a session for this document on `bind`, and returns the link.
    pub fn host_session_on(&mut self, bind: &str, name: &str) -> Result<String, String> {
        let file_name = self.path.as_deref().and_then(Path::file_name).map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let session = Session::host_on(bind, &self.text, name, &file_name)?;
        let link = session.share_link().unwrap_or_default().to_string();
        self.session = Some(session);
        self.compile();
        Ok(link)
    }

    /// Joins the session behind `link`, replacing what's open.
    pub fn join_session_link(&mut self, ctx: &egui::Context, link: &str, name: &str) {
        self.user_name = name.to_string();
        self.share_dialog = Some(ShareDialog::Join { link: link.to_string() });
        self.join_session(ctx);
        self.share_dialog = None;
    }

    pub fn leave_session(&mut self) {
        self.session = None;
    }

    /// Starts on this theme, without saving it as the user's choice.
    pub fn start_with_theme(&mut self, choice: ThemeChoice) {
        self.theme = choice;
    }

    /// Opens the File view, on its first page.
    pub fn open_file_view(&mut self) {
        self.open_backstage(FilePage::Home);
    }

    /// Shows only the pages.
    pub fn set_ruler(&mut self, on: bool) {
        self.ruler = on;
        crate::recent::write_setting(RULER_FILE, if on { "on" } else { "off" });
    }

    /// Where the rulers were last drawn, if they were: along the top, then down the side.
    /// Where the pointer grabs this margin, on the rulers.
    pub fn margin_handle(&self, edge: Edge) -> Option<egui::Pos2> {
        self.margin_handles.iter().find(|(e, _)| *e == edge).map(|(_, at)| *at)
    }

    /// Where a page is on screen.
    pub fn page_rect(&self, page: usize) -> Option<Rect> {
        self.page_rects.get(page).copied()
    }

    /// The caret drawn on the pages: page, x, baseline and text size, in points.
    pub fn page_caret(&self) -> Option<(usize, f32, f32, f32)> {
        self.page_caret
    }

    pub fn ruler_strips(&self) -> Option<(Rect, Rect)> {
        self.ruler_strips
    }

    pub fn show_pages_only(&mut self) {
        self.layout = Layout::Pages;
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
        self.compile();
    }

    pub fn compiled_once(&self) -> bool {
        self.millis.is_some()
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    fn compile(&self) {
        // A guest's document compiles in the session's folder, where the host's files arrive.
        let path = self.session.as_ref().and_then(Session::guest_path).or_else(|| self.path.clone());
        let shown = self.viewed.as_ref().filter(|_| self.previewing).map_or(&self.text, |v| &v.text);
        self.compiler.send(Job::Compile { text: shown.clone(), path });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let events: Vec<Event> = self.compiler.events.try_iter().collect();
        for event in events {
            match event {
                Event::Compiled { pages, diags, millis } => {
                    if let Some(pages) = pages {
                        self.textures.retain(|h, _| pages.iter().any(|p| p.hash == *h));
                        for p in &pages {
                            self.textures.entry(p.hash).or_insert_with(|| {
                                let image = egui::ColorImage::from_rgba_unmultiplied(p.pixels.size, &p.pixels.rgba);
                                ctx.load_texture(format!("page-{:x}", p.hash), image, TextureOptions::LINEAR)
                            });
                        }
                        self.pages = pages;
                    }
                    self.millis = Some(millis);
                    self.compiles += 1;
                    // A message stays long enough to be read: compiles come right after most edits.
                    let now = ctx.input(|i| i.time);
                    if self.status == self.compile_status || self.status.is_empty() || now - self.status_seen.1 >= NOTICE_SECONDS {
                        self.status = tr!("compiled-in", ms = millis.to_string());
                        self.compile_status.clone_from(&self.status);
                    }
                    self.diags = diags;
                }
                Event::JumpToSource { byte } => {
                    self.pending_caret = Some(edit::char_of(&self.text, byte));
                    self.scroll_caret = true;
                    self.want_focus = true;
                    self.page_typing = true;
                }
                Event::Images(images) => self.figures.placed = images,
                Event::DropTarget { block, byte, above } => self.finish_move(ctx, block, byte, above),
                Event::ExampleDone { request, result } => self.docs.receive(ctx, request, result),
                Event::CaretAt { byte, page, x, y, size } => {
                    let moved = self.page_caret.is_none_or(|(p, old_x, old_y, _)| p != page || old_x != x || old_y != y);
                    self.follow_caret |= moved && self.page_typing;
                    let current = self.caret_asked.map(|(b, _)| b);
                    if current == Some(byte) {
                        self.page_caret = Some((page, x, y, size));
                    }
                }
                Event::JumpToPage { page, y_pt, x_pt } => {
                    self.scroll_page = Some((page, y_pt));
                    self.marker = Some((page, pos2(x_pt, y_pt), ctx.input(|i| i.time)));
                }
                Event::Hovered { request, tip } => {
                    if request == self.hover.request {
                        let byte = self.hover.byte.unwrap_or(0);
                        self.hover.tip = tip.filter(|(words, _)| !words.trim().is_empty()).map(|(words, code)| (words, code, byte));
                    }
                }
                Event::Completions { request, from, suggestions } => {
                    if request == self.completion_request {
                        let (a, b) = self.selection(ctx);
                        let caret = edit::byte_of(&self.text, a);
                        // No list for nothing, or once a selection or another place makes it moot.
                        self.completions =
                            (a == b && !suggestions.is_empty() && from <= caret).then_some(Completions { from, caret, suggestions, selected: 0 });
                        self.completion_scroll = true;
                    }
                }
                Event::Dependencies(files) => {
                    if let Some(session) = &self.session {
                        session.share_files(files);
                    }
                }
                Event::Exported(Ok(path)) => self.status = tr!("pdf-saved", path = path.display().to_string()),
                Event::Exported(Err(e)) => self.status = tr!("export-failed", error = e.to_string()),
                Event::Failed(e) => self.status = e,
            }
        }
    }

    /// The selection, as chars. The editor collapses its own when it loses the focus, so while
    /// something else has the focus (a ribbon button, the search, Claude's panel) this is the one it
    /// had, which is the one they mean.
    fn selection(&self, ctx: &egui::Context) -> Sel {
        let n = self.text.chars().count();
        let id = Id::unique(EDITOR);
        if !ctx.memory(|m| m.has_focus(id))
            && let Some((a, b)) = self.kept_selection
        {
            return (a.min(n), b.min(n));
        }
        let range = egui::TextEdit::load_state(ctx, id).and_then(|s| s.cursor.char_range());
        let (a, b) = range.map_or((n, n), |r| {
            let (p, q): (usize, usize) = (r.primary.index.into(), r.secondary.index.into());
            (p.min(q), p.max(q))
        });
        (a.min(n), b.min(n))
    }

    fn set_selection(&mut self, ctx: &egui::Context, (a, b): Sel) {
        let id = Id::unique(EDITOR);
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
        egui::TextEdit::store_state(ctx, id, state);
        self.kept_selection = Some((a, b));
    }

    fn edit(&mut self, ctx: &egui::Context, f: impl FnOnce(&mut String, Sel) -> Sel) {
        self.previewing = false;
        let sel = self.selection(ctx);
        // One undo step for each of these, also from the pages, when the editor hasn't had the focus
        // and so hasn't seen the text as it was.
        let id = Id::unique(EDITOR);
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        let range = state.cursor.char_range().unwrap_or_else(|| CCursorRange::two(CCursor::new(sel.0), CCursor::new(sel.1)));
        let mut undoer = state.undoer();
        undoer.add_undo(&(range, self.text.clone()));
        state.set_undoer(undoer);
        egui::TextEdit::store_state(ctx, id, state);
        // The keyboard follows: with only the pages on show, it needs the editor out of sight.
        self.page_typing = true;
        let next = f(&mut self.text, sel);
        self.set_selection(ctx, next);
        self.dirty = true;
        self.changed_at = ctx.input(|i| i.time);
        self.want_focus = true;
        self.scroll_caret = true;
        self.compile();
    }

    fn wrap(&mut self, ctx: &egui::Context, pre: &str, post: &str) {
        self.edit(ctx, |t, s| edit::wrap(t, s, pre, post));
    }

    fn insert(&mut self, ctx: &egui::Context, snippet: &str) {
        self.edit(ctx, |t, s| edit::insert(t, s, snippet));
    }

    fn marker_line(&mut self, ctx: &egui::Context, marker: &str) {
        self.edit(ctx, |t, s| edit::line_marker(t, s, marker));
    }

    fn rule(&mut self, ctx: &egui::Context, key: &str, line: &str) {
        self.edit(ctx, |t, s| edit::set_rule(t, s, key, line));
    }

    fn undo(&mut self, ctx: &egui::Context, redo: bool) {
        // In a session each person undoes only their own changes. The editor's own history holds
        // whole texts, and restoring one would wipe out what the others typed since.
        if let Some(session) = &mut self.session {
            let (a, b) = {
                let range = egui::TextEdit::load_state(ctx, Id::unique(EDITOR)).and_then(|s| s.cursor.char_range());
                range.map_or((0, 0), |r| (r.primary.index.into(), r.secondary.index.into()))
            };
            let done = if redo { session.redo() } else { session.undo() };
            if done {
                self.text = session.text().to_string();
                let n = self.text.chars().count();
                self.set_selection(ctx, (a.min(b).min(n), a.max(b).min(n)));
                self.dirty = true;
                self.changed_at = ctx.input(|i| i.time);
                self.compile();
            }
            self.want_focus = true;
            return;
        }
        let id = Id::unique(EDITOR);
        let Some(mut state) = egui::TextEdit::load_state(ctx, id) else { return };
        let Some(range) = state.cursor.char_range() else { return };
        let mut undoer = state.undoer();
        let current = (range, self.text.clone());
        let next = if redo { undoer.redo(&current).cloned() } else { undoer.undo(&current).cloned() };
        if let Some((range, text)) = next {
            self.text = text;
            state.cursor.set_char_range(Some(range));
            state.set_undoer(undoer);
            egui::TextEdit::store_state(ctx, id, state);
            self.dirty = true;
            self.changed_at = ctx.input(|i| i.time);
            self.want_focus = true;
            self.compile();
        }
    }

    // ---- Files -----------------------------------------------------------------------------

    fn dialog(&self) -> rfd::FileDialog {
        let dialog = rfd::FileDialog::new();
        match self.path.as_deref().and_then(Path::parent) {
            Some(dir) if !dir.as_os_str().is_empty() => dialog.set_directory(dir),
            _ => dialog,
        }
    }

    fn request(&mut self, ctx: &egui::Context, action: Pending) {
        if self.dirty {
            self.confirm = Some(action);
        } else {
            self.perform(ctx, action);
        }
    }

    fn perform(&mut self, ctx: &egui::Context, action: Pending) {
        // Another document leaves the session; quitting does too, with the window.
        if !matches!(action, Pending::Quit | Pending::Join | Pending::Open) {
            self.session = None;
        }
        match action {
            Pending::Join => self.join_session(ctx),
            Pending::New => self.start_document(ctx, String::new()),
            Pending::Template(i) => {
                if let Some(template) = crate::templates::TEMPLATES.get(i) {
                    self.start_document(ctx, template.source.to_string());
                }
            }
            Pending::Open => {
                if let Some(path) = self.dialog().add_filter("Typst", &["typ"]).pick_file() {
                    self.session = None;
                    self.open_path(ctx, path);
                }
            }
            Pending::Recent(i) => {
                if let Some(path) = self.recent.get(i).cloned() {
                    self.open_path(ctx, path);
                }
            }
            Pending::Quit => {
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// A new document with no file yet.
    fn start_document(&mut self, ctx: &egui::Context, text: String) {
        self.text = text;
        self.path = None;
        self.dirty = false;
        self.pages.clear();
        self.textures.clear();
        self.backstage = None;
        self.want_focus = true;
        self.end_viewing();
        self.versions.clear();
        self.set_selection(ctx, (0, 0));
        self.compile();
    }

    fn open_path(&mut self, ctx: &egui::Context, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                self.text = text;
                self.path = Some(path);
                self.dirty = false;
                self.backstage = None;
                self.want_focus = true;
                self.remember_path();
                self.end_viewing();
                self.keep_version(history::Kind::Opened);
                self.set_selection(ctx, (0, 0));
                self.compile();
            }
            Err(e) => self.status = tr!("cant-open", path = path.display().to_string(), error = e.to_string()),
        }
    }

    /// Puts the open document first on the recent list.
    fn remember_path(&mut self) {
        if let Some(path) = self.path.clone().filter(|p| p.exists())
            && crate::recent::push(&mut self.recent, &path)
        {
            crate::recent::save(&self.recent);
        }
    }

    /// Saves, asking where when the document has no file yet. False when nothing was written.
    fn save(&mut self, save_as: bool) -> bool {
        let target = match (&self.path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => self.dialog().add_filter("Typst", &["typ"]).set_file_name("documento.typ").save_file(),
        };
        let Some(path) = target else { return false };
        match std::fs::write(&path, &self.text) {
            Ok(()) => {
                self.status = tr!("saved-to", path = path.display().to_string());
                self.path = Some(path);
                self.dirty = false;
                self.remember_path();
                let now = self.time_now;
                if !self.autosaving {
                    self.keep_version(history::Kind::Saved);
                } else if self.last_auto_version.is_none_or(|last| now - last >= AUTO_VERSION_EVERY) {
                    // Autosave writes every moment: one version in a while is enough.
                    self.last_auto_version = Some(now);
                    self.keep_version(history::Kind::Autosaved);
                }
                self.compile();
                true
            }
            Err(e) => {
                self.status = tr!("cant-save", error = e.to_string());
                false
            }
        }
    }

    fn export_pdf(&mut self) {
        if self.previewing {
            self.status = tx("the-pages-are-showing-an-old-version").into();
            return;
        }
        if self.diags.iter().any(|d| d.error) {
            self.status = tx("fix-the-errors-before-exporting-the-pdf").into();
            return;
        }
        let name = self.path.as_deref().and_then(Path::file_stem).map_or_else(|| "documento".to_string(), |s| s.to_string_lossy().into_owned());
        if let Some(path) = self.dialog().add_filter("PDF", &["pdf"]).set_file_name(format!("{name}.pdf")).save_file() {
            self.compiler.send(Job::ExportPdf(path));
        }
    }

    /// The folder pictures go in, or says why this document can't take any.
    /// The document's folder, asking where to save it first if it has none; `refusal` says why not.
    fn saved_folder(&mut self, refusal: &str) -> Option<PathBuf> {
        if self.path.is_none() && !(self.asks_to_save && self.save(true)) {
            self.status = refusal.into();
        }
        self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf)
    }

    fn images_folder(&mut self) -> Option<PathBuf> {
        // A guest's folder is its own: the others would be left with a document that can't find the file.
        if self.session.as_ref().is_some_and(|s| !s.is_host()) {
            self.status = tx("only-the-host-of-a-session-can-add-images").into();
            return None;
        }
        // Pictures need a folder: an unsaved document gets one now, rather than the picture being lost.
        self.saved_folder(tx("save-the-document-before-inserting-images-they"))
    }

    fn insert_image(&mut self, ctx: &egui::Context) {
        let Some(dir) = self.images_folder() else { return };
        if let Some(file) = rfd::FileDialog::new().set_directory(&dir).add_filter(tx("images"), &images::EXTENSIONS).pick_file() {
            self.bring_images(ctx, &[file], &[]);
        }
    }

    /// Takes pictures dragged onto the window or pasted, before the editor sees them as text.
    fn receive_images(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files)).iter().map(|f| f.path().to_path_buf()).collect();
        let focused = ctx.memory(egui::Memory::focused);
        let (mut files, mut pixels) = (dropped, Vec::new());
        // In a field of a pane, a paste is for that field.
        if focused.is_none() || focused == Some(Id::unique(EDITOR)) {
            ctx.input_mut(|i| {
                i.events.retain(|event| match event {
                    egui::Event::PasteImage(image) => {
                        pixels.push(image.clone());
                        false
                    }
                    egui::Event::Paste(text) => match images::files_in_text(text) {
                        Some(list) if list.iter().all(|f| images::is_image(f)) => {
                            files.extend(list);
                            false
                        }
                        _ => true,
                    },
                    _ => true,
                });
            });
        }
        self.bring_images(ctx, &files, &pixels);
    }

    /// Puts a figure at the caret for each picture, all as one change.
    fn bring_images(&mut self, ctx: &egui::Context, files: &[PathBuf], pixels: &[Arc<egui::ColorImage>]) {
        if files.is_empty() && pixels.is_empty() {
            return;
        }
        let Some(dir) = self.images_folder() else { return };
        let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S").to_string();
        let from_files = files.iter().map(|file| images::adopt(&dir, file));
        let from_pixels = pixels.iter().map(|image| images::adopt_pixels(&dir, image, &stamp));
        let (mut snippet, mut failure) = (String::new(), None);
        for result in from_files.chain(from_pixels) {
            match result {
                Ok(path) => snippet += &edit::file_snippet(&path),
                Err(id) => failure = Some(id),
            }
        }
        if !snippet.is_empty() {
            self.edit(ctx, |t, s| edit::insert_block(t, s, &snippet));
        }
        if let Some(id) = failure {
            self.status = tx(id).into();
        }
    }

    /// Says what letting go will do, while a file is carried over the window.
    fn drop_hint(&self, ctx: &egui::Context, t: &Tokens) {
        if ctx.input(|i| i.raw.hovered_files.is_empty()) {
            return;
        }
        let area = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::unique("drop-hint")));
        painter.rect_filled(area, 0.0, t.grid_bg.gamma_multiply(0.8));
        painter.text(area.center(), egui::Align2::CENTER_CENTER, tx("drop-to-insert-the-image-at-the-caret"), FontId::proportional(20.0), t.text);
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        let shift = Modifiers::COMMAND | Modifiers::SHIFT;
        if pressed(shift, Key::S) {
            self.save(true);
        }
        if pressed(Modifiers::COMMAND, Key::S) {
            self.save(false);
        }
        if pressed(Modifiers::COMMAND, Key::O) {
            self.request(ctx, Pending::Open);
        }
        if pressed(Modifiers::COMMAND, Key::N) {
            self.request(ctx, Pending::New);
        }
        if pressed(Modifiers::COMMAND, Key::E) {
            self.export_pdf();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1)) {
            self.help_at_caret(ctx);
        }
        // The rest act on the editor and the pages, which the File view covers.
        if self.backstage.is_some() {
            return;
        }
        self.figure_keys(ctx);
        self.receive_images(ctx);
        if pressed(Modifiers::COMMAND, Key::Space) {
            self.ask_completions(ctx, true);
        }
        self.completion_keys(ctx);
        self.typing_help(ctx);
        // Before the editor sees them: its own history doesn't suit a session (see `undo`).
        if self.session.is_some() {
            if pressed(shift, Key::Z) || pressed(Modifiers::COMMAND, Key::Y) {
                self.undo(ctx, true);
            }
            if pressed(Modifiers::COMMAND, Key::Z) {
                self.undo(ctx, false);
            }
        }
        if pressed(Modifiers::COMMAND, Key::B) {
            self.wrap(ctx, "*", "*");
        }
        if pressed(Modifiers::COMMAND, Key::I) {
            self.wrap(ctx, "_", "_");
        }
        if pressed(Modifiers::COMMAND, Key::U) {
            self.wrap(ctx, "#underline[", "]");
        }
        if pressed(Modifiers::COMMAND, Key::Equals) || pressed(Modifiers::COMMAND, Key::Plus) {
            self.step_zoom(1);
        }
        if pressed(Modifiers::COMMAND, Key::Minus) {
            self.step_zoom(-1);
        }
        if pressed(Modifiers::COMMAND, Key::Num0) {
            self.zoom = Some(1.0);
        }
        if pressed(Modifiers::COMMAND, Key::F) {
            self.open_find();
        }
        if pressed(Modifiers::COMMAND, Key::J) {
            self.locate(ctx);
        }
    }

    /// Asks what could be typed at the caret. The compile for the current text was sent before this,
    /// and the compiler takes them in that order.
    fn ask_completions(&mut self, ctx: &egui::Context, explicit: bool) {
        let (a, b) = self.selection(ctx);
        self.completion_request += 1;
        // The compiler holds the past version's text, whose offsets aren't these.
        if a != b || self.previewing {
            self.completions = None;
            return;
        }
        self.compiler.send(Job::Complete { request: self.completion_request, byte: edit::byte_of(&self.text, a), explicit });
    }

    /// Whether what was just typed may start or narrow a completion: not after a space or line end.
    fn worth_completing(&self, ctx: &egui::Context) -> bool {
        let (a, b) = self.selection(ctx);
        // Its list goes beside the source, which isn't on show.
        self.layout != Layout::Pages && a == b && a > 0 && self.text.chars().nth(a - 1).is_some_and(|c| !c.is_whitespace())
    }

    fn accept_completion(&mut self, ctx: &egui::Context, index: usize) {
        let Some(list) = self.completions.take() else { return };
        let Some(chosen) = list.suggestions.get(index) else { return };
        self.completion_request += 1;
        let apply = chosen.apply.clone();
        // Up to the caret as it is now: a letter may have been typed since the list was asked for.
        let (caret, _) = self.selection(ctx);
        let to = edit::byte_of(&self.text, caret).max(list.from);
        self.edit(ctx, |text, _| edit::apply_completion(text, list.from, to, &apply));
    }

    /// Brackets and quotes typed into the editor, taken before it sees them. All of a frame's typing
    /// goes through here when any of it is one of those, so it stays in order.
    fn typing_help(&mut self, ctx: &egui::Context) {
        if !ctx.memory(|m| m.has_focus(Id::unique(EDITOR))) {
            return;
        }
        let single = |t: &str| {
            let mut chars = t.chars();
            chars.next().filter(|c| chars.next().is_none() && crate::typing::is_special(*c))
        };
        let wanted = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(t) if single(t).is_some())));
        let backspace = ctx.input(|i| {
            i.events.iter().any(|e| matches!(e, egui::Event::Key { key: Key::Backspace, pressed: true, modifiers, .. } if modifiers.is_none()))
        });
        if wanted {
            let mut typed = Vec::new();
            ctx.input_mut(|i| {
                i.events.retain(|e| match e {
                    egui::Event::Text(t) => {
                        typed.push(t.clone());
                        false
                    }
                    _ => true,
                });
            });
            for piece in typed {
                self.edit(ctx, |text, sel| match single(&piece).and_then(|c| crate::typing::type_char(text, sel, c)) {
                    Some(next) => next,
                    None => edit::insert_plain(text, sel, &piece),
                });
            }
            if self.worth_completing(ctx) {
                self.ask_completions(ctx, false);
            } else {
                self.completions = None;
            }
        } else if backspace {
            let sel = self.selection(ctx);
            let mut trial = self.text.clone();
            if crate::typing::backspace_pair(&mut trial, sel).is_some() {
                ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Backspace));
                self.edit(ctx, |text, sel| crate::typing::backspace_pair(text, sel).unwrap_or(sel));
                self.completions = None;
                self.completion_request += 1;
            }
        }
    }

    /// The keys a list on show takes before the editor sees them.
    fn completion_keys(&mut self, ctx: &egui::Context) {
        let pressed = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if self.completions.is_none() {
            return;
        }
        if pressed(Key::Escape) {
            self.completions = None;
            self.completion_request += 1;
            // egui lets go of the focus on Esc before anyone can take the key.
            self.want_focus = true;
            return;
        }
        let down = pressed(Key::ArrowDown);
        let up = pressed(Key::ArrowUp);
        let accept = pressed(Key::Enter) || pressed(Key::Tab);
        if let Some(list) = &mut self.completions {
            let n = list.suggestions.len().max(1);
            if down {
                list.selected = (list.selected + 1) % n;
                self.completion_scroll = true;
            }
            if up {
                list.selected = (list.selected + n - 1) % n;
                self.completion_scroll = true;
            }
            if accept {
                let index = list.selected;
                self.accept_completion(ctx, index);
            }
        }
    }

    fn locate(&self, ctx: &egui::Context) {
        if self.previewing {
            return;
        }
        let (a, _) = self.selection(ctx);
        self.compiler.send(Job::Locate { byte: edit::byte_of(&self.text, a) });
    }

    // ---- Interface ---------------------------------------------------------------------------

    pub fn show(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if self.status != self.status_seen.0 {
            self.status_seen = (self.status.clone(), ctx.input(|i| i.time));
        }
        self.session_receive(&ctx);
        self.assistant_receive(&ctx);
        self.show_window(ui);
        self.session_send(&ctx);
    }

    fn store(&self) -> Option<Store> {
        Store::for_document(self.path.as_deref()?)
    }

    /// Keeps the document as a version, if it has a file and has changed since the last. A failure
    /// here says so but stops nothing: the document itself is saved apart from this.
    fn keep_version(&mut self, kind: history::Kind) {
        let (Some(store), Some(path)) = (self.store(), self.path.clone()) else { return };
        if let Err(e) = store.keep(&path, &self.text, kind, history::now_ms()) {
            self.status = tr!("cant-keep-version", error = e.to_string());
        }
        self.versions = store.list();
    }

    fn end_viewing(&mut self) {
        let showing = self.previewing;
        self.viewed = None;
        self.previewing = false;
        if showing {
            self.compile();
        }
    }

    /// Looks at the version at this place on the list.
    pub fn view_version(&mut self, index: usize) {
        let Some((store, version)) = self.store().zip(self.versions.get(index).cloned()) else { return };
        match store.read(&version) {
            Some(text) => {
                let changes = history::line_changes(&text, &self.text);
                self.viewed = Some(Viewed { version, text, changes, compared_with: egui::epaint::util::hash(&self.text) });
                if self.previewing {
                    self.compile();
                }
            }
            None => self.status = tx("couldnt-read-this-version").into(),
        }
    }

    /// Shows the viewed version in the pages, or goes back to the document.
    pub fn preview_version(&mut self, on: bool) {
        self.previewing = on && self.viewed.is_some();
        self.compile();
    }

    /// Puts the viewed version in the document. What was there is kept first, and in a session it
    /// reaches everyone as any edit does, so this too can be undone.
    pub fn restore_version(&mut self, ctx: &egui::Context) {
        let Some(viewed) = self.viewed.take() else { return };
        self.keep_version(history::Kind::BeforeRestore);
        let when = history::describe(viewed.version.stamp, history::now_ms());
        self.edit(ctx, |text, _| {
            *text = viewed.text.clone();
            (0, 0)
        });
        self.status = tr!("version-restored", when = when);
    }

    pub fn version_labels(&self) -> Vec<&'static str> {
        self.versions.iter().map(|v| v.kind.label()).collect()
    }

    fn pane_versions(&mut self, ui: &mut Ui, t: &Tokens) {
        let ctx = ui.ctx().clone();
        if self.path.is_none() {
            ui.colored_label(t.text_dim, tx("the-history-starts-when-the-document-is"));
            return;
        }
        if self.versions.is_empty() {
            self.versions = self.store().map_or_else(Vec::new, |s| s.list());
        }
        // The comparison is with the document as it is now.
        if self.viewed.as_ref().is_some_and(|v| v.compared_with != egui::epaint::util::hash(&self.text))
            && let Some(viewed) = &mut self.viewed
        {
            viewed.changes = history::line_changes(&viewed.text, &self.text);
            viewed.compared_with = egui::epaint::util::hash(&self.text);
        }
        let now = history::now_ms();
        let mut chosen = None;
        for (i, version) in self.versions.iter().enumerate() {
            let on = self.viewed.as_ref().is_some_and(|v| v.version.stamp == version.stamp);
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
            let when = history::describe(version.stamp, now);
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, &when));
            if on {
                ui.painter().rect_filled(rect, 0.0, t.accent_soft);
                ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 0.0, t.accent);
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 0.0, t.hover);
            }
            ui.painter().text(pos2(rect.left() + 10.0, rect.center().y - 9.0), egui::Align2::LEFT_CENTER, &when, theme::ui_bold(12.5), t.text);
            ui.painter().text(
                pos2(rect.left() + 10.0, rect.center().y + 9.0),
                egui::Align2::LEFT_CENTER,
                version.kind.label(),
                theme::ui_font(11.5),
                t.text_dim,
            );
            if resp.clicked() {
                chosen = Some(i);
            }
            // The chosen version's actions and differences, right under it.
            if on && let Some(viewed) = &self.viewed {
                let same = viewed.text == self.text;
                let mut action = 0;
                ui.horizontal(|ui| {
                    let label = if self.previewing { tx("back-to-the-document") } else { tx("see-on-the-pages") };
                    if ui.button(label).clicked() {
                        action = 1;
                    }
                    if ui.add_enabled(!same, egui::Button::new(tx("restore"))).clicked() {
                        action = 2;
                    }
                });
                if same {
                    ui.colored_label(t.text_dim, tx("same-as-the-current-document"));
                } else {
                    let green = t.error_check_green;
                    for change in viewed.changes.iter().take(MAX_CHANGES_SHOWN) {
                        // The document has what's marked +; going back to this version would lose it.
                        let (mark, color, line) = match change {
                            history::Change::Added(l) => ("+", green, l),
                            history::Change::Removed(l) => ("−", t.danger, l),
                        };
                        ui.add(egui::Label::new(egui::RichText::new(format!("{mark} {line}")).font(FontId::monospace(11.5)).color(color)).truncate());
                    }
                    if viewed.changes.len() > MAX_CHANGES_SHOWN {
                        ui.colored_label(t.text_dim, tr!("more-lines", n = viewed.changes.len() - MAX_CHANGES_SHOWN));
                    }
                }
                ui.add_space(8.0);
                if action == 1 {
                    self.pending_version_action = Some(!self.previewing);
                } else if action == 2 {
                    self.restore_requested = true;
                }
            }
        }
        if let Some(i) = chosen {
            self.view_version(i);
        }
        if let Some(on) = self.pending_version_action.take() {
            self.preview_version(on);
        }
        if std::mem::take(&mut self.restore_requested) {
            self.restore_version(&ctx);
        }
        ui.add_space(14.0);
        ui.colored_label(t.text_dim, tx("versions-stay-on-this-computer-only-outside"));
        ui.add_space(6.0);
        if self.confirm_clear_history {
            ui.label(tx("delete-all-versions-of-this-document-this"));
            ui.horizontal(|ui| {
                if ui.button(tx("delete")).clicked() {
                    if let Some(Err(e)) = self.store().map(|store| store.clear()) {
                        self.status = tr!("cant-delete", error = e.to_string());
                    }
                    self.versions.clear();
                    self.end_viewing();
                    self.confirm_clear_history = false;
                }
                if ui.button(tx("cancel")).clicked() {
                    self.confirm_clear_history = false;
                }
            });
        } else if ui.small_button(tx("delete-this-documents-history")).clicked() {
            self.confirm_clear_history = true;
        }
    }

    /// The way to the API: a saved key, or the environment's. The address can be overridden as in the
    /// official clients.
    fn api_connection() -> Option<Connection> {
        let from_env = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        let credential = crate::recent::read_setting(KEY_FILE)
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .or_else(|| from_env("ANTHROPIC_API_KEY"))
            .map(Credential::ApiKey)
            .or_else(|| from_env("ANTHROPIC_AUTH_TOKEN").map(Credential::Token))?;
        Some(Connection { url: from_env("ANTHROPIC_BASE_URL").unwrap_or_else(|| api::DEFAULT_URL.to_string()), credential })
    }

    /// Sends Claude what the person asked, bringing it into the session the first time: into the one
    /// under way, or into a private one on this machine.
    pub fn ask_claude(&mut self, ctx: &egui::Context, instruction: &str) {
        let Some(connection) = Self::api_connection() else {
            self.transcript.push(Said::Problem(tx("the-api-key-is-missing").to_string()));
            return;
        };
        if self.session.is_none() {
            let file_name = self.path.as_deref().and_then(Path::file_name).map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            match Session::host_on("127.0.0.1:0", &self.text, &self.user_name, &file_name) {
                Ok(session) => {
                    session.set_waker(ctx);
                    self.session = Some(session);
                    self.session_private = true;
                }
                Err(e) => {
                    self.transcript.push(Said::Problem(e));
                    return;
                }
            }
        }
        let Some(session) = &self.session else { return };
        if session.is_waiting() {
            self.transcript.push(Said::Problem(tx("wait-for-the-session-to-finish-connecting").to_string()));
            return;
        }
        if self.assistant.is_none() {
            match Assistant::start(session.link().to_string(), connection) {
                Ok(assistant) => {
                    assistant.set_waker(ctx);
                    self.assistant = Some(assistant);
                }
                Err(e) => {
                    self.transcript.push(Said::Problem(e));
                    return;
                }
            }
        }
        let selection = self.selection(ctx);
        let selected = (selection.0 != selection.1).then(|| edit::selected(&self.text, selection).to_string());
        self.transcript.push(Said::Person(instruction.to_string()));
        self.assistant_busy = true;
        self.keep_version(history::Kind::BeforeClaude);
        if let Some(assistant) = &self.assistant {
            assistant.ask(instruction.to_string(), selected);
        }
    }

    fn assistant_receive(&mut self, _ctx: &egui::Context) {
        if self.session.is_none() {
            self.session_private = false;
        }
        let Some(assistant) = &self.assistant else { return };
        let updates: Vec<Update> = assistant.updates.try_iter().collect();
        for update in updates {
            match update {
                Update::Connected => {}
                Update::Text(text) => match self.transcript.last_mut() {
                    Some(Said::Claude(said)) => said.push_str(&text),
                    _ => self.transcript.push(Said::Claude(text)),
                },
                Update::Action(action) => self.transcript.push(Said::Action(action)),
                Update::Usage(usage) => self.assistant_usage.add(usage),
                Update::Done => {
                    self.assistant_busy = false;
                    // In a moment: its last edit is still on its way here.
                    self.version_due = Some((self.time_now + 1.5, history::Kind::AfterClaude));
                }
                Update::Stopped => {
                    self.assistant_busy = false;
                    self.transcript.push(Said::Action(tx("stopped").to_string()));
                }
                Update::Undone => self.transcript.push(Said::Action(tx("what-claude-did-was-undone").to_string())),
                Update::Problem(message) => {
                    self.assistant_busy = false;
                    self.transcript.push(Said::Problem(message));
                }
                Update::Gone(reason) => {
                    self.assistant_busy = false;
                    self.assistant = None;
                    self.transcript.push(Said::Problem(reason));
                }
            }
        }
    }

    /// Sends Claude away, and with it the session that existed only for it.
    fn dismiss_claude(&mut self) {
        self.assistant = None;
        self.assistant_busy = false;
        if self.session_private {
            self.session = None;
            self.session_private = false;
            self.compile();
        }
    }

    /// The conversation with Claude, as lines.
    pub fn claude_said(&self) -> Vec<String> {
        self.transcript
            .iter()
            .map(|s| match s {
                Said::Person(t) | Said::Claude(t) | Said::Action(t) | Said::Problem(t) => t.clone(),
            })
            .collect()
    }

    pub fn claude_is_busy(&self) -> bool {
        self.assistant_busy
    }

    fn pane_claude(&mut self, ui: &mut Ui, t: &Tokens) {
        let ctx = ui.ctx().clone();
        let Some(_) = Self::api_connection() else {
            ui.label(tx("claude-joins-the-document-as-one-more"));
            ui.add_space(8.0);
            ui.label(tx("paste-an-anthropic-api-key-consoleanthropiccom"));
            ui.add(egui::TextEdit::singleline(&mut self.key_input).password(true).desired_width(f32::INFINITY).hint_text("sk-ant-…"));
            ui.add_space(4.0);
            if ui.add_enabled(!self.key_input.trim().is_empty(), egui::Button::new(tx("save-the-key"))).clicked() {
                if crate::recent::write_secret(KEY_FILE, self.key_input.trim()) {
                    self.key_input.clear();
                } else {
                    self.status = tx("couldnt-store-the-key").into();
                }
            }
            ui.add_space(8.0);
            ui.colored_label(t.text_dim, tx("the-key-stays-on-this-computer-and"));
            return;
        };
        for said in &self.transcript {
            match said {
                Said::Person(text) => {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(text).font(theme::ui_bold(12.5)).color(t.text));
                }
                Said::Claude(text) => {
                    ui.label(egui::RichText::new(text).color(t.text));
                }
                Said::Action(text) => {
                    ui.label(egui::RichText::new(text).font(theme::ui_font(11.5)).color(t.text_dim));
                }
                Said::Problem(text) => {
                    ui.label(egui::RichText::new(text).color(t.danger));
                }
            }
        }
        if self.transcript.is_empty() {
            ui.colored_label(t.text_dim, tx("ask-for-anything-in-the-document-you"));
        }
        ui.add_space(10.0);
        let input_id = Id::unique("claude-input");
        // Before the field sees it: Enter sends, and Shift+Enter is a new line.
        let enter = !self.assistant_busy && ui.memory(|m| m.has_focus(input_id)) && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        ui.add(
            egui::TextEdit::multiline(&mut self.assistant_input)
                .id(input_id)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text(tx("ask-claude-for-something")),
        );
        ui.horizontal(|ui| {
            if self.assistant_busy {
                if ui.button(tx("stop")).clicked()
                    && let Some(assistant) = &self.assistant
                {
                    assistant.stop();
                }
            } else {
                let ready = !self.assistant_input.trim().is_empty();
                if (ui.add_enabled(ready, egui::Button::new(tx("send"))).clicked() || enter) && ready {
                    let instruction = std::mem::take(&mut self.assistant_input);
                    self.ask_claude(&ctx, instruction.trim());
                }
            }
            if self.assistant.is_some()
                && ui.add_enabled(!self.assistant_busy, egui::Button::new(tx("undo-what-it-did"))).clicked()
                && let Some(assistant) = &self.assistant
            {
                assistant.undo_all();
            }
        });
        if self.assistant.is_some() && ui.add_enabled(!self.assistant_busy, egui::Button::new(tx("take-claude-out-of-the-document"))).clicked() {
            self.dismiss_claude();
        }
        ui.add_space(10.0);
        let u = self.assistant_usage;
        if u.input + u.output + u.cache_read > 0 {
            ui.colored_label(
                t.text_dim,
                tr!("tokens", input = (u.input + u.cache_write).to_string(), cached = u.cache_read.to_string(), output = u.output.to_string()),
            );
        }
        ui.colored_label(t.text_dim, tr!("claude-costs", model = api::MODEL));
        if crate::recent::read_setting(KEY_FILE).is_some() && ui.small_button(tx("remove-the-saved-key")).clicked() {
            crate::recent::remove_setting(KEY_FILE);
            self.dismiss_claude();
        }
    }

    /// Takes in what the others did, keeping this person's selection on the same words.
    fn session_receive(&mut self, ctx: &egui::Context) {
        let Some(session) = &self.session else { return };
        session.set_waker(ctx);
        let range = egui::TextEdit::load_state(ctx, Id::unique(EDITOR)).and_then(|s| s.cursor.char_range());
        let bytes = range.map(|r| {
            let (a, b): (usize, usize) = (r.primary.index.into(), r.secondary.index.into());
            (edit::byte_of(&self.text, a), edit::byte_of(&self.text, b))
        });
        let pins = bytes.and_then(|(a, b)| Some((session.pin(a)?, session.pin(b)?)));
        let Some(session) = &mut self.session else { return };
        let changes = session.poll();
        let mut ended = None;
        let mut text_changed = false;
        let mut recompile = false;
        for change in changes {
            match change {
                Change::Text => text_changed = true,
                Change::File => recompile = true,
                Change::Joined(name) => self.status = tr!("joined", name = name),
                Change::Left(name) => self.status = tr!("left", name = name),
                Change::Ended(reason) => ended = Some(reason),
            }
        }
        if text_changed {
            let others_edited = self.text != session.text();
            self.text = session.text().to_string();
            let new_range = pins.and_then(|(a, b)| Some((session.locate(&a)?, session.locate(&b)?)));
            let host = session.is_host();
            // The host's file holds the document: what the others type counts as a change to it.
            if others_edited && host {
                self.dirty = true;
                self.changed_at = ctx.input(|i| i.time);
            }
            if let Some((a, b)) = new_range {
                let (a, b) = (edit::char_of(&self.text, a), edit::char_of(&self.text, b));
                self.set_selection(ctx, (a.min(b), a.max(b)));
            }
            recompile = true;
        }
        if let Some(reason) = ended {
            // The text stays: what was typed isn't lost with the connection, and it has no file.
            let guest = self.session.as_ref().is_some_and(|s| !s.is_host());
            self.session = None;
            self.dirty |= guest;
            self.share_dialog = None;
            self.status = reason;
            recompile = true;
        }
        if recompile {
            self.compile();
        }
    }

    /// Sends what changed here this frame.
    fn session_send(&mut self, ctx: &egui::Context) {
        if self.session.is_none() {
            return;
        }
        let selection = self.selection(ctx);
        let Some(session) = &mut self.session else { return };
        session.sync_local(&self.text);
        session.send_cursor(edit::byte_of(&self.text, selection.0), edit::byte_of(&self.text, selection.1));
    }

    fn start_session(&mut self, ctx: &egui::Context) {
        let Some(path) = self.path.clone() else {
            self.status = tx("save-the-document-before-sharing-the-session").into();
            return;
        };
        let file_name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        match Session::host(&self.text, &self.user_name, &file_name) {
            Ok(session) => {
                session.set_waker(ctx);
                self.session = Some(session);
                self.share_dialog = Some(ShareDialog::Running { copied: false });
                self.compile();
            }
            Err(e) => self.status = e,
        }
    }

    fn join_session(&mut self, ctx: &egui::Context) {
        let Some(ShareDialog::Join { link }) = &self.share_dialog else { return };
        match Session::join(link, &self.user_name) {
            Ok(session) => {
                session.set_waker(ctx);
                self.session = Some(session);
                self.text.clear();
                self.path = None;
                self.dirty = false;
                self.pages.clear();
                self.textures.clear();
                self.backstage = None;
                self.set_selection(ctx, (0, 0));
                self.share_dialog = Some(ShareDialog::Running { copied: false });
                self.status = tx("connecting-to-the-session").into();
            }
            Err(e) => self.status = e,
        }
    }

    fn session_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.share_dialog.take() else { return };
        let mut keep = true;
        let mut action: Option<fn(&mut App, &egui::Context)> = None;
        let title = match dialog {
            ShareDialog::Start => tx("start-a-shared-session"),
            ShareDialog::Join { .. } => tx("join-a-session"),
            ShareDialog::Running { .. } => tx("shared-session"),
        };
        egui::Window::new(title).collapsible(false).resizable(false).default_width(460.0).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(
            ctx,
            |ui| {
                ui.set_width(460.0);
                match &mut dialog {
                    ShareDialog::Start => {
                        ui.label(tx("whoever-gets-the-link-can-edit-this"));
                        ui.label(tx("a-document-can-read-other-files-in"));
                        ui.add_space(8.0);
                        name_field(ui, &mut self.user_name);
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button(tx("start")).clicked() {
                                action = Some(|app, ctx| {
                                    crate::recent::write_setting("name.txt", &app.user_name);
                                    app.start_session(ctx);
                                });
                                keep = false;
                            }
                            if ui.button(tx("cancel")).clicked() {
                                keep = false;
                            }
                        });
                    }
                    ShareDialog::Join { link } => {
                        ui.label(tx("paste-the-link-sent-by-the-person"));
                        ui.add(egui::TextEdit::singleline(link).desired_width(f32::INFINITY).hint_text("pearl://…"));
                        ui.add_space(8.0);
                        name_field(ui, &mut self.user_name);
                        ui.add_space(4.0);
                        ui.label(tx("the-document-open-here-will-be-replaced"));
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button(tx("join")).clicked() {
                                action = Some(|app, ctx| {
                                    crate::recent::write_setting("name.txt", &app.user_name);
                                    app.request(ctx, Pending::Join);
                                });
                            }
                            if ui.button(tx("cancel")).clicked() {
                                keep = false;
                            }
                        });
                    }
                    ShareDialog::Running { copied } => {
                        let Some(session) = &self.session else {
                            keep = false;
                            return;
                        };
                        if self.session_private {
                            ui.label(tx("this-session-is-only-between-you-and"));
                            ui.label(tx("to-edit-with-other-people-take-claude"));
                            ui.add_space(8.0);
                        } else if let Some(link) = session.share_link() {
                            ui.label(tx("send-this-link-to-whoever-will-edit"));
                            let mut shown = link.to_string();
                            ui.add(egui::TextEdit::singleline(&mut shown).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
                            if ui.button(if *copied { tx("copied") } else { tx("copy-link") }).clicked() {
                                ctx.copy_text(link.to_string());
                                *copied = true;
                            }
                            ui.add_space(4.0);
                            ui.label(tx("works-on-the-same-network-over-the"));
                            ui.add_space(8.0);
                        } else if session.is_waiting() {
                            ui.label(tx("connecting"));
                        }
                        ui.label(egui::RichText::new(tx("people-in-the-session")).font(theme::ui_bold(13.0)));
                        ui.label(tr!("you", name = session.name()));
                        for name in session.others() {
                            ui.label(name);
                        }
                        ui.add_space(8.0);
                        let leave = if session.is_host() { tx("end-the-session") } else { tx("leave-the-session") };
                        ui.horizontal(|ui| {
                            if ui.button(tx("close")).clicked() {
                                keep = false;
                            }
                            if ui.button(leave).clicked() {
                                action = Some(|app, _| {
                                    let guest = app.session.as_ref().is_some_and(|s| !s.is_host());
                                    app.session = None;
                                    app.dirty |= guest;
                                    app.status = tx("you-left-the-session").into();
                                    app.compile();
                                });
                                keep = false;
                            }
                        });
                    }
                }
            },
        );
        if keep {
            self.share_dialog = Some(dialog);
        }
        if let Some(run) = action {
            run(self, ctx);
        }
    }

    fn show_window(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if !self.setup {
            self.setup = true;
            theme::apply(&ctx, self.theme);
            let _ = self.compiler.repaint.set(ctx.clone());
        }
        self.compiler.max_side.store(ctx.input(|i| i.max_texture_side), std::sync::atomic::Ordering::Relaxed);
        self.poll(&ctx);
        self.shortcuts(&ctx);
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(Pending::Quit);
        }
        let name = self.path.as_deref().and_then(Path::file_name).map_or_else(|| tx("untitled").to_string(), |n| n.to_string_lossy().into_owned());
        // Only when it changes: a viewport command wakes the window again, so sending it every frame
        // would keep an idle window redrawing.
        let title = format!("{}{name} — Pearl", if self.dirty { "• " } else { "" });
        if self.title != title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        self.autosave_tick(&ctx);
        self.recovery_tick(&ctx);
        self.spell_tick(&ctx);
        if self.backstage.is_some() {
            self.backstage_ui(ui);
            self.dialogs(&ctx);
            return;
        }
        let t = Tokens::get(&ctx);
        self.drop_hint(&ctx, &t);
        self.title_bar(ui, &t, &name);
        self.top_bar(ui, &t);
        // The foot is split off by hand: a bottom panel reserves what its content used, not what it
        // was asked for, and the central panel then painted over the messages.
        let area = ui.available_rect_before_wrap();
        ui.painter().hline(area.x_range(), area.top(), Stroke::new(1.0, t.ribbon_border));
        let foot = self.footer_height();
        let (main, bottom) = area.split_top_bottom_at_y(area.max.y - foot);
        ui.scope_builder(egui::UiBuilder::new().max_rect(bottom), |ui| self.footer(ui, &t, &name));
        // Each column in a place of its own: what one holds never pushes the next.
        let rail = Rect::from_min_size(main.min, vec2(RAIL_WIDTH, main.height()));
        let strip = Rect::from_min_max(pos2(main.right() - SIDE_STRIP, main.top()), main.max);
        let pane_w = if self.side.is_some() { SIDE_WIDTH.min((main.width() - RAIL_WIDTH - SIDE_STRIP - 200.0).max(0.0)) } else { 0.0 };
        let pane = Rect::from_min_max(pos2(strip.left() - pane_w, main.top()), pos2(strip.left(), main.bottom()));
        let middle = Rect::from_min_max(pos2(rail.right(), main.top()), pos2(pane.left(), main.bottom()));
        ui.scope_builder(egui::UiBuilder::new().max_rect(rail), |ui| self.view_rail(ui, rail, &t));
        ui.scope_builder(egui::UiBuilder::new().max_rect(strip), |ui| self.side_strip(ui, strip, &t));
        if let Some(side) = self.side.filter(|_| pane_w > 0.0) {
            ui.scope_builder(egui::UiBuilder::new().max_rect(pane), |ui| {
                ui.shrink_clip_rect(pane);
                ui.painter().rect_filled(pane, 0.0, t.ribbon);
                ui.painter().vline(pane.left(), pane.y_range(), Stroke::new(1.0, t.separator));
                self.side_pane(ui, side, pane, &t);
            });
        }
        ui.scope_builder(egui::UiBuilder::new().max_rect(middle), |ui| {
            ui.shrink_clip_rect(middle);
            match self.layout {
                Layout::Both => {
                    let half = ui.available_width() / 2.0;
                    egui::Panel::left("source")
                        .default_size(half)
                        .size_range(280.0..=(ui.available_width() - 280.0).max(280.0))
                        .frame(egui::Frame::NONE.fill(t.input_bg))
                        .show(ui, |ui| self.editor(ui, &t));
                    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.desk)).show(ui, |ui| self.preview(ui, &t));
                }
                Layout::Source => {
                    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.input_bg)).show(ui, |ui| self.editor(ui, &t));
                }
                Layout::Pages => {
                    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.desk)).show(ui, |ui| {
                        // Out of sight and out of the pointer's way, the editor still takes the keyboard:
                        // what's typed on a page has the same undo, typing help and sessions.
                        if self.page_typing {
                            let rect = Rect::from_min_size(ui.max_rect().min, vec2(ui.max_rect().width(), 1.0));
                            let mut hidden = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                            hidden.set_clip_rect(Rect::NOTHING);
                            self.editor(&mut hidden, &t);
                        }
                        self.preview(ui, &t);
                    });
                }
            }
        });
        self.dialogs(&ctx);
        if WINDOW_CONTROLS {
            resize_edges(&ctx);
        }
    }

    fn title_bar(&mut self, ui: &mut Ui, t: &Tokens, name: &str) {
        let ctx = ui.ctx().clone();
        let bar_h = TITLE_BAR_H;
        let ch = TITLE_CONTROL_H;
        egui::Panel::top("title_bar").resizable(false).show_separator_line(false).exact_size(bar_h).frame(egui::Frame::NONE.fill(t.window)).show(
            ui,
            |ui| {
                let rect = ui.max_rect();
                // Dragging the empty title area moves the window.
                let bg = ui.interact(rect, ui.scope_id().with("drag_title"), Sense::click_and_drag());
                if bg.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if bg.double_clicked() {
                    let max = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                }
                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(tx("autosave")).font(theme::ui_font(12.5)).color(t.text_dim));
                    let on = self.autosave;
                    let (r, resp) = ui.allocate_exact_size(vec2(30.0, 16.0), Sense::click());
                    ui.painter().rect_filled(r, 8.0, if on { t.accent } else { Color32::TRANSPARENT });
                    ui.painter().rect_stroke(r, 8.0, Stroke::new(1.0, if on { t.accent } else { t.text_dim }), egui::StrokeKind::Inside);
                    let knob = if on { pos2(r.right() - 8.0, r.center().y) } else { pos2(r.left() + 8.0, r.center().y) };
                    ui.painter().circle_filled(knob, 5.0, if on { t.on_accent } else { t.text_dim });
                    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, tx("autosave")));
                    if resp.on_hover_text(tx("saves-after-each-change-in-documents-that")).clicked() {
                        self.autosave = !on;
                    }
                    ui.add_space(6.0);
                    if icon_button_mono(ui, Icon::Save, t.text_dim, tx("save-s"), vec2(26.0, ch)).clicked() {
                        self.save(false);
                    }
                    if icon_button_mono(ui, Icon::Undo, t.text_dim, tx("undo-z"), vec2(26.0, ch)).clicked() {
                        self.undo(&ctx, false);
                    }
                    let undo_list = icon_button_mono(ui, Icon::Chevron, t.text_dim, tx("undo-several-steps"), vec2(14.0, ch));
                    egui::Popup::menu(&undo_list).show(|ui| {
                        for steps in [2, 5, 10, 20] {
                            if ui.button(tr!("undo-steps", n = steps)).clicked() {
                                for _ in 0..steps {
                                    self.undo(&ctx, false);
                                }
                                ui.close();
                            }
                        }
                    });
                    if icon_button_mono(ui, Icon::Redo, t.text_dim, tx("redo-shiftz"), vec2(26.0, ch)).clicked() {
                        self.undo(&ctx, true);
                    }
                    let more = icon_button_mono(ui, Icon::More, t.text_dim, tx("more-commands"), vec2(26.0, ch));
                    egui::Popup::menu(&more).show(|ui| {
                        ui.set_min_width(180.0);
                        self.file_menu(ui, &ctx);
                    });
                    let left_end = ui.min_rect().right();
                    let right = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if WINDOW_CONTROLS {
                            window_controls(ui, bar_h, t);
                            ui.add_space(4.0);
                        } else {
                            ui.add_space(8.0);
                        }
                        if icon_button_mono(ui, Icon::Search, t.text_dim, tx("find-in-the-document-f"), vec2(28.0, ch)).clicked() {
                            self.open_find();
                        }
                        let share = icon_button_mono(ui, Icon::Share, t.text_dim, tx("share-export-or-save-a-copy"), vec2(28.0, ch));
                        egui::Popup::menu(&share).show(|ui| {
                            ui.set_min_width(190.0);
                            if self.session.is_some() {
                                if ui.button(tx("see-the-session")).clicked() {
                                    self.share_dialog = Some(ShareDialog::Running { copied: false });
                                    ui.close();
                                }
                            } else {
                                if ui.button(tx("start-a-shared-session-2")).clicked() {
                                    if self.path.is_none() {
                                        self.save(true);
                                    }
                                    if self.path.is_some() {
                                        self.share_dialog = Some(ShareDialog::Start);
                                    }
                                    ui.close();
                                }
                                if ui.button(tx("join-a-session-2")).clicked() {
                                    self.share_dialog = Some(ShareDialog::Join { link: String::new() });
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.button(tx("export-as-pdf")).clicked() {
                                self.export_pdf();
                                ui.close();
                            }
                            if ui.button(tx("save-a-copy-typ")).clicked() {
                                self.save_copy();
                                ui.close();
                            }
                        });
                        if icon_button_mono(ui, Icon::Comment, t.text_dim, tx("comment-the-selected-lines"), vec2(28.0, ch)).clicked() {
                            self.edit(&ctx, edit::toggle_comment);
                        }
                        let theme_button = icon_button_mono(ui, Icon::Appearance, t.text_dim, tx("theme"), vec2(28.0, ch));
                        egui::Popup::menu(&theme_button).show(|ui| {
                            ui.set_min_width(120.0);
                            for choice in ThemeChoice::ALL {
                                if ui.selectable_label(self.theme == choice, choice.label()).clicked() {
                                    self.choose_theme(&ctx, choice);
                                    ui.close();
                                }
                            }
                        });
                        ui.min_rect().left()
                    });
                    // Centred title, cut with "…" so it never runs under the controls on either side.
                    let half = (rect.center().x - left_end).min(right.inner - rect.center().x) - 12.0;
                    let title = if self.dirty { format!("{name} •") } else { name.to_string() };
                    let mut job = egui::text::LayoutJob::simple_singleline(title, theme::ui_bold(13.0), t.text);
                    job.wrap = egui::text::TextWrapping::truncate_at_width((2.0 * half).max(0.0));
                    let galley = ui.painter().layout_job(job);
                    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, t.text);
                });
            },
        );
    }

    fn top_bar(&mut self, ui: &mut Ui, t: &Tokens) {
        let ctx = ui.ctx().clone();
        const MENU_H: f32 = 30.0;
        const GAP: f32 = 8.0;
        const ROW_H: f32 = 50.0;
        let h = MENU_H + if self.ribbon_collapsed { 0.0 } else { GAP + ROW_H + 2.0 };
        egui::Panel::top("ribbon")
            .resizable(false)
            .exact_size(h)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(t.window).inner_margin(egui::Margin { left: 10, right: 10, top: 0, bottom: 0 }))
            .show(ui, |ui| {
                let bar = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), MENU_H));
                ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let file = top_label(ui, tx("file"), theme::ui_font(13.0), t.accent, MENU_H, t);
                        if file.clicked() {
                            self.open_backstage(FilePage::Home);
                        }
                        for tab in TABS {
                            let active = self.tab == tab;
                            let (font, color) = if active { (theme::ui_bold(13.0), t.text) } else { (theme::ui_font(13.0), t.text_dim) };
                            let name = crate::i18n::tx(tab);
                            let resp = top_label(ui, name, font, color, MENU_H, t);
                            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, active, name));
                            // A click on the active tab, or a double click, folds the row away.
                            if resp.double_clicked() {
                                self.ribbon_collapsed = !self.ribbon_collapsed;
                            } else if resp.clicked() {
                                if active && !self.ribbon_collapsed {
                                    self.ribbon_collapsed = true;
                                } else {
                                    // By id, or by its name in the language on show.
                                    self.tab =
                                        TABS.iter().find(|id| **id == tab || tx(id) == tab).map_or_else(|| tab.to_string(), |id| (*id).to_string());
                                    self.ribbon_collapsed = false;
                                }
                            }
                        }
                        let help = top_label(ui, tx("help"), theme::ui_font(13.0), t.text, MENU_H, t);
                        egui::Popup::menu(&help).show(|ui| {
                            ui.set_min_width(230.0);
                            if ui.button(tx("documentation")).clicked() {
                                self.side = Some(Side::Docs);
                                ui.close();
                            }
                            for (label, url) in [
                                (tx("typst-documentation"), "https://typst.app/docs/"),
                                (tx("math-reference"), "https://typst.app/docs/reference/math/"),
                                (tx("packages-typst-universe"), "https://typst.app/universe/"),
                            ] {
                                if ui.button(label).clicked() {
                                    ctx.open_url(egui::OpenUrl::new_tab(url));
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.button(tx("about-pearl")).clicked() {
                                self.about = true;
                                ui.close();
                            }
                        });
                    });
                });
                if self.ribbon_collapsed {
                    return;
                }
                // The active tab's controls in one row, scrolling sideways when the window is narrower.
                let row = Rect::from_min_size(pos2(bar.left(), bar.bottom() + GAP), vec2(bar.width(), ROW_H));
                let tab = self.tab.clone();
                ui.scope_builder(egui::UiBuilder::new().max_rect(row), |ui| {
                    egui::ScrollArea::horizontal()
                        .id_salt(("toolbar_scroll", tab.as_str()))
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            ui.set_min_height(ROW_H);
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                                ui.set_height(crate::widgets::TOOL_ROW_H);
                                group_begin(ui);
                                self.ribbon(ui);
                            });
                        });
                });
            });
    }

    fn file_menu(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let item = |ui: &mut Ui, label: &str, key: &str| {
            ui.add(egui::Button::new(label).shortcut_text(key).frame_when_inactive(false).min_size(vec2(180.0, 24.0))).clicked()
        };
        if item(ui, tx("new"), "Ctrl+N") {
            self.request(ctx, Pending::New);
            ui.close();
        }
        if item(ui, tx("open"), "Ctrl+O") {
            self.request(ctx, Pending::Open);
            ui.close();
        }
        ui.separator();
        if item(ui, tx("save"), "Ctrl+S") {
            self.save(false);
            ui.close();
        }
        if item(ui, tx("save-as"), "Ctrl+Shift+S") {
            self.save(true);
            ui.close();
        }
        if item(ui, tx("export-pdf"), "Ctrl+E") {
            self.export_pdf();
            ui.close();
        }
    }

    fn ribbon(&mut self, ui: &mut Ui) {
        match self.tab.as_str() {
            "insert" => self.tab_insert(ui),
            "page-layout" => self.tab_layout(ui),
            "references" => self.tab_references(ui),
            "math" => self.tab_math(ui),
            "view" => self.tab_view(ui),
            _ => self.tab_home(ui),
        }
    }

    /// The font set by the document's `#set text(font: …)` rule, or Typst's default.
    fn current_font(&self) -> String {
        self.text
            .lines()
            .find_map(|l| l.strip_prefix("#set text(font: \"")?.split('"').next().map(str::to_string))
            .unwrap_or_else(|| "Libertinus Serif".to_string())
    }

    /// The caret line's style, from its heading marker.
    fn current_style(&self, ctx: &egui::Context) -> String {
        let (a, _) = self.selection(ctx);
        let byte = edit::byte_of(&self.text, a);
        let start = self.text.get(..byte).and_then(|t| t.rfind('\n')).map_or(0, |i| i + 1);
        let line = self.text.get(start..).unwrap_or_default();
        let level = line.chars().take_while(|c| *c == '=').count();
        match level {
            1..=6 if line.get(level..).is_some_and(|r| r.starts_with(' ')) => tr!("heading-level", level = level),
            _ if line.starts_with("- ") => tx(LIST_BULLETS).to_string(),
            _ if line.starts_with("+ ") => tx(LIST_NUMBERED).to_string(),
            _ if line.starts_with("/ ") => tx(LIST_TERMS).to_string(),
            _ => tx("normal").to_string(),
        }
    }

    fn tab_home(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if tool(ui, Icon::Paste, tx("paste-v")).clicked() {
            self.want_focus = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        }
        if tool(ui, Icon::Cut, tx("cut-x")).clicked() {
            let sel = self.selection(&ctx);
            if sel.0 != sel.1 {
                ctx.copy_text(edit::selected(&self.text, sel).to_string());
                self.edit(&ctx, edit::delete);
            }
        }
        if tool(ui, Icon::Copy, tx("copy-c")).clicked() {
            let sel = self.selection(&ctx);
            if sel.0 != sel.1 {
                ctx.copy_text(edit::selected(&self.text, sel).to_string());
            }
            self.want_focus = true;
        }
        sep(ui, tx("clipboard"));
        let current = self.current_font();
        field_menu(ui, &current, 135.0, tx("document-font"), |ui| {
            for font in ["Libertinus Serif", "New Computer Modern", "DejaVu Sans Mono"] {
                if ui.selectable_label(current == font, font).clicked() {
                    self.rule(&ctx, "#set text(font", &format!("#set text(font: \"{font}\")"));
                    ui.close();
                }
            }
        });
        field_menu(ui, tx("size"), 54.0, tx("size-of-the-selected-text"), |ui| {
            for size in [8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 32] {
                if ui.button(format!("{size} pt")).clicked() {
                    self.wrap(&ctx, &format!("#text(size: {size}pt)["), "]");
                    ui.close();
                }
            }
        });
        if tool(ui, Icon::Plus, tx("grow-font")).clicked() {
            self.wrap(&ctx, "#text(size: 1.2em)[", "]");
        }
        if tool(ui, Icon::Minus, tx("shrink-font")).clicked() {
            self.wrap(&ctx, "#text(size: 0.85em)[", "]");
        }
        ui.add_space(6.0);
        let wraps: [(Icon, &str, &str, &str); 4] = [
            (Icon::Bold, tx("bold-b"), "*", "*"),
            (Icon::Italic, tx("italic-i"), "_", "_"),
            (Icon::Underline, tx("underline-u"), "#underline[", "]"),
            (Icon::Strike, tx("strikethrough"), "#strike[", "]"),
        ];
        for (icon, tip, pre, post) in wraps {
            if tool(ui, icon, tip).clicked() {
                self.wrap(&ctx, pre, post);
            }
        }
        if glyphs(ui, "x₂", tx("subscript")).clicked() {
            self.wrap(&ctx, "#sub[", "]");
        }
        if glyphs(ui, "x²", tx("superscript")).clicked() {
            self.wrap(&ctx, "#super[", "]");
        }
        if glyphs(ui, "Aa", tx("small-caps")).clicked() {
            self.wrap(&ctx, "#smallcaps[", "]");
        }
        if tool(ui, Icon::Script, tx("inline-code")).clicked() {
            self.wrap(&ctx, "`", "`");
        }
        tool_menu(ui, Icon::Fill, tx("highlight"), |ui| {
            for (name, color) in [(tx("yellow"), "yellow"), (tx("green"), "lime"), (tx("blue"), "aqua"), (tx("pink"), "fuchsia")] {
                if ui.button(name).clicked() {
                    self.wrap(&ctx, &format!("#highlight(fill: {color})["), "]");
                    ui.close();
                }
            }
        });
        tool_menu(ui, Icon::FontColor, tx("text-colour"), |ui| {
            for (name, color) in [
                (tx("red"), "red"),
                (tx("blue"), "blue"),
                (tx("green"), "green"),
                (tx("orange"), "orange"),
                (tx("purple"), "purple"),
                (tx("grey"), "gray"),
            ] {
                if ui.button(name).clicked() {
                    self.wrap(&ctx, &format!("#text(fill: {color})["), "]");
                    ui.close();
                }
            }
        });
        sep(ui, tx("font"));
        for (icon, tip, align) in
            [(Icon::AlignLeft, tx("align-left"), "left"), (Icon::AlignCenter, tx("centre"), "center"), (Icon::AlignRight, tx("align-right"), "right")]
        {
            if tool(ui, icon, tip).clicked() {
                self.wrap(&ctx, &format!("#align({align})["), "]\n");
            }
        }
        if tool(ui, Icon::Wrap, tx("justify-the-document")).clicked() {
            self.rule(&ctx, "#set par(justify", "#set par(justify: true)");
        }
        ui.add_space(6.0);
        if tool(ui, Icon::SheetList, tx("bulleted-list")).clicked() {
            self.marker_line(&ctx, "- ");
        }
        if glyphs(ui, "1.", tx("numbered-list")).clicked() {
            self.marker_line(&ctx, "+ ");
        }
        sep(ui, tx("paragraph"));
        let style = self.current_style(&ctx);
        field_menu(ui, &style, 156.0, tx("style-of-the-carets-line"), |ui| {
            if ui.selectable_label(style == tx("normal"), tx("normal")).clicked() {
                self.clear_marker(&ctx);
                ui.close();
            }
            for (level, marker) in [(1, "= "), (2, "== "), (3, "=== "), (4, "==== ")] {
                let label = tr!("heading-level", level = level);
                if ui.selectable_label(style == label, label).clicked() {
                    self.marker_line(&ctx, marker);
                    ui.close();
                }
            }
            ui.separator();
            for (label, marker) in [(LIST_BULLETS, "- "), (LIST_NUMBERED, "+ "), (LIST_TERMS, "/ ")] {
                let label = tx(label);
                if ui.selectable_label(style == label, label).clicked() {
                    self.marker_line(&ctx, marker);
                    ui.close();
                }
            }
        });
        sep(ui, tx("styles"));
        if tool(ui, Icon::Find, tx("find-f")).clicked() {
            self.open_find();
        }
        if tool(ui, Icon::Print, tx("export-pdf-e")).clicked() {
            self.export_pdf();
        }
        group_end(ui, tx("editing"));
    }

    /// Removes the caret line's heading or list marker, whichever it has.
    fn clear_marker(&mut self, ctx: &egui::Context) {
        let (a, _) = self.selection(ctx);
        let byte = edit::byte_of(&self.text, a);
        let start = self.text[..byte].rfind('\n').map_or(0, |i| i + 1);
        let line = &self.text[start..];
        let eq = line.chars().take_while(|c| *c == '=').count();
        let marker = if eq > 0 && line[eq..].starts_with(' ') {
            Some(format!("{} ", "=".repeat(eq)))
        } else if line.starts_with("- ") {
            Some("- ".to_string())
        } else if line.starts_with("+ ") {
            Some("+ ".to_string())
        } else if line.starts_with("/ ") {
            Some("/ ".to_string())
        } else {
            None
        };
        if let Some(m) = marker {
            self.marker_line(ctx, &m);
        }
    }

    fn tab_insert(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if tool(ui, Icon::PageBreak, tx("page-break")).clicked() {
            self.insert(&ctx, PAGEBREAK);
        }
        sep(ui, tx("pages"));
        if tool(ui, Icon::Table, tx("insert-table")).clicked() {
            self.table_dialog = Some(TableDialog { rows: 3, cols: 3, header: true, figure: false });
        }
        sep(ui, tx("tables"));
        if tool(ui, Icon::Picture, tx("image-from-a-file")).clicked() {
            self.insert_image(&ctx);
        }
        if tool(ui, Icon::Shapes, tx("figure-with-caption")).clicked() {
            self.insert(&ctx, IMAGE_FIG);
        }
        sep(ui, tx("illustrations"));
        if tool(ui, Icon::Link, tx("insert-link")).clicked() {
            self.insert(&ctx, LINK);
        }
        sep(ui, tx("links"));
        if tool(ui, Icon::TextBox, tx("block-quote")).clicked() {
            self.insert(&ctx, QUOTE);
        }
        if tool(ui, Icon::Script, tx("code-block")).clicked() {
            self.insert(&ctx, CODE);
        }
        if tool(ui, Icon::Minus, tx("horizontal-line")).clicked() {
            self.insert(&ctx, LINE);
        }
        sep(ui, tx("text"));
        if tool(ui, Icon::Fx, tx("block-equation")).clicked() {
            self.insert(&ctx, EQ_BLOCK);
        }
        group_end(ui, tx("symbols"));
    }

    fn tab_layout(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        tool_menu(ui, Icon::Margins, tx("page-margins"), |ui| {
            for (name, m) in [(tx("normal"), "2.5cm"), (tx("narrow"), "1.27cm"), (tx("moderate"), "2cm"), (tx("wide"), "3.5cm")] {
                if ui.button(format!("{name} ({m})")).clicked() {
                    self.set_margin(&ctx, m);
                    ui.close();
                }
            }
            if ui.button("ABNT (3 / 2 cm)").clicked() {
                self.set_margin(&ctx, "(top: 3cm, left: 3cm, bottom: 2cm, right: 2cm)");
                ui.close();
            }
        });
        tool_menu(ui, Icon::Orient, tx("page-orientation"), |ui| {
            if ui.button(tx("portrait")).clicked() {
                self.rule(&ctx, "#set page(flipped", "#set page(flipped: false)");
                ui.close();
            }
            if ui.button(tx("landscape")).clicked() {
                self.rule(&ctx, "#set page(flipped", "#set page(flipped: true)");
                ui.close();
            }
        });
        tool_menu(ui, Icon::Paper, tx("paper-size"), |ui| {
            for (name, paper) in [("A4", "a4"), ("A5", "a5"), (tx("letter"), "us-letter"), (tx("legal"), "us-legal")] {
                if ui.button(name).clicked() {
                    self.rule(&ctx, "#set page(paper", &format!("#set page(paper: \"{paper}\")"));
                    ui.close();
                }
            }
        });
        tool_menu(ui, Icon::TextColumns, tx("document-columns"), |ui| {
            for n in 1..=3 {
                if ui.button(format!("{n}")).clicked() {
                    self.rule(&ctx, "#set page(columns", &format!("#set page(columns: {n})"));
                    ui.close();
                }
            }
        });
        sep(ui, tx("page-setup"));
        tool_menu(ui, Icon::PageNumber, tx("number-the-pages"), |ui| {
            for (name, format) in [("1", "1"), ("1 / N", "1 / 1"), ("i", "i")] {
                if ui.button(name).clicked() {
                    self.rule(&ctx, "#set page(numbering", &format!("#set page(numbering: \"{format}\")"));
                    ui.close();
                }
            }
        });
        tool_menu(ui, Icon::Name, tx("number-the-headings"), |ui| {
            for format in ["1.", "1.1", "I.", "A."] {
                if ui.button(format).clicked() {
                    self.rule(&ctx, "#set heading(numbering", &format!("#set heading(numbering: \"{format}\")"));
                    ui.close();
                }
            }
        });
        sep(ui, tx("numbering"));
        tool_menu(ui, Icon::AlignMiddle, tx("line-spacing"), |ui| {
            for (name, leading) in [(tx("single"), "0.65em"), ("1,15", "0.8em"), ("1,5", "1.05em"), (tx("double"), "1.4em")] {
                if ui.button(name).clicked() {
                    self.rule(&ctx, "#set par(leading", &format!("#set par(leading: {leading})"));
                    ui.close();
                }
            }
        });
        if glyphs(ui, "a-b", tx("hyphenate-the-document")).clicked() {
            self.rule(&ctx, "#set text(hyphenate", "#set text(hyphenate: true)");
        }
        sep(ui, tx("paragraph"));
        if glyphs(ui, tx("two-col"), tx("passage-in-two-columns")).clicked() {
            self.insert(&ctx, COLUMNS);
        }
        group_end(ui, tx("passages"));
    }

    fn tab_references(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if tool(ui, Icon::SheetList, tx("insert-table-of-contents")).clicked() {
            self.insert(&ctx, OUTLINE);
        }
        if tool(ui, Icon::Picture, tx("insert-list-of-figures")).clicked() {
            self.insert(&ctx, LIST_OF_FIGURES);
        }
        if tool(ui, Icon::Table, tx("insert-list-of-tables")).clicked() {
            self.insert(&ctx, LIST_OF_TABLES);
        }
        sep(ui, tx("contents"));
        if tool(ui, Icon::Note, tx("insert-footnote")).clicked() {
            self.insert(&ctx, FOOTNOTE);
        }
        sep(ui, tx("notes"));
        if tool(ui, Icon::Name, tx("figure-with-caption-and-label")).clicked() {
            self.insert(&ctx, LABEL);
        }
        if tool(ui, Icon::Link, tx("reference-to-a-label-rotulo")).clicked() {
            self.insert(&ctx, "@|rotulo");
        }
        sep(ui, tx("captions"));
        if glyphs(ui, "@", tx("cite-a-bibliography-key-chave")).clicked() {
            self.insert(&ctx, "@|chave");
        }
        tool_menu(ui, Icon::Book, tx("bibliography-from-refsbib-in-the-documents-folder"), |ui| {
            for (name, style) in
                [("ABNT", "associacao-brasileira-de-normas-tecnicas"), ("APA", "apa"), ("IEEE", "ieee"), ("Chicago", "chicago-author-date")]
            {
                if ui.button(name).clicked() {
                    self.insert(&ctx, &format!("#bibliography(\"refs.bib\", style: \"{style}\")\n|"));
                    ui.close();
                }
            }
        });
        if tool(ui, Icon::Find, tx("search-and-cite-references")).clicked() {
            self.side = Some(Side::References);
        }
        group_end(ui, tx("citations"));
    }

    fn tab_math(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if labelled(ui, Icon::Function, tx("ready-made-formulas"), tx("ready-made-formulas")).clicked() {
            self.open_formulas();
        }
        sep(ui, tx("formulas"));
        if tool(ui, Icon::Fx, tx("block-equation")).clicked() {
            self.insert(&ctx, EQ_BLOCK);
        }
        if glyphs(ui, "$x$", tx("equation-within-the-text")).clicked() {
            self.insert(&ctx, EQ_INLINE);
        }
        sep(ui, tx("equation"));
        for (mark, tip, snippet) in [
            ("a/b", tx("fraction"), FRACTION),
            ("√x", tx("square-root"), ROOT),
            ("Σ", tx("sum"), SUM),
            ("∫", tx("definite-integral"), INTEGRAL),
            ("lim", tx("limit"), LIMIT),
            ("[ ]", tx("matrix"), MATRIX),
            ("{", tx("piecewise-function"), CASES),
            ("v", tx("column-vector"), VECTOR),
            ("C", tx("binomial-coefficient"), BINOM),
        ] {
            if glyphs(ui, mark, tip).clicked() {
                self.insert(&ctx, snippet);
            }
        }
        sep(ui, tx("structures"));
        const SYMBOLS: [(&str, &str); 18] = [
            ("α", "alpha"),
            ("β", "beta"),
            ("γ", "gamma"),
            ("δ", "delta"),
            ("θ", "theta"),
            ("λ", "lambda"),
            ("μ", "mu"),
            ("π", "pi"),
            ("σ", "sigma"),
            ("∞", "infinity"),
            ("≤", "<="),
            ("≥", ">="),
            ("≠", "!="),
            ("±", "plus.minus"),
            ("×", "times"),
            ("→", "->"),
            ("∈", "in"),
            ("∂", "diff"),
        ];
        for (glyph, code) in SYMBOLS {
            if glyphs(ui, glyph, code).clicked() {
                self.insert(&ctx, &format!("{code} |"));
            }
        }
        group_end(ui, tx("symbols"));
    }

    fn tab_view(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        for (icon, label, tip, layout) in [
            (Icon::TextColumns, tx("side-by-side"), tx("source-and-pages"), Layout::Both),
            (Icon::Script, tx("source"), tx("source-only"), Layout::Source),
            (Icon::Paper, tx("pages"), tx("pages-only"), Layout::Pages),
        ] {
            if toggle(ui, icon, label, tip, self.layout == layout).clicked() {
                self.layout = layout;
            }
        }
        sep(ui, tx("panes"));
        if toggle(ui, Icon::Validation, tx("spelling"), tx("underlines-misspelt-words-portuguese-from-brazil"), self.spell_on).clicked() {
            self.set_spell(!self.spell_on);
        }
        if toggle(ui, Icon::Ruler, tx("ruler"), tx("rulers-in-centimetres-around-the-pages"), self.ruler).clicked() {
            self.set_ruler(!self.ruler);
        }
        sep(ui, tx("show"));
        if toggle(ui, Icon::Zoom, tx("width"), tx("fit-to-the-panes-width"), self.zoom.is_none()).clicked() {
            self.zoom = None;
        }
        if glyphs(ui, "100%", tx("actual-size")).clicked() {
            self.zoom = Some(1.0);
        }
        if tool(ui, Icon::Plus, tx("zoom-in")).clicked() {
            self.step_zoom(1);
        }
        if tool(ui, Icon::Minus, tx("zoom-out")).clicked() {
            self.step_zoom(-1);
        }
        sep(ui, tx("zoom"));
        if labelled(ui, Icon::Right, tx("go-to-the-page"), tx("shows-the-carets-passage-on-the-page")).clicked() {
            self.locate(&ctx);
        }
        sep(ui, tx("navigate"));
        for choice in [ThemeChoice::Light, ThemeChoice::Dark, ThemeChoice::Pearl] {
            if toggle(ui, Icon::Appearance, choice.label(), tx("interface-theme"), self.theme == choice).clicked() {
                self.choose_theme(&ctx, choice);
            }
        }
        group_end(ui, tx("theme"));
    }

    fn jump_to(&mut self, byte: usize) {
        self.pending_caret = Some(edit::char_of(&self.text, byte));
        self.scroll_caret = true;
        self.want_focus = true;
    }

    /// The left rail: how the source and the pages share the window, and the line numbers' switch.
    fn view_rail(&mut self, ui: &mut Ui, rect: Rect, t: &Tokens) {
        ui.painter().rect_filled(rect, 0.0, t.ribbon);
        ui.painter().vline(rect.right() - 1.0, rect.y_range(), Stroke::new(1.0, t.separator));
        let views = [
            (Icon::TextColumns, tx("side-by-side"), tx("source-and-pages-side-by-side"), Layout::Both),
            (Icon::Script, tx("source"), tx("source-only"), Layout::Source),
            (Icon::Paper, tx("pages"), tx("pages-only"), Layout::Pages),
        ];
        for (k, (icon, name, tip, layout)) in views.into_iter().enumerate() {
            let on = self.layout == layout;
            let slot = Rect::from_min_size(rect.min + vec2(0.0, 4.0 + 42.0 * k as f32), vec2(RAIL_WIDTH - 1.0, 40.0));
            let resp = ui.interact(slot, ui.scope_id().with(("rail", k)), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, name));
            if on {
                ui.painter().rect_filled(slot, 0.0, t.accent_soft);
                ui.painter().rect_filled(Rect::from_min_size(slot.min, vec2(3.0, slot.height())), 0.0, t.accent);
            } else if resp.hovered() {
                ui.painter().rect_filled(slot, 0.0, t.hover);
            }
            crate::icons::paint(ui.painter(), Rect::from_center_size(slot.center(), vec2(20.0, 20.0)), icon, t.text);
            if resp.on_hover_text(tip).clicked() {
                self.layout = layout;
            }
        }
        // Under the views, a hairline and the line numbers' switch.
        let top = rect.top() + 4.0 + 42.0 * views.len() as f32;
        ui.painter().hline((rect.left() + 10.0)..=(rect.right() - 11.0), top + 2.0, Stroke::new(1.0, t.separator));
        let slot = Rect::from_min_size(pos2(rect.left(), top + 6.0), vec2(RAIL_WIDTH - 1.0, 40.0));
        let shown = self.line_numbers;
        let resp = ui.interact(slot, ui.scope_id().with("rail-numbers"), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, shown, tx("line-numbers")));
        if shown {
            ui.painter().rect_filled(slot.shrink(4.0), 0.0, t.accent_soft);
        } else if resp.hovered() {
            ui.painter().rect_filled(slot.shrink(4.0), 0.0, t.hover);
        }
        ui.painter().text(slot.center(), egui::Align2::CENTER_CENTER, "12", theme::ui_font(13.0), t.text);
        if resp.on_hover_text(if shown { tx("hide-line-numbers") } else { tx("show-line-numbers") }).clicked() {
            self.line_numbers = !shown;
        }
    }

    /// The right strip: the side panes, their names turned a quarter clockwise.
    fn side_strip(&mut self, ui: &mut Ui, rect: Rect, t: &Tokens) {
        ui.painter().rect_filled(rect, 0.0, t.ribbon);
        ui.painter().vline(rect.left(), rect.y_range(), Stroke::new(1.0, t.separator));
        let errors = self.diags.iter().filter(|d| d.error).count();
        let mut top = rect.top() + 4.0;
        let items = [
            (Side::Details, Icon::Info, tx("details"), 0),
            (Side::Outline, Icon::SheetList, tx("outline"), 0),
            (Side::Files, Icon::Folder, tx("files"), 0),
            (Side::References, Icon::Book, tx("references"), 0),
            (Side::Symbols, Icon::Symbol, tx("formulas"), 0),
            (Side::Problems, Icon::Validation, tx("problems"), errors),
            (Side::Versions, Icon::Clock, tx("versions"), 0),
            (Side::Claude, Icon::Pen, "Claude", 0),
            (Side::Docs, Icon::Note, tx("documentation"), 0),
        ];
        let font = theme::ui_font(13.0);
        let full_height = |name: &str, badge: usize| {
            26.0 + ui.painter().layout_no_wrap(name.to_string(), font.clone(), t.text).size().x + if badge > 0 { 18.0 } else { 6.0 }
        };
        // Where the names don't all fit, the icons stand alone, and each says its name under the pointer.
        let needed: f32 = items.iter().map(|(_, _, name, badge)| full_height(name, *badge) + 2.0).sum();
        let compact = needed + 4.0 > rect.height();
        for (k, (side, icon, name, badge)) in items.into_iter().enumerate() {
            let on = self.side == Some(side);
            let galley = ui.painter().layout_no_wrap(name.to_string(), font.clone(), t.text);
            let height = if compact { 26.0 } else { full_height(name, badge) };
            let slot = Rect::from_min_size(pos2(rect.left() + 1.0, top), vec2(rect.width() - 1.0, height));
            top += height + 2.0;
            let resp = ui.interact(slot, ui.scope_id().with(("side", k)), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, name));
            if on {
                ui.painter().rect_filled(slot, 0.0, t.accent_soft);
                ui.painter().rect_filled(Rect::from_min_size(pos2(slot.right() - 3.0, slot.top()), vec2(3.0, slot.height())), 0.0, t.accent);
            } else if resp.hovered() {
                ui.painter().rect_filled(slot, 0.0, t.hover);
            }
            let x = slot.center().x;
            crate::icons::paint(ui.painter(), Rect::from_center_size(pos2(x, slot.top() + 12.0), vec2(14.0, 14.0)), icon, t.text);
            // Turned a quarter clockwise, it reads downwards and its height lies to the left of `at`.
            let at = pos2(x + galley.size().y / 2.0, slot.top() + 24.0);
            if !compact {
                ui.painter()
                    .add(egui::epaint::TextShape::new(at, galley, if on { t.accent } else { t.text }).with_angle(std::f32::consts::FRAC_PI_2));
            }
            if badge > 0 {
                let place = if compact { pos2(slot.right() - 7.0, slot.top() + 6.0) } else { pos2(x, slot.bottom() - 9.0) };
                ui.painter().text(place, egui::Align2::CENTER_CENTER, badge.to_string(), theme::ui_font(12.0), t.danger);
            }
            if resp.on_hover_text(name).clicked() {
                self.side = if on { None } else { Some(side) };
                if self.side == Some(Side::Files) {
                    self.read_files();
                }
            }
        }
    }

    fn side_pane(&mut self, ui: &mut Ui, side: Side, rect: Rect, t: &Tokens) {
        let (icon, title) = match side {
            Side::Details => (Icon::Info, tx("details")),
            Side::Outline => (Icon::SheetList, tx("outline")),
            Side::Files => (Icon::Folder, tx("files")),
            Side::References => (Icon::Book, tx("references")),
            Side::Docs => (Icon::Note, tx("documentation")),
            Side::Symbols => (Icon::Symbol, tx("formulas-and-symbols")),
            Side::Problems => (Icon::Validation, tx("problems")),
            Side::Versions => (Icon::Clock, tx("versions")),
            Side::Claude => (Icon::Pen, "Claude"),
        };
        let inner = rect.shrink2(vec2(10.0, 6.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            ui.horizontal(|ui| {
                ui.set_height(26.0);
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                crate::icons::paint(ui.painter(), r, icon, t.text);
                ui.label(egui::RichText::new(title).font(theme::ui_bold(13.0)).color(t.text));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if icon_button_mono(ui, Icon::Right, t.text_dim, tx("close-the-pane"), vec2(24.0, 22.0)).clicked() {
                        self.side = None;
                    }
                    if side == Side::Files && icon_button_mono(ui, Icon::Calc, t.text_dim, tx("refresh-the-list"), vec2(24.0, 22.0)).clicked() {
                        self.read_files();
                    }
                });
            });
            ui.separator();
            egui::ScrollArea::vertical().id_salt(("side-pane", title)).auto_shrink([false, false]).show(ui, |ui| match side {
                Side::Details => self.pane_details(ui, t),
                Side::Outline => self.pane_outline(ui, t),
                Side::Files => self.pane_files(ui, t),
                Side::References => self.pane_references(ui, t),
                Side::Docs => self.pane_docs(ui, t),
                Side::Symbols => self.pane_symbols(ui, t),
                Side::Versions => self.pane_versions(ui, t),
                Side::Claude => self.pane_claude(ui, t),
                Side::Problems => {
                    if self.diags.is_empty() {
                        ui.colored_label(t.text_dim, tx("no-problems-in-this-document"));
                    } else if let Some(byte) = self.problems_list(ui) {
                        self.jump_to(byte);
                    }
                }
            });
        });
    }

    fn pane_details(&self, ui: &mut Ui, t: &Tokens) {
        let chars = self.text.chars().count();
        let words = self.text.split_whitespace().count();
        let rows = [
            (tx("file"), self.path.as_deref().map_or_else(|| tx("not-saved-yet").to_string(), |p| p.display().to_string())),
            (tx("pages"), self.pages.len().to_string()),
            (tx("words-in-the-source"), words.to_string()),
            (tx("characters"), chars.to_string()),
            (tx("lines"), self.text.lines().count().to_string()),
            (
                tx("page-size"),
                self.pages
                    .first()
                    .map_or_else(|| "—".to_string(), |p| format!("{:.0} × {:.0} mm", p.width_pt * 25.4 / 72.0, p.height_pt * 25.4 / 72.0)),
            ),
            (tx("last-compile"), self.millis.map_or_else(|| "—".to_string(), |m| format!("{m} ms"))),
        ];
        for (name, value) in rows {
            ui.colored_label(t.text_dim, name);
            ui.add(egui::Label::new(egui::RichText::new(value).color(t.text)).wrap());
            ui.add_space(6.0);
        }
    }

    fn pane_outline(&mut self, ui: &mut Ui, t: &Tokens) {
        let headings = highlight::outline(&self.text);
        if headings.is_empty() {
            ui.colored_label(t.text_dim, tx("the-documents-headings-heading-appear-here"));
        }
        let mut jump = None;
        for h in &headings {
            ui.horizontal(|ui| {
                ui.add_space(12.0 * h.level.saturating_sub(1) as f32);
                let font = if h.level == 1 { theme::ui_bold(13.0) } else { theme::ui_font(13.0) };
                let button = egui::Button::new(egui::RichText::new(&h.title).font(font).color(t.text)).frame_when_inactive(false).truncate();
                if ui.add(button).on_hover_text(tx("go-to-this-heading")).clicked() {
                    jump = Some(h.byte);
                }
            });
        }
        if let Some(byte) = jump {
            self.jump_to(byte);
            self.compiler.send(Job::Locate { byte });
        }
    }

    /// Lists the document's folder, skipping hidden entries and build output, within a limit.
    fn read_files(&mut self) {
        const MAX_FILES: usize = 500;
        const MAX_DEPTH: usize = 3;
        self.files.clear();
        let Some(root) = self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf) else { return };
        let mut pending = vec![(root.clone(), 0)];
        while let Some((dir, depth)) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                let hidden = entry.file_name().to_string_lossy().starts_with('.');
                if hidden || self.path.as_deref() == Some(path.as_path()) || self.files.len() >= MAX_FILES {
                    continue;
                }
                if path.is_dir() {
                    if depth < MAX_DEPTH && entry.file_name() != "target" {
                        pending.push((path, depth + 1));
                    }
                } else if let Ok(relative) = path.strip_prefix(&root) {
                    self.files.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        self.files.sort();
    }

    fn pane_files(&mut self, ui: &mut Ui, t: &Tokens) {
        if self.path.is_none() {
            ui.colored_label(t.text_dim, tx("save-the-document-to-see-the-files"));
            return;
        }
        if self.files.is_empty() {
            ui.colored_label(t.text_dim, tx("there-are-no-other-files-in-the"));
            return;
        }
        ui.colored_label(t.text_dim, tx("click-to-insert-at-the-caret"));
        ui.add_space(4.0);
        let mut chosen = None;
        for file in &self.files {
            let button = egui::Button::new(egui::RichText::new(file).color(t.text)).frame_when_inactive(false).truncate();
            if ui.add(button).on_hover_text(file).clicked() {
                chosen = Some(edit::file_snippet(file));
            }
        }
        if let Some(snippet) = chosen {
            let ctx = ui.ctx().clone();
            self.edit(&ctx, |text, sel| edit::insert_plain(text, sel, &snippet));
        }
    }

    fn pane_symbols(&mut self, ui: &mut Ui, t: &Tokens) {
        let ctx = ui.ctx().clone();
        if self.formulas_tabs(ui) {
            self.pane_formulas(ui, t);
            return;
        }
        for (group, symbols) in SYMBOL_GROUPS {
            ui.colored_label(t.text_dim, tx(group));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
                for (glyph, code) in *symbols {
                    if glyphs(ui, glyph, code).clicked() {
                        self.edit(&ctx, |text, sel| edit::insert_plain(text, sel, &format!("{code} ")));
                    }
                }
            });
            ui.add_space(8.0);
        }
    }

    /// Switches the theme and remembers the choice for the next time.
    fn choose_theme(&mut self, ctx: &egui::Context, choice: ThemeChoice) {
        self.theme = choice;
        theme::set_choice(ctx, choice);
        choice.save();
    }

    fn open_find(&mut self) {
        if self.find.is_none() {
            self.find = Some(String::new());
        }
        self.find_focus = true;
    }

    fn find_step(&mut self, ctx: &egui::Context, backwards: bool) {
        let (a, b) = self.selection(ctx);
        self.find_from(ctx, if backwards { a } else { b }, backwards);
    }

    fn find_from(&mut self, ctx: &egui::Context, from: usize, backwards: bool) {
        let Some(query) = self.find.clone() else { return };
        if let Some(found) = edit::find(&self.text, &query, from, backwards) {
            self.set_selection(ctx, found);
            self.scroll_caret = true;
        }
    }

    /// Saves the source to a file of the user's choice, leaving the open document as it is.
    fn save_copy(&mut self) {
        let Some(path) = self.dialog().add_filter("Typst", &["typ"]).set_file_name("copia.typ").save_file() else { return };
        self.status = match std::fs::write(&path, &self.text) {
            Ok(()) => tr!("copy-saved", path = path.display().to_string()),
            Err(e) => tr!("cant-save", error = e.to_string()),
        };
    }

    /// Saves a changed document a moment after the last keystroke. A failure turns autosave off, so
    /// it isn't retried every frame, and says so.
    fn autosave_tick(&mut self, ctx: &egui::Context) {
        self.time_now = ctx.input(|i| i.time);
        if let Some((due, kind)) = self.version_due {
            if self.time_now >= due {
                self.version_due = None;
                self.keep_version(kind);
            } else {
                ctx.request_repaint_after_secs((due - self.time_now) as f32);
            }
        }
        if !(self.autosave && self.dirty && self.path.is_some()) {
            return;
        }
        let wait = AUTOSAVE_DELAY - (ctx.input(|i| i.time) - self.changed_at);
        if wait > 0.0 {
            ctx.request_repaint_after_secs(wait as f32);
        } else {
            self.autosaving = true;
            let saved = self.save(false);
            self.autosaving = false;
            if !saved {
                self.autosave = false;
            }
        }
    }

    /// Lets a picture brought into an unsaved document open the dialog to save it. Off in tests,
    /// where a dialog would wait for a person.
    pub fn enable_save_prompt(&mut self) {
        self.asks_to_save = true;
    }

    /// Starts keeping copies of unsaved text, and looks for what an earlier run left.
    pub fn enable_recovery(&mut self) {
        let Some(base) = history::data_folder() else { return };
        let dir = base.join("pearl").join("recovery");
        self.orphans = recovery::orphans(&dir);
        match Recovery::start(&dir) {
            Ok(run) => self.recovery = Some(run),
            Err(e) => self.status = tr!("no-recovery", error = e.to_string()),
        }
    }

    pub fn open_licenses(&mut self) {
        self.licenses = true;
    }

    /// The documents an earlier run left unsaved, by name.
    pub fn recoverable(&self) -> Vec<String> {
        self.orphans.iter().map(|o| document_name(o.document.as_deref())).collect()
    }

    pub fn has_recovery_copy(&self) -> bool {
        self.recovery.as_ref().is_some_and(Recovery::is_written)
    }

    /// The program is closing as it should.
    pub fn finish_recovery(&mut self) {
        if let Some(run) = self.recovery.take() {
            run.finish();
        }
    }

    fn write_recovery(&mut self) -> bool {
        let Some(run) = self.recovery.as_mut() else { return false };
        match run.write(self.path.as_deref(), &self.text) {
            Ok(()) => {
                self.recovery_written_at = self.time_now;
                true
            }
            Err(e) => {
                // Said once, not every few seconds.
                self.status = tr!("recovery-stopped", error = e.to_string());
                self.recovery = None;
                false
            }
        }
    }

    /// Keeps the copy of unsaved text current, at most every few seconds however fast the typing.
    fn recovery_tick(&mut self, ctx: &egui::Context) {
        let Some(run) = self.recovery.as_mut() else { return };
        if !self.dirty {
            run.clear();
            self.recovery_written_at = f64::NEG_INFINITY;
            return;
        }
        if self.changed_at <= self.recovery_written_at && run.is_written() {
            return;
        }
        let wait = self.recovery_written_at + RECOVERY_EVERY - self.time_now;
        if wait > 0.0 {
            ctx.request_repaint_after_secs(wait as f32);
        } else {
            self.write_recovery();
        }
    }

    /// Brings back a copy over a document with nothing unsaved. What the file says stays in its
    /// history, and the old copy goes only once this run has its own.
    fn recover(&mut self, ctx: &egui::Context, index: usize) {
        let orphan = self.orphans.remove(index);
        let name = document_name(orphan.document.as_deref());
        let file = orphan.document.clone().filter(|p| p.exists());
        let moved = orphan.document.is_some() && file.is_none();
        self.start_document(ctx, String::new());
        if let Some(path) = file {
            self.open_path(ctx, path);
        }
        self.text.clone_from(&orphan.text);
        self.dirty = true;
        self.time_now = ctx.input(|i| i.time);
        self.changed_at = self.time_now;
        self.set_selection(ctx, (0, 0));
        self.compile();
        if self.write_recovery() {
            orphan.discard();
        }
        self.status = if moved { tr!("recovered-moved", name = name) } else { tr!("recovered", name = name) };
    }

    fn recovery_dialog(&mut self, ctx: &egui::Context) {
        if self.orphans.is_empty() || self.orphans_later {
            return;
        }
        let blocked = if self.session.is_some() {
            Some(tx("leave-the-session-before-recovering"))
        } else if self.dirty {
            Some(tx("save-the-open-document-before-recovering"))
        } else {
            None
        };
        let now = history::now_ms();
        let (mut recover, mut discard, mut later) = (None, None, false);
        egui::Window::new(tx("recover-documents")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(
            ctx,
            |ui| {
                ui.label(tx("pearl-ended-without-saving-these-documents"));
                ui.add_space(6.0);
                for (i, orphan) in self.orphans.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(document_name(orphan.document.as_deref())).strong());
                            let lines = orphan.text.lines().count();
                            ui.small(tr!("recovery-entry", when = history::describe(orphan.stamp, now), n = lines));
                        });
                        ui.add_space(16.0);
                        let button = ui.add_enabled(blocked.is_none(), egui::Button::new(tx("recover")));
                        if button.clicked() {
                            recover = Some(i);
                        }
                        if let Some(why) = blocked {
                            button.on_disabled_hover_text(why);
                        }
                        if ui.button(tx("discard")).clicked() {
                            discard = Some(i);
                        }
                    });
                }
                ui.add_space(6.0);
                later = ui.button(tx("later")).on_hover_text(tx("asks-again-the-next-time-pearl-opens")).clicked();
            },
        );
        if let Some(i) = recover {
            self.recover(ctx, i);
        } else if let Some(i) = discard {
            self.orphans.remove(i).discard();
        }
        self.orphans_later = later;
    }

    pub fn set_spell(&mut self, on: bool) {
        self.spell_on = on;
        self.spell_menu = None;
        crate::recent::write_setting(SPELL_FILE, if on { "on" } else { "off" });
    }

    /// The words of the text on show known to be misspelt, in order.
    pub fn misspelt_words(&self) -> Vec<String> {
        if !self.spell_on || !spell::in_known_language(&self.text) {
            return Vec::new();
        }
        spell::document_words(&self.text)
            .into_iter()
            .filter_map(|r| self.text.get(r))
            .filter(|w| self.spell_known.get(*w) == Some(&false))
            .map(str::to_string)
            .collect()
    }

    /// Takes the dictionary's answers, and asks about new words once the typing pauses.
    fn spell_tick(&mut self, ctx: &egui::Context) {
        let Some(speller) = &self.spell else { return };
        let _ = speller.repaint.set(ctx.clone());
        let (mut fresh, mut failed) = (false, None);
        for event in speller.events.try_iter() {
            match event {
                spell::Event::Checked(answers) => {
                    fresh |= !answers.is_empty();
                    self.spell_known.extend(answers);
                }
                spell::Event::Suggested(word, found) => {
                    if let Some(menu) = self.spell_menu.as_mut().filter(|m| m.word == word) {
                        menu.suggestions = Some(found);
                    }
                }
                spell::Event::Failed(why) => failed = Some(why),
            }
        }
        self.spell_generation += u64::from(fresh);
        if let Some(why) = failed {
            self.status = tr!("spelling-stopped", error = why);
            self.spell = None;
            return;
        }
        if !self.spell_on {
            return;
        }
        let hash = egui::epaint::util::hash(&self.text);
        if self.spell_sent == Some(hash) {
            return;
        }
        let wait = self.changed_at + SPELL_DELAY - self.time_now;
        if wait > 0.0 {
            ctx.request_repaint_after_secs(wait as f32);
            return;
        }
        self.spell_sent = Some(hash);
        if !spell::in_known_language(&self.text) {
            return;
        }
        let mut new: Vec<String> = spell::document_words(&self.text)
            .into_iter()
            .filter_map(|r| self.text.get(r))
            .filter(|w| !self.spell_known.contains_key(*w))
            .map(str::to_string)
            .collect();
        new.sort_unstable();
        new.dedup();
        if !new.is_empty()
            && let Some(speller) = &self.spell
        {
            speller.check(new);
        }
    }

    /// Opens the menu of the misspelt word at this byte, if there's one there.
    pub fn open_spell_menu(&mut self, byte: usize, at: egui::Pos2) {
        let Some(range) = spell::word_at(&self.text, byte) else { return };
        let Some(word) = self.text.get(range.clone()).map(str::to_string) else { return };
        if !self.spell_on || self.spell_known.get(&word) != Some(&false) {
            return;
        }
        if let Some(speller) = &self.spell {
            speller.suggest(&word);
        }
        self.spell_menu = Some(SpellMenu { word, range, at, suggestions: None });
    }

    /// Replaces the word, unless someone in the session changed that text meanwhile.
    fn replace_word(&mut self, ctx: &egui::Context, menu: &SpellMenu, with: &str) {
        if self.text.get(menu.range.clone()) != Some(menu.word.as_str()) {
            self.status = tx("the-word-has-changed-meanwhile").to_string();
            return;
        }
        let range = menu.range.clone();
        self.edit(ctx, |text, _| {
            text.replace_range(range.clone(), with);
            let end = edit::char_of(text, range.start + with.len());
            (end, end)
        });
    }

    fn know_word(&mut self, word: &str, keep: bool) {
        self.spell_known.insert(word.to_string(), true);
        self.spell_generation += 1;
        if keep {
            self.spell_personal.insert(word.to_string());
            let mut words: Vec<&str> = self.spell_personal.iter().map(String::as_str).collect();
            words.sort_unstable();
            crate::recent::write_setting(PERSONAL_FILE, &words.join("\n"));
        }
    }

    fn spell_menu_ui(&mut self, ctx: &egui::Context) {
        let Some(menu) = self.spell_menu.take() else { return };
        let (mut replace, mut ignore, mut add) = (None, false, false);
        let area = egui::Area::new(Id::unique("spell-menu")).order(egui::Order::Foreground).fixed_pos(menu.at).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(170.0);
                match &menu.suggestions {
                    None => {
                        ui.weak(tx("looking"));
                    }
                    Some(found) if found.is_empty() => {
                        ui.weak(tx("no-suggestions"));
                    }
                    Some(found) => {
                        for suggestion in found {
                            if ui.button(egui::RichText::new(suggestion).strong()).clicked() {
                                replace = Some(suggestion.clone());
                            }
                        }
                    }
                }
                ui.separator();
                ignore = ui.button(tx("ignore")).on_hover_text(tx("until-pearl-closes")).clicked();
                add = ui.button(tx("add-to-the-dictionary")).on_hover_text(tx("on-this-computer")).clicked();
            });
        });
        let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p)));
        if let Some(with) = replace {
            self.replace_word(ctx, &menu, &with);
        } else if ignore {
            self.know_word(&menu.word, false);
        } else if add {
            self.know_word(&menu.word, true);
        } else if !outside && !ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.spell_menu = Some(menu);
        }
    }

    fn step_zoom(&mut self, dir: i32) {
        let z = self.zoom.unwrap_or(self.fit_zoom);
        let next = if dir > 0 {
            ZOOMS.iter().copied().find(|v| *v > z + 0.01).unwrap_or(2.5)
        } else {
            ZOOMS.iter().rev().copied().find(|v| *v < z - 0.01).unwrap_or(0.5)
        };
        self.zoom = Some(next);
    }

    fn editor(&mut self, ui: &mut Ui, t: &Tokens) {
        let ctx = ui.ctx().clone();
        let id = Id::unique(EDITOR);
        let font = FontId::monospace(13.0);
        // The cached colours belong to one palette; its keyword colour tells them apart.
        let palette = t.syntax.keyword;
        let tokens = *t;
        let mut scroll_to = None;
        let want_focus = std::mem::take(&mut self.want_focus);
        let pending = self.pending_caret.take();
        if let Some(c) = pending {
            self.set_selection(&ctx, (c, c));
        }
        // Coming back to the editor: with the focus before it draws, or it would collapse the
        // selection again, and with the one it had.
        if want_focus {
            if let Some(kept) = self.kept_selection
                && !ctx.memory(|m| m.has_focus(id))
            {
                self.set_selection(&ctx, kept);
            }
            ctx.memory_mut(|m| m.request_focus(id));
        }
        let scroll_caret = std::mem::take(&mut self.scroll_caret);
        let show_numbers = self.line_numbers;
        let scroll_target = self.selection(&ctx).0;
        let found = self.find.as_deref().map_or_else(Vec::new, |q| edit::find_all(&self.text, q, MAX_MATCHES_SHOWN));
        let current_match = self.find.as_ref().map(|_| self.selection(&ctx));
        let waiting = self.session.as_ref().is_some_and(Session::is_waiting);
        let carets = self.session.as_ref().map_or_else(Vec::new, Session::cursors);
        let spell_active = self.spell_on && self.spell.is_some() && spell::in_known_language(&self.text);
        let spell_generation = self.spell_generation;
        let App { text, colored, spell_known, .. } = self;
        let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, wrap: f32| {
            let src = buffer.as_str();
            let hash = egui::epaint::util::hash((src, spell_active, spell_generation));
            if !matches!(colored, Some((h, p, _)) if *h == hash && *p == palette) {
                let misspelt = |word: &str| spell_known.get(word) == Some(&false);
                let misspelt: Option<&dyn Fn(&str) -> bool> = if spell_active { Some(&misspelt) } else { None };
                *colored = Some((hash, palette, highlight::job(src, font.clone(), &tokens, misspelt)));
            }
            let mut job = colored.as_ref().map_or_else(egui::text::LayoutJob::default, |c| c.2.clone());
            job.wrap.max_width = wrap;
            ui.painter().layout_job(job)
        };
        let mut changed = false;
        let mut caret_rect = None;
        let mut focused = false;
        let mut hover_byte = None;
        let mut asked_about = None;
        let mut hover_pos = None;
        let mut kept = None;
        egui::ScrollArea::vertical().id_salt("source-scroll").auto_shrink([false, false]).show(ui, |ui| {
            let min_h = ui.available_height();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let (gutter, _) = ui.allocate_exact_size(vec2(if show_numbers { 46.0 } else { 8.0 }, min_h), Sense::hover());
                let rows_fit = (min_h / 18.0).ceil() as usize;
                let output = egui::TextEdit::multiline(text)
                    .id(id)
                    .frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 4, right: 10, top: 8, bottom: 8 }))
                    .font(egui::TextStyle::Monospace)
                    .code_editor()
                    .lock_focus(true)
                    .desired_width(f32::INFINITY)
                    .desired_rows(rows_fit)
                    .layouter(&mut layouter)
                    .interactive(!waiting)
                    .show(ui);
                output.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::MultilineTextInput, true, tx("source-2")));
                changed = output.response.changed();
                if want_focus {
                    output.response.request_focus();
                }
                // Line numbers: one per source line, on the first row it wraps into.
                let mut line = 1;
                let mut starts_line = true;
                let number = FontId::monospace(11.5);
                for placed in &output.galley.rows {
                    if starts_line && show_numbers {
                        let y = output.galley_pos.y + placed.pos.y + placed.row.size.y / 2.0;
                        ui.painter().text(pos2(gutter.max.x - 8.0, y), egui::Align2::RIGHT_CENTER, line.to_string(), number.clone(), t.header_text);
                        line += 1;
                    }
                    starts_line = placed.ends_with_newline;
                }
                // Everyone else's carets, each with their name above it.
                for caret in &carets {
                    let chars = edit::char_of(text, caret.byte);
                    let at = output.galley.pos_from_cursor(CCursor::new(chars)).translate(output.galley_pos.to_vec2());
                    let color = PEOPLE_COLORS.get((caret.id % PEOPLE_COLORS.len() as u64) as usize).copied().unwrap_or(t.accent);
                    let painter = ui.painter();
                    painter.rect_filled(Rect::from_min_size(at.min, vec2(2.0, at.height())), 0.0, color);
                    let name = painter.layout_no_wrap(caret.name.clone(), theme::ui_font(10.5), Color32::WHITE);
                    let tag = Rect::from_min_size(at.min - vec2(0.0, name.size().y + 1.0), name.size() + vec2(6.0, 1.0));
                    painter.rect_filled(tag, 0.0, color);
                    painter.galley(tag.min + vec2(3.0, 0.0), name, Color32::WHITE);
                }
                if let Some(range) = output.cursor_range {
                    caret_rect = Some(output.galley.pos_from_cursor(range.primary).translate(output.galley_pos.to_vec2()));
                }
                focused = output.response.has_focus();
                if focused && let Some(range) = output.cursor_range {
                    let (p, q): (usize, usize) = (range.primary.index.into(), range.secondary.index.into());
                    kept = Some((p.min(q), p.max(q)));
                }
                // What's under the pointer: nothing when it's off the text or past a line's end.
                if output.response.secondary_clicked()
                    && let Some(p) = output.response.interact_pointer_pos()
                {
                    let cursor = output.galley.cursor_from_pos(p - output.galley_pos);
                    asked_about = Some((edit::byte_of(text, cursor.index.into()), p));
                }
                hover_byte = ui.ctx().pointer_hover_pos().filter(|_| output.response.hovered()).and_then(|p| {
                    let cursor = output.galley.cursor_from_pos(p - output.galley_pos);
                    let at = output.galley.pos_from_cursor(cursor).translate(output.galley_pos.to_vec2());
                    let near = (p.y - at.center().y).abs() <= at.height() / 2.0 + 1.0 && (p.x - at.left()).abs() <= 12.0;
                    near.then(|| edit::byte_of(text, cursor.index.into()))
                });
                hover_pos = ui.ctx().pointer_hover_pos();
                // By the selection itself: the editor only gives its range while it has the focus, and
                // a search keeps the focus in its own field.
                if scroll_caret {
                    let caret = output.galley.pos_from_cursor(CCursor::new(scroll_target));
                    scroll_to = Some(caret.translate(output.galley_pos.to_vec2()));
                }
                // The search's matches, over the text: the editor shows a selection only with the
                // focus, and this way all of them can be seen.
                for (a, b) in &found {
                    let place = |c: usize| output.galley.pos_from_cursor(CCursor::new(c)).translate(output.galley_pos.to_vec2());
                    let current = current_match == Some((*a, *b));
                    let fill = if current { t.accent.gamma_multiply(0.55) } else { t.accent.gamma_multiply(0.22) };
                    let (from, to) = (place(*a), place(*b));
                    if (from.top() - to.top()).abs() < 1.0 {
                        ui.painter().rect_filled(Rect::from_min_max(from.min, to.max), 1.0, fill);
                    } else {
                        // Wrapped across rows: letter by letter, each on its own.
                        for c in *a..*b {
                            let (one, next) = (place(c), place(c + 1));
                            let right = if (one.top() - next.top()).abs() < 1.0 { next.left() } else { one.left() + 8.0 };
                            ui.painter().rect_filled(Rect::from_min_max(one.min, pos2(right, one.bottom())), 1.0, fill);
                        }
                    }
                }
            });
            if let Some(rect) = scroll_to {
                ui.scroll_to_rect(rect.expand(30.0), Some(Align::Center));
            }
        });
        if kept.is_some() {
            self.kept_selection = kept;
        }
        if changed {
            self.dirty = true;
            self.previewing = false;
            self.changed_at = ctx.input(|i| i.time);
            self.compile();
            if self.worth_completing(&ctx) {
                self.ask_completions(&ctx, false);
            } else {
                self.completions = None;
                self.completion_request += 1;
            }
        }
        // A list belongs to the place it was asked for: moving the caret or leaving closes it.
        let caret_now = self.selection(&ctx);
        let moved = self.completions.as_ref().is_some_and(|c| caret_now.0 != caret_now.1 || edit::byte_of(&self.text, caret_now.0) != c.caret);
        let pointer_on_list = self.completions.is_some() && ctx.input(|i| i.pointer.any_down());
        if !changed && (moved || (!focused && !pointer_on_list && !want_focus)) {
            self.completions = None;
        }
        self.hover_tick(&ctx, hover_byte, hover_pos, changed, t);
        if let Some((byte, at)) = asked_about {
            self.open_spell_menu(byte, at);
        }
        self.spell_menu_ui(&ctx);
        if let Some(caret) = caret_rect {
            self.completion_popup(&ctx, caret, t);
        }
    }

    /// Asks about what the pointer has rested on, and shows the answer beside it.
    fn hover_tick(&mut self, ctx: &egui::Context, byte: Option<usize>, pointer: Option<egui::Pos2>, changed: bool, t: &Tokens) {
        const REST: f64 = 0.4;
        let now = ctx.input(|i| i.time);
        // Typing, or a list on show, leaves no room for a tip: and the answer would be about old text.
        let busy = changed || self.completions.is_some() || ctx.input(|i| i.pointer.any_down());
        if busy || byte != self.hover.byte {
            self.hover.byte = if busy { None } else { byte };
            self.hover.since = now;
            self.hover.asked = false;
            self.hover.tip = None;
            self.hover.request += 1;
        }
        if let Some(at) = self.hover.byte.filter(|_| !self.previewing)
            && !self.hover.asked
        {
            let waited = now - self.hover.since;
            if waited >= REST {
                self.hover.asked = true;
                self.compiler.send(Job::Hover { request: self.hover.request, byte: at });
            } else {
                ctx.request_repaint_after_secs((REST - waited) as f32);
            }
        }
        let (Some((words, code, _)), Some(pointer)) = (&self.hover.tip, pointer) else { return };
        egui::Area::new(Id::unique("source-tip"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pointer + vec2(10.0, 16.0))
            .constrain(true)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).fill(t.menu_bg).stroke(Stroke::new(1.0, t.separator)).inner_margin(8).show(ui, |ui| {
                    ui.set_max_width(420.0);
                    // A long documentation entry is cut: this is a reminder, not the manual.
                    let shown: String = words.chars().take(600).collect();
                    let cut = shown.len() < words.len();
                    let text = if cut { format!("{shown}…") } else { shown };
                    let rich = egui::RichText::new(text).color(t.text);
                    let rich = if *code { rich.font(FontId::monospace(12.5)) } else { rich.font(theme::ui_font(12.5)) };
                    ui.add(egui::Label::new(rich).wrap());
                });
            });
    }

    /// The suggestions, under the caret (over it when there's no room below), with what the chosen
    /// one does beside the list's foot.
    fn completion_popup(&mut self, ctx: &egui::Context, caret: Rect, t: &Tokens) {
        const WIDTH: f32 = 360.0;
        const LIST_H: f32 = 200.0;
        const ROW_H: f32 = 24.0;
        let Some(list) = &self.completions else { return };
        let screen = ctx.content_rect();
        let selected = list.selected;
        let detail = list.suggestions.get(selected).and_then(|s| s.detail.clone());
        // Room for the list and two or three lines of description.
        let total = LIST_H + 70.0;
        let top = if caret.bottom() + total > screen.bottom() { caret.top() - total } else { caret.bottom() + 2.0 };
        let left = caret.left().min(screen.right() - WIDTH - 12.0).max(screen.left());
        let scroll_now = std::mem::take(&mut self.completion_scroll);
        let mut clicked = None;
        let Some(list) = &self.completions else { return };
        egui::Area::new(Id::unique("completions")).order(egui::Order::Foreground).fixed_pos(pos2(left, top)).constrain(true).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).fill(t.menu_bg).stroke(Stroke::new(1.0, t.separator)).inner_margin(4).show(ui, |ui| {
                ui.set_width(WIDTH);
                egui::ScrollArea::vertical().id_salt("completion-list").max_height(LIST_H).auto_shrink([false, true]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (i, s) in list.suggestions.iter().enumerate() {
                        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, i == selected, &s.label));
                        if i == selected {
                            ui.painter().rect_filled(rect, 0.0, t.accent_soft);
                            ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 0.0, t.accent);
                            if scroll_now {
                                ui.scroll_to_rect(rect, None);
                            }
                        } else if resp.hovered() {
                            ui.painter().rect_filled(rect, 0.0, t.hover);
                        }
                        let right = rect.right() - 74.0;
                        let name = ui.painter().with_clip_rect(Rect::from_min_max(rect.min, pos2(right, rect.bottom())).intersect(ui.clip_rect()));
                        name.text(pos2(rect.left() + 9.0, rect.center().y), egui::Align2::LEFT_CENTER, &s.label, FontId::monospace(13.0), t.text);
                        ui.painter().text(
                            pos2(rect.right() - 6.0, rect.center().y),
                            egui::Align2::RIGHT_CENTER,
                            s.kind,
                            theme::ui_font(11.5),
                            t.text_dim,
                        );
                        if resp.clicked() {
                            clicked = Some(i);
                        }
                    }
                });
                if let Some(detail) = detail {
                    ui.separator();
                    let text = detail.replace('`', "").replace("[", "").replace("]", "");
                    ui.add(egui::Label::new(egui::RichText::new(text).font(theme::ui_font(12.0)).color(t.text_dim)).wrap());
                }
            });
        });
        if let Some(index) = clicked {
            self.accept_completion(ctx, index);
        }
    }

    fn preview(&mut self, ui: &mut Ui, t: &Tokens) {
        if self.previewing
            && let Some(viewed) = &self.viewed
        {
            let when = history::describe(viewed.version.stamp, history::now_ms());
            let mut back = false;
            let mut restore = false;
            egui::Frame::NONE.fill(t.accent_soft).inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(tr!("version-banner", when = when)).color(t.text));
                    back = ui.button(tx("back-to-the-document")).clicked();
                    restore = ui.button(tx("restore")).clicked();
                });
            });
            let ctx = ui.ctx().clone();
            if back {
                self.preview_version(false);
            }
            if restore {
                self.restore_version(&ctx);
            }
        }
        if self.pages.is_empty() {
            ui.centered_and_justified(|ui| {
                let message = if !self.compiled_once() {
                    tx("compiling")
                } else if self.diags.iter().any(|d| d.error) {
                    tx("fix-the-error-shown-below-to-see")
                } else {
                    tx("nothing-to-show-yet")
                };
                ui.colored_label(t.text_dim, message);
            });
            return;
        }
        const PAD: f32 = 20.0;
        let area = ui.available_rect_before_wrap();
        let inset = if self.ruler { RULER } else { 0.0 };
        let pages_area = Rect::from_min_max(area.min + vec2(inset, inset), area.max);
        ui.advance_cursor_after_rect(area);
        let widest = self.pages.iter().map(|p| p.width_pt).fold(1.0, f32::max);
        self.fit_zoom = fit_zoom(pages_area.width() - 2.0 * PAD - 14.0, widest);
        // Ctrl + wheel over the pages.
        if ui.rect_contains_pointer(ui.max_rect()) {
            let delta = ui.input(|i| i.zoom_delta());
            if (delta - 1.0).abs() > f32::EPSILON {
                self.zoom = Some((self.zoom.unwrap_or(self.fit_zoom) * delta).clamp(MIN_ZOOM, MAX_ZOOM));
            }
        }
        let zoom = self.zoom.unwrap_or(self.fit_zoom);
        let scroll_page = self.scroll_page.take();
        let now = ui.input(|i| i.time);
        // The caret on the pages, while the keyboard writes in the source.
        let typing = !self.previewing && ui.ctx().memory(|m| m.has_focus(Id::unique(EDITOR)));
        if typing {
            let byte = edit::byte_of(&self.text, self.selection(ui.ctx()).0);
            if self.caret_asked != Some((byte, self.compiles)) {
                self.caret_asked = Some((byte, self.compiles));
                self.compiler.send(Job::Caret { byte });
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        }
        let page_caret = self.page_caret.filter(|_| typing);
        let follow = std::mem::take(&mut self.follow_caret);
        let mut click = None;
        let mut placed = Vec::with_capacity(self.pages.len());
        let mut figure_action = None;
        let figures_on = !self.previewing;
        let mut pages_ui = ui.new_child(egui::UiBuilder::new().max_rect(pages_area));
        let scrolled = egui::ScrollArea::both().id_salt("pages-scroll").auto_shrink([false, false]).show(&mut pages_ui, |ui| {
            ui.add_space(PAD);
            ui.spacing_mut().item_spacing = vec2(0.0, 14.0);
            ui.vertical_centered(|ui| {
                for (i, page) in self.pages.iter().enumerate() {
                    let size = vec2(page.width_pt, page.height_pt) * zoom;
                    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                    placed.push(rect);
                    if !ui.is_rect_visible(rect.expand(200.0)) && !scroll_page.is_some_and(|(p, _)| p == i) {
                        continue;
                    }
                    ui.painter().rect_filled(rect.translate(vec2(0.0, 2.0)).expand(1.0), 1.0, Color32::from_black_alpha(45));
                    if let Some(tex) = self.textures.get(&page.hash) {
                        ui.painter().image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    if resp.clicked()
                        && let Some(p) = resp.interact_pointer_pos()
                    {
                        let at = (p - rect.min) / zoom;
                        click = Some((i, at.x, at.y));
                    }
                    if figures_on && let Some(action) = figures::on_page(ui, &self.figures, i, rect, zoom, page.width_pt, t) {
                        figure_action = Some(action);
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                    }
                    if let Some((p, x, y, size)) = page_caret
                        && p == i
                    {
                        let base = rect.min + vec2(x, y) * zoom;
                        let line = Rect::from_min_max(base - vec2(0.75, size * 0.8 * zoom), base + vec2(0.75, size * 0.2 * zoom));
                        if (now * 2.0) as i64 % 2 == 0 {
                            ui.painter().rect_filled(line, 0.0, t.accent);
                        }
                        if follow {
                            ui.scroll_to_rect(line.expand(40.0), None);
                        }
                    }
                    if let Some((p, y)) = scroll_page
                        && p == i
                    {
                        let target = rect.min.y + y * zoom;
                        ui.scroll_to_rect(Rect::from_min_max(pos2(rect.min.x, target - 30.0), pos2(rect.max.x, target + 30.0)), Some(Align::Center));
                    }
                    if let Some((p, at, since)) = self.marker
                        && p == i
                    {
                        let age = (now - since) as f32;
                        if age < 1.5 {
                            let c = rect.min + at.to_vec2() * zoom;
                            ui.painter().circle_filled(c, 9.0, t.accent.gamma_multiply(0.45 * (1.0 - age / 1.5)));
                            ui.ctx().request_repaint();
                        }
                    }
                }
            });
            ui.add_space(PAD);
        });
        self.ruler_strips = None;
        self.margin_handles.clear();
        if self.ruler {
            self.paint_rulers(ui, area, scrolled.inner_rect, &placed, zoom, t);
        }
        self.page_rects = placed;
        self.figures_after_pages(ui.ctx(), figure_action);
        // A past version's pages don't correspond to the source on show.
        if let Some((page, x, y)) = click.filter(|_| !self.previewing) {
            self.compiler.send(Job::Click { page, x: f64::from(x), y: f64::from(y) });
        }
    }

    /// The rulers follow the pages: the top one measures the page most in view, and the side one
    /// every page passing by.
    fn paint_rulers(&mut self, ui: &Ui, area: Rect, view: Rect, placed: &[Rect], zoom: f32, t: &Tokens) {
        let top = Rect::from_min_max(pos2(view.left(), area.top()), pos2(view.right(), area.top() + RULER));
        let side = Rect::from_min_max(pos2(area.left(), view.top()), pos2(area.left() + RULER, view.bottom()));
        let p = ui.painter();
        p.rect_filled(Rect::from_min_size(area.min, vec2(RULER, RULER)), 0.0, t.window);
        let shown = |rect: &&Rect| rect.intersect(view).height() > 0.0;
        let visible = |rect: &Rect| rect.intersect(view).height();
        let main = self.pages.iter().zip(placed).max_by(|a, b| visible(a.1).total_cmp(&visible(b.1)));
        let across: Vec<_> =
            main.iter().map(|(page, rect)| ruler::Span::new(rect.left(), rect.right(), page.margins.map(|[l, r, ..]| (l, r)), zoom)).collect();
        let down: Vec<_> = self
            .pages
            .iter()
            .zip(placed)
            .filter(|(_, rect)| shown(rect))
            .map(|(page, rect)| ruler::Span::new(rect.top(), rect.bottom(), page.margins.map(|[_, _, a, b]| (a, b)), zoom))
            .collect();
        ruler::paint(p, top, true, &across, zoom, t);
        ruler::paint(p, side, false, &down, zoom, t);
        self.ruler_strips = Some((top, side));
        if self.previewing {
            return;
        }
        let page = self
            .pages
            .iter()
            .zip(placed)
            .max_by(|a, b| visible(a.1).total_cmp(&visible(b.1)))
            .and_then(|(page, rect)| page.margins.map(|m| (m, *rect)));
        let Some((margins, rect)) = page else { return };
        let across = ruler::Span::new(rect.left(), rect.right(), Some((margins[0], margins[1])), zoom);
        let down = ruler::Span::new(rect.top(), rect.bottom(), Some((margins[2], margins[3])), zoom);
        for (edge, strip, span, start, at, slot) in [
            (Edge::Left, top, across, true, across.margin_from, 0),
            (Edge::Right, top, across, false, across.margin_to, 1),
            (Edge::Top, side, down, true, down.margin_from, 2),
            (Edge::Bottom, side, down, false, down.margin_to, 3),
        ] {
            let horizontal = matches!(edge, Edge::Left | Edge::Right);
            let centre = if horizontal { pos2(at, strip.center().y) } else { pos2(strip.center().x, at) };
            if !strip.contains(centre) {
                continue;
            }
            self.margin_handles.push((edge, centre));
            let size = if horizontal { vec2(9.0, RULER) } else { vec2(RULER, 9.0) };
            let handle = ui.interact(Rect::from_center_size(centre, size), ui.scope_id().with(("margin", slot)), Sense::drag());
            if handle.hovered() || handle.dragged() {
                ui.ctx().set_cursor_icon(if horizontal { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical });
            }
            let pointer = handle.interact_pointer_pos();
            if handle.dragged()
                && let Some(pointer) = pointer
            {
                let value = span.dragged(if horizontal { pointer.x } else { pointer.y }, start, zoom);
                self.margin_drag = Some((edge, value));
                // Where the writing area would end, and how far that is, while nothing changes yet.
                let line = if start { span.from + value * zoom } else { span.to - value * zoom };
                let (a, b) = if horizontal {
                    (pos2(line, view.top()), pos2(line, view.bottom()))
                } else {
                    (pos2(view.left(), line), pos2(view.right(), line))
                };
                let guide = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, ui.scope_id().with("margin-guide")));
                guide.line_segment([a, b], Stroke::new(1.0, t.accent));
                let label = if horizontal { pos2(line + 6.0, view.top() + 4.0) } else { pos2(view.left() + 6.0, line + 4.0) };
                guide.text(label, egui::Align2::LEFT_TOP, ruler::in_cm(value), theme::ui_font(12.0), t.accent);
            }
            if handle.drag_stopped()
                && let Some((dragged, value)) = self.margin_drag.take()
                && dragged == edge
            {
                let mut new = margins;
                new[slot] = value;
                if ruler::in_cm(value) != ruler::in_cm(margins[slot]) {
                    self.set_margin(ui.ctx(), &ruler::margin_value(new));
                }
            }
        }
    }

    /// Writes the page's margins in the source, as one change.
    fn set_margin(&mut self, ctx: &egui::Context, value: &str) {
        self.edit(ctx, |text, sel| edit::set_page_margin(text, sel, value));
    }

    fn footer_height(&self) -> f32 {
        STATUS_H + self.problems_height()
    }

    fn problems_height(&self) -> f32 {
        if self.diags.is_empty() { 0.0 } else { (self.diags.len().min(5) as f32).mul_add(22.0, 10.0) }
    }

    /// The foot of the window: the compiler's messages, when there are any, over the status bar.
    fn footer(&mut self, ui: &mut Ui, t: &Tokens, name: &str) {
        let area = ui.max_rect();
        let (top, bottom) = area.split_top_bottom_at_y(area.max.y - STATUS_H);
        let mut jump = None;
        if self.problems_height() > 0.0 {
            ui.painter().rect_filled(top, 0.0, t.ribbon);
            ui.painter().hline(top.x_range(), top.min.y, Stroke::new(1.0, t.separator));
            ui.scope_builder(egui::UiBuilder::new().max_rect(top.shrink2(vec2(10.0, 5.0))), |ui| jump = self.problems(ui));
        }
        ui.painter().rect_filled(bottom, 0.0, t.status_bar);
        ui.scope_builder(egui::UiBuilder::new().max_rect(bottom.shrink2(vec2(10.0, 0.0))), |ui| self.status_bar(ui, t, name));
        if let Some(byte) = jump {
            self.jump_to(byte);
        }
    }

    /// The compiler's messages in the foot, scrolling; returns the place of the one clicked.
    fn problems(&self, ui: &mut Ui) -> Option<usize> {
        egui::ScrollArea::vertical().id_salt("problems-scroll").auto_shrink([false, false]).show(ui, |ui| self.problems_list(ui)).inner
    }

    /// The compiler's messages; returns the place of the one clicked.
    fn problems_list(&self, ui: &mut Ui) -> Option<usize> {
        let mut jump = None;
        {
            for d in &self.diags {
                let tokens = Tokens::get(ui.ctx());
                let (kind, color) = if d.error { (tx("error"), tokens.danger) } else { (tx("warning"), tokens.warning) };
                let place = d.byte.map_or_else(String::new, |b| format!(" {}", tr!("on-line", line = edit::line_of(&self.text, b))));
                ui.horizontal(|ui| {
                    ui.colored_label(color, egui::RichText::new(format!("{kind}{place}:")).font(theme::ui_bold(12.5)));
                    if ui.add(egui::Label::new(&d.message).sense(Sense::click())).on_hover_text(tx("go-to-the-passage")).clicked() {
                        jump = d.byte;
                    }
                });
            }
        }
        jump
    }

    fn status_bar(&mut self, ui: &mut Ui, t: &Tokens, name: &str) {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing = vec2(14.0, 0.0);
            let color = t.text_dim;
            let small = |ui: &mut Ui, s: &str| {
                ui.label(egui::RichText::new(s).font(theme::ui_font(12.0)).color(color));
            };
            small(ui, &format!("{}{name}", if self.dirty { "• " } else { "" }));
            let n = self.pages.len();
            small(ui, &tr!("pages-count", n = n));
            let words = self.text.split_whitespace().count();
            small(ui, &tr!("words-count", n = words));
            if let Some(session) = &self.session {
                let others = session.others().len();
                small(ui, &if session.is_waiting() { tx("connecting").to_string() } else { tr!("session-people", n = others + 1) });
            }
            small(ui, &self.status);
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let z = self.zoom.unwrap_or(self.fit_zoom);
                let percent = ui.add(
                    egui::Label::new(egui::RichText::new(format!("{:.0}%", z * 100.0)).font(theme::ui_font(12.0)).color(color)).sense(Sense::click()),
                );
                if percent.on_hover_text(tx("back-to-100-0")).clicked() {
                    self.zoom = Some(1.0);
                }
                if icon_button_mono(ui, Icon::Plus, t.text_dim, tx("zoom-in-2"), vec2(18.0, 18.0)).clicked() {
                    self.step_zoom(1);
                }
                ui.spacing_mut().slider_width = 110.0;
                let mut slider = (z * 100.0).round().clamp(MIN_ZOOM * 100.0, MAX_ZOOM * 100.0);
                let moved =
                    ui.add(egui::Slider::new(&mut slider, (MIN_ZOOM * 100.0)..=(MAX_ZOOM * 100.0)).show_value(false)).on_hover_text(tx("page-zoom"));
                if moved.changed() {
                    self.zoom = Some(slider / 100.0);
                }
                if icon_button_mono(ui, Icon::Minus, t.text_dim, tx("zoom-out-2"), vec2(18.0, 18.0)).clicked() {
                    self.step_zoom(-1);
                }
                ui.add_space(14.0);
                small(ui, "Typst 0.15");
            });
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        self.session_dialog(ctx);
        if self.about {
            let mut open = true;
            egui::Window::new(tx("about-pearl"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(format!("Pearl {}", env!("CARGO_PKG_VERSION")));
                    ui.label(tx("a-document-editor-in-rust-with-typesetting"));
                    ui.label("Scripta manent.");
                    ui.add_space(6.0);
                    if ui.link(tx("thirdparty-licences")).clicked() {
                        self.licenses = true;
                    }
                });
            self.about = open;
        }
        if self.licenses {
            let mut open = true;
            egui::Window::new(tx("thirdparty-licences"))
                .open(&mut open)
                .collapsible(false)
                .default_size([720.0, 520.0])
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(ctx.content_rect().center())
                .show(ctx, |ui| {
                    let lines = crate::licenses::lines();
                    let height = ui.text_style_height(&egui::TextStyle::Monospace);
                    egui::ScrollArea::both().id_salt("licenses").auto_shrink([false, false]).show_rows(ui, height, lines.len(), |ui, rows| {
                        for line in &lines[rows] {
                            ui.add(egui::Label::new(egui::RichText::new(*line).monospace()).extend());
                        }
                    });
                });
            self.licenses = open;
        }
        self.recovery_dialog(ctx);
        if let Some(mut query) = self.find.take() {
            let mut keep = true;
            let (mut next, mut previous, mut edited) = (false, false, false);
            let matches = edit::find_all(&self.text, &query, MAX_MATCHES_SHOWN);
            let selection = self.selection(ctx);
            let place = matches.iter().position(|m| *m == selection);
            egui::Window::new(tx("find")).collapsible(false).resizable(false).title_bar(false).anchor(egui::Align2::RIGHT_TOP, [-44.0, 134.0]).show(
                ctx,
                |ui| {
                    ui.horizontal(|ui| {
                        let field = ui.add(egui::TextEdit::singleline(&mut query).desired_width(200.0).hint_text(tx("find-in-the-document")));
                        if std::mem::take(&mut self.find_focus) {
                            field.request_focus();
                        }
                        edited = field.changed();
                        if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            next = !ui.input(|i| i.modifiers.shift);
                            previous = !next;
                            field.request_focus();
                        }
                        if ui.input(|i| i.key_pressed(Key::Escape)) {
                            keep = false;
                        }
                        let t = Tokens::get(ui.ctx());
                        let count = match (query.is_empty(), matches.len(), place) {
                            (true, ..) => String::new(),
                            (false, 0, _) => tx("none").to_string(),
                            (false, n, Some(i)) => tr!("match-of", at = i + 1, n = n),
                            (false, n, None) => format!("{n}"),
                        };
                        ui.add_sized(
                            [64.0, 20.0],
                            egui::Label::new(
                                egui::RichText::new(count).font(theme::ui_font(12.0)).color(if matches.is_empty() && !query.is_empty() {
                                    t.danger
                                } else {
                                    t.text_dim
                                }),
                            ),
                        );
                        previous |= icon_button_mono(ui, Icon::ChevronUp, t.text_dim, tx("previous-shiftenter"), vec2(24.0, 22.0)).clicked();
                        next |= icon_button_mono(ui, Icon::Chevron, t.text_dim, tx("next-enter"), vec2(24.0, 22.0)).clicked();
                        if icon_button_mono(ui, Icon::Close, t.text_dim, tx("close-esc"), vec2(24.0, 22.0)).clicked() {
                            keep = false;
                        }
                    });
                },
            );
            self.find = keep.then_some(query);
            if edited {
                // As it's typed, from where the current match or the caret begins.
                self.find_from(ctx, selection.0, false);
            } else if next || previous {
                self.find_step(ctx, previous);
            }
            if !keep {
                self.want_focus = true;
            }
        }
        if let Some(mut dialog) = self.table_dialog.take() {
            let mut keep = true;
            let mut insert = false;
            egui::Window::new(tx("insert-table")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(
                ctx,
                |ui| {
                    egui::Grid::new("table-size").spacing(vec2(10.0, 6.0)).show(ui, |ui| {
                        ui.label(tx("columns"));
                        ui.add(egui::DragValue::new(&mut dialog.cols).range(1..=26));
                        ui.end_row();
                        ui.label(tx("lines"));
                        ui.add(egui::DragValue::new(&mut dialog.rows).range(1..=100));
                        ui.end_row();
                    });
                    ui.checkbox(&mut dialog.header, tx("header-row"));
                    ui.checkbox(&mut dialog.figure, tx("with-caption"));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button(tx("insert")).clicked() {
                            insert = true;
                            keep = false;
                        }
                        if ui.button(tx("cancel")).clicked() {
                            keep = false;
                        }
                    });
                },
            );
            if insert {
                let table = edit::table_snippet(dialog.rows, dialog.cols, dialog.header);
                let snippet = if dialog.figure {
                    let body = table.trim_start_matches('#').trim_end().replace('\n', "\n  ");
                    format!("#figure(\n  {body},\n  caption: [Legenda],\n)\n")
                } else {
                    table
                };
                self.insert(ctx, &snippet);
            }
            if keep {
                self.table_dialog = Some(dialog);
            }
        }
        if let Some(action) = self.confirm {
            let mut answer = None;
            egui::Modal::new(Id::unique("save-changes")).show(ctx, |ui| {
                ui.set_width(340.0);
                ui.heading(tx("save-the-changes"));
                ui.label(tx("the-document-has-changes-that-havent-been"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(tx("save")).clicked() {
                        answer = Some(0);
                    }
                    if ui.button(tx("discard")).clicked() {
                        answer = Some(1);
                    }
                    if ui.button(tx("cancel")).clicked() {
                        answer = Some(2);
                    }
                });
            });
            match answer {
                Some(0) => {
                    self.confirm = None;
                    if self.save(false) {
                        self.perform(ctx, action);
                    }
                }
                Some(1) => {
                    self.confirm = None;
                    self.dirty = false;
                    self.perform(ctx, action);
                }
                Some(_) => self.confirm = None,
                None => {}
            }
        }
    }
}

/// The field for the name others see over this person's caret.
fn name_field(ui: &mut Ui, name: &mut String) {
    ui.horizontal(|ui| {
        ui.label(tx("your-name"));
        ui.add(egui::TextEdit::singleline(name).desired_width(220.0).hint_text(tx("how-others-will-see-you")));
    });
}

/// One of the top bar's labels: File, a tab or Help.
fn top_label(ui: &mut Ui, label: &str, font: FontId, color: Color32, menu_h: f32, t: &Tokens) -> egui::Response {
    let width = ui.painter().layout_no_wrap(label.to_string(), font.clone(), color).size().x;
    let (r, resp) = ui.allocate_exact_size(vec2(width + 18.0, menu_h - 4.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 0.0, t.hover);
    }
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, label, font, color);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    resp
}

/// Minimize, maximize (or restore) and close, flush with the title bar's right end, for a window
/// drawn without the system's frame. `ui` lays them out right to left.
fn window_controls(ui: &mut Ui, h: f32, t: &Tokens) {
    let maximized = ui.ctx().input(|i| i.viewport().maximized).unwrap_or(false);
    for (button, tip) in [("close", tx("close")), ("max", if maximized { tx("restore") } else { tx("maximize") }), ("min", tx("minimize"))] {
        let (r, resp) = ui.allocate_exact_size(vec2(46.0, h), Sense::click());
        let close = button == "close";
        if resp.hovered() {
            ui.painter().rect_filled(r, 0.0, if close { t.danger } else { t.hover });
        }
        let ink = if close && resp.hovered() { Color32::WHITE } else { t.text };
        let s = Stroke::new(1.0, ink);
        let c = r.center();
        let p = ui.painter();
        match button {
            "min" => {
                p.line_segment([c + vec2(-5.0, 0.0), c + vec2(5.0, 0.0)], s);
            }
            "max" => {
                p.rect_stroke(Rect::from_center_size(c, vec2(10.0, 10.0)), 0.0, s, egui::StrokeKind::Inside);
            }
            _ => {
                p.line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], s);
                p.line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], s);
            }
        }
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
        if resp.on_hover_text(tip).clicked() {
            ui.ctx().send_viewport_cmd(match button {
                "min" => egui::ViewportCommand::Minimized(true),
                "max" => egui::ViewportCommand::Maximized(!maximized),
                _ => egui::ViewportCommand::Close,
            });
        }
    }
}

/// Resizing from the edges and corners of a window drawn without the system's frame.
fn resize_edges(ctx: &egui::Context) {
    const EDGE: f32 = 5.0;
    let maximized = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
    let Some(p) = ctx.input(|i| i.pointer.hover_pos()) else { return };
    if maximized {
        return;
    }
    let r = ctx.content_rect();
    let (w, e) = (p.x - r.left() <= EDGE, r.right() - p.x <= EDGE);
    let (n, s) = (p.y - r.top() <= EDGE, r.bottom() - p.y <= EDGE);
    use egui::{CursorIcon as C, ResizeDirection as D};
    let (dir, cursor) = match (n, s, w, e) {
        (true, _, true, _) => (D::NorthWest, C::ResizeNwSe),
        (true, _, _, true) => (D::NorthEast, C::ResizeNeSw),
        (_, true, true, _) => (D::SouthWest, C::ResizeNeSw),
        (_, true, _, true) => (D::SouthEast, C::ResizeNwSe),
        (true, ..) => (D::North, C::ResizeVertical),
        (_, true, ..) => (D::South, C::ResizeVertical),
        (_, _, true, _) => (D::West, C::ResizeHorizontal),
        (_, _, _, true) => (D::East, C::ResizeHorizontal),
        _ => return,
    };
    ctx.set_cursor_icon(cursor);
    if ctx.input(|i| i.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.finish_recovery();
    }
}

/// The menu of a misspelt word.
struct SpellMenu {
    word: String,
    range: std::ops::Range<usize>,
    at: egui::Pos2,
    /// `None` until the dictionary answers.
    suggestions: Option<Vec<String>>,
}

fn spell_personal_words() -> std::collections::HashSet<String> {
    crate::recent::read_setting(PERSONAL_FILE).map_or_else(Default::default, |text| spell::parse_personal(&text))
}

fn document_name(path: Option<&Path>) -> String {
    path.and_then(Path::file_name).map_or_else(|| tx("untitled").to_string(), |n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_to_width_shrinks_freely_but_enlarges_only_so_far() {
        assert_eq!(fit_zoom(297.0, 595.0), 297.0 / 595.0);
        assert_eq!(fit_zoom(1800.0, 595.0), MAX_FIT_ZOOM);
        assert_eq!(fit_zoom(0.0, 595.0), 0.2);
        assert_eq!(fit_zoom(500.0, 0.0), MAX_FIT_ZOOM);
    }
}
