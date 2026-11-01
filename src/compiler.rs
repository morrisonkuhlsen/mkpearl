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
/// Most pages kept rasterised in the cache beyond the current document.
const CACHE_SLACK: usize = 32;

pub enum Job {
    Compile { text: String, path: Option<PathBuf> },
    /// A click on a page, in points from its top left.
    Click { page: usize, x: f64, y: f64 },
    /// Shows the place in the pages for a byte offset of the source.
    Locate { byte: usize },
    ExportPdf(PathBuf),
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
            for job in std::iter::once(first).chain(jobs.try_iter()) {
                match job {
                    Job::Compile { text, path } => compile = Some((text, path)),
                    other => others.push(other),
                }
            }
            if let Some((text, path)) = compile {
                self.compile(&text, path);
            }
            for job in others {
                self.other(job);
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
            Job::Compile { .. } => {}
            Job::Click { page, x, y } => self.click(page, x, y),
            Job::Locate { byte } => self.locate(byte),
            Job::ExportPdf(path) => {
                let result = self.export(&path).map(|()| path);
                self.emit(Event::Exported(result));
            }
        }
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
