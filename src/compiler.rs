//! The background compiler: one thread owns the Typst world, so incremental compilation carries
//! over between edits and typing never waits for layout.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use typst::WorldExt;
use typst::diag::Severity;
use typst::introspection::PagedPosition;
use typst::layout::{Abs, Point};
use typst::utils::hash128;
use typst_ide::Jump;
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;
use typst_render::RenderOptions;

use crate::world::{DocWorld, load_fonts};

/// Pixels per point the pages are rasterised at; sharp up to about 250% zoom.
pub const RENDER_SCALE: f32 = 2.5;
/// The largest texture every GPU takes, used until the interface reports the real limit.
const SAFE_TEXTURE_SIDE: usize = 2048;
/// Most suggestions kept; the list is for reading, and typing narrows it.
const MAX_SUGGESTIONS: usize = 200;
/// Most pages kept rasterised in the cache beyond the current document.
const CACHE_SLACK: usize = 32;

pub enum Job {
    Compile { text: String, path: Option<PathBuf> },
    /// A click on a page, in points from its top left.
    Click { page: usize, x: f64, y: f64 },
    /// Shows the place in the pages for a byte offset of the source.
    Locate { byte: usize },
    ExportPdf(PathBuf),
    /// What the thing at a byte offset is, for the pointer resting on it.
    Hover { request: u64, byte: usize },
    /// What could be typed at a byte offset. `explicit`: asked for with Ctrl+Space.
    Complete { request: u64, byte: usize, explicit: bool },
}

/// One suggestion of the autocompletion.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub label: String,
    /// What goes in the text, with `${…}` placeholders.
    pub apply: String,
    /// A short description, in Typst's own English.
    pub detail: Option<String>,
    /// One word for the kind of thing it is.
    pub kind: &'static str,
}

#[derive(Clone)]
pub struct Pixels {
    pub size: [usize; 2],
    /// Premultiplied RGBA.
    pub rgba: Arc<Vec<u8>>,
}

#[derive(Clone)]
pub struct Page {
    pub hash: u128,
    pub width_pt: f32,
    pub height_pt: f32,
    pub pixels: Pixels,
}

#[derive(Clone, Debug)]
pub struct Diag {
    pub error: bool,
    pub message: String,
    pub byte: Option<usize>,
}

pub enum Event {
    /// A compile finished. `pages` is `None` when it failed and the last good pages stay.
    Compiled { pages: Option<Vec<Page>>, diags: Vec<Diag>, millis: u128 },
    JumpToSource { byte: usize },
    JumpToPage { page: usize, y_pt: f32, x_pt: f32 },
    /// The answer to [`Job::Hover`]: the words to show, and whether they're code.
    Hovered { request: u64, tip: Option<(String, bool)> },
    /// The answer to [`Job::Complete`]: suggestions replacing the text from `from` to the caret.
    Completions { request: u64, from: usize, suggestions: Vec<Suggestion> },
    /// The files of the project the document read (path from its folder, path on disk).
    Dependencies(Vec<(String, PathBuf)>),
    Exported(Result<PathBuf, String>),
    Failed(String),
}

pub struct Compiler {
    jobs: Sender<Job>,
    pub events: Receiver<Event>,
    /// Woken when an event arrives, once the interface has given it.
    pub repaint: Arc<OnceLock<egui::Context>>,
    /// The longest side a page texture may have on this GPU.
    pub max_side: Arc<AtomicUsize>,
}

impl Compiler {
    pub fn start() -> Result<Self, String> {
        let (jobs, job_rx) = channel();
        let (event_tx, events) = channel();
        let repaint = Arc::new(OnceLock::new());
        let wake = Arc::clone(&repaint);
        let max_side = Arc::new(AtomicUsize::new(SAFE_TEXTURE_SIDE));
        let limit = Arc::clone(&max_side);
        std::thread::Builder::new()
            .name("typst".into())
            // Deeply nested documents recurse far; the default stack would overflow first.
            .stack_size(32 * 1024 * 1024)
            .spawn(move || Worker::new(event_tx, wake, limit).run(job_rx))
            .map_err(|e| e.to_string())?;
        Ok(Self { jobs, events, repaint, max_side })
    }

    pub fn send(&self, job: Job) {
        // The worker only stops when this handle is gone.
        let _ = self.jobs.send(job);
    }
}

struct Worker {
    events: Sender<Event>,
    wake: Arc<OnceLock<egui::Context>>,
    max_side: Arc<AtomicUsize>,
    fonts: Arc<typst_kit::fonts::FontStore>,
    world: Option<DocWorld>,
    path: Option<PathBuf>,
    doc: Option<PagedDocument>,
    rendered: HashMap<u128, Pixels>,
}

impl Worker {
    fn new(events: Sender<Event>, wake: Arc<OnceLock<egui::Context>>, max_side: Arc<AtomicUsize>) -> Self {
        Self { events, wake, max_side, fonts: load_fonts(), world: None, path: None, doc: None, rendered: HashMap::new() }
    }

    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
        if let Some(ctx) = self.wake.get() {
            ctx.request_repaint();
        }
    }

    fn run(mut self, jobs: Receiver<Job>) {
        while let Ok(first) = jobs.recv() {
            // Only the newest text matters; other jobs keep their order after it.
            let mut compile = None;
            let mut others = Vec::new();
            let mut complete = None;
            let mut hover = None;
            for job in std::iter::once(first).chain(jobs.try_iter()) {
                match job {
                    Job::Compile { text, path } => compile = Some((text, path)),
                    Job::Hover { request, byte } => hover = Some((request, byte)),
                    // Only the newest question matters too: answers to old ones go unused.
                    Job::Complete { request, byte, explicit } => complete = Some((request, byte, explicit)),
                    other => others.push(other),
                }
            }
            if let Some((text, path)) = compile {
                self.compile(&text, path);
            }
            for job in others {
                self.other(job);
            }
            if let Some((request, byte)) = hover {
                self.hover(request, byte);
            }
            if let Some((request, byte, explicit)) = complete {
                self.complete(request, byte, explicit);
            }
        }
    }

    fn compile(&mut self, text: &str, path: Option<PathBuf>) {
        let started = Instant::now();
        if self.world.is_none() || self.path != path {
            self.world = DocWorld::new(Arc::clone(&self.fonts), path.as_deref());
            self.path = path;
        }
        let Some(world) = self.world.as_mut() else {
            self.emit(Event::Failed("Não foi possível abrir a pasta do documento.".into()));
            return;
        };
        world.update(text);
        let world = &*world;
        // A panic in the compiler must not take the unsaved text down with it.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| typst::compile::<PagedDocument>(world)));
        let Ok(warned) = result else {
            self.emit(Event::Failed("O compilador falhou neste documento.".into()));
            self.world = None;
            return;
        };
        let to_diag = |d: &typst::diag::SourceDiagnostic| {
            let in_main = d.span.id() == Some(typst::World::main(world));
            Diag { error: d.severity == Severity::Error, message: d.message.to_string(), byte: if in_main { world.range(d.span).map(|r| r.start) } else { None } }
        };
        let mut diags: Vec<Diag> = warned.warnings.iter().map(to_diag).collect();
        let pages = match warned.output {
            Ok(doc) => {
                let pages = self.render(&doc);
                self.doc = Some(doc);
                Some(pages)
            }
            Err(errors) => {
                let mut all: Vec<Diag> = errors.iter().map(to_diag).collect();
                all.append(&mut diags);
                diags = all;
                None
            }
        };
        comemo::evict(10);
        if let Some(world) = self.world.as_mut() {
            let files = world.project_files();
            self.emit(Event::Dependencies(files));
        }
        self.emit(Event::Compiled { pages, diags, millis: started.elapsed().as_millis() });
    }

    /// Rasterises the pages that changed; the others come from the cache.
    fn render(&mut self, doc: &PagedDocument) -> Vec<Page> {
        let limit = self.max_side.load(Ordering::Relaxed);
        let mut out = Vec::with_capacity(doc.pages().len());
        for page in doc.pages() {
            let size = page.frame.size();
            let scale = page_scale(size.x.to_pt(), size.y.to_pt(), limit);
            let options = RenderOptions { pixel_per_pt: typst::utils::Scalar::new(scale), render_bleed: false };
            let hash = hash128(&(&page.frame, &page.fill, scale.to_bits()));
            let pixels = self
                .rendered
                .entry(hash)
                .or_insert_with(|| {
                    let map = typst_render::render(page, &options);
                    Pixels { size: [map.width() as usize, map.height() as usize], rgba: Arc::new(map.take_demultiplied()) }
                })
                .clone();
            out.push(Page { hash, width_pt: size.x.to_pt() as f32, height_pt: size.y.to_pt() as f32, pixels });
        }
        if self.rendered.len() > out.len() + CACHE_SLACK {
            self.rendered.retain(|h, _| out.iter().any(|p| p.hash == *h));
        }
        out
    }

    fn other(&mut self, job: Job) {
        match job {
            Job::Compile { .. } | Job::Complete { .. } | Job::Hover { .. } => {}
            Job::Click { page, x, y } => self.click(page, x, y),
            Job::Locate { byte } => self.locate(byte),
            Job::ExportPdf(path) => {
                let result = self.export(&path).map(|()| path);
                self.emit(Event::Exported(result));
            }
        }
    }

    fn hover(&self, request: u64, byte: usize) {
        let Some(world) = &self.world else { return };
        let source = world.source();
        let tip = source.text().is_char_boundary(byte).then(|| {
            // Analysis runs the document's code: a panic in it must not take the thread along.
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // The character under the pointer is the one after the offset.
                typst_ide::tooltip(world, self.doc.as_ref(), source, byte, typst::syntax::Side::After)
            }))
            .ok()
            .flatten()
        });
        let tip = tip.flatten().map(|t| match t {
            typst_ide::Tooltip::Text(text) => (crate::edit::plain_doc(&text), false),
            typst_ide::Tooltip::Code(code) => (code.to_string(), true),
        });
        self.emit(Event::Hovered { request, tip });
    }

    fn complete(&mut self, request: u64, byte: usize, explicit: bool) {
        let Some(world) = &mut self.world else { return };
        let original = world.source().text().to_string();
        if !original.is_char_boundary(byte) {
            return;
        }
        let doc = self.doc.as_ref();
        // Analysis runs the document's code: a panic in it must not take the compiler thread along.
        let ask = |world: &DocWorld| {
            let source = world.source();
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| typst_ide::autocomplete(world, doc, source, byte, explicit))).ok().flatten()
        };
        let mut found = ask(world).filter(|(_, c)| !c.is_empty());
        // Typst analyses a call by running the document, which a bracket still open stops: `#set
        // text(` suggests nothing. Then, for this question only, close what's open at the caret.
        if found.is_none() && !world.source().root().errors_and_warnings().0.is_empty() {
            let closers = crate::edit::open_brackets(&original, byte);
            if !closers.is_empty() {
                let mut repaired = original.clone();
                repaired.insert_str(byte, &closers);
                world.update(&repaired);
                found = ask(world);
                world.update(&original);
            }
        }
        let (from, completions) = found.unwrap_or((byte, Vec::new()));
        // Typst offers everything that fits the place; narrowing it to what's been typed is ours.
        let query = original.get(from..byte).unwrap_or_default();
        let mut ranked: Vec<(u8, typst_ide::Completion)> =
            completions.into_iter().filter_map(|c| Some((crate::edit::match_rank(query, &c.label)?, c))).collect();
        // Among equals the shortest is the likeliest: `align` before `alignment`.
        ranked.sort_by_key(|(rank, c)| (*rank, c.label.chars().count()));
        let suggestions = ranked
            .into_iter()
            .take(MAX_SUGGESTIONS)
            .map(|(_, c)| Suggestion {
                apply: c.apply.as_ref().unwrap_or(&c.label).to_string(),
                label: c.label.to_string(),
                detail: c.detail.map(|d| d.to_string()),
                kind: kind_name(&c.kind),
            })
            .collect();
        self.emit(Event::Completions { request, from, suggestions });
    }

    fn click(&self, page: usize, x: f64, y: f64) {
        let (Some(world), Some(doc), Some(page)) = (&self.world, &self.doc, NonZeroUsize::new(page + 1)) else {
            return;
        };
        let position = PagedPosition { page, point: Point::new(Abs::pt(x), Abs::pt(y)) };
        match typst_ide::jump_from_click(world, doc, &position) {
            Some(Jump::File(id, byte)) if id == typst::World::main(world) => self.emit(Event::JumpToSource { byte }),
            Some(Jump::Position(p)) => self.emit(Event::JumpToPage { page: p.page.get() - 1, y_pt: p.point.y.to_pt() as f32, x_pt: p.point.x.to_pt() as f32 }),
            _ => {}
        }
    }

    fn locate(&self, byte: usize) {
        let (Some(world), Some(doc)) = (&self.world, &self.doc) else {
            return;
        };
        if let Some(p) = typst_ide::jump_from_cursor(doc, world.source(), byte).first() {
            self.emit(Event::JumpToPage { page: p.page.get() - 1, y_pt: p.point.y.to_pt() as f32, x_pt: p.point.x.to_pt() as f32 });
        }
    }

    fn export(&self, path: &std::path::Path) -> Result<(), String> {
        let doc = self.doc.as_ref().ok_or("O documento ainda não compilou.")?;
        let bytes = typst_pdf::pdf(doc, &PdfOptions::default()).map_err(|errors| errors.iter().map(|e| e.message.to_string()).collect::<Vec<_>>().join("; "))?;
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    }
}

fn kind_name(kind: &typst_ide::CompletionKind) -> &'static str {
    use typst_ide::CompletionKind as K;
    match kind {
        K::Syntax => "sintaxe",
        K::Func => "função",
        K::Type => "tipo",
        K::Param => "parâmetro",
        K::Constant => "valor",
        K::Path => "arquivo",
        K::Package => "pacote",
        K::Label => "rótulo",
        K::Font => "fonte",
        K::Symbol(_) => "símbolo",
    }
}

/// Pixels per point for a page: the usual scale, or less when the page wouldn't fit in a texture.
fn page_scale(width_pt: f64, height_pt: f64, max_side: usize) -> f64 {
    let longest = width_pt.max(height_pt).max(1.0);
    // Render rounds to the nearest pixel, so stay a pixel clear of the limit.
    f64::from(RENDER_SCALE).min((max_side.max(2) as f64 - 1.0) / longest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::table_snippet;
    use crate::sample::SAMPLE;
    use std::time::Duration;

    fn compile(text: &str) -> (Option<Vec<Page>>, Vec<Diag>) {
        let c = Compiler::start().unwrap();
        c.send(Job::Compile { text: text.into(), path: None });
        loop {
            match c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                Event::Compiled { pages, diags, .. } => return (pages, diags),
                Event::Failed(e) => panic!("{e}"),
                _ => {}
            }
        }
    }

    #[test]
    fn page_scale_never_exceeds_the_texture_limit() {
        assert_eq!(page_scale(595.0, 842.0, 8192), f64::from(RENDER_SCALE));
        for (w, h, max) in [(595.28, 841.89, 2048), (14400.0, 14400.0, 2048), (0.0, 0.0, 16), (1e9, 1.0, 4096), (500.0, 500.0, 0)] {
            let s = page_scale(w, h, max);
            assert!((w.max(h) * s).round() <= max.max(2) as f64, "{w} {h} {max}");
        }
    }

    #[test]
    fn a_page_larger_than_the_texture_limit_still_renders_inside_it() {
        let (pages, _) = compile("#set page(width: 9000pt, height: 9000pt)\nolá");
        let pages = pages.unwrap();
        assert!(pages[0].pixels.size.iter().all(|s| *s <= SAFE_TEXTURE_SIDE), "{:?}", pages[0].pixels.size);
    }

    #[test]
    fn the_sample_compiles_without_errors() {
        let (pages, diags) = compile(SAMPLE);
        assert!(diags.iter().all(|d| !d.error), "{diags:?}");
        assert!(!pages.unwrap().is_empty());
    }

    #[test]
    fn errors_point_at_their_place_and_keep_no_pages() {
        let (pages, diags) = compile("olá\n#nao_existe");
        assert!(pages.is_none());
        let error = diags.iter().find(|d| d.error).unwrap();
        assert_eq!(crate::edit::line_of("olá\n#nao_existe", error.byte.unwrap()), 2);
    }

    #[test]
    fn every_ribbon_snippet_is_valid_typst() {
        let mut doc = String::new();
        for s in crate::app::SNIPPETS {
            doc += &s.replacen('|', "", 1);
            doc += "\n\n";
        }
        doc += &table_snippet(2, 3, true).replacen('|', "", 1);
        let (pages, diags) = compile(&doc);
        assert!(pages.is_some(), "{diags:?}");
    }

    #[test]
    fn every_template_compiles_without_errors() {
        for template in crate::templates::TEMPLATES {
            let (pages, diags) = compile(template.source);
            assert!(pages.is_some() && diags.iter().all(|d| !d.error), "{}: {diags:?}", template.name);
        }
    }

    #[test]
    fn every_symbol_in_the_pane_is_valid_math() {
        let codes: Vec<&str> = crate::app::SYMBOL_GROUPS.iter().flat_map(|(_, s)| s.iter().map(|(_, c)| *c)).collect();
        let doc: String = codes.iter().map(|c| format!("$ x {c} y $\n")).collect();
        let (pages, diags) = compile(&doc);
        assert!(pages.is_some(), "{diags:?}");
    }

    /// Compiles `text` and asks what fits at the end of it.
    fn suggestions(text: &str, explicit: bool) -> (usize, Vec<Suggestion>) {
        let c = Compiler::start().unwrap();
        c.send(Job::Compile { text: text.into(), path: None });
        c.send(Job::Complete { request: 7, byte: text.len(), explicit });
        loop {
            if let Event::Completions { request, from, suggestions } = c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                assert_eq!(request, 7);
                return (from, suggestions);
            }
        }
    }

    #[test]
    fn functions_complete_after_a_hash() {
        let (from, found) = suggestions("olá #ali", false);
        assert_eq!(from, "olá #".len());
        let align = found.iter().find(|s| s.label == "align").unwrap();
        assert_eq!(align.kind, "função");
        assert!(align.apply.starts_with("align"));
        assert!(align.detail.is_some());
    }

    #[test]
    fn parameters_complete_inside_a_call() {
        let (_, found) = suggestions("#set text(", true);
        assert!(found.iter().any(|s| s.label == "fill" && s.kind == "parâmetro"), "{found:?}");
    }

    #[test]
    fn suggestions_are_narrowed_to_what_was_typed_closest_first() {
        let (_, found) = suggestions("#ali", false);
        assert_eq!(found.first().map(|s| s.label.as_str()), Some("align"));
        assert!(found.iter().all(|s| crate::edit::match_rank("ali", &s.label).is_some()), "{found:?}");
    }

    #[test]
    fn the_repair_for_a_question_does_not_stay_in_the_document() {
        let c = Compiler::start().unwrap();
        c.send(Job::Compile { text: "#set text(".into(), path: None });
        c.send(Job::Complete { request: 1, byte: 10, explicit: true });
        loop {
            if let Event::Completions { .. } = c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                break;
            }
        }
        // Clicking asks about the text as it is, and exporting still works from the last good one.
        c.send(Job::Complete { request: 2, byte: 10, explicit: true });
        c.send(Job::Compile { text: "#set text(".into(), path: None });
        loop {
            if let Event::Compiled { pages, diags, .. } = c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                assert!(pages.is_none() && diags.iter().any(|d| d.error), "the unclosed bracket is still an error");
                break;
            }
        }
    }

    fn hovered(text: &str, byte: usize) -> Option<(String, bool)> {
        let c = Compiler::start().unwrap();
        c.send(Job::Compile { text: text.into(), path: None });
        c.send(Job::Hover { request: 3, byte });
        loop {
            if let Event::Hovered { request, tip } = c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                assert_eq!(request, 3);
                return tip;
            }
        }
    }

    #[test]
    fn resting_on_a_function_tells_what_it_does() {
        let (words, code) = hovered("#align(center)[x]", 3).unwrap();
        assert!(!code);
        assert!(words.to_lowercase().contains("align"), "{words}");
        assert!(!words.contains("]("), "links are gone: {words}");
    }

    #[test]
    fn resting_on_a_variable_shows_its_value() {
        let text = "#let total = 40 + 2\n#total";
        let (words, code) = hovered(text, text.rfind("total").unwrap() + 1).unwrap();
        assert!(code);
        assert!(words.contains("42"), "{words}");
    }

    #[test]
    fn resting_on_plain_text_tells_nothing() {
        assert_eq!(hovered("um texto comum", 5), None);
    }

    #[test]
    fn resting_past_the_end_or_inside_a_character_does_not_panic() {
        assert_eq!(hovered("ação", 2), None);
        assert_eq!(hovered("ação", 999), None);
    }

    #[test]
    fn plain_prose_asks_for_nothing_unless_requested() {
        let (_, found) = suggestions("um texto comum", false);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn math_symbols_complete() {
        let (_, found) = suggestions("$ alp", true);
        assert!(found.iter().any(|s| s.label == "alpha"), "{found:?}");
    }

    #[test]
    fn a_broken_document_still_completes() {
        let (_, found) = suggestions("#nao_existe(\n#ali", false);
        assert!(found.iter().any(|s| s.label == "align"), "{found:?}");
    }

    #[test]
    fn clicking_on_text_jumps_to_its_source() {
        let c = Compiler::start().unwrap();
        c.send(Job::Compile { text: "#set page(margin: 10pt)\nAAAAAAAAAAAAAAAAAAAA".into(), path: None });
        c.send(Job::Click { page: 0, x: 20.0, y: 15.0 });
        loop {
            if let Event::JumpToSource { byte } = c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                assert!(byte >= 24);
                return;
            }
        }
    }
}
