//! The window: tab strip, ribbon, source editor, live pages and status bar.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::text::{CCursor, CCursorRange};
use egui::{Align, Color32, FontId, Id, Key, KeyboardShortcut, Modifiers, Rect, Sense, Stroke, TextureHandle, TextureOptions, Ui, pos2, vec2};

use self::backstage::Page as FilePage;
use crate::collab::{Change, Session};
use crate::compiler::{Compiler, Diag, Event, Job, Page};
use crate::edit::{self, Sel};
use crate::highlight;
use crate::sample::SAMPLE;
use crate::icons::Icon;
use crate::theme::{self, ThemeChoice, Tokens};
use crate::widgets::{field_menu, glyphs, group_begin, group_end, icon_button_mono, labelled, sep, tool, tool_menu, toggle};

mod backstage;

const TABS: [&str; 6] = ["Página Inicial", "Inserir", "Layout da Página", "Referências", "Matemática", "Exibir"];
const RAIL_WIDTH: f32 = 44.0;
const SIDE_WIDTH: f32 = 320.0;
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
    EQ_INLINE, EQ_BLOCK, IMAGE_FIG, LINK, FOOTNOTE, PAGEBREAK, OUTLINE, LIST_OF_FIGURES, LIST_OF_TABLES, QUOTE, CODE, COLUMNS, LABEL, LINE, FRACTION, ROOT, SUM,
    INTEGRAL, LIMIT, MATRIX, CASES, VECTOR, BINOM,
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
    Symbols,
    Problems,
}

/// The Symbols pane: groups of the glyph shown and the Typst math code it inserts.
pub const SYMBOL_GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "Letras gregas",
        &[
            ("α", "alpha"), ("β", "beta"), ("γ", "gamma"), ("δ", "delta"), ("ε", "epsilon"), ("ζ", "zeta"), ("η", "eta"), ("θ", "theta"),
            ("λ", "lambda"), ("μ", "mu"), ("ν", "nu"), ("ξ", "xi"), ("π", "pi"), ("ρ", "rho"), ("σ", "sigma"), ("τ", "tau"),
            ("φ", "phi"), ("χ", "chi"), ("ψ", "psi"), ("ω", "omega"), ("Γ", "Gamma"), ("Δ", "Delta"), ("Θ", "Theta"), ("Λ", "Lambda"),
            ("Σ", "Sigma"), ("Φ", "Phi"), ("Ω", "Omega"),
        ],
    ),
    (
        "Operadores",
        &[
            ("±", "plus.minus"), ("×", "times"), ("÷", "div"), ("·", "dot.op"), ("∑", "sum"), ("∏", "product"), ("∫", "integral"),
            ("∂", "partial"), ("∇", "nabla"), ("∞", "infinity"), ("√", "sqrt(x)"),
        ],
    ),
    ("Relações", &[("≤", "<="), ("≥", ">="), ("≠", "!="), ("≈", "approx"), ("≡", "equiv"), ("∝", "prop")]),
    ("Setas", &[("→", "->"), ("←", "<-"), ("↔", "<->"), ("⇒", "=>"), ("⇔", "<=>"), ("↦", "|->")]),
    (
        "Conjuntos e lógica",
        &[
            ("∈", "in"), ("∉", "in.not"), ("⊂", "subset"), ("⊃", "supset"), ("∪", "union"), ("∩", "inter"), ("∅", "nothing"), ("∀", "forall"),
            ("∃", "exists"), ("∧", "and"), ("∨", "or"), ("¬", "not"),
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
    side: Option<Side>,
    line_numbers: bool,
    /// The Files pane's listing, read when the pane opens or on Refresh.
    files: Vec<String>,
    /// The File view, when open, on this page.
    backstage: Option<FilePage>,
    theme: ThemeChoice,
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
            tab: "Página Inicial".into(),
            layout: Layout::Both,
            zoom: None,
            fit_zoom: 1.0,
            pages: Vec::new(),
            textures: HashMap::new(),
            diags: Vec::new(),
            millis: None,
            status: "Carregando fontes…".into(),
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
            side: None,
            line_numbers: true,
            files: Vec::new(),
            backstage: None,
            theme: ThemeChoice::load(),
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
        app.compile();
        Ok(app)
    }

    pub fn set_tab(&mut self, tab: &str) {
        self.tab = tab.to_string();
    }

    /// Opens a side pane by its place on the strip.
    pub fn open_side(&mut self, index: usize) {
        self.side = [Side::Details, Side::Outline, Side::Files, Side::Symbols, Side::Problems].get(index).copied();
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
        self.compiler.send(Job::Compile { text: self.text.clone(), path });
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
                    self.status = format!("Compilado em {millis} ms");
                    self.diags = diags;
                }
                Event::JumpToSource { byte } => {
                    self.pending_caret = Some(edit::char_of(&self.text, byte));
                    self.scroll_caret = true;
                    self.want_focus = true;
                }
                Event::JumpToPage { page, y_pt, x_pt } => {
                    self.scroll_page = Some((page, y_pt));
                    self.marker = Some((page, pos2(x_pt, y_pt), ctx.input(|i| i.time)));
                }
                Event::Dependencies(files) => {
                    if let Some(session) = &self.session {
                        session.share_files(files);
                    }
                }
                Event::Exported(Ok(path)) => self.status = format!("PDF salvo em {}", path.display()),
                Event::Exported(Err(e)) => self.status = format!("Falha ao exportar: {e}"),
                Event::Failed(e) => self.status = e,
            }
        }
    }

    fn selection(&self, ctx: &egui::Context) -> Sel {
        let n = self.text.chars().count();
        let range = egui::TextEdit::load_state(ctx, Id::unique(EDITOR)).and_then(|s| s.cursor.char_range());
        let (a, b) = range.map_or((n, n), |r| {
            let (p, q): (usize, usize) = (r.primary.index.into(), r.secondary.index.into());
            (p.min(q), p.max(q))
        });
        (a.min(n), b.min(n))
    }

    fn set_selection(&self, ctx: &egui::Context, (a, b): Sel) {
        let id = Id::unique(EDITOR);
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
        egui::TextEdit::store_state(ctx, id, state);
    }

    fn edit(&mut self, ctx: &egui::Context, f: impl FnOnce(&mut String, Sel) -> Sel) {
        let sel = self.selection(ctx);
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
                self.set_selection(ctx, (0, 0));
                self.compile();
            }
            Err(e) => self.status = format!("Não foi possível abrir {}: {e}", path.display()),
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
                self.status = format!("Salvo em {}", path.display());
                self.path = Some(path);
                self.dirty = false;
                self.remember_path();
                self.compile();
                true
            }
            Err(e) => {
                self.status = format!("Não foi possível salvar: {e}");
                false
            }
        }
    }

    fn export_pdf(&mut self) {
        if self.diags.iter().any(|d| d.error) {
            self.status = "Corrija os erros antes de exportar: o PDF sairia desatualizado.".into();
            return;
        }
        let name = self.path.as_deref().and_then(Path::file_stem).map_or_else(|| "documento".to_string(), |s| s.to_string_lossy().into_owned());
        if let Some(path) = self.dialog().add_filter("PDF", &["pdf"]).set_file_name(format!("{name}.pdf")).save_file() {
            self.compiler.send(Job::ExportPdf(path));
        }
    }

    /// Inserts a picture from the document's folder, the only place Typst may read it from.
    fn insert_image(&mut self, ctx: &egui::Context) {
        let Some(dir) = self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf) else {
            self.status = "Salve o documento antes de inserir imagens: elas são lidas da pasta dele.".into();
            return;
        };
        let Some(file) = rfd::FileDialog::new().set_directory(&dir).add_filter("Imagens", &["png", "jpg", "jpeg", "gif", "svg", "webp"]).pick_file() else {
            return;
        };
        match file.strip_prefix(&dir) {
            Ok(relative) => {
                let relative = relative.to_string_lossy().replace('\\', "/");
                let escaped = relative.replace('"', "\\\"");
                let snippet = format!("#figure(\n  image(\"{escaped}\"),\n  caption: [Legenda],\n)\n");
                self.edit(ctx, |t, s| edit::insert_plain(t, s, &snippet));
            }
            Err(_) => self.status = "A imagem precisa estar na pasta do documento.".into(),
        }
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
        // The rest act on the editor and the pages, which the File view covers.
        if self.backstage.is_some() {
            return;
        }
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

    fn locate(&self, ctx: &egui::Context) {
        let (a, _) = self.selection(ctx);
        self.compiler.send(Job::Locate { byte: edit::byte_of(&self.text, a) });
    }

    // ---- Interface ---------------------------------------------------------------------------

    pub fn show(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.session_receive(&ctx);
        self.show_window(ui);
        self.session_send(&ctx);
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
                Change::Joined(name) => self.status = format!("{name} entrou na sessão."),
                Change::Left(name) => self.status = format!("{name} saiu da sessão."),
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
            self.status = "Salve o documento antes de compartilhar: a sessão usa o arquivo e a pasta dele.".into();
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
                self.status = "Conectando à sessão…".into();
            }
            Err(e) => self.status = e,
        }
    }

    fn session_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.share_dialog.take() else { return };
        let mut keep = true;
        let mut action: Option<fn(&mut App, &egui::Context)> = None;
        let title = match dialog {
            ShareDialog::Start => "Iniciar sessão compartilhada",
            ShareDialog::Join { .. } => "Entrar em uma sessão",
            ShareDialog::Running { .. } => "Sessão compartilhada",
        };
        egui::Window::new(title).collapsible(false).resizable(false).default_width(460.0).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_width(460.0);
            match &mut dialog {
                ShareDialog::Start => {
                    ui.label("Quem receber o link poderá editar este documento e verá os arquivos que ele usa.");
                    ui.label("Um documento pode ler outros arquivos da pasta dele. Compartilhe só com quem você confia.");
                    ui.add_space(8.0);
                    name_field(ui, &mut self.user_name);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Iniciar").clicked() {
                            action = Some(|app, ctx| {
                                crate::recent::write_setting("name.txt", &app.user_name);
                                app.start_session(ctx);
                            });
                            keep = false;
                        }
                        if ui.button("Cancelar").clicked() {
                            keep = false;
                        }
                    });
                }
                ShareDialog::Join { link } => {
                    ui.label("Cole o link que a pessoa que abriu a sessão enviou:");
                    ui.add(egui::TextEdit::singleline(link).desired_width(f32::INFINITY).hint_text("pearl://…"));
                    ui.add_space(8.0);
                    name_field(ui, &mut self.user_name);
                    ui.add_space(4.0);
                    ui.label("O documento aberto aqui será substituído pelo da sessão.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Entrar").clicked() {
                            action = Some(|app, ctx| {
                                crate::recent::write_setting("name.txt", &app.user_name);
                                app.request(ctx, Pending::Join);
                            });
                        }
                        if ui.button("Cancelar").clicked() {
                            keep = false;
                        }
                    });
                }
                ShareDialog::Running { copied } => {
                    let Some(session) = &self.session else {
                        keep = false;
                        return;
                    };
                    if let Some(link) = session.share_link() {
                        ui.label("Envie este link a quem vai editar com você:");
                        let mut shown = link.to_string();
                        ui.add(egui::TextEdit::singleline(&mut shown).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
                        if ui.button(if *copied { "Copiado" } else { "Copiar link" }).clicked() {
                            ctx.copy_text(link.to_string());
                            *copied = true;
                        }
                        ui.add_space(4.0);
                        ui.label("Funciona na mesma rede. Pela internet, use uma VPN como o Tailscale e troque o endereço pelo dela.");
                        ui.add_space(8.0);
                    } else if session.is_waiting() {
                        ui.label("Conectando…");
                    }
                    ui.label(egui::RichText::new("Pessoas na sessão").font(theme::ui_bold(13.0)));
                    ui.label(format!("{} (você)", session.name()));
                    for name in session.others() {
                        ui.label(name);
                    }
                    ui.add_space(8.0);
                    let leave = if session.is_host() { "Encerrar a sessão" } else { "Sair da sessão" };
                    ui.horizontal(|ui| {
                        if ui.button("Fechar").clicked() {
                            keep = false;
                        }
                        if ui.button(leave).clicked() {
                            action = Some(|app, _| {
                                let guest = app.session.as_ref().is_some_and(|s| !s.is_host());
                                app.session = None;
                                app.dirty |= guest;
                                app.status = "Você saiu da sessão.".into();
                                app.compile();
                            });
                            keep = false;
                        }
                    });
                }
            }
        });
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
        let name = self.path.as_deref().and_then(Path::file_name).map_or_else(|| "Sem título".to_string(), |n| n.to_string_lossy().into_owned());
        // Only when it changes: a viewport command wakes the window again, so sending it every frame
        // would keep an idle window redrawing.
        let title = format!("{}{name} — Pearl", if self.dirty { "• " } else { "" });
        if self.title != title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        self.autosave_tick(&ctx);
        if self.backstage.is_some() {
            self.backstage_ui(ui);
            self.dialogs(&ctx);
            return;
        }
        let t = Tokens::get(&ctx);
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
                    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.desk)).show(ui, |ui| self.preview(ui, &t));
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
        egui::Panel::top("title_bar").resizable(false).show_separator_line(false).exact_size(bar_h).frame(egui::Frame::NONE.fill(t.window)).show(ui, |ui| {
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
                ui.label(egui::RichText::new("Salvamento Automático").font(theme::ui_font(12.5)).color(t.text_dim));
                let on = self.autosave;
                let (r, resp) = ui.allocate_exact_size(vec2(30.0, 16.0), Sense::click());
                ui.painter().rect_filled(r, 8.0, if on { t.accent } else { Color32::TRANSPARENT });
                ui.painter().rect_stroke(r, 8.0, Stroke::new(1.0, if on { t.accent } else { t.text_dim }), egui::StrokeKind::Inside);
                let knob = if on { pos2(r.right() - 8.0, r.center().y) } else { pos2(r.left() + 8.0, r.center().y) };
                ui.painter().circle_filled(knob, 5.0, if on { t.on_accent } else { t.text_dim });
                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, "Salvamento Automático"));
                if resp.on_hover_text("Salva depois de cada alteração, em documentos que já têm um arquivo").clicked() {
                    self.autosave = !on;
                }
                ui.add_space(6.0);
                if icon_button_mono(ui, Icon::Save, t.text_dim, "Salvar (Ctrl+S)", vec2(26.0, ch)).clicked() {
                    self.save(false);
                }
                if icon_button_mono(ui, Icon::Undo, t.text_dim, "Desfazer (Ctrl+Z)", vec2(26.0, ch)).clicked() {
                    self.undo(&ctx, false);
                }
                let undo_list = icon_button_mono(ui, Icon::Chevron, t.text_dim, "Desfazer vários passos", vec2(14.0, ch));
                egui::Popup::menu(&undo_list).show(|ui| {
                    for steps in [2, 5, 10, 20] {
                        if ui.button(format!("Desfazer {steps} passos")).clicked() {
                            for _ in 0..steps {
                                self.undo(&ctx, false);
                            }
                            ui.close();
                        }
                    }
                });
                if icon_button_mono(ui, Icon::Redo, t.text_dim, "Refazer (Ctrl+Shift+Z)", vec2(26.0, ch)).clicked() {
                    self.undo(&ctx, true);
                }
                let more = icon_button_mono(ui, Icon::More, t.text_dim, "Mais comandos", vec2(26.0, ch));
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
                    if icon_button_mono(ui, Icon::Search, t.text_dim, "Localizar no documento (Ctrl+F)", vec2(28.0, ch)).clicked() {
                        self.open_find();
                    }
                    let share = icon_button_mono(ui, Icon::Share, t.text_dim, "Compartilhar: exportar ou salvar uma cópia", vec2(28.0, ch));
                    egui::Popup::menu(&share).show(|ui| {
                        ui.set_min_width(190.0);
                        if self.session.is_some() {
                            if ui.button("Ver a sessão…").clicked() {
                                self.share_dialog = Some(ShareDialog::Running { copied: false });
                                ui.close();
                            }
                        } else {
                            if ui.button("Iniciar sessão compartilhada…").clicked() {
                                if self.path.is_none() {
                                    self.save(true);
                                }
                                if self.path.is_some() {
                                    self.share_dialog = Some(ShareDialog::Start);
                                }
                                ui.close();
                            }
                            if ui.button("Entrar em uma sessão…").clicked() {
                                self.share_dialog = Some(ShareDialog::Join { link: String::new() });
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button("Exportar como PDF…").clicked() {
                            self.export_pdf();
                            ui.close();
                        }
                        if ui.button("Salvar uma cópia (.typ)…").clicked() {
                            self.save_copy();
                            ui.close();
                        }
                    });
                    if icon_button_mono(ui, Icon::Comment, t.text_dim, "Comentar as linhas selecionadas", vec2(28.0, ch)).clicked() {
                        self.edit(&ctx, edit::toggle_comment);
                    }
                    let theme_button = icon_button_mono(ui, Icon::Appearance, t.text_dim, "Tema", vec2(28.0, ch));
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
        });
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
                        let file = top_label(ui, "Arquivo", theme::ui_font(13.0), t.accent, MENU_H, t);
                        if file.clicked() {
                            self.open_backstage(FilePage::Home);
                        }
                        for tab in TABS {
                            let active = self.tab == tab;
                            let (font, color) = if active { (theme::ui_bold(13.0), t.text) } else { (theme::ui_font(13.0), t.text_dim) };
                            let resp = top_label(ui, tab, font, color, MENU_H, t);
                            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, active, tab));
                            // A click on the active tab, or a double click, folds the row away.
                            if resp.double_clicked() {
                                self.ribbon_collapsed = !self.ribbon_collapsed;
                            } else if resp.clicked() {
                                if active && !self.ribbon_collapsed {
                                    self.ribbon_collapsed = true;
                                } else {
                                    self.tab = tab.to_string();
                                    self.ribbon_collapsed = false;
                                }
                            }
                        }
                        let help = top_label(ui, "Ajuda", theme::ui_font(13.0), t.text, MENU_H, t);
                        egui::Popup::menu(&help).show(|ui| {
                            ui.set_min_width(230.0);
                            for (label, url) in [
                                ("Documentação do Typst", "https://typst.app/docs/"),
                                ("Referência de matemática", "https://typst.app/docs/reference/math/"),
                                ("Pacotes (Typst Universe)", "https://typst.app/universe/"),
                            ] {
                                if ui.button(label).clicked() {
                                    ctx.open_url(egui::OpenUrl::new_tab(url));
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.button("Sobre o Pearl").clicked() {
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
        let item = |ui: &mut Ui, label: &str, key: &str| ui.add(egui::Button::new(label).shortcut_text(key).frame_when_inactive(false).min_size(vec2(180.0, 24.0))).clicked();
        if item(ui, "Novo", "Ctrl+N") {
            self.request(ctx, Pending::New);
            ui.close();
        }
        if item(ui, "Abrir…", "Ctrl+O") {
            self.request(ctx, Pending::Open);
            ui.close();
        }
        ui.separator();
        if item(ui, "Salvar", "Ctrl+S") {
            self.save(false);
            ui.close();
        }
        if item(ui, "Salvar como…", "Ctrl+Shift+S") {
            self.save(true);
            ui.close();
        }
        if item(ui, "Exportar PDF…", "Ctrl+E") {
            self.export_pdf();
            ui.close();
        }
    }

    fn ribbon(&mut self, ui: &mut Ui) {
        match self.tab.as_str() {
            "Inserir" => self.tab_insert(ui),
            "Layout da Página" => self.tab_layout(ui),
            "Referências" => self.tab_references(ui),
            "Matemática" => self.tab_math(ui),
            "Exibir" => self.tab_view(ui),
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
            1..=6 if line.get(level..).is_some_and(|r| r.starts_with(' ')) => format!("Título {level}"),
            _ => "Normal".to_string(),
        }
    }

    fn tab_home(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if tool(ui, Icon::Paste, "Colar (Ctrl+V)").clicked() {
            self.want_focus = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        }
        if tool(ui, Icon::Cut, "Recortar (Ctrl+X)").clicked() {
            let sel = self.selection(&ctx);
            if sel.0 != sel.1 {
                ctx.copy_text(edit::selected(&self.text, sel).to_string());
                self.edit(&ctx, edit::delete);
            }
        }
        if tool(ui, Icon::Copy, "Copiar (Ctrl+C)").clicked() {
            let sel = self.selection(&ctx);
            if sel.0 != sel.1 {
                ctx.copy_text(edit::selected(&self.text, sel).to_string());
            }
            self.want_focus = true;
        }
        sep(ui, "Área de Transferência");
        let current = self.current_font();
        field_menu(ui, &current, 135.0, "Fonte do documento", |ui| {
            for font in ["Libertinus Serif", "New Computer Modern", "DejaVu Sans Mono"] {
                if ui.selectable_label(current == font, font).clicked() {
                    self.rule(&ctx, "#set text(font", &format!("#set text(font: \"{font}\")"));
                    ui.close();
                }
            }
        });
        field_menu(ui, "Tam.", 54.0, "Tamanho do texto selecionado", |ui| {
            for size in [8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 32] {
                if ui.button(format!("{size} pt")).clicked() {
                    self.wrap(&ctx, &format!("#text(size: {size}pt)["), "]");
                    ui.close();
                }
            }
        });
        if tool(ui, Icon::Plus, "Aumentar a fonte").clicked() {
            self.wrap(&ctx, "#text(size: 1.2em)[", "]");
        }
        if tool(ui, Icon::Minus, "Diminuir a fonte").clicked() {
            self.wrap(&ctx, "#text(size: 0.85em)[", "]");
        }
        ui.add_space(6.0);
        const WRAPS: [(Icon, &str, &str, &str); 4] = [
            (Icon::Bold, "Negrito (Ctrl+B)", "*", "*"),
            (Icon::Italic, "Itálico (Ctrl+I)", "_", "_"),
            (Icon::Underline, "Sublinhado (Ctrl+U)", "#underline[", "]"),
            (Icon::Strike, "Tachado", "#strike[", "]"),
        ];
        for (icon, tip, pre, post) in WRAPS {
            if tool(ui, icon, tip).clicked() {
                self.wrap(&ctx, pre, post);
            }
        }
        if glyphs(ui, "x₂", "Subscrito").clicked() {
            self.wrap(&ctx, "#sub[", "]");
        }
        if glyphs(ui, "x²", "Sobrescrito").clicked() {
            self.wrap(&ctx, "#super[", "]");
        }
        if glyphs(ui, "Aa", "Versalete").clicked() {
            self.wrap(&ctx, "#smallcaps[", "]");
        }
        if tool(ui, Icon::Script, "Código na linha").clicked() {
            self.wrap(&ctx, "`", "`");
        }
        tool_menu(ui, Icon::Fill, "Realce", |ui| {
            for (name, color) in [("Amarelo", "yellow"), ("Verde", "lime"), ("Azul", "aqua"), ("Rosa", "fuchsia")] {
                if ui.button(name).clicked() {
                    self.wrap(&ctx, &format!("#highlight(fill: {color})["), "]");
                    ui.close();
                }
            }
        });
        tool_menu(ui, Icon::FontColor, "Cor do texto", |ui| {
            for (name, color) in [("Vermelho", "red"), ("Azul", "blue"), ("Verde", "green"), ("Laranja", "orange"), ("Roxo", "purple"), ("Cinza", "gray")] {
                if ui.button(name).clicked() {
                    self.wrap(&ctx, &format!("#text(fill: {color})["), "]");
                    ui.close();
                }
            }
        });
        sep(ui, "Fonte");
        for (icon, tip, align) in [(Icon::AlignLeft, "Alinhar à esquerda", "left"), (Icon::AlignCenter, "Centralizar", "center"), (Icon::AlignRight, "Alinhar à direita", "right")] {
            if tool(ui, icon, tip).clicked() {
                self.wrap(&ctx, &format!("#align({align})["), "]\n");
            }
        }
        if tool(ui, Icon::Wrap, "Justificar o documento").clicked() {
            self.rule(&ctx, "#set par(justify", "#set par(justify: true)");
        }
        ui.add_space(6.0);
        if tool(ui, Icon::SheetList, "Lista com marcadores").clicked() {
            self.marker_line(&ctx, "- ");
        }
        if glyphs(ui, "1.", "Lista numerada").clicked() {
            self.marker_line(&ctx, "+ ");
        }
        sep(ui, "Parágrafo");
        let style = self.current_style(&ctx);
        field_menu(ui, &style, 100.0, "Estilo da linha do cursor", |ui| {
            if ui.selectable_label(style == "Normal", "Normal").clicked() {
                self.clear_marker(&ctx);
                ui.close();
            }
            for (label, marker) in [("Título 1", "= "), ("Título 2", "== "), ("Título 3", "=== "), ("Título 4", "==== ")] {
                if ui.selectable_label(style == label, label).clicked() {
                    self.marker_line(&ctx, marker);
                    ui.close();
                }
            }
        });
        sep(ui, "Estilos");
        if tool(ui, Icon::Find, "Localizar (Ctrl+F)").clicked() {
            self.open_find();
        }
        if tool(ui, Icon::Print, "Exportar PDF (Ctrl+E)").clicked() {
            self.export_pdf();
        }
        group_end(ui, "Edição");
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
        } else {
            None
        };
        if let Some(m) = marker {
            self.marker_line(ctx, &m);
        }
    }

    fn tab_insert(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if tool(ui, Icon::PageBreak, "Quebra de página").clicked() {
            self.insert(&ctx, PAGEBREAK);
        }
        sep(ui, "Páginas");
        if tool(ui, Icon::Table, "Inserir tabela").clicked() {
            self.table_dialog = Some(TableDialog { rows: 3, cols: 3, header: true, figure: false });
        }
        sep(ui, "Tabelas");
        if tool(ui, Icon::Picture, "Imagem da pasta do documento").clicked() {
            self.insert_image(&ctx);
        }
        if tool(ui, Icon::Shapes, "Figura com legenda").clicked() {
            self.insert(&ctx, IMAGE_FIG);
        }
        sep(ui, "Ilustrações");
        if tool(ui, Icon::Link, "Inserir link").clicked() {
            self.insert(&ctx, LINK);
        }
        sep(ui, "Links");
        if tool(ui, Icon::TextBox, "Bloco de citação").clicked() {
            self.insert(&ctx, QUOTE);
        }
        if tool(ui, Icon::Script, "Bloco de código").clicked() {
            self.insert(&ctx, CODE);
        }
        if tool(ui, Icon::Minus, "Linha horizontal").clicked() {
            self.insert(&ctx, LINE);
        }
        sep(ui, "Texto");
        if tool(ui, Icon::Fx, "Equação em bloco").clicked() {
            self.insert(&ctx, EQ_BLOCK);
        }
        group_end(ui, "Símbolos");
    }

    fn tab_layout(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        tool_menu(ui, Icon::Margins, "Margens da página", |ui| {
            for (name, m) in [("Normal", "2.5cm"), ("Estreita", "1.27cm"), ("Moderada", "2cm"), ("Larga", "3.5cm")] {
                if ui.button(format!("{name} ({m})")).clicked() {
                    self.rule(&ctx, "#set page(margin", &format!("#set page(margin: {m})"));
                    ui.close();
                }
            }
            if ui.button("ABNT (3 / 2 cm)").clicked() {
                self.rule(&ctx, "#set page(margin", "#set page(margin: (top: 3cm, left: 3cm, bottom: 2cm, right: 2cm))");
                ui.close();
            }
        });
        tool_menu(ui, Icon::Orient, "Orientação da página", |ui| {
            if ui.button("Retrato").clicked() {
                self.rule(&ctx, "#set page(flipped", "#set page(flipped: false)");
                ui.close();
            }
            if ui.button("Paisagem").clicked() {
                self.rule(&ctx, "#set page(flipped", "#set page(flipped: true)");
                ui.close();
            }
        });
        tool_menu(ui, Icon::Paper, "Tamanho do papel", |ui| {
            for (name, paper) in [("A4", "a4"), ("A5", "a5"), ("Carta", "us-letter"), ("Ofício", "us-legal")] {
                if ui.button(name).clicked() {
                    self.rule(&ctx, "#set page(paper", &format!("#set page(paper: \"{paper}\")"));
                    ui.close();
                }
            }
        });
        tool_menu(ui, Icon::TextColumns, "Colunas do documento", |ui| {
            for n in 1..=3 {
                if ui.button(format!("{n}")).clicked() {
                    self.rule(&ctx, "#set page(columns", &format!("#set page(columns: {n})"));
                    ui.close();
                }
            }
        });
        sep(ui, "Configurar Página");
        tool_menu(ui, Icon::PageNumber, "Numerar as páginas", |ui| {
            for (name, format) in [("1", "1"), ("1 / N", "1 / 1"), ("i", "i")] {
                if ui.button(name).clicked() {
                    self.rule(&ctx, "#set page(numbering", &format!("#set page(numbering: \"{format}\")"));
                    ui.close();
                }
            }
        });
        tool_menu(ui, Icon::Name, "Numerar títulos", |ui| {
            for format in ["1.", "1.1", "I.", "A."] {
                if ui.button(format).clicked() {
                    self.rule(&ctx, "#set heading(numbering", &format!("#set heading(numbering: \"{format}\")"));
                    ui.close();
                }
            }
        });
        sep(ui, "Numeração");
        tool_menu(ui, Icon::AlignMiddle, "Espaço entre linhas", |ui| {
            for (name, leading) in [("Simples", "0.65em"), ("1,15", "0.8em"), ("1,5", "1.05em"), ("Duplo", "1.4em")] {
                if ui.button(name).clicked() {
                    self.rule(&ctx, "#set par(leading", &format!("#set par(leading: {leading})"));
                    ui.close();
                }
            }
        });
        if glyphs(ui, "a-b", "Hifenizar o documento").clicked() {
            self.rule(&ctx, "#set text(hyphenate", "#set text(hyphenate: true)");
        }
        sep(ui, "Parágrafo");
        if glyphs(ui, "2 col.", "Trecho em duas colunas").clicked() {
            self.insert(&ctx, COLUMNS);
        }
        group_end(ui, "Trechos");
    }

    fn tab_references(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if tool(ui, Icon::SheetList, "Inserir sumário").clicked() {
            self.insert(&ctx, OUTLINE);
        }
        if tool(ui, Icon::Picture, "Inserir lista de figuras").clicked() {
            self.insert(&ctx, LIST_OF_FIGURES);
        }
        if tool(ui, Icon::Table, "Inserir lista de tabelas").clicked() {
            self.insert(&ctx, LIST_OF_TABLES);
        }
        sep(ui, "Sumário");
        if tool(ui, Icon::Note, "Inserir nota de rodapé").clicked() {
            self.insert(&ctx, FOOTNOTE);
        }
        sep(ui, "Notas");
        if tool(ui, Icon::Name, "Figura com legenda e rótulo").clicked() {
            self.insert(&ctx, LABEL);
        }
        if tool(ui, Icon::Link, "Referência a um rótulo: @rotulo").clicked() {
            self.insert(&ctx, "@|rotulo");
        }
        sep(ui, "Legendas");
        if glyphs(ui, "@", "Citar uma chave da bibliografia: @chave").clicked() {
            self.insert(&ctx, "@|chave");
        }
        tool_menu(ui, Icon::Book, "Bibliografia de refs.bib (na pasta do documento)", |ui| {
            for (name, style) in [("ABNT", "associacao-brasileira-de-normas-tecnicas"), ("APA", "apa"), ("IEEE", "ieee"), ("Chicago", "chicago-author-date")] {
                if ui.button(name).clicked() {
                    self.insert(&ctx, &format!("#bibliography(\"refs.bib\", style: \"{style}\")\n|"));
                    ui.close();
                }
            }
        });
        group_end(ui, "Citações");
    }

    fn tab_math(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if tool(ui, Icon::Fx, "Equação em bloco").clicked() {
            self.insert(&ctx, EQ_BLOCK);
        }
        if glyphs(ui, "$x$", "Equação no meio do texto").clicked() {
            self.insert(&ctx, EQ_INLINE);
        }
        sep(ui, "Equação");
        for (mark, tip, snippet) in [
            ("a/b", "Fração", FRACTION),
            ("√x", "Raiz quadrada", ROOT),
            ("Σ", "Somatório", SUM),
            ("∫", "Integral definida", INTEGRAL),
            ("lim", "Limite", LIMIT),
            ("[ ]", "Matriz", MATRIX),
            ("{", "Função por casos", CASES),
            ("v", "Vetor coluna", VECTOR),
            ("C", "Coeficiente binomial", BINOM),
        ] {
            if glyphs(ui, mark, tip).clicked() {
                self.insert(&ctx, snippet);
            }
        }
        sep(ui, "Estruturas");
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
        group_end(ui, "Símbolos");
    }

    fn tab_view(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        for (icon, label, tip, layout) in [
            (Icon::TextColumns, "Lado a lado", "Código e páginas", Layout::Both),
            (Icon::Script, "Código", "Só o código", Layout::Source),
            (Icon::Paper, "Páginas", "Só as páginas", Layout::Pages),
        ] {
            if toggle(ui, icon, label, tip, self.layout == layout).clicked() {
                self.layout = layout;
            }
        }
        sep(ui, "Painéis");
        if toggle(ui, Icon::Zoom, "Largura", "Ajustar à largura do painel", self.zoom.is_none()).clicked() {
            self.zoom = None;
        }
        if glyphs(ui, "100%", "Tamanho real").clicked() {
            self.zoom = Some(1.0);
        }
        if tool(ui, Icon::Plus, "Ampliar").clicked() {
            self.step_zoom(1);
        }
        if tool(ui, Icon::Minus, "Reduzir").clicked() {
            self.step_zoom(-1);
        }
        sep(ui, "Zoom");
        if labelled(ui, Icon::Right, "Ir para a página", "Mostra o trecho do cursor na página (Ctrl+J)").clicked() {
            self.locate(&ctx);
        }
        sep(ui, "Navegar");
        for choice in [ThemeChoice::Light, ThemeChoice::Dark, ThemeChoice::Pearl] {
            if toggle(ui, Icon::Appearance, choice.label(), "Tema da interface", self.theme == choice).clicked() {
                self.choose_theme(&ctx, choice);
            }
        }
        group_end(ui, "Tema");
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
            (Icon::TextColumns, "Lado a lado", "Código e páginas lado a lado", Layout::Both),
            (Icon::Script, "Código", "Só o código", Layout::Source),
            (Icon::Paper, "Páginas", "Só as páginas", Layout::Pages),
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
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, shown, "Números de linha"));
        if shown {
            ui.painter().rect_filled(slot.shrink(4.0), 0.0, t.accent_soft);
        } else if resp.hovered() {
            ui.painter().rect_filled(slot.shrink(4.0), 0.0, t.hover);
        }
        ui.painter().text(slot.center(), egui::Align2::CENTER_CENTER, "12", theme::ui_font(13.0), t.text);
        if resp.on_hover_text(if shown { "Ocultar os números de linha" } else { "Mostrar os números de linha" }).clicked() {
            self.line_numbers = !shown;
        }
    }

    /// The right strip: the side panes, their names turned a quarter clockwise.
    fn side_strip(&mut self, ui: &mut Ui, rect: Rect, t: &Tokens) {
        ui.painter().rect_filled(rect, 0.0, t.ribbon);
        ui.painter().vline(rect.left(), rect.y_range(), Stroke::new(1.0, t.separator));
        let errors = self.diags.iter().filter(|d| d.error).count();
        let mut top = rect.top() + 4.0;
        for (k, (side, icon, name, badge)) in [
            (Side::Details, Icon::Info, "Detalhes", 0),
            (Side::Outline, Icon::SheetList, "Estrutura", 0),
            (Side::Files, Icon::Folder, "Arquivos", 0),
            (Side::Symbols, Icon::Symbol, "Símbolos", 0),
            (Side::Problems, Icon::Validation, "Problemas", errors),
        ]
        .into_iter()
        .enumerate()
        {
            let on = self.side == Some(side);
            let galley = ui.painter().layout_no_wrap(name.to_string(), theme::ui_font(13.0), t.text);
            let height = 28.0 + galley.size().x + if badge > 0 { 18.0 } else { 6.0 };
            let slot = Rect::from_min_size(pos2(rect.left() + 1.0, top), vec2(rect.width() - 1.0, height));
            top += height + 6.0;
            let resp = ui.interact(slot, ui.scope_id().with(("side", k)), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, name));
            if on {
                ui.painter().rect_filled(slot, 0.0, t.accent_soft);
                ui.painter().rect_filled(Rect::from_min_size(pos2(slot.right() - 3.0, slot.top()), vec2(3.0, slot.height())), 0.0, t.accent);
            } else if resp.hovered() {
                ui.painter().rect_filled(slot, 0.0, t.hover);
            }
            let x = slot.center().x;
            crate::icons::paint(ui.painter(), Rect::from_center_size(pos2(x, slot.top() + 13.0), vec2(14.0, 14.0)), icon, t.text);
            // Turned a quarter clockwise, it reads downwards and its height lies to the left of `at`.
            let at = pos2(x + galley.size().y / 2.0, slot.top() + 26.0);
            ui.painter().add(egui::epaint::TextShape::new(at, galley, if on { t.accent } else { t.text }).with_angle(std::f32::consts::FRAC_PI_2));
            if badge > 0 {
                ui.painter().text(pos2(x, slot.bottom() - 9.0), egui::Align2::CENTER_CENTER, badge.to_string(), theme::ui_font(12.0), t.danger);
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
            Side::Details => (Icon::Info, "Detalhes"),
            Side::Outline => (Icon::SheetList, "Estrutura"),
            Side::Files => (Icon::Folder, "Arquivos"),
            Side::Symbols => (Icon::Symbol, "Símbolos"),
            Side::Problems => (Icon::Validation, "Problemas"),
        };
        let inner = rect.shrink2(vec2(10.0, 6.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            ui.horizontal(|ui| {
                ui.set_height(26.0);
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                crate::icons::paint(ui.painter(), r, icon, t.text);
                ui.label(egui::RichText::new(title).font(theme::ui_bold(13.0)).color(t.text));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if icon_button_mono(ui, Icon::Right, t.text_dim, "Fechar o painel", vec2(24.0, 22.0)).clicked() {
                        self.side = None;
                    }
                    if side == Side::Files && icon_button_mono(ui, Icon::Calc, t.text_dim, "Atualizar a lista", vec2(24.0, 22.0)).clicked() {
                        self.read_files();
                    }
                });
            });
            ui.separator();
            egui::ScrollArea::vertical().id_salt(("side-pane", title)).auto_shrink([false, false]).show(ui, |ui| match side {
                Side::Details => self.pane_details(ui, t),
                Side::Outline => self.pane_outline(ui, t),
                Side::Files => self.pane_files(ui, t),
                Side::Symbols => self.pane_symbols(ui, t),
                Side::Problems => {
                    if self.diags.is_empty() {
                        ui.colored_label(t.text_dim, "Nenhum problema neste documento.");
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
            ("Arquivo", self.path.as_deref().map_or_else(|| "Ainda não salvo".to_string(), |p| p.display().to_string())),
            ("Páginas", self.pages.len().to_string()),
            ("Palavras no código", words.to_string()),
            ("Caracteres", chars.to_string()),
            ("Linhas", self.text.lines().count().to_string()),
            ("Tamanho da página", self.pages.first().map_or_else(|| "—".to_string(), |p| format!("{:.0} × {:.0} mm", p.width_pt * 25.4 / 72.0, p.height_pt * 25.4 / 72.0))),
            ("Última compilação", self.millis.map_or_else(|| "—".to_string(), |m| format!("{m} ms"))),
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
            ui.colored_label(t.text_dim, "Os títulos do documento (= Título) aparecem aqui.");
        }
        let mut jump = None;
        for h in &headings {
            ui.horizontal(|ui| {
                ui.add_space(12.0 * h.level.saturating_sub(1) as f32);
                let font = if h.level == 1 { theme::ui_bold(13.0) } else { theme::ui_font(13.0) };
                let button = egui::Button::new(egui::RichText::new(&h.title).font(font).color(t.text)).frame_when_inactive(false).truncate();
                if ui.add(button).on_hover_text("Ir para este título").clicked() {
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
            ui.colored_label(t.text_dim, "Salve o documento para ver os arquivos da pasta dele.");
            return;
        }
        if self.files.is_empty() {
            ui.colored_label(t.text_dim, "Não há outros arquivos na pasta do documento.");
            return;
        }
        ui.colored_label(t.text_dim, "Clique para inserir no cursor.");
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
        for (group, symbols) in SYMBOL_GROUPS {
            ui.colored_label(t.text_dim, *group);
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
        let Some(query) = self.find.clone() else { return };
        let (a, b) = self.selection(ctx);
        let from = if backwards { a } else { b };
        match edit::find(&self.text, &query, from, backwards) {
            Some(found) => {
                self.set_selection(ctx, found);
                self.scroll_caret = true;
            }
            None if !query.is_empty() => self.status = format!("“{query}” não foi encontrado."),
            None => {}
        }
    }

    /// Saves the source to a file of the user's choice, leaving the open document as it is.
    fn save_copy(&mut self) {
        let Some(path) = self.dialog().add_filter("Typst", &["typ"]).set_file_name("copia.typ").save_file() else { return };
        self.status = match std::fs::write(&path, &self.text) {
            Ok(()) => format!("Cópia salva em {}", path.display()),
            Err(e) => format!("Não foi possível salvar: {e}"),
        };
    }

    /// Saves a changed document a moment after the last keystroke. A failure turns autosave off, so
    /// it isn't retried every frame, and says so.
    fn autosave_tick(&mut self, ctx: &egui::Context) {
        if !(self.autosave && self.dirty && self.path.is_some()) {
            return;
        }
        let wait = AUTOSAVE_DELAY - (ctx.input(|i| i.time) - self.changed_at);
        if wait > 0.0 {
            ctx.request_repaint_after_secs(wait as f32);
        } else if !self.save(false) {
            self.autosave = false;
        }
    }

    fn step_zoom(&mut self, dir: i32) {
        let z = self.zoom.unwrap_or(self.fit_zoom);
        let next = if dir > 0 { ZOOMS.iter().copied().find(|v| *v > z + 0.01).unwrap_or(2.5) } else { ZOOMS.iter().rev().copied().find(|v| *v < z - 0.01).unwrap_or(0.5) };
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
        let scroll_caret = std::mem::take(&mut self.scroll_caret);
        let show_numbers = self.line_numbers;
        let waiting = self.session.as_ref().is_some_and(Session::is_waiting);
        let carets = self.session.as_ref().map_or_else(Vec::new, Session::cursors);
        let App { text, colored, .. } = self;
        let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, wrap: f32| {
            let src = buffer.as_str();
            let hash = egui::epaint::util::hash(src);
            if !matches!(colored, Some((h, p, _)) if *h == hash && *p == palette) {
                *colored = Some((hash, palette, highlight::job(src, font.clone(), &tokens)));
            }
            let mut job = colored.as_ref().map_or_else(egui::text::LayoutJob::default, |c| c.2.clone());
            job.wrap.max_width = wrap;
            ui.painter().layout_job(job)
        };
        let mut changed = false;
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
                output.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::MultilineTextInput, true, "Código-fonte"));
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
                if scroll_caret && let Some(range) = output.cursor_range {
                    let caret = output.galley.pos_from_cursor(range.primary);
                    scroll_to = Some(caret.translate(output.galley_pos.to_vec2()));
                }
            });
            if let Some(rect) = scroll_to {
                ui.scroll_to_rect(rect.expand(30.0), Some(Align::Center));
            }
        });
        if changed {
            self.dirty = true;
            self.changed_at = ctx.input(|i| i.time);
            self.compile();
        }
    }

    fn preview(&mut self, ui: &mut Ui, t: &Tokens) {
        if self.pages.is_empty() {
            ui.centered_and_justified(|ui| {
                let message = if !self.compiled_once() {
                    "Compilando…"
                } else if self.diags.iter().any(|d| d.error) {
                    "Corrija o erro indicado abaixo para ver as páginas."
                } else {
                    "Nada para mostrar ainda."
                };
                ui.colored_label(t.text_dim, message);
            });
            return;
        }
        const PAD: f32 = 20.0;
        let widest = self.pages.iter().map(|p| p.width_pt).fold(1.0, f32::max);
        self.fit_zoom = fit_zoom(ui.available_width() - 2.0 * PAD - 14.0, widest);
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
        let mut click = None;
        egui::ScrollArea::both().id_salt("pages-scroll").auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(PAD);
            ui.spacing_mut().item_spacing = vec2(0.0, 14.0);
            ui.vertical_centered(|ui| {
                for (i, page) in self.pages.iter().enumerate() {
                    let size = vec2(page.width_pt, page.height_pt) * zoom;
                    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                    if !ui.is_rect_visible(rect.expand(200.0)) && !scroll_page.is_some_and(|(p, _)| p == i) {
                        continue;
                    }
                    ui.painter().rect_filled(rect.translate(vec2(0.0, 2.0)).expand(1.0), 1.0, Color32::from_black_alpha(45));
                    if let Some(tex) = self.textures.get(&page.hash) {
                        ui.painter().image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    if resp.clicked() && let Some(p) = resp.interact_pointer_pos() {
                        let at = (p - rect.min) / zoom;
                        click = Some((i, at.x, at.y));
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if let Some((p, y)) = scroll_page && p == i {
                        let target = rect.min.y + y * zoom;
                        ui.scroll_to_rect(Rect::from_min_max(pos2(rect.min.x, target - 30.0), pos2(rect.max.x, target + 30.0)), Some(Align::Center));
                    }
                    if let Some((p, at, since)) = self.marker && p == i {
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
        if let Some((page, x, y)) = click {
            self.compiler.send(Job::Click { page, x: f64::from(x), y: f64::from(y) });
        }
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
                let (kind, color) = if d.error { ("Erro", tokens.danger) } else { ("Aviso", tokens.warning) };
                let place = d.byte.map_or_else(String::new, |b| format!(" na linha {}", edit::line_of(&self.text, b)));
                ui.horizontal(|ui| {
                    ui.colored_label(color, egui::RichText::new(format!("{kind}{place}:")).font(theme::ui_bold(12.5)));
                    if ui.add(egui::Label::new(&d.message).sense(Sense::click())).on_hover_text("Ir para o trecho").clicked() {
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
            small(ui, &format!("{n} {}", if n == 1 { "página" } else { "páginas" }));
            let words = self.text.split_whitespace().count();
            small(ui, &format!("{words} {}", if words == 1 { "palavra" } else { "palavras" }));
            if let Some(session) = &self.session {
                let others = session.others().len();
                small(ui, &if session.is_waiting() {
                    "Conectando…".to_string()
                } else {
                    format!("Sessão: {} {}", others + 1, if others == 0 { "pessoa" } else { "pessoas" })
                });
            }
            small(ui, &self.status);
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let z = self.zoom.unwrap_or(self.fit_zoom);
                let percent = ui.add(egui::Label::new(egui::RichText::new(format!("{:.0}%", z * 100.0)).font(theme::ui_font(12.0)).color(color)).sense(Sense::click()));
                if percent.on_hover_text("Voltar a 100% (Ctrl+0)").clicked() {
                    self.zoom = Some(1.0);
                }
                if icon_button_mono(ui, Icon::Plus, t.text_dim, "Ampliar (Ctrl++)", vec2(18.0, 18.0)).clicked() {
                    self.step_zoom(1);
                }
                ui.spacing_mut().slider_width = 110.0;
                let mut slider = (z * 100.0).round().clamp(MIN_ZOOM * 100.0, MAX_ZOOM * 100.0);
                let moved = ui.add(egui::Slider::new(&mut slider, (MIN_ZOOM * 100.0)..=(MAX_ZOOM * 100.0)).show_value(false)).on_hover_text("Zoom das páginas");
                if moved.changed() {
                    self.zoom = Some(slider / 100.0);
                }
                if icon_button_mono(ui, Icon::Minus, t.text_dim, "Reduzir (Ctrl+-)", vec2(18.0, 18.0)).clicked() {
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
            egui::Window::new("Sobre o Pearl").open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label(format!("Pearl {}", env!("CARGO_PKG_VERSION")));
                ui.label("Editor de documentos em Rust, com composição por Typst.");
                ui.label("Scripta manent.");
            });
            self.about = open;
        }
        if let Some(mut query) = self.find.take() {
            let mut keep = true;
            let (mut next, mut previous) = (false, false);
            egui::Window::new("Localizar").collapsible(false).resizable(false).title_bar(false).anchor(egui::Align2::RIGHT_TOP, [-16.0, 134.0]).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let field = ui.add(egui::TextEdit::singleline(&mut query).desired_width(200.0).hint_text("Localizar no documento"));
                    if std::mem::take(&mut self.find_focus) {
                        field.request_focus();
                    }
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    if enter {
                        next = !ui.input(|i| i.modifiers.shift);
                        previous = !next;
                        field.request_focus();
                    }
                    if ui.input(|i| i.key_pressed(Key::Escape)) {
                        keep = false;
                    }
                    let t = Tokens::get(ui.ctx());
                    previous |= icon_button_mono(ui, Icon::ChevronUp, t.text_dim, "Anterior (Shift+Enter)", vec2(24.0, 22.0)).clicked();
                    next |= icon_button_mono(ui, Icon::Chevron, t.text_dim, "Próximo (Enter)", vec2(24.0, 22.0)).clicked();
                    if icon_button_mono(ui, Icon::Close, t.text_dim, "Fechar (Esc)", vec2(24.0, 22.0)).clicked() {
                        keep = false;
                    }
                });
            });
            self.find = keep.then_some(query);
            if next || previous {
                self.find_step(ctx, previous);
            }
            if !keep {
                self.want_focus = true;
            }
        }
        if let Some(mut dialog) = self.table_dialog.take() {
            let mut keep = true;
            let mut insert = false;
            egui::Window::new("Inserir tabela").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                egui::Grid::new("table-size").spacing(vec2(10.0, 6.0)).show(ui, |ui| {
                    ui.label("Colunas");
                    ui.add(egui::DragValue::new(&mut dialog.cols).range(1..=26));
                    ui.end_row();
                    ui.label("Linhas");
                    ui.add(egui::DragValue::new(&mut dialog.rows).range(1..=100));
                    ui.end_row();
                });
                ui.checkbox(&mut dialog.header, "Linha de cabeçalho");
                ui.checkbox(&mut dialog.figure, "Com legenda");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Inserir").clicked() {
                        insert = true;
                        keep = false;
                    }
                    if ui.button("Cancelar").clicked() {
                        keep = false;
                    }
                });
            });
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
                ui.heading("Salvar as alterações?");
                ui.label("O documento tem alterações que ainda não foram salvas.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Salvar").clicked() {
                        answer = Some(0);
                    }
                    if ui.button("Descartar").clicked() {
                        answer = Some(1);
                    }
                    if ui.button("Cancelar").clicked() {
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
        ui.label("Seu nome:");
        ui.add(egui::TextEdit::singleline(name).desired_width(220.0).hint_text("Como os outros verão você"));
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
    for (button, tip) in [("close", "Fechar"), ("max", if maximized { "Restaurar" } else { "Maximizar" }), ("min", "Minimizar")] {
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

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}
