//! The Typst `World`: the editor's text as the main file, the project folder for everything else.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{Datelike, Local, TimeDelta, Utc};
use typst::diag::FileResult;
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::downloader::SystemDownloader;
use typst_kit::files::{FileStore, FsRoot, SystemFiles};
use typst_kit::fonts::{self, FontStore};
use typst_kit::packages::SystemPackages;

pub fn load_fonts() -> Arc<FontStore> {
    let mut store = FontStore::new();
    store.extend(fonts::embedded());
    store.extend(fonts::system());
    Arc::new(store)
}

pub struct DocWorld {
    library: LazyHash<Library>,
    fonts: Arc<FontStore>,
    files: FileStore<SystemFiles>,
    main: Source,
    root: PathBuf,
}

fn main_id(path: Option<&Path>) -> Option<FileId> {
    let name = path.and_then(Path::file_name).and_then(|n| n.to_str()).unwrap_or("sem-titulo.typ");
    let vpath = VirtualPath::new(name).or_else(|_| VirtualPath::new("sem-titulo.typ")).ok()?;
    Some(RootedPath::new(VirtualRoot::Project, vpath).intern())
}

impl DocWorld {
    /// A world for the document at `path`; an unsaved one resolves files in the current folder.
    pub fn new(fonts: Arc<FontStore>, path: Option<&Path>) -> Option<Self> {
        let root: PathBuf =
            path.and_then(Path::parent).map(Path::to_path_buf).filter(|p| !p.as_os_str().is_empty()).or_else(|| std::env::current_dir().ok())?;
        let downloader = SystemDownloader::new(concat!("pearl/", env!("CARGO_PKG_VERSION")));
        let files = SystemFiles::new(FsRoot::new(root.clone()), SystemPackages::new(downloader));
        Some(Self {
            library: LazyHash::new(Library::default()),
            fonts,
            files: FileStore::new(files),
            main: Source::new(main_id(path)?, String::new()),
            root,
        })
    }

    /// Prepares for a compile: new text, and project files read afresh.
    pub fn update(&mut self, text: &str) {
        self.main.replace(text);
        self.files.reset();
    }

    /// The files of the project the last compile read, by path from its folder and on disk. Packages
    /// are left out: each machine fetches its own.
    pub fn project_files(&mut self) -> Vec<(String, PathBuf)> {
        let (loader, ids) = self.files.dependencies();
        ids.filter(|id| matches!(id.root(), VirtualRoot::Project))
            .filter_map(|id| Some((id.vpath().get_without_slash().to_string(), loader.resolve(id).ok()?)))
            .collect()
    }

    pub fn source(&self) -> &Source {
        &self.main
    }
}

impl World for DocWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }
    fn main(&self) -> FileId {
        self.main.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() { Ok(self.main.clone()) } else { self.files.source(id) }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main.id() { Ok(Bytes::from_string(self.main.clone())) } else { self.files.file(id) }
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }
    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        let date = match offset {
            None => Local::now().date_naive(),
            Some(o) => (Utc::now() + TimeDelta::try_seconds(o.seconds() as i64)?).date_naive(),
        };
        Datetime::from_ymd(date.year(), date.month().try_into().ok()?, date.day().try_into().ok()?)
    }
}

impl typst_ide::IdeWorld for DocWorld {
    fn upcast(&self) -> &dyn World {
        self
    }

    /// The project's files, so `image("` completes to them. Bounded: a document in a huge folder
    /// mustn't stall the analysis.
    fn files(&self) -> Vec<FileId> {
        const MAX_FILES: usize = 1000;
        const MAX_DEPTH: usize = 4;
        let mut out = Vec::new();
        let mut pending = vec![(self.root.clone(), 0)];
        while let Some((dir, depth)) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                if out.len() >= MAX_FILES {
                    return out;
                }
                let path = entry.path();
                if entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                if path.is_dir() {
                    if depth < MAX_DEPTH && entry.file_name() != "target" {
                        pending.push((path, depth + 1));
                    }
                } else if let Ok(vpath) = VirtualPath::virtualize(&self.root, &path) {
                    out.push(RootedPath::new(VirtualRoot::Project, vpath).intern());
                }
            }
        }
        out
    }
}
