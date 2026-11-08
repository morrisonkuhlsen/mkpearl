//! The background compiler: one thread owns the Typst world, so incremental compilation carries
//! over between edits and typing never waits for layout.

use crate::i18n::tx;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use typst::WorldExt;
use typst::diag::Severity;
use typst::foundations::{Label, Value};
use typst::introspection::{Introspector, MetadataElem, PagedPosition};
use typst::layout::{Abs, Frame, FrameItem, Point, Transform};
use typst::syntax::{LinkedNode, Side, Span, SyntaxKind};
use typst::utils::{PicoStr, hash128};
use typst_ide::Jump;
use typst_layout::PagedDocument;
use typst_pdf::{PdfOptions, PdfStandard, PdfStandards};
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
/// The pages of a variant kept to look at, and the most pixels each may take.
const VARIANT_PAGES: usize = 4;
const THUMBNAIL_SIDES: [f64; 2] = [380.0, 540.0];
/// A search for fitting stops here with the best it has.
/// The most pages of a past version kept to look at.
const TIMELINE_PAGES: usize = 24;
const FIT_TIME_LIMIT: std::time::Duration = std::time::Duration::from_secs(60);

pub enum Job {
    Compile {
        text: String,
        path: Option<PathBuf>,
    },
    /// A click on a page, in points from its top left.
    Click {
        page: usize,
        x: f64,
        y: f64,
    },
    /// Shows the place in the pages for a byte offset of the source.
    Locate {
        byte: usize,
    },
    /// The old version's pages and what differs from the document on both sides.
    Compare {
        old: String,
        request: u64,
    },
    /// What's between two points of a page, as a range of the source, for a review mark.
    ReviewPick {
        request: u64,
        page: usize,
        from: [f64; 2],
        to: [f64; 2],
    },
    /// Where these ranges of the source fall on the pages.
    Places {
        ranges: Vec<std::ops::Range<usize>>,
    },
    /// Where a block of the source, at `block`, would go if dropped here on a page.
    Drop {
        page: usize,
        x: f64,
        y: f64,
        block: std::ops::Range<usize>,
    },
    /// Compiles an example of the documentation, on its own. `request` comes back with the answer.
    Example {
        request: u64,
        text: String,
    },
    /// Draws the first page of template `index` small, for its tile in File › New.
    Thumbnail {
        index: usize,
        text: String,
    },
    /// The document set in other ways, each on its own and off this thread, so it's never in the way
    /// of the compile that follows the typing. A newer request, or `CancelVariants`, ends an older one.
    Variants {
        request: u64,
        text: String,
        path: Option<PathBuf>,
        variants: Vec<crate::variants::Variant>,
    },
    CancelVariants,
    /// Looks for the smallest changes that fit the document in `target` pages, off this thread as well.
    Fit {
        request: u64,
        text: String,
        path: Option<PathBuf>,
        target: usize,
    },
    CancelFit,
    /// Reads a past version and compiles it, off this thread.
    TimelineVersion {
        request: u64,
        store: crate::history::Store,
        version: crate::history::Version,
        path: Option<PathBuf>,
    },
    /// Where a byte offset of the source falls in the pages, for the caret drawn there.
    Caret {
        byte: usize,
    },
    /// `accessible` asks for PDF/UA-1, which is refused when the document doesn't meet it.
    ExportPdf {
        path: PathBuf,
        accessible: bool,
    },
    /// What the thing at a byte offset is, for the pointer resting on it.
    Hover {
        request: u64,
        byte: usize,
    },
    /// What could be typed at a byte offset. `explicit`: asked for with Ctrl+Space.
    Complete {
        request: u64,
        byte: usize,
        explicit: bool,
    },
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

#[derive(Debug, Clone)]
pub struct Pixels {
    pub size: [usize; 2],
    /// Premultiplied RGBA.
    pub rgba: Arc<Vec<u8>>,
}

/// Something changed, on a page, in points from its top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub kind: crate::compare::Kind,
}

/// Two versions on their pages.
#[derive(Clone)]
pub struct Comparison {
    pub old_pages: Vec<Page>,
    pub old_marks: Vec<Mark>,
    pub new_marks: Vec<Mark>,
    /// Changes in the source that show on neither page's own text, such as a rule.
    pub unseen: usize,
    /// Changes in all.
    pub total: usize,
}

/// Why a review mark can't be made where it was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickRefusal {
    /// There's no text there.
    Nothing,
    /// The words there come out of the document's code, or elsewhere, with no text of their own to change.
    Generated,
}

/// A stretch of a range of the source on a page, in points from its top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PagePlace {
    pub index: usize,
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A document as a variant would set it.
#[derive(Clone)]
pub struct VariantView {
    pub pages: usize,
    /// Its first pages, small.
    pub thumbnails: Vec<Pixels>,
    /// It looks just like the document does now: its rules are overruled, or already there.
    pub same_as_now: bool,
}

/// A past version: its text, and its pages or why it doesn't compile.
pub struct TimelineView {
    pub text: String,
    /// How many pages it has, and the first of them, small.
    pub pages: Result<(usize, Vec<Pixels>), String>,
}

/// Why a document isn't an accessible PDF, in Typst's own words, and where.
#[derive(Clone, Debug)]
pub struct ExportProblem {
    pub message: String,
    pub byte: Option<usize>,
}

/// What a search for fitting found. With no plan, there's nothing to change: see `ending`.
#[derive(Clone)]
pub struct FitAnswer {
    pub ending: crate::fit::Ending,
    pub plan: Option<crate::fit::Plan>,
    pub now: Option<crate::fit::Current>,
    pub pages_before: usize,
    pub pages_after: usize,
    /// The first and last pages, small.
    pub before: Vec<Pixels>,
    pub after: Vec<Pixels>,
}

/// An image on a page, in points from its top left, and where its call starts in the source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedImage {
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub byte: usize,
}

#[derive(Clone)]
pub struct Page {
    pub hash: u128,
    pub width_pt: f32,
    pub height_pt: f32,
    /// Distances from the page's left, right, top and bottom edges to its writing area, in points.
    pub margins: Option<[f32; 4]>,
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
    Compiled {
        pages: Option<Vec<Page>>,
        diags: Vec<Diag>,
        millis: u128,
    },
    JumpToSource {
        byte: usize,
    },
    JumpToPage {
        page: usize,
        y_pt: f32,
        x_pt: f32,
    },
    /// The answer to [`Job::Compare`].
    Compared {
        request: u64,
        result: Result<Comparison, String>,
    },
    /// The images on the pages, with what the source says about them.
    Images(Vec<PlacedImage>),
    /// What the typography watch found on the pages.
    Typography(Vec<crate::typography::Finding>),
    /// The answer to [`Job::Drop`]: the byte of the text nearest the drop, and whether it was above it.
    DropTarget {
        block: std::ops::Range<usize>,
        byte: usize,
        above: bool,
    },
    /// The answer to [`Job::Example`]: its first page, or why it failed.
    ExampleDone {
        request: u64,
        result: Result<Pixels, String>,
    },
    /// The answer to [`Job::Thumbnail`]: nothing when the template doesn't compile.
    Thumbnail {
        index: usize,
        pixels: Option<Pixels>,
    },
    /// One of the answers to [`Job::Variants`], or why that variant doesn't compile.
    VariantDone {
        request: u64,
        index: usize,
        result: Result<VariantView, String>,
    },
    /// The answer to [`Job::Fit`], unless it was cancelled, or why the document can't be measured.
    FitDone {
        request: u64,
        result: Result<FitAnswer, String>,
    },
    /// The answer to [`Job::TimelineVersion`]. Nothing, for a version that can't be read.
    TimelineDone {
        request: u64,
        stamp: u64,
        result: Option<TimelineView>,
    },
    /// The answer to [`Job::ReviewPick`].
    ReviewPicked {
        request: u64,
        result: Result<std::ops::Range<usize>, PickRefusal>,
    },
    /// The answer to [`Job::Places`], with `index` the range's place in what was asked.
    Places(Vec<PagePlace>),
    /// The answer to [`Job::Caret`], in points from the page's top left: `y` is the text's baseline.
    CaretAt {
        byte: usize,
        page: usize,
        x: f32,
        y: f32,
        size: f32,
    },
    /// The answer to [`Job::Hover`]: the words to show, and whether they're code.
    Hovered {
        request: u64,
        tip: Option<(String, bool)>,
    },
    /// The answer to [`Job::Complete`]: suggestions replacing the text from `from` to the caret.
    Completions {
        request: u64,
        from: usize,
        suggestions: Vec<Suggestion>,
    },
    /// The files of the project the document read (path from its folder, path on disk).
    Dependencies(Vec<(String, PathBuf)>),
    Exported(Result<PathBuf, String>),
    /// An accessible PDF wasn't written, because of these. Nothing changed on disk.
    ExportRefused(Vec<ExportProblem>),
    Failed(String),
}

/// Pixels per point for a template's tile: an A4 page comes out about 670 pixels tall, enough for
/// the top of it, enlarged in the tile, on a screen at twice the density.
const THUMBNAIL_SCALE: f64 = 0.8;

pub struct Compiler {
    jobs: Sender<Job>,
    pub events: Receiver<Event>,
    /// Woken when an event arrives, once the interface has given it.
    pub repaint: Arc<OnceLock<egui::Context>>,
    /// The longest side a page texture may have on this GPU.
    pub max_side: Arc<AtomicUsize>,
    /// Whether pages are shown on dark paper; takes effect at the next compile.
    pub dark_paper: Arc<AtomicBool>,
}

impl Compiler {
    pub fn start() -> Result<Self, String> {
        let (jobs, job_rx) = channel();
        let (event_tx, events) = channel();
        let repaint = Arc::new(OnceLock::new());
        let wake = Arc::clone(&repaint);
        let max_side = Arc::new(AtomicUsize::new(SAFE_TEXTURE_SIDE));
        let limit = Arc::clone(&max_side);
        let dark_paper = Arc::new(AtomicBool::new(false));
        let dark = Arc::clone(&dark_paper);
        std::thread::Builder::new()
            .name("typst".into())
            // Deeply nested documents recurse far; the default stack would overflow first.
            .stack_size(32 * 1024 * 1024)
            .spawn(move || Worker::new(event_tx, wake, limit, dark).run(job_rx))
            .map_err(|e| e.to_string())?;
        Ok(Self { jobs, events, repaint, max_side, dark_paper })
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
    dark_paper: Arc<AtomicBool>,
    fonts: Arc<typst_kit::fonts::FontStore>,
    world: Option<DocWorld>,
    path: Option<PathBuf>,
    probe_world: Option<DocWorld>,
    example_world: Option<DocWorld>,
    compare_world: Option<DocWorld>,
    compare_path: Option<PathBuf>,
    compare_rendered: HashMap<u128, Pixels>,
    probe_path: Option<PathBuf>,
    /// The request for variants whose answers are still wanted.
    variants_wanted: Arc<AtomicU64>,
    fit_wanted: Arc<AtomicU64>,
    doc: Option<PagedDocument>,
    rendered: HashMap<u128, Pixels>,
    /// The project the worlds were made for (see `world::generation`).
    generation: u64,
}

impl Worker {
    fn new(events: Sender<Event>, wake: Arc<OnceLock<egui::Context>>, max_side: Arc<AtomicUsize>, dark_paper: Arc<AtomicBool>) -> Self {
        Self {
            events,
            wake,
            max_side,
            dark_paper,
            fonts: load_fonts(),
            world: None,
            path: None,
            probe_world: None,
            example_world: None,
            compare_world: None,
            compare_path: None,
            compare_rendered: HashMap::new(),
            probe_path: None,
            variants_wanted: Arc::new(AtomicU64::new(0)),
            fit_wanted: Arc::new(AtomicU64::new(0)),
            doc: None,
            rendered: HashMap::new(),
            generation: 0,
        }
    }

    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
        if let Some(ctx) = self.wake.get() {
            ctx.request_repaint();
        }
    }

    fn run(mut self, jobs: Receiver<Job>) {
        while let Ok(first) = jobs.recv() {
            // Another project opened: every world resolves paths from somewhere else now.
            let generation = crate::world::generation();
            if generation != self.generation {
                self.generation = generation;
                self.world = None;
                self.probe_world = None;
                self.compare_world = None;
            }
            // Only the newest text matters; other jobs keep their order after it.
            let mut compile = None;
            let mut others = Vec::new();
            let mut complete = None;
            let mut hover = None;
            let mut caret = None;
            for job in std::iter::once(first).chain(jobs.try_iter()) {
                match job {
                    Job::Compile { text, path } => compile = Some((text, path)),
                    Job::Hover { request, byte } => hover = Some((request, byte)),
                    Job::Caret { byte } => caret = Some(byte),
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
            if let Some(byte) = caret {
                self.caret(byte);
            }
            if let Some((request, byte)) = hover {
                self.hover(request, byte);
            }
            if let Some((request, byte, explicit)) = complete {
                self.complete(request, byte, explicit);
            }
        }
    }

    /// The size and margins of a page at the end of this text. It compiles in a world of its own, so
    /// the text and the positions everything else works from aren't touched, and a document it
    /// can't be added to (a body wrapped in `columns()`, say) just goes without.
    fn measure_margins(&mut self, text: &str, path: Option<&std::path::Path>) -> Option<(f32, f32, [f32; 4])> {
        if self.probe_world.is_none() || self.probe_path.as_deref() != path {
            self.probe_world = DocWorld::new(Arc::clone(&self.fonts), path);
            self.probe_path = path.map(std::path::Path::to_path_buf);
        }
        let world = self.probe_world.as_mut()?;
        world.update(&format!("{text}{MARGIN_PROBE}"));
        let world = &*world;
        let Ok(warned) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| typst::compile::<PagedDocument>(world))) else {
            self.probe_world = None;
            return None;
        };
        let doc = warned.output.ok()?;
        let label = Label::new(PicoStr::intern("pearl-margins"))?;
        let found = doc.introspector().query_label(label).ok()?;
        let value = found.to_packed::<MetadataElem>()?.value.clone();
        let Value::Array(numbers) = value else { return None };
        let report: Vec<f64> = numbers.into_iter().filter_map(|n| n.cast::<f64>().ok()).collect();
        let size = doc.pages().last()?.frame.size();
        let margins = margins_from(&report, size.x.to_pt(), size.y.to_pt())?;
        Some((size.x.to_pt() as f32, size.y.to_pt() as f32, margins))
    }

    fn compile(&mut self, text: &str, path: Option<PathBuf>) {
        let started = Instant::now();
        if self.world.is_none() || self.path != path {
            self.world = DocWorld::new(Arc::clone(&self.fonts), path.as_deref());
            self.path = path;
        }
        let Some(world) = self.world.as_mut() else {
            self.emit(Event::Failed(tx("couldnt-open-the-documents-folder").into()));
            return;
        };
        world.update(text);
        let world = &*world;
        // A panic in the compiler must not take the unsaved text down with it.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| typst::compile::<PagedDocument>(world)));
        let Ok(warned) = result else {
            self.emit(Event::Failed(tx("the-compiler-failed-on-this-document").into()));
            self.world = None;
            return;
        };
        let path_for_probe = self.path.clone();
        let to_diag = |d: &typst::diag::SourceDiagnostic| {
            let in_main = d.span.id() == Some(typst::World::main(world));
            Diag {
                error: d.severity == Severity::Error,
                message: d.message.to_string(),
                byte: if in_main { world.range(d.span).map(|r| r.start) } else { None },
            }
        };
        let mut diags: Vec<Diag> = warned.warnings.iter().map(to_diag).collect();
        let pages = match warned.output {
            Ok(doc) => {
                let mut pages = self.render(&doc);
                self.doc = Some(doc);
                if let Some((width, height, margins)) = self.measure_margins(text, path_for_probe.as_deref()) {
                    for page in pages.iter_mut().filter(|p| (p.width_pt - width).abs() < 0.01 && (p.height_pt - height).abs() < 0.01) {
                        page.margins = Some(margins);
                    }
                }
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
        if pages.is_some()
            && let Some(doc) = &self.doc
        {
            self.emit(Event::Images(self.placed_images(doc)));
            if let (Some(world), Some(pages)) = (&self.world, &pages) {
                let margins: Vec<_> = pages.iter().map(|page| page.margins).collect();
                self.emit(Event::Typography(crate::typography::analyse(doc, world.source(), &margins)));
            }
        }
        self.emit(Event::Compiled { pages, diags, millis: started.elapsed().as_millis() });
    }

    /// Rasterises the pages that changed; the others come from the cache.
    fn render(&mut self, doc: &PagedDocument) -> Vec<Page> {
        let limit = self.max_side.load(Ordering::Relaxed);
        let dark = self.dark_paper.load(Ordering::Relaxed);
        let mut out = Vec::with_capacity(doc.pages().len());
        for page in doc.pages() {
            let size = page.frame.size();
            let scale = page_scale(size.x.to_pt(), size.y.to_pt(), limit);
            let options = RenderOptions { pixel_per_pt: typst::utils::Scalar::new(scale), render_bleed: false };
            let hash = hash128(&(&page.frame, &page.fill, scale.to_bits(), dark));
            let pixels = self
                .rendered
                .entry(hash)
                .or_insert_with(|| {
                    let map =
                        if dark { typst_render::render(&crate::dark_paper::page(page), &options) } else { typst_render::render(page, &options) };
                    Pixels { size: [map.width() as usize, map.height() as usize], rgba: Arc::new(map.take_demultiplied()) }
                })
                .clone();
            out.push(Page { hash, width_pt: size.x.to_pt() as f32, height_pt: size.y.to_pt() as f32, margins: None, pixels });
        }
        if self.rendered.len() > out.len() + CACHE_SLACK {
            self.rendered.retain(|h, _| out.iter().any(|p| p.hash == *h));
        }
        out
    }

    fn other(&mut self, job: Job) {
        match job {
            Job::Compile { .. } | Job::Complete { .. } | Job::Hover { .. } | Job::Caret { .. } => {}
            Job::Click { page, x, y } => self.click(page, x, y),
            Job::Locate { byte } => self.locate(byte),
            Job::ReviewPick { request, page, from, to } => {
                let result = self.review_pick(page, from, to);
                self.emit(Event::ReviewPicked { request, result });
            }
            Job::Places { ranges } => {
                let places = self.places(&ranges);
                self.emit(Event::Places(places));
            }
            Job::Drop { page, x, y, block } => self.drop_target(page, x, y, block),
            Job::Compare { old, request } => {
                let result = self.compare(&old);
                self.emit(Event::Compared { request, result });
            }
            Job::Example { request, text } => {
                let result = self.example(&text);
                self.emit(Event::ExampleDone { request, result });
            }
            Job::Thumbnail { index, text } => {
                let pixels = self.first_page(&text, THUMBNAIL_SCALE).ok();
                self.emit(Event::Thumbnail { index, pixels });
            }
            Job::Variants { request, text, path, variants } => self.start_variants(request, text, path, variants),
            Job::CancelVariants => self.variants_wanted.store(0, Ordering::Relaxed),
            Job::Fit { request, text, path, target } => self.start_fit(request, text, path, target),
            Job::CancelFit => self.fit_wanted.store(0, Ordering::Relaxed),
            Job::TimelineVersion { request, store, version, path } => self.start_timeline_version(request, store, version, path),
            Job::ExportPdf { path, accessible } => {
                let event = match self.export(&path, accessible) {
                    Ok(()) => Event::Exported(Ok(path)),
                    Err(Refusal::Problems(problems)) => Event::ExportRefused(problems),
                    Err(Refusal::Failure(why)) => Event::Exported(Err(why)),
                };
                self.emit(event);
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
            Some(Jump::Position(p)) => {
                self.emit(Event::JumpToPage { page: p.page.get() - 1, y_pt: p.point.y.to_pt() as f32, x_pt: p.point.x.to_pt() as f32 })
            }
            Some(Jump::File(..) | Jump::Url(_)) => {}
            // In the blank beside or below the text: the nearest of this document's own text, as in a
            // word processor.
            None => {
                let main = typst::World::main(world);
                let mut nearest: Option<Nearest> = None;
                if let Some(page) = doc.pages().get(position.page.get() - 1) {
                    nearest_glyph(&page.frame, Transform::identity(), position.point, &|span| span.id() == Some(main), &mut nearest);
                }
                if let Some(found) = nearest
                    && let Some(range) = world.range(found.span)
                {
                    self.emit(Event::JumpToSource { byte: range.start + usize::from(found.offset) });
                }
            }
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

    /// The old version in a world of its own, so the document's stays as it is.
    fn compare(&mut self, old: &str) -> Result<Comparison, String> {
        let failed = || tx("the-versions-could-not-be-compared").to_string();
        if self.compare_world.is_none() || self.compare_path != self.path {
            self.compare_world = DocWorld::new(Arc::clone(&self.fonts), self.path.as_deref());
            self.compare_path.clone_from(&self.path);
        }
        let world = self.compare_world.as_mut().ok_or_else(failed)?;
        world.update(old);
        let world = &*world;
        let Ok(warned) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| typst::compile::<PagedDocument>(world))) else {
            self.compare_world = None;
            return Err(failed());
        };
        let old_doc = warned.output.map_err(|errors| errors.first().map_or_else(failed, |e| e.message.to_string()))?;
        let (Some(new_world), Some(new_doc)) = (&self.world, &self.doc) else { return Err(failed()) };
        let found = crate::compare::changes(old, new_world.source().text());
        let total = found.old.len() + found.new.len();
        let (old_marks, old_seen) = marks_of(&old_doc, world, &found.old);
        let (new_marks, new_seen) = marks_of(new_doc, new_world, &found.new);
        let unseen = total.saturating_sub(old_seen + new_seen);
        let limit = self.max_side.load(Ordering::Relaxed);
        let mut old_pages = Vec::with_capacity(old_doc.pages().len());
        for page in old_doc.pages() {
            let size = page.frame.size();
            let scale = page_scale(size.x.to_pt(), size.y.to_pt(), limit);
            let hash = hash128(&(&page.frame, &page.fill, scale.to_bits()));
            let pixels = self
                .compare_rendered
                .entry(hash)
                .or_insert_with(|| {
                    let options = RenderOptions { pixel_per_pt: typst::utils::Scalar::new(scale), render_bleed: false };
                    let map = typst_render::render(page, &options);
                    Pixels { size: [map.width() as usize, map.height() as usize], rgba: Arc::new(map.take_demultiplied()) }
                })
                .clone();
            old_pages.push(Page { hash, width_pt: size.x.to_pt() as f32, height_pt: size.y.to_pt() as f32, margins: None, pixels });
        }
        self.compare_rendered.retain(|hash, _| old_pages.iter().any(|page| page.hash == *hash));
        Ok(Comparison { old_pages, old_marks, new_marks, unseen, total })
    }

    /// The images of the document's own source, for selecting and dragging on the pages.
    fn placed_images(&self, doc: &PagedDocument) -> Vec<PlacedImage> {
        let Some(world) = &self.world else { return Vec::new() };
        let main = typst::World::main(world);
        let mut found = Vec::new();
        for (index, page) in doc.pages().iter().enumerate() {
            images_in_frame(&page.frame, Transform::identity(), index, &mut found);
        }
        found
            .into_iter()
            .filter(|(_, _, span)| span.id() == Some(main))
            .filter_map(|(page, [x, y, width, height], span)| {
                Some(PlacedImage { page, x: x as f32, y: y as f32, width: width as f32, height: height as f32, byte: world.range(span)?.start })
            })
            .collect()
    }

    /// The source between two points of a page, out to whole words. Only what's written as text in this
    /// document can be marked: anything else says so, rather than guess at where it came from.
    fn review_pick(&self, page: usize, from: [f64; 2], to: [f64; 2]) -> Result<std::ops::Range<usize>, PickRefusal> {
        /// How far from a letter a point may be, by `nearest_glyph`'s measure.
        const REACH: f64 = 80.0;
        let (Some(world), Some(doc)) = (&self.world, &self.doc) else { return Err(PickRefusal::Nothing) };
        let page = doc.pages().get(page).ok_or(PickRefusal::Nothing)?;
        let source = world.source();
        let byte_at = |[x, y]: [f64; 2]| -> Result<usize, PickRefusal> {
            let mut nearest: Option<Nearest> = None;
            nearest_glyph(&page.frame, Transform::identity(), Point::new(Abs::pt(x), Abs::pt(y)), &|_| true, &mut nearest);
            let found = nearest.filter(|n| n.distance <= REACH).ok_or(PickRefusal::Nothing)?;
            let node = source.find(found.span).ok_or(PickRefusal::Generated)?;
            if !matches!(
                node.kind(),
                SyntaxKind::Text | SyntaxKind::SmartQuote | SyntaxKind::Escape | SyntaxKind::Shorthand | SyntaxKind::MathText | SyntaxKind::MathIdent
            ) {
                return Err(PickRefusal::Generated);
            }
            Ok(node.offset() + usize::from(found.offset))
        };
        let (a, b) = (byte_at(from)?, byte_at(to)?);
        let text = source.text();
        let word = |letter: char| letter.is_alphanumeric() || letter == '_';
        let (low, high) = (a.min(b), a.max(b));
        let start = text
            .get(..low)
            .map_or(low, |before| before.rfind(|c| !word(c)).map_or(0, |at| at + before[at..].chars().next().map_or(1, char::len_utf8)));
        let after = text.get(high..).ok_or(PickRefusal::Generated)?;
        // On a letter that's no part of a word, as a stop, its own width is the end.
        let own = after.chars().next().map_or(0, char::len_utf8);
        let end = high + after.find(|c| !word(c)).map_or(after.len(), |at| at.max(own));
        (start < end && text.get(start..end).is_some()).then_some(start..end).ok_or(PickRefusal::Nothing)
    }

    /// Where ranges of the source are on the pages.
    fn places(&self, ranges: &[std::ops::Range<usize>]) -> Vec<PagePlace> {
        let (Some(world), Some(doc)) = (&self.world, &self.doc) else { return Vec::new() };
        let main = typst::World::main(world);
        let mut starts = HashMap::new();
        let mut start_of = |span: Span| -> Option<usize> {
            if span.id() != Some(main) {
                return None;
            }
            *starts.entry(span).or_insert_with(|| world.range(span).map(|range| range.start))
        };
        let mut found = Vec::new();
        for (index, page) in doc.pages().iter().enumerate() {
            places_in_frame(&page.frame, Transform::identity(), index, ranges, &mut start_of, &mut found);
        }
        found
    }

    fn drop_target(&self, page: usize, x: f64, y: f64, block: std::ops::Range<usize>) {
        let (Some(world), Some(doc)) = (&self.world, &self.doc) else { return };
        let Some(page) = doc.pages().get(page) else { return };
        let main = typst::World::main(world);
        let wanted = |span: Span| span.id() == Some(main) && world.range(span).is_some_and(|r| r.start < block.start || r.start >= block.end);
        let mut nearest = None;
        nearest_glyph(&page.frame, Transform::identity(), Point::new(Abs::pt(x), Abs::pt(y)), &wanted, &mut nearest);
        if let Some(found) = nearest
            && let Some(range) = world.range(found.span)
        {
            self.emit(Event::DropTarget { block, byte: range.start + usize::from(found.offset), above: found.below < 0.0 });
        }
    }

    fn start_variants(&self, request: u64, text: String, path: Option<PathBuf>, variants: Vec<crate::variants::Variant>) {
        self.variants_wanted.store(request, Ordering::Relaxed);
        let (wanted, fonts, events, wake) = (Arc::clone(&self.variants_wanted), Arc::clone(&self.fonts), self.events.clone(), Arc::clone(&self.wake));
        let tell = move |index: usize, result: Result<VariantView, String>| {
            let _ = events.send(Event::VariantDone { request, index, result });
            if let Some(ctx) = wake.get() {
                ctx.request_repaint();
            }
        };
        let told = tell.clone();
        let count = variants.len();
        let work = move || {
            let mut world = None;
            let mut now: Option<VariantView> = None;
            for (index, variant) in variants.iter().enumerate() {
                if wanted.load(Ordering::Relaxed) != request {
                    return;
                }
                let result = compile_variant(&variant.applied_to(&text), &mut world, &fonts, path.as_deref()).map(|mut view| {
                    match &now {
                        Some(current) => view.same_as_now = looks_the_same(current, &view),
                        None => {
                            if variant.changes_nothing() {
                                now = Some(view.clone());
                            }
                        }
                    }
                    view
                });
                tell(index, result);
            }
        };
        // Deep documents need the stack the compiler's thread has.
        if std::thread::Builder::new().name("variants".into()).stack_size(32 * 1024 * 1024).spawn(work).is_err() {
            for index in 0..count {
                told(index, Err(tx("the-compiler-failed-on-this-document").to_string()));
            }
        }
    }

    fn start_timeline_version(&self, request: u64, store: crate::history::Store, version: crate::history::Version, path: Option<PathBuf>) {
        let (fonts, events, wake) = (Arc::clone(&self.fonts), self.events.clone(), Arc::clone(&self.wake));
        let stamp = version.stamp;
        let tell = move |result: Option<TimelineView>| {
            let _ = events.send(Event::TimelineDone { request, stamp, result });
            if let Some(ctx) = wake.get() {
                ctx.request_repaint();
            }
        };
        let told = tell.clone();
        let work = move || {
            let result = store.read(&version).map(|text| {
                let pages = with_document(&text, &mut None, &fonts, path.as_deref(), |doc| {
                    (doc.pages().len(), doc.pages().iter().take(TIMELINE_PAGES).map(thumbnail).collect::<Vec<_>>())
                });
                TimelineView { text, pages }
            });
            tell(result);
        };
        if std::thread::Builder::new().name("timeline".into()).stack_size(32 * 1024 * 1024).spawn(work).is_err() {
            told(None);
        }
    }

    fn start_fit(&self, request: u64, text: String, path: Option<PathBuf>, target: usize) {
        self.fit_wanted.store(request, Ordering::Relaxed);
        let (wanted, fonts, events, wake) = (Arc::clone(&self.fit_wanted), Arc::clone(&self.fonts), self.events.clone(), Arc::clone(&self.wake));
        let tell = move |result: Result<FitAnswer, String>| {
            let _ = events.send(Event::FitDone { request, result });
            if let Some(ctx) = wake.get() {
                ctx.request_repaint();
            }
        };
        let told = tell.clone();
        let work = move || {
            let still_wanted = || wanted.load(Ordering::Relaxed) == request;
            let result = fit_document(&text, target, &fonts, path.as_deref(), FIT_TIME_LIMIT, &still_wanted);
            if still_wanted() {
                tell(result);
            }
        };
        if std::thread::Builder::new().name("fit".into()).stack_size(32 * 1024 * 1024).spawn(work).is_err() {
            told(Err(tx("the-compiler-failed-on-this-document").to_string()));
        }
    }

    /// An example, in a world of its own so the document's stays as it is, on a page the width of the pane.
    fn example(&mut self, text: &str) -> Result<Pixels, String> {
        const PAGE: &str = "#set page(width: 260pt, height: auto, margin: 8pt)\n";
        self.first_page(&format!("{PAGE}{text}"), 2.0)
    }

    /// Compiles `text` on its own, out of the document's way, and draws its first page.
    fn first_page(&mut self, text: &str, scale: f64) -> Result<Pixels, String> {
        const SCALE_RANGE: std::ops::RangeInclusive<f64> = 0.05..=4.0;
        let scale = scale.clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end());
        if self.example_world.is_none() {
            self.example_world = DocWorld::new(Arc::clone(&self.fonts), None);
        }
        let world = self.example_world.as_mut().ok_or(tx("the-example-could-not-run"))?;
        world.update(text);
        let world = &*world;
        let Ok(warned) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| typst::compile::<PagedDocument>(world))) else {
            self.example_world = None;
            return Err(tx("the-example-could-not-run").to_string());
        };
        let doc = warned.output.map_err(|errors| errors.first().map_or_else(String::new, |e| e.message.to_string()))?;
        let page = doc.pages().first().ok_or(tx("the-example-could-not-run"))?;
        // A page too large to draw at this scale is drawn smaller: its pixels stay within bounds.
        let size = page.frame.size();
        let scale = scale.min(page_scale(size.x.to_pt(), size.y.to_pt(), SAFE_TEXTURE_SIDE));
        let options = RenderOptions { pixel_per_pt: typst::utils::Scalar::new(scale), render_bleed: false };
        let map = typst_render::render(page, &options);
        Ok(Pixels { size: [map.width() as usize, map.height() as usize], rgba: Arc::new(map.take_demultiplied()) })
    }

    /// Typst's own answer gives the start of the whole run of text, so the glyphs are searched here
    /// for the one that came from this byte.
    fn caret(&self, byte: usize) {
        let (Some(world), Some(doc)) = (&self.world, &self.doc) else { return };
        let source = world.source();
        let root = LinkedNode::new(source.root());
        let is_text = |node: &LinkedNode| matches!(node.kind(), SyntaxKind::Text | SyntaxKind::MathText);
        let Some(node) = root.leaf_at(byte, Side::Before).filter(is_text).or_else(|| root.leaf_at(byte, Side::After).filter(is_text)) else {
            return;
        };
        let wanted = byte.saturating_sub(node.offset());
        let Ok(wanted) = u16::try_from(wanted) else { return };
        let mut best: Option<(u16, usize, Point, f32)> = None;
        for (index, page) in doc.pages().iter().enumerate() {
            caret_in_frame(&page.frame, Transform::identity(), node.span(), wanted, index, &mut best);
        }
        if let Some((_, page, at, size)) = best {
            self.emit(Event::CaretAt { byte, page, x: at.x.to_pt() as f32, y: at.y.to_pt() as f32, size });
        }
    }

    fn export(&self, path: &std::path::Path, accessible: bool) -> Result<(), Refusal> {
        let doc = self.doc.as_ref().ok_or_else(|| Refusal::Failure(tx("the-document-hasnt-compiled-yet").to_string()))?;
        let standards = if accessible {
            PdfStandards::new(&[PdfStandard::Ua_1]).map_err(|e| Refusal::Failure(e.message().to_string()))?
        } else {
            PdfStandards::default()
        };
        let bytes = typst_pdf::pdf(doc, &PdfOptions { standards, ..PdfOptions::default() }).map_err(|errors| {
            if accessible {
                let world = self.world.as_ref();
                let main = world.map(typst::World::main);
                let problems = errors.iter().map(|e| {
                    let byte = world.filter(|_| e.span.id() == main && main.is_some()).and_then(|w| w.range(e.span)).map(|range| range.start);
                    ExportProblem { message: e.message.to_string(), byte }
                });
                Refusal::Problems(problems.collect())
            } else {
                Refusal::Failure(errors.iter().map(|e| e.message.to_string()).collect::<Vec<_>>().join("; "))
            }
        })?;
        write_whole(path, &bytes).map_err(|e| Refusal::Failure(e.to_string()))
    }
}

enum Refusal {
    Problems(Vec<ExportProblem>),
    Failure(String),
}

/// Writes a file whole or not at all, and on the disk before this returns: a failure at any point,
/// or the machine's end, leaves what was there before.
pub(crate) fn write_whole(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    replace_file(path, |file| std::io::Write::write_all(file, bytes))
}

/// Tells this run's temporary files apart, as threads write different files at once.
static PARTS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// A new file takes the place of the old one all at once, as the old one would have been written:
/// through a link, with its permissions, and only if this user may write to it. A rename by itself
/// would replace the link and go over a read-only file.
fn replace_file(path: &std::path::Path, fill: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let existing = match std::fs::metadata(&target) {
        Ok(metadata) => Some(metadata),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    if let Some(metadata) = &existing {
        // Before opening: a pipe would wait for a reader, and nothing here is meant for one.
        if !metadata.is_file() {
            return Err(Error::new(ErrorKind::InvalidInput, "not a regular file"));
        }
        // Changes nothing: opening without truncating only asks the system.
        std::fs::OpenOptions::new().write(true).open(&target)?;
    }
    let mut name = target.file_name().map(std::ffi::OsStr::to_os_string).unwrap_or_default();
    let n = PARTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    name.push(format!(".{}-{n}.pearl-part", std::process::id()));
    let part = target.with_file_name(name);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    // What may be private stays so until it has its permissions. A new file gets the usual ones.
    #[cfg(unix)]
    if existing.is_some() {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&part)?;
    let written = fill(&mut file)
        .and_then(|()| existing.map_or(Ok(()), |metadata| file.set_permissions(metadata.permissions())))
        .and_then(|()| file.sync_all())
        .and_then(|()| {
            drop(file);
            std::fs::rename(&part, &target)
        });
    if written.is_err() {
        let _ = std::fs::remove_file(&part);
        return written;
    }
    // The rename reaches the disk with the folder. Not every system syncs one; the file is in place.
    #[cfg(unix)]
    if let Some(folder) = target.parent() {
        let folder = if folder.as_os_str().is_empty() { std::path::Path::new(".") } else { folder };
        let _ = std::fs::File::open(folder).and_then(|f| f.sync_all());
    }
    Ok(())
}

/// The text closest to a point.
struct Nearest {
    distance: f64,
    span: Span,
    offset: u16,
    /// How far the point is below the text, in points.
    below: f64,
}

/// Keeps, in `nearest`, the glyph that's `wanted` closest to `to`. Distance across lines counts for
/// more than along them, so that a click beside a line goes to that line.
fn nearest_glyph(frame: &Frame, ts: Transform, to: Point, wanted: &dyn Fn(Span) -> bool, nearest: &mut Option<Nearest>) {
    for (pos, item) in frame.items() {
        match item {
            FrameItem::Group(group) => {
                let inner = ts.pre_concat(Transform::translate(pos.x, pos.y)).pre_concat(group.transform);
                nearest_glyph(&group.frame, inner, to, wanted, nearest);
            }
            FrameItem::Text(text) => {
                let mut x = Abs::zero();
                for glyph in &text.glyphs {
                    let advance = glyph.x_advance.at(text.size);
                    let (span, offset) = glyph.span;
                    if wanted(span) {
                        let middle = text.size * 0.3;
                        let start = Point::new(pos.x + x, pos.y - middle).transform(ts);
                        let (across, down) = ((start.x - to.x).to_pt(), (start.y - to.y).to_pt());
                        let distance = across.abs() + down.abs() * 8.0;
                        if nearest.as_ref().is_none_or(|best| distance < best.distance) {
                            *nearest = Some(Nearest { distance, span, offset, below: -down });
                        }
                    }
                    x += advance;
                }
            }
            _ => {}
        }
    }
}

/// The marks of a document's changed stretches, and how many stretches showed in it.
fn marks_of(doc: &PagedDocument, world: &DocWorld, stretches: &[(std::ops::Range<usize>, crate::compare::Kind)]) -> (Vec<Mark>, usize) {
    let main = typst::World::main(world);
    let mut starts = HashMap::new();
    let mut start_of = |span: Span| -> Option<usize> {
        if span.id() != Some(main) {
            return None;
        }
        *starts.entry(span).or_insert_with(|| world.range(span).map(|range| range.start))
    };
    let mut marks = Vec::new();
    let mut seen = vec![false; stretches.len()];
    for (index, page) in doc.pages().iter().enumerate() {
        marks_in_frame(&page.frame, Transform::identity(), index, stretches, &mut start_of, &mut seen, &mut marks);
    }
    (marks, seen.iter().filter(|shown| **shown).count())
}

#[allow(clippy::too_many_arguments)]
fn marks_in_frame(
    frame: &Frame,
    ts: Transform,
    page: usize,
    stretches: &[(std::ops::Range<usize>, crate::compare::Kind)],
    start_of: &mut dyn FnMut(Span) -> Option<usize>,
    seen: &mut [bool],
    marks: &mut Vec<Mark>,
) {
    use crate::compare::kind_at;
    for (pos, item) in frame.items() {
        match item {
            FrameItem::Group(group) => {
                let inner = ts.pre_concat(Transform::translate(pos.x, pos.y)).pre_concat(group.transform);
                marks_in_frame(&group.frame, inner, page, stretches, start_of, seen, marks);
            }
            FrameItem::Text(text) => {
                let mut x = Abs::zero();
                let mut open: Option<(usize, Abs, Abs)> = None;
                let close = |open: &mut Option<(usize, Abs, Abs)>, marks: &mut Vec<Mark>| {
                    if let Some((stretch, from, to)) = open.take()
                        && let Some((_, kind)) = stretches.get(stretch)
                    {
                        let a = Point::new(pos.x + from, pos.y - text.size * 0.8).transform(ts);
                        let b = Point::new(pos.x + to, pos.y + text.size * 0.25).transform(ts);
                        marks.push(Mark {
                            page,
                            x: a.x.to_pt() as f32,
                            y: a.y.to_pt() as f32,
                            width: (b.x - a.x).to_pt() as f32,
                            height: (b.y - a.y).to_pt() as f32,
                            kind: *kind,
                        });
                    }
                };
                for glyph in &text.glyphs {
                    let advance = glyph.x_advance.at(text.size);
                    let (span, offset) = glyph.span;
                    let found = start_of(span).and_then(|start| kind_at(stretches, start + usize::from(offset)));
                    match (found, &mut open) {
                        (Some(stretch), Some((current, _, to))) if *current == stretch => *to = x + advance,
                        (Some(stretch), _) => {
                            close(&mut open, marks);
                            open = Some((stretch, x, x + advance));
                            if let Some(shown) = seen.get_mut(stretch) {
                                *shown = true;
                            }
                        }
                        (None, _) => close(&mut open, marks),
                    }
                    x += advance;
                }
                close(&mut open, marks);
            }
            FrameItem::Image(_, size, span) => {
                if let Some(stretch) = start_of(*span).and_then(|start| kind_at(stretches, start))
                    && let Some((_, kind)) = stretches.get(stretch)
                {
                    let a = pos.transform(ts);
                    marks.push(Mark {
                        page,
                        x: a.x.to_pt() as f32,
                        y: a.y.to_pt() as f32,
                        width: size.x.to_pt() as f32,
                        height: size.y.to_pt() as f32,
                        kind: *kind,
                    });
                    if let Some(shown) = seen.get_mut(stretch) {
                        *shown = true;
                    }
                }
            }
            _ => {}
        }
    }
}

/// Where ranges fall in a frame, one stretch for each run of text in each.
fn places_in_frame(
    frame: &Frame,
    ts: Transform,
    page: usize,
    ranges: &[std::ops::Range<usize>],
    start_of: &mut dyn FnMut(Span) -> Option<usize>,
    found: &mut Vec<PagePlace>,
) {
    for (pos, item) in frame.items() {
        match item {
            FrameItem::Group(group) => {
                let inner = ts.pre_concat(Transform::translate(pos.x, pos.y)).pre_concat(group.transform);
                places_in_frame(&group.frame, inner, page, ranges, start_of, found);
            }
            FrameItem::Text(text) => {
                let mut runs: Vec<Option<(Abs, Abs)>> = vec![None; ranges.len()];
                let mut x = Abs::zero();
                for glyph in &text.glyphs {
                    let advance = glyph.x_advance.at(text.size);
                    if let Some(byte) = start_of(glyph.span.0).map(|start| start + usize::from(glyph.span.1)) {
                        for (index, range) in ranges.iter().enumerate() {
                            if range.contains(&byte)
                                && let Some(run) = runs.get_mut(index)
                            {
                                *run = Some(run.map_or((x, x + advance), |(from, _)| (from, x + advance)));
                            }
                        }
                    }
                    x += advance;
                }
                for (index, run) in runs.iter().enumerate() {
                    if let Some((from, to)) = run {
                        let a = Point::new(pos.x + *from, pos.y - text.size * 0.8).transform(ts);
                        let b = Point::new(pos.x + *to, pos.y + text.size * 0.25).transform(ts);
                        found.push(PagePlace {
                            index,
                            page,
                            x: a.x.to_pt() as f32,
                            y: a.y.to_pt() as f32,
                            width: (b.x - a.x).to_pt() as f32,
                            height: (b.y - a.y).to_pt() as f32,
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

/// Compiles a text in a world only this thread uses, and reads the result. A panic loses the world, not
/// the document.
fn with_document<T>(
    text: &str,
    slot: &mut Option<DocWorld>,
    fonts: &Arc<typst_kit::fonts::FontStore>,
    path: Option<&std::path::Path>,
    read: impl FnOnce(&PagedDocument) -> T,
) -> Result<T, String> {
    if slot.is_none() {
        *slot = DocWorld::new(Arc::clone(fonts), path);
    }
    let failed = || tx("the-compiler-failed-on-this-document").to_string();
    let world = slot.as_mut().ok_or_else(failed)?;
    world.update(text);
    let world = &*world;
    let Ok(answer) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<T, String> {
        let doc =
            typst::compile::<PagedDocument>(world).output.map_err(|errors| errors.first().map_or_else(String::new, |e| e.message.to_string()))?;
        Ok(read(&doc))
    })) else {
        *slot = None;
        return Err(failed());
    };
    answer
}

fn compile_variant(
    text: &str,
    slot: &mut Option<DocWorld>,
    fonts: &Arc<typst_kit::fonts::FontStore>,
    path: Option<&std::path::Path>,
) -> Result<VariantView, String> {
    with_document(text, slot, fonts, path, |doc| {
        let thumbnails = doc.pages().iter().take(VARIANT_PAGES).map(thumbnail).collect();
        VariantView { pages: doc.pages().len(), thumbnails, same_as_now: false }
    })
}

/// The first and last pages of a document, and how many it has.
fn ends_of(doc: &PagedDocument) -> (usize, Vec<Pixels>) {
    let pages = doc.pages();
    let ends = if pages.len() > 1 { [pages.first(), pages.last()] } else { [pages.first(), None] };
    (pages.len(), ends.into_iter().flatten().map(thumbnail).collect())
}

/// Measures what a document has: its type size, spacing and, if they can be asked for, its margins.
fn measure_for_fit(
    text: &str,
    slot: &mut Option<DocWorld>,
    fonts: &Arc<typst_kit::fonts::FontStore>,
    path: Option<&std::path::Path>,
) -> Option<crate::fit::Current> {
    let numbers = |doc: &PagedDocument, name: &str| -> Option<Vec<f64>> {
        let label = Label::new(PicoStr::intern(name))?;
        let found = doc.introspector().query_label(label).ok()?;
        let Value::Array(numbers) = found.to_packed::<MetadataElem>()?.value.clone() else { return None };
        Some(numbers.into_iter().filter_map(|n| n.cast::<f64>().ok()).collect())
    };
    let report = with_document(&format!("{text}{FIT_PROBE}"), slot, fonts, path, |doc| numbers(doc, "pearl-fit")).ok()??;
    let &[size, leading, spacing] = report.as_slice() else { return None };
    let margins = with_document(&format!("{text}{MARGIN_PROBE}"), slot, fonts, path, |doc| {
        let page = doc.pages().last()?.frame.size();
        margins_from(&numbers(doc, "pearl-margins")?, page.x.to_pt(), page.y.to_pt())
    })
    .ok()
    .flatten();
    crate::fit::Current::measured(size, leading, spacing, margins)
}

/// The search for fitting a document, with all it compiles. `still_wanted` going false ends it early.
fn fit_document(
    text: &str,
    target: usize,
    fonts: &Arc<typst_kit::fonts::FontStore>,
    path: Option<&std::path::Path>,
    limit: std::time::Duration,
    still_wanted: &dyn Fn() -> bool,
) -> Result<FitAnswer, String> {
    use crate::fit::{Ending, Plan, ladder, search};
    let started = Instant::now();
    let mut world = None;
    let (pages_before, before) = with_document(text, &mut world, fonts, path, ends_of)?;
    let answer = |ending, plan: Option<Plan>, now, pages_after, after| FitAnswer {
        ending,
        plan,
        now,
        pages_before,
        pages_after,
        before: before.clone(),
        after,
    };
    if pages_before <= target {
        return Ok(answer(Ending::Reached, None, None, pages_before, Vec::new()));
    }
    let Some(now) = measure_for_fit(text, &mut world, fonts, path) else {
        return Ok(answer(Ending::OutOfReach, None, None, pages_before, Vec::new()));
    };
    let plans = ladder(&now);
    let found = search(
        plans.len(),
        target,
        |place| {
            let plan = plans.get(place).copied().unwrap_or_default();
            with_document(&plan.applied_to(text), &mut world, fonts, path, |doc| doc.pages().len())
        },
        || still_wanted() && started.elapsed() < limit,
    )?;
    let Some((place, _)) = found.plan else { return Ok(answer(found.ending, None, Some(now), pages_before, Vec::new())) };
    let plan = plans.get(place).copied().unwrap_or_default();
    let (pages_after, after) = with_document(&plan.applied_to(text), &mut world, fonts, path, ends_of)?;
    Ok(answer(found.ending, Some(plan), Some(now), pages_after, after))
}

fn thumbnail(page: &typst_layout::Page) -> Pixels {
    let size = page.frame.size();
    let scale = (THUMBNAIL_SIDES[0] / size.x.to_pt().max(1.0)).min(THUMBNAIL_SIDES[1] / size.y.to_pt().max(1.0));
    let options = RenderOptions { pixel_per_pt: typst::utils::Scalar::new(scale), render_bleed: false };
    let map = typst_render::render(page, &options);
    Pixels { size: [map.width() as usize, map.height() as usize], rgba: Arc::new(map.take_demultiplied()) }
}

/// By the pixels: the same look from a different source has different spans in its frames.
fn looks_the_same(a: &VariantView, b: &VariantView) -> bool {
    a.pages == b.pages
        && a.thumbnails.len() == b.thumbnails.len()
        && a.thumbnails.iter().zip(&b.thumbnails).all(|(x, y)| x.size == y.size && x.rgba == y.rgba)
}

/// The images in a frame, with their boxes as they end up on the page.
fn images_in_frame(frame: &Frame, ts: Transform, page: usize, found: &mut Vec<(usize, Rect2, Span)>) {
    for (pos, item) in frame.items() {
        match item {
            FrameItem::Group(group) => {
                let inner = ts.pre_concat(Transform::translate(pos.x, pos.y)).pre_concat(group.transform);
                images_in_frame(&group.frame, inner, page, found);
            }
            FrameItem::Image(_, size, span) => {
                let corners = [Point::zero(), Point::new(size.x, Abs::zero()), Point::new(Abs::zero(), size.y), Point::new(size.x, size.y)]
                    .map(|corner| (*pos + corner).transform(ts));
                let xs = corners.map(|c| c.x.to_pt());
                let ys = corners.map(|c| c.y.to_pt());
                let (left, right) = (xs.iter().copied().fold(f64::INFINITY, f64::min), xs.iter().copied().fold(f64::NEG_INFINITY, f64::max));
                let (top, bottom) = (ys.iter().copied().fold(f64::INFINITY, f64::min), ys.iter().copied().fold(f64::NEG_INFINITY, f64::max));
                found.push((page, [left, top, right - left, bottom - top], *span));
            }
            _ => {}
        }
    }
}

/// Left, top, width and height, in points.
type Rect2 = [f64; 4];

/// Keeps, in `best`, the place of the glyph of `span` closest before the byte `wanted` of it.
fn caret_in_frame(frame: &Frame, ts: Transform, span: Span, wanted: u16, page: usize, best: &mut Option<(u16, usize, Point, f32)>) {
    for (pos, item) in frame.items() {
        match item {
            FrameItem::Group(group) => {
                let inner = ts.pre_concat(Transform::translate(pos.x, pos.y)).pre_concat(group.transform);
                caret_in_frame(&group.frame, inner, span, wanted, page, best);
            }
            FrameItem::Text(text) => {
                let mut x = Abs::zero();
                for glyph in &text.glyphs {
                    let advance = glyph.x_advance.at(text.size);
                    let (glyph_span, offset) = glyph.span;
                    if glyph_span == span && offset <= wanted && best.as_ref().is_none_or(|(b, ..)| offset > *b) {
                        // The caret goes after this glyph once `wanted` has passed its start.
                        let across = if wanted > offset { x + advance } else { x };
                        let at = Point::new(pos.x + across, pos.y).transform(ts);
                        *best = Some((offset, page, at, text.size.to_pt() as f32));
                    }
                    x += advance;
                }
            }
            _ => {}
        }
    }
}

fn kind_name(kind: &typst_ide::CompletionKind) -> &'static str {
    use typst_ide::CompletionKind as K;
    match kind {
        K::Syntax => "sintaxe",
        K::Func => tx("function"),
        K::Type => "tipo",
        K::Param => tx("parameter"),
        K::Constant => "valor",
        K::Path => "arquivo",
        K::Package => "pacote",
        K::Label => tx("label"),
        K::Font => "fonte",
        K::Symbol(_) => tx("symbol"),
    }
}

/// Pixels per point for a page: the usual scale, or less when the page wouldn't fit in a texture.
/// A last page that reports where its writing area lies. Typst keeps no margins on the pages it
/// returns, and what it places on them says nothing reliable about them, so it is asked. `page()`
/// inherits the document's settings; one column makes the area's width the whole width.
const MARGIN_PROBE: &str = "\n#page(columns: 1)[#layout(size => context [#metadata((here().position().x / 1pt, here().position().y / 1pt, size.width / 1pt, size.height / 1pt)) <pearl-margins>])]\n";

/// What's set at the end of a document, in points. It takes no room, so the pages stay as they are.
const FIT_PROBE: &str =
    "\n#context [#metadata((text.size.to-absolute() / 1pt, par.leading.to-absolute() / 1pt, par.spacing.to-absolute() / 1pt)) <pearl-fit>]\n";

/// A page's left, right, top and bottom margins from what the probe reported, if they make sense:
/// a page of automatic height has no bottom to measure from, for one.
fn margins_from(report: &[f64], width: f64, height: f64) -> Option<[f32; 4]> {
    let &[x, y, w, h] = report else { return None };
    let margins = [x, width - x - w, y, height - y - h];
    margins.iter().all(|m| m.is_finite() && *m > -0.01).then(|| margins.map(|m| m.max(0.0) as f32))
}

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

    #[test]
    fn dark_paper_changes_the_page_but_not_its_pictures() {
        let text = r##"#set page(width: 200pt, height: 200pt, margin: 0pt)
#place(top + left, image(bytes("<svg xmlns='http://www.w3.org/2000/svg' width='50' height='50'><rect width='50' height='50' fill='white'/></svg>"), format: "svg", width: 50pt))
#place(bottom + right, rect(width: 50pt, height: 50pt, fill: black))"##;
        let c = Compiler::start().unwrap();
        let pixel = |page: &Page, x: usize, y: usize| {
            let at = (y * page.pixels.size[0] + x) * 4;
            let rgba = &page.pixels.rgba[at..at + 3];
            [rgba[0], rgba[1], rgba[2]]
        };
        for dark in [false, true] {
            c.dark_paper.store(dark, Ordering::Relaxed);
            c.send(Job::Compile { text: text.into(), path: None });
            let page = loop {
                match c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                    Event::Compiled { pages, diags, .. } => break pages.unwrap_or_else(|| panic!("{diags:?}")).remove(0),
                    Event::Failed(e) => panic!("{e}"),
                    _ => {}
                }
            };
            let [w, h] = page.pixels.size;
            let (paper, picture, box_) = (pixel(&page, w / 2, h / 2), pixel(&page, w / 16, h / 16), pixel(&page, w - w / 16, h - h / 16));
            assert_eq!(picture, [255, 255, 255], "the picture keeps its colours (dark {dark})");
            if dark {
                assert!(paper[0] < 50 && paper[1] < 50, "the paper is dark: {paper:?}");
                assert!(box_[0] > 200, "black turned light: {box_:?}");
            } else {
                assert_eq!(paper, [255, 255, 255]);
                assert_eq!(box_, [0, 0, 0]);
            }
        }
    }

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
    fn every_example_of_the_lessons_runs_in_both_languages() {
        let c = Compiler::start().unwrap();
        let mut asked = Vec::new();
        for lang in [crate::i18n::Lang::PtBr, crate::i18n::Lang::En] {
            crate::i18n::set(lang);
            for lesson in crate::guide::lessons() {
                for block in &lesson.blocks {
                    if let crate::guide::Block::Example(text) = block {
                        c.send(Job::Example { request: asked.len() as u64, text: text.clone() });
                        asked.push((lesson.title.clone(), text.clone()));
                    }
                }
            }
        }
        crate::i18n::set(crate::i18n::Lang::PtBr);
        assert!(asked.len() > 40);
        let mut answered = 0;
        while answered < asked.len() {
            if let Event::ExampleDone { request, result } = c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                let (title, text) = &asked[request as usize];
                let pixels = result.unwrap_or_else(|e| panic!("{title}: {e}\n{text}"));
                assert!(pixels.size[0] > 100 && pixels.size[1] > 10);
                answered += 1;
            }
        }
    }

    fn compared(old: &str, new: &str) -> Comparison {
        let c = Compiler::start().unwrap();
        c.send(Job::Compile { text: new.into(), path: None });
        c.send(Job::Compare { old: old.into(), request: 9 });
        loop {
            if let Event::Compared { request, result } = c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                assert_eq!(request, 9);
                return result.unwrap();
            }
        }
    }

    #[test]
    fn what_changed_is_marked_on_the_pages_of_both_versions() {
        use crate::compare::Kind;
        let found = compared("Um gato dorme aqui.\n", "Um gato branco dorme aqui.\n");
        assert_eq!(found.old_marks, []);
        assert_eq!(found.new_marks.len(), 1);
        let mark = found.new_marks[0];
        assert_eq!(mark.kind, Kind::Added);
        assert!(mark.width > 15.0 && mark.width < 60.0 && mark.height > 8.0 && mark.x > 60.0, "{mark:?}");

        let found = compared("Um gato branco dorme.\n", "Um gato dorme.\n");
        assert_eq!((found.old_marks.len(), found.new_marks.len()), (1, 0));
        assert_eq!(found.old_marks[0].kind, Kind::Removed);
        assert_eq!(found.old_pages.len(), 1);
    }

    #[test]
    fn what_moved_is_marked_as_moved_and_a_rule_as_unseen() {
        use crate::compare::Kind;
        let moved = "Este parágrafo só mudou de lugar.";
        let found = compared(&format!("{moved}\n\nOutro.\n"), &format!("Outro.\n\n{moved}\n"));
        assert!(!found.new_marks.is_empty() && found.new_marks.iter().all(|m| m.kind == Kind::Moved), "{:?}", found.new_marks);
        assert!(found.old_marks.iter().all(|m| m.kind == Kind::Moved));

        let found = compared("#set text(size: 11pt)\nTexto\n", "#set text(size: 14pt)\nTexto\n");
        assert_eq!(found.new_marks, []);
        assert_eq!((found.total, found.unseen), (2, 2));
    }

    #[test]
    fn an_old_version_that_fails_says_why_and_the_document_stays() {
        let c = Compiler::start().unwrap();
        c.send(Job::Compile { text: "Bom\n".into(), path: None });
        c.send(Job::Compare { old: "#nao_existe()".into(), request: 1 });
        loop {
            if let Event::Compared { result, .. } = c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                assert!(result.err().unwrap().contains("nao_existe"));
                break;
            }
        }
    }

    #[test]
    fn letterheads_of_every_kind_compile() {
        use crate::letterhead::Letterhead;
        let c = Compiler::start().unwrap();
        let mut cases = Vec::new();
        for head_on in [false, true] {
            for first_page_only in [false, true] {
                for logo in ["", "x.png"] {
                    for mark_is_image in [false, true] {
                        for footer in ["", "Rodapé \"com\" #marcas e *asteriscos*"] {
                            cases.push(Letterhead {
                                head_on,
                                first_page_only,
                                logo: logo.to_string(),
                                mark_on: true,
                                mark_is_image,
                                footer: footer.to_string(),
                                name: "Empresa #1 $ _x_".to_string(),
                                mark_text: "RASCUNHO".to_string(),
                                ..Letterhead::default()
                            });
                        }
                    }
                }
            }
        }
        for (i, case) in cases.iter().enumerate() {
            c.send(Job::Example { request: i as u64, text: case.preview("Texto") });
        }
        let mut answered = 0;
        while answered < cases.len() {
            if let Event::ExampleDone { request, result } = c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                assert!(result.is_ok(), "{:?}\n{:?}", result.err(), cases[request as usize].rule(true));
                answered += 1;
            }
        }
    }

    fn variants_of(text: &str, variants: Vec<crate::variants::Variant>) -> Vec<Result<VariantView, String>> {
        let c = Compiler::start().unwrap();
        let total = variants.len();
        c.send(Job::Variants { request: 7, text: text.into(), path: None, variants });
        let mut answers: Vec<Option<Result<VariantView, String>>> = vec![None; total];
        while answers.iter().any(Option::is_none) {
            if let Event::VariantDone { request: 7, index, result } = c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                answers[index] = Some(result);
            }
        }
        answers.into_iter().flatten().collect()
    }

    #[test]
    fn each_variant_comes_back_with_its_pages() {
        use crate::variants::PRESETS;
        let paragraph = "Um texto corrido que ocupa várias linhas nas páginas. ".repeat(6);
        let text = format!("#set page(paper: \"a4\")\n{}", format!("{paragraph}\n\n").repeat(50));
        let answers = variants_of(&text, PRESETS.to_vec());
        let views: Vec<&VariantView> = answers.iter().map(|a| a.as_ref().unwrap()).collect();
        assert!(views[1].pages < views[0].pages, "compact takes fewer pages");
        assert!(views[3].pages > views[1].pages, "comfortable takes more than compact");
        assert!(views.iter().all(|v| !v.thumbnails.is_empty() && v.thumbnails.len() <= VARIANT_PAGES));
        assert!(
            views
                .iter()
                .flat_map(|v| &v.thumbnails)
                .all(|t| t.size[0] as f64 <= THUMBNAIL_SIDES[0] + 1.0 && t.size[1] as f64 <= THUMBNAIL_SIDES[1] + 1.0)
        );
        assert!(!views[0].same_as_now && views[1..].iter().all(|v| !v.same_as_now));
    }

    #[test]
    fn a_variant_that_changes_nothing_on_the_page_says_so() {
        use crate::variants::PRESETS;
        // Its own later rule is in a function, so the one the variant adds is overruled.
        let text = "#let own(body) = { set text(font: \"Libertinus Serif\"); body }\n#show: own\nTexto.\n";
        let answers = variants_of(text, vec![PRESETS[0], PRESETS[5]]);
        assert!(answers[1].as_ref().unwrap().same_as_now);
    }

    fn long_document() -> String {
        format!("{}\n\n", "Um texto corrido que ocupa várias linhas nas páginas. ".repeat(6)).repeat(1500)
    }

    #[test]
    fn the_typing_never_waits_for_the_variants() {
        let c = Compiler::start().unwrap();
        c.send(Job::Variants { request: 7, text: long_document(), path: None, variants: crate::variants::PRESETS.to_vec() });
        c.send(Job::Compile { text: "Pouco texto.".into(), path: None });
        let mut done = 0;
        loop {
            match c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                Event::VariantDone { .. } => done += 1,
                Event::Compiled { .. } => break,
                _ => {}
            }
        }
        assert!(done < crate::variants::PRESETS.len(), "the compile came after all {done} variants");
    }

    #[test]
    fn closing_stops_the_variants_still_to_come() {
        let c = Compiler::start().unwrap();
        let started = std::time::Instant::now();
        c.send(Job::Variants { request: 7, text: long_document(), path: None, variants: crate::variants::PRESETS.to_vec() });
        while !matches!(c.events.recv_timeout(Duration::from_secs(120)).unwrap(), Event::VariantDone { .. }) {}
        let one = started.elapsed();
        c.send(Job::CancelVariants);
        // Time for several more, if they were still being made. The one under way may still finish.
        std::thread::sleep(one * 3);
        let more = c.events.try_iter().filter(|event| matches!(event, Event::VariantDone { .. })).count();
        assert!(more <= 1, "{more} more came after closing");
    }

    fn fitted(text: &str, target: usize, limit: Duration) -> FitAnswer {
        fit_document(text, target, &load_fonts(), None, limit, &|| true).unwrap()
    }

    fn pages_of(text: &str) -> usize {
        compile(text).0.unwrap().len()
    }

    #[test]
    fn a_document_is_fitted_with_as_little_change_as_it_takes() {
        use crate::fit::Ending;
        let paragraph = "Um texto corrido que ocupa várias linhas nas páginas. ".repeat(6);
        let text = format!("#set text(lang: \"pt\", size: 11pt)\n{}", format!("{paragraph}\n\n").repeat(30));
        let pages = pages_of(&text);
        assert!(pages >= 3, "{pages}");

        let answer = fitted(&text, pages - 1, FIT_TIME_LIMIT);
        assert_eq!(answer.ending, Ending::Reached);
        let plan = answer.plan.unwrap();
        let done = plan.applied_to(&text);
        assert_eq!(answer.pages_after, pages_of(&done), "what's promised is what the document gets");
        assert!(answer.pages_after < pages);
        assert!(done.ends_with(&format!("{paragraph}\n\n")), "the words stay");
        assert_eq!(answer.before.len(), 2, "first and last pages");
        let now = answer.now.unwrap();
        assert_eq!((now.size, (now.leading * 100.0).round(), (now.spacing * 10.0).round()), (11.0, 65.0, 12.0), "{now:?}");
        assert!(now.margins.is_some_and(|m| (m[0] - 2.5).abs() < 0.05), "{now:?}");
    }

    #[test]
    fn a_target_out_of_reach_says_how_close_it_got() {
        use crate::fit::Ending;
        let text = format!("{}\n\n", "Um texto corrido que ocupa várias linhas nas páginas. ".repeat(6)).repeat(60);
        let answer = fitted(&text, 1, FIT_TIME_LIMIT);
        assert_eq!(answer.ending, Ending::OutOfReach);
        assert!(answer.plan.is_some());
        assert!(answer.pages_after > 1 && answer.pages_after < answer.pages_before, "{} of {}", answer.pages_after, answer.pages_before);
    }

    #[test]
    fn what_already_fits_is_left_alone() {
        let answer = fitted("Pouco texto.\n", 3, FIT_TIME_LIMIT);
        assert!(answer.plan.is_none());
        assert_eq!(answer.pages_before, 1);
    }

    #[test]
    fn a_search_out_of_time_says_so() {
        let text = format!("{}\n\n", "Texto corrido em muitas linhas. ".repeat(9)).repeat(40);
        let answer = fitted(&text, 1, Duration::ZERO);
        assert_eq!(answer.ending, crate::fit::Ending::Stopped);
        assert!(answer.plan.is_none());
    }

    #[test]
    fn a_cancelled_search_never_answers() {
        let c = Compiler::start().unwrap();
        c.send(Job::Fit { request: 7, text: long_document(), path: None, target: 1 });
        c.send(Job::CancelFit);
        c.send(Job::Fit { request: 8, text: "Pouco texto.".into(), path: None, target: 1 });
        loop {
            if let Event::FitDone { request, .. } = c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                assert_eq!(request, 8, "the cancelled one came back");
                return;
            }
        }
    }

    /// Compiles a document and exports it, with the answer that comes back.
    fn exported(text: &str, path: &std::path::Path, accessible: bool) -> Event {
        let c = Compiler::start().unwrap();
        c.send(Job::Compile { text: text.into(), path: None });
        c.send(Job::ExportPdf { path: path.to_path_buf(), accessible });
        loop {
            let event = c.events.recv_timeout(Duration::from_secs(120)).unwrap();
            if matches!(event, Event::Exported(_) | Event::ExportRefused(_)) {
                return event;
            }
        }
    }

    fn export_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("pearl-export-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn an_accessible_pdf_is_written_when_the_document_meets_it() {
        let folder = export_folder("ua");
        let path = folder.join("doc.pdf");
        let text = "#set document(title: [Um título])\n#set text(lang: \"pt\")\n= Um título\nTexto.\n";
        assert!(matches!(exported(text, &path, true), Event::Exported(Ok(_))));
        assert!(std::fs::read(&path).unwrap().starts_with(b"%PDF"));
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1, "nothing but the PDF is left");
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn an_accessible_pdf_refused_says_why_and_leaves_the_disk_alone() {
        let folder = export_folder("refused");
        let path = folder.join("doc.pdf");
        // No title: refused, and no file left behind.
        let text = "Texto.\n";
        let Event::ExportRefused(problems) = exported(text, &path, true) else { panic!("it wasn't refused") };
        assert!(problems.iter().any(|p| p.message.contains("title")), "{problems:?}");
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);

        // A PDF already there stays as it was.
        std::fs::write(&path, b"antes").unwrap();
        assert!(matches!(exported(text, &path, true), Event::ExportRefused(_)));
        assert_eq!(std::fs::read(&path).unwrap(), b"antes");

        // The same document exports as a common PDF.
        assert!(matches!(exported(text, &path, false), Event::Exported(Ok(_))));
        assert!(std::fs::read(&path).unwrap().starts_with(b"%PDF"));
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_pdf_that_cant_be_written_says_so_and_leaves_no_part() {
        let folder = export_folder("unwritable");
        let path = folder.join("nada").join("doc.pdf");
        assert!(matches!(exported("Texto.\n", &path, false), Event::Exported(Err(_))));
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_failure_halfway_through_a_write_leaves_the_old_file_and_no_part() {
        use std::io::Write;
        let folder = export_folder("halfway");
        let path = folder.join("doc.typ");
        std::fs::write(&path, "o texto inteiro\n").unwrap();
        let failed = replace_file(&path, |file| {
            file.write_all(b"o te")?;
            Err(std::io::Error::other("sem espa\u{e7}o"))
        });
        assert!(failed.is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "o texto inteiro\n");
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn writing_whole_makes_a_new_file_and_replaces_an_old_one() {
        let folder = export_folder("whole");
        let path = folder.join("novo.typ");
        write_whole(&path, b"um").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"um");
        write_whole(&path, b"dois").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"dois");
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn writing_whole_refuses_what_is_not_a_file() {
        let folder = export_folder("not-a-file");
        let inner = folder.join("pasta");
        std::fs::create_dir(&inner).unwrap();
        assert!(write_whole(&inner, b"x").is_err());
        assert!(inner.is_dir());
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn writing_whole_refuses_a_read_only_file() {
        let folder = export_folder("read-only");
        let path = folder.join("doc.typ");
        std::fs::write(&path, "antes").unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions.clone()).unwrap();
        // Whoever the system lets write anyway, as root, has nothing to prove here.
        if std::fs::OpenOptions::new().write(true).open(&path).is_err() {
            assert!(write_whole(&path, b"depois").is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "antes");
            assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
        }
        #[expect(clippy::permissions_set_readonly_false, reason = "only to clean up what the test made")]
        permissions.set_readonly(false);
        std::fs::set_permissions(&path, permissions).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn writing_whole_goes_through_a_link_and_keeps_the_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let folder = export_folder("link");
        let (real, link) = (folder.join("real.typ"), folder.join("link.typ"));
        std::fs::write(&real, "antes").unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        write_whole(&link, b"depois").unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "the link is still one");
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "depois");
        assert_eq!(std::fs::metadata(&real).unwrap().permissions().mode() & 0o7777, 0o600);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_broken_document_only_says_why() {
        let result = fit_document("#nada_disso()\n", 1, &load_fonts(), None, FIT_TIME_LIMIT, &|| true);
        assert!(result.err().is_some_and(|e| e.contains("nada_disso")));
    }

    #[test]
    fn a_variant_that_fails_leaves_the_others_alone() {
        use crate::variants::PRESETS;
        let broken = crate::variants::Variant { margin: Some("((("), ..PRESETS[0] };
        let answers = variants_of("Texto.\n", vec![PRESETS[0], broken, PRESETS[1]]);
        assert!(answers[0].is_ok() && answers[2].is_ok());
        assert!(answers[1].as_ref().err().is_some_and(|e| !e.is_empty()));
    }

    #[test]
    fn a_document_that_doesnt_compile_only_says_why() {
        let answers = variants_of("#nada_disso()\n", crate::variants::PRESETS.to_vec());
        assert!(answers.iter().all(|a| a.as_ref().err().is_some_and(|e| e.contains("nada_disso"))));
    }

    #[test]
    fn every_ready_made_formula_compiles() {
        let c = Compiler::start().unwrap();
        let all = &crate::formulas::catalog().formulas;
        for (i, formula) in all.iter().enumerate() {
            c.send(Job::Example { request: i as u64, text: formula.code() });
        }
        let mut answered = 0;
        while answered < all.len() {
            if let Event::ExampleDone { request, result } = c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                let formula = &all[request as usize];
                assert!(result.is_ok(), "{}: {:?}\n{}", formula.name(), result.err(), formula.code());
                answered += 1;
            }
        }
    }

    #[test]
    fn a_broken_example_says_why_and_leaves_the_document_alone() {
        let c = Compiler::start().unwrap();
        c.send(Job::Compile { text: "Documento".into(), path: None });
        c.send(Job::Example { request: 1, text: "#nao_existe()".into() });
        loop {
            if let Event::ExampleDone { request, result } = c.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                assert_eq!(request, 1);
                assert!(result.unwrap_err().contains("nao_existe"));
                break;
            }
        }
    }

    fn margins_of(text: &str) -> [f32; 4] {
        let (pages, diags) = compile(text);
        assert!(diags.iter().all(|d| !d.error), "{:?}", diags.iter().map(|d| &d.message).collect::<Vec<_>>());
        pages.unwrap()[0].margins.unwrap()
    }

    fn assert_cm(got: [f32; 4], want: [f32; 4]) {
        for (g, w) in got.iter().zip(want) {
            assert!((g * 2.54 / 72.0 - w).abs() < 0.01, "{got:?} is not {want:?} cm");
        }
    }

    #[test]
    fn the_page_margins_are_read_from_the_layout() {
        let set = "#set page(margin: (left: 1cm, right: 2cm, top: 3cm, bottom: 4cm))";
        assert_cm(margins_of(&format!("{set}\nA")), [1.0, 2.0, 3.0, 4.0]);
        assert_cm(margins_of(&format!("{set}\n#lorem(900)")), [1.0, 2.0, 3.0, 4.0]);
        assert_cm(margins_of(&format!("{set}\n")), [1.0, 2.0, 3.0, 4.0]);
        assert_cm(margins_of("#set page(margin: 0pt)\nA"), [0.0; 4]);
        // Typst's default on A4: 2.5/21 of the shorter side.
        let default = 2.5;
        assert_cm(margins_of("A"), [default; 4]);
        assert_cm(margins_of(crate::sample::SAMPLE), [default; 4]);
        // The tables and figures on a page aren't its body, and columns don't narrow it.
        assert_cm(margins_of("#set page(columns: 2, margin: (x: 1cm, y: 2cm))\n#lorem(500)"), [1.0, 1.0, 2.0, 2.0]);
        assert_cm(margins_of(&format!("{set}\n#figure(table(columns: 2)[a][b], caption: [c])\n// fim")), [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn measuring_the_margins_changes_nothing_in_the_document() {
        let (pages, diags) = compile("A\n#pagebreak()\nB\n#context counter(page).final().first()");
        assert_eq!(pages.as_ref().unwrap().len(), 2);
        assert!(diags.is_empty());
        assert!(pages.unwrap().iter().all(|p| p.margins.is_some()));
    }

    #[test]
    fn a_page_that_cant_be_measured_goes_without_margins() {
        for text in ["#set page(height: auto)\nA", "#show: body => columns(2, body)\nA"] {
            let (pages, diags) = compile(text);
            assert!(diags.iter().all(|d| !d.error), "{text}");
            assert_eq!(pages.unwrap()[0].margins, None, "{text}");
        }
        // Another paper at the end: the probe's size doesn't match the earlier page.
        let (pages, _) = compile("A\n#set page(\"a5\")\nB");
        let pages = pages.unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].margins, None);
        assert!(pages[1].margins.is_some());
    }

    #[test]
    fn what_decorates_a_page_is_not_mistaken_for_its_body() {
        let set = "#set page(margin: (left: 1cm, right: 2cm, top: 15cm, bottom: 4cm), header: [cabeçalho], footer: [rodapé], background: rect(width: 100%, height: 100%), foreground: [f], numbering: \"1\")";
        assert_cm(margins_of(&format!("{set}\nA")), [1.0, 2.0, 15.0, 4.0]);
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

    /// Compiles a text and answers a job.
    fn answered<T>(text: &str, jobs: Vec<Job>, mut pick: impl FnMut(Event) -> Option<T>) -> T {
        let c = Compiler::start().unwrap();
        c.send(Job::Compile { text: text.into(), path: None });
        for job in jobs {
            c.send(job);
        }
        loop {
            if let Some(answer) = pick(c.events.recv_timeout(Duration::from_secs(120)).unwrap()) {
                return answer;
            }
        }
    }

    fn places_of_range(text: &str, part: &str) -> Vec<PagePlace> {
        let at = text.find(part).unwrap();
        answered(text, vec![Job::Places { ranges: std::iter::once(at..at + part.len()).collect() }], |event| {
            if let Event::Places(places) = event { Some(places) } else { None }
        })
    }

    fn picked(text: &str, from: [f64; 2], to: [f64; 2]) -> Result<std::ops::Range<usize>, PickRefusal> {
        let job = Job::ReviewPick { request: 1, page: 0, from, to };
        answered(text, vec![job], |event| if let Event::ReviewPicked { result, .. } = event { Some(result) } else { None })
    }

    fn middle(place: &PagePlace) -> [f64; 2] {
        [f64::from(place.x + place.width / 2.0), f64::from(place.y + place.height / 2.0)]
    }

    const REVIEWED: &str = "= Um título\nUma palavra errada, aqui mesmo.\n";

    #[test]
    fn a_point_on_a_word_marks_the_whole_word() {
        let place = places_of_range(REVIEWED, "errada")[0];
        let range = picked(REVIEWED, middle(&place), middle(&place)).unwrap();
        assert_eq!(REVIEWED.get(range), Some("errada"));
    }

    #[test]
    fn a_drag_marks_what_is_between_either_way() {
        let (a, b) = (places_of_range(REVIEWED, "palavra")[0], places_of_range(REVIEWED, "aqui")[0]);
        for (from, to) in [(middle(&a), middle(&b)), (middle(&b), middle(&a))] {
            let range = picked(REVIEWED, from, to).unwrap();
            assert_eq!(REVIEWED.get(range), Some("palavra errada, aqui"));
        }
    }

    #[test]
    fn words_with_accents_stay_whole() {
        let text = "Uma ação correção aqui.\n";
        let place = places_of_range(text, "correção")[0];
        let range = picked(text, middle(&place), middle(&place)).unwrap();
        assert_eq!(text.get(range), Some("correção"));
    }

    #[test]
    fn what_the_document_generates_cant_be_marked_and_says_so() {
        // The number of a heading, a call's words, and a reference come from no text of their own.
        let lorem = "#set page(margin: 40pt)\n#lorem(30)\n";
        assert_eq!(picked(lorem, [120.0, 45.0], [120.0, 45.0]), Err(PickRefusal::Generated));
        let numbered = "#set page(margin: 40pt)\n#set heading(numbering: \"1.\")\n= Título\n";
        assert_eq!(picked(numbered, [42.0, 48.0], [42.0, 48.0]), Err(PickRefusal::Generated));
        // Far from any letter, there's nothing to mark.
        assert_eq!(picked("#set page(margin: 40pt)\nTexto.\n", [300.0, 700.0], [300.0, 700.0]), Err(PickRefusal::Nothing));
        // A page that isn't there, likewise.
        let job = Job::ReviewPick { request: 1, page: 99, from: [1.0, 1.0], to: [1.0, 1.0] };
        let answer = answered("Texto.\n", vec![job], |event| if let Event::ReviewPicked { result, .. } = event { Some(result) } else { None });
        assert_eq!(answer, Err(PickRefusal::Nothing));
    }

    #[test]
    fn a_range_is_found_on_each_line_it_runs_over() {
        let text = format!("#set page(width: 8cm, margin: 1cm)\n{}\n", "palavras para quebrar em linhas ".repeat(6));
        let at = text.find("palavras").unwrap();
        let job = Job::Places { ranges: std::iter::once(at..text.len() - 2).collect() };
        let places = answered(&text, vec![job], |event| if let Event::Places(places) = event { Some(places) } else { None });
        assert!(places.len() >= 3, "{places:?}");
        assert!(places.iter().all(|p| p.index == 0 && p.page == 0 && p.width > 0.0));
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
