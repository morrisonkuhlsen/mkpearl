//! The Typst `World`: the editor's text as the main file, the project folder for everything else.
//! The project is the folder the user opened, when the document is inside it, or else the
//! document's own folder.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

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

/// The folder opened as a project, if any.
static PROJECT: RwLock<Option<PathBuf>> = RwLock::new(None);
/// Counts changes of the project, so worlds made for the last one are made again.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Opens `folder` as the project, or closes it. Give an absolute path.
pub fn set_project(folder: Option<PathBuf>) {
    if let Ok(mut project) = PROJECT.write() {
        *project = folder;
    }
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

pub fn project() -> Option<PathBuf> {
    PROJECT.read().ok().and_then(|p| p.clone())
}

/// Changes with each project opened or closed.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

/// Where Typst's paths start for a document at `path`: the project, if the document is in it, or
/// else its folder. An unsaved document uses the project, or the current folder.
pub fn root_for(path: Option<&Path>, project: Option<&Path>) -> Option<PathBuf> {
    let absolute = path.map(|p| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()));
    match (absolute.as_deref(), project) {
        (Some(file), Some(project)) if file.starts_with(project) => Some(project.to_path_buf()),
        (Some(file), _) => file.parent().map(Path::to_path_buf).filter(|p| !p.as_os_str().is_empty()),
        (None, Some(project)) => Some(project.to_path_buf()),
        (None, None) => None,
    }
    .or_else(|| std::env::current_dir().ok())
}

/// The document's name for Typst: its path from the root, so that its own relative paths work.
fn main_id(path: Option<&Path>, root: &Path) -> Option<FileId> {
    let absolute = path.map(|p| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()));
    let vpath = absolute
        .as_deref()
        .and_then(|file| VirtualPath::virtualize(root, file).ok())
        .or_else(|| VirtualPath::new(path.and_then(Path::file_name).and_then(|n| n.to_str())?).ok())
        .or_else(|| VirtualPath::new("sem-titulo.typ").ok())?;
    Some(RootedPath::new(VirtualRoot::Project, vpath).intern())
}

impl DocWorld {
    /// A world for the document at `path`; an unsaved one resolves files in the project, or the
    /// current folder.
    pub fn new(fonts: Arc<FontStore>, path: Option<&Path>) -> Option<Self> {
        let root = root_for(path, project().as_deref())?;
        let downloader = SystemDownloader::new(concat!("pearl/", env!("CARGO_PKG_VERSION")));
        let files = SystemFiles::new(FsRoot::new(root.clone()), SystemPackages::new(downloader));
        Some(Self {
            library: LazyHash::new(Library::default()),
            fonts,
            files: FileStore::new(files),
            main: Source::new(main_id(path, &root)?, String::new()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_root_is_the_project_only_for_a_document_inside_it() {
        let project = Path::new("/tmp/livro");
        assert_eq!(root_for(Some(Path::new("/tmp/livro/cap/um.typ")), Some(project)), Some(project.to_path_buf()));
        assert_eq!(root_for(Some(Path::new("/tmp/outro/a.typ")), Some(project)), Some(PathBuf::from("/tmp/outro")));
        assert_eq!(root_for(Some(Path::new("/tmp/livro-2/a.typ")), Some(project)), Some(PathBuf::from("/tmp/livro-2")));
        assert_eq!(root_for(None, Some(project)), Some(project.to_path_buf()));
        assert_eq!(root_for(Some(Path::new("/tmp/a.typ")), None), Some(PathBuf::from("/tmp")));
    }

    #[test]
    fn the_document_is_named_from_the_root() {
        let id = main_id(Some(Path::new("/tmp/livro/cap/um.typ")), Path::new("/tmp/livro")).unwrap();
        assert_eq!(id.vpath().get_without_slash(), "cap/um.typ");
        let id = main_id(None, Path::new("/tmp/livro")).unwrap();
        assert_eq!(id.vpath().get_without_slash(), "sem-titulo.typ");
    }
}
