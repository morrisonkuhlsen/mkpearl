//! The references a document can cite: the entries of the `.bib` files in its folder, read with the
//! same library Typst reads them with, so what the pane lists is what the document can cite.
//!
//! These files are hostile input and may be large: they're read on a thread of their own, within
//! limits, and a file that makes the reader panic counts as one it couldn't read.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant, SystemTime};

use typst::syntax::{LinkedNode, SyntaxKind, parse};

const MAX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_FILES: usize = 64;
const MAX_DEPTH: usize = 3;
/// Files can change from outside: how often the open pane looks.
const CHECK_EVERY: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub title: String,
    pub authors: String,
    pub year: String,
    pub doi: Option<String>,
    /// What a search looks through, folded.
    haystack: String,
}

impl Entry {
    fn new(key: String, title: String, authors: String, year: String, doi: Option<String>) -> Self {
        let haystack = fold(&format!("{key} {title} {authors} {year} {}", doi.as_deref().unwrap_or_default()));
        Self { key, title, authors, year, doi, haystack }
    }

    /// What a check's result belongs to: when these change, the result no longer holds.
    pub fn identity(&self) -> String {
        format!("{}\u{0}{}\u{0}{}", self.key, self.title, self.doi.as_deref().unwrap_or_default())
    }

    /// Whether it has every word of a search, ignoring case and accents.
    pub fn matches(&self, query: &str) -> bool {
        fold(query).split_whitespace().all(|word| self.haystack.contains(word))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BibFile {
    /// From the document's folder, as Typst writes it.
    pub path: String,
    pub entries: Vec<Entry>,
    pub failed: bool,
}

/// Lower case and without accents, so that "goncalves" finds "Gonçalves".
pub fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

/// The entries of a `.bib` file's text. `None` when it can't be read as one.
pub fn parse_bib(text: &str) -> Option<Vec<Entry>> {
    let library = std::panic::catch_unwind(|| hayagriva::io::from_biblatex_str(text)).ok()?.ok()?;
    let entries = library.iter().map(|entry| {
        let authors: Vec<String> = entry.authors().unwrap_or_default().iter().map(|p| p.given_first(false)).collect();
        Entry::new(
            entry.key().to_string(),
            entry.title().map_or_else(String::new, |t| t.value.to_string()),
            authors.join(", "),
            entry.date().map_or_else(String::new, |d| d.year.to_string()),
            entry.keyed_serial_number("doi").map(str::to_string),
        )
    });
    Some(entries.collect())
}

/// The `.bib` files in a document's folder, skipping what its Files pane skips.
fn find_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') || found.len() >= MAX_FILES {
                continue;
            }
            if path.is_dir() {
                if depth < MAX_DEPTH && name != "target" {
                    pending.push((path, depth + 1));
                }
            } else if is_bib(&path) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn is_bib(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("bib"))
}

fn read_file(root: &Path, file: &Path) -> BibFile {
    let path = crate::images::portable(file.strip_prefix(root).unwrap_or(file));
    let entries = fs::metadata(file)
        .ok()
        .filter(|m| m.len() <= MAX_BYTES)
        .and_then(|_| fs::read(file).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| parse_bib(&text));
    let failed = entries.is_none();
    BibFile { path, entries: entries.unwrap_or_default(), failed }
}

/// Brings a `.bib` file into the document's folder, unless it's already there. Returns its path from
/// the folder and how many references it has; a failure is the id of the message that explains it.
pub fn import(dir: &Path, file: &Path) -> Result<(String, usize), &'static str> {
    if !is_bib(file) {
        return Err("only-bib-files-can-be-imported");
    }
    let meta = fs::metadata(file).map_err(|_| "the-bib-file-could-not-be-read")?;
    if meta.len() > MAX_BYTES {
        return Err("the-bib-file-is-too-large");
    }
    let bytes = fs::read(file).map_err(|_| "the-bib-file-could-not-be-read")?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "the-bib-file-could-not-be-read")?;
    let count = parse_bib(text).ok_or("the-bib-file-could-not-be-read")?.len();
    if count == 0 {
        return Err("the-bib-file-has-no-references");
    }
    if let (Ok(root), Ok(real)) = (dir.canonicalize(), file.canonicalize())
        && let Ok(inside) = real.strip_prefix(&root)
    {
        return Ok((crate::images::portable(inside), count));
    }
    let name = file.file_name().and_then(|n| n.to_str()).ok_or("the-bib-file-could-not-be-copied")?;
    let kept = crate::images::keep(dir, name, &bytes).ok_or("the-bib-file-could-not-be-copied")?;
    Ok((kept, count))
}

/// What the document's `#bibliography` calls name: `None` when it has no such call. The strings of
/// their arguments, a style among them, which matches no file.
pub fn used_files(source: &str) -> Option<Vec<String>> {
    let root = parse(source);
    let mut pending = vec![LinkedNode::new(&root)];
    let mut found = None;
    while let Some(node) = pending.pop() {
        if node.kind() == SyntaxKind::FuncCall
            && node.children().next().is_some_and(|callee| callee.kind() == SyntaxKind::Ident && source.get(callee.range()) == Some("bibliography"))
        {
            let names = found.get_or_insert_with(Vec::new);
            let mut strings = vec![node.clone()];
            while let Some(part) = strings.pop() {
                if part.kind() == SyntaxKind::Str {
                    names.push(source.get(part.range()).unwrap_or_default().trim_matches('"').trim_start_matches("./").to_string());
                }
                strings.extend(part.children());
            }
        } else {
            pending.extend(node.children());
        }
    }
    found
}

/// How the document uses references, from its syntax.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Usage {
    /// Keys in `@key`, `#cite` and `#ref`, in the order written, each once.
    pub cited: Vec<String>,
    /// Labels the document puts on its own things, which `@` may point at too.
    pub labels: Vec<String>,
    /// It includes other files, whose citations aren't seen here.
    pub includes: bool,
}

pub fn usage(source: &str) -> Usage {
    fn add(list: &mut Vec<String>, item: &str) {
        if !list.iter().any(|known| known == item) {
            list.push(item.to_string());
        }
    }
    let text = |node: &LinkedNode| source.get(node.range()).unwrap_or_default().to_string();
    let root = parse(source);
    let mut found = Usage::default();
    let mut pending = vec![LinkedNode::new(&root)];
    while let Some(node) = pending.pop() {
        match node.kind() {
            SyntaxKind::RefMarker => add(&mut found.cited, text(&node).trim_start_matches('@')),
            SyntaxKind::Label => add(&mut found.labels, text(&node).trim_matches(['<', '>'])),
            SyntaxKind::ModuleInclude => found.includes = true,
            _ => {}
        }
        // `#cite(<key>)` and `#cite(label("key"))`: the first is no label being defined.
        let is_citing = node.kind() == SyntaxKind::FuncCall
            && node.children().next().is_some_and(|callee| callee.kind() == SyntaxKind::Ident && matches!(text(&callee).as_str(), "cite" | "ref"));
        if is_citing {
            let args = node.children().find(|child| child.kind() == SyntaxKind::Args);
            for arg in args.iter().flat_map(LinkedNode::children) {
                match arg.kind() {
                    SyntaxKind::Label => add(&mut found.cited, text(&arg).trim_matches(['<', '>'])),
                    SyntaxKind::FuncCall if arg.children().next().is_some_and(|callee| text(&callee) == "label") => {
                        let inner = arg.children().find(|child| child.kind() == SyntaxKind::Args);
                        for value in inner.iter().flat_map(LinkedNode::children).filter(|v| v.kind() == SyntaxKind::Str) {
                            add(&mut found.cited, text(&value).trim_matches('"'));
                        }
                    }
                    SyntaxKind::Named => pending.push(arg),
                    _ => {}
                }
            }
        } else {
            // Children go last-first off the stack, so that keys come in the order written.
            let children: Vec<_> = node.children().collect();
            pending.extend(children.into_iter().rev());
        }
    }
    found
}

/// Keys the document cites with no entry to back them.
pub fn missing<'a>(usage: &'a Usage, entries: &[&Entry]) -> Vec<&'a str> {
    let known = |key: &&String| entries.iter().any(|entry| &entry.key == *key) || usage.labels.contains(key);
    usage.cited.iter().filter(|key| !known(key)).map(String::as_str).collect()
}

/// Adds one entry fetched from outside to a `.bib` file, creating it if needed, and never touching what
/// is there. A key already in use gets a number. Returns the entry's key, or the id of a message.
pub fn append_entry(file: &Path, bibtex: &str, known_keys: &[&str]) -> Result<String, &'static str> {
    let entries = parse_bib(bibtex).ok_or("the-doi-gave-no-usable-reference")?;
    let [entry] = entries.as_slice() else { return Err("the-doi-gave-no-usable-reference") };
    let key = &entry.key;
    let mut free = key.clone();
    let mut number = 2;
    while known_keys.contains(&free.as_str()) {
        free = format!("{key}-{number}");
        number += 1;
    }
    // The key is what follows the entry's first brace.
    let opening = bibtex.find('{').ok_or("the-doi-gave-no-usable-reference")?;
    let after = opening + 1;
    let rest = bibtex.get(after..).ok_or("the-doi-gave-no-usable-reference")?;
    let tail = rest.trim_start().strip_prefix(key.as_str()).ok_or("the-doi-gave-no-usable-reference")?;
    let text = format!("{}{free}{}", bibtex.get(..after).unwrap_or_default(), tail);
    let mut out = fs::OpenOptions::new().create(true).append(true).open(file).map_err(|_| "the-reference-could-not-be-saved")?;
    use std::io::Write;
    writeln!(out, "\n{}", text.trim()).map_err(|_| "the-reference-could-not-be-saved")?;
    Ok(free)
}

/// The files whose references can be cited: those the document's bibliography names; before it has
/// one, or when what it names isn't found here, all of them.
pub fn citable<'a>(files: &'a [BibFile], used: Option<&[String]>) -> Vec<&'a BibFile> {
    let named: Vec<&BibFile> = files.iter().filter(|f| used.is_some_and(|u| u.contains(&f.path))).collect();
    if named.is_empty() { files.iter().collect() } else { named }
}

/// What cites a key. `@key` takes only some characters and leaves a final `.` or `:` to the text;
/// any other key needs the long form.
pub fn citation(key: &str) -> String {
    let plain = !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ':' | '.')) && !key.ends_with(['.', ':']);
    if plain { format!("@{key}") } else { format!("#cite(label(\"{}\"))", key.replace('\\', "\\\\").replace('"', "\\\"")) }
}

/// The references of the open document, kept up to date on a thread of their own.
#[derive(Default)]
pub struct Shelf {
    folder: Option<PathBuf>,
    pub files: Vec<BibFile>,
    stamp: Vec<(PathBuf, Option<SystemTime>, u64)>,
    checked: Option<Instant>,
    loading: Option<Receiver<Loaded>>,
}

struct Loaded {
    folder: PathBuf,
    stamp: Vec<(PathBuf, Option<SystemTime>, u64)>,
    /// `None` when nothing changed.
    files: Option<Vec<BibFile>>,
}

impl Shelf {
    /// Looks again at the next tick.
    pub fn refresh(&mut self) {
        self.checked = None;
        self.stamp.clear();
    }

    /// While the pane is open: takes in what was read, and looks at the files now and then.
    pub fn tick(&mut self, ctx: &egui::Context, folder: Option<PathBuf>) {
        if self.folder != folder {
            *self = Self { folder: folder.clone(), ..Self::default() };
        }
        if let Some(rx) = &self.loading
            && let Ok(loaded) = rx.try_recv()
        {
            self.loading = None;
            if self.folder.as_ref() == Some(&loaded.folder) {
                self.stamp = loaded.stamp;
                if let Some(files) = loaded.files {
                    self.files = files;
                }
            }
        }
        ctx.request_repaint_after(CHECK_EVERY);
        let due = self.checked.is_none_or(|at| at.elapsed() >= CHECK_EVERY);
        let (Some(folder), true, None) = (folder, due, &self.loading) else { return };
        self.checked = Some(Instant::now());
        let (tx, rx) = channel();
        let known = self.stamp.clone();
        let waker = ctx.clone();
        let started = std::thread::Builder::new().name("references".into()).spawn(move || {
            let paths = find_files(&folder);
            let stamp: Vec<_> = paths
                .iter()
                .map(|p| {
                    let meta = fs::metadata(p).ok();
                    (p.clone(), meta.as_ref().and_then(|m| m.modified().ok()), meta.as_ref().map_or(0, fs::Metadata::len))
                })
                .collect();
            // An empty folder has nothing to read either way.
            let files = (stamp != known || known.is_empty()).then(|| paths.iter().map(|p| read_file(&folder, p)).collect());
            let _ = tx.send(Loaded { folder, stamp, files });
            waker.request_repaint();
        });
        if started.is_ok() {
            self.loading = Some(rx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIB: &str = r#"
@article{goncalves2020, title = {Sobre as pérolas}, author = {Gonçalves, Maria and Souza, João}, year = {2020}, journal = {Revista} }
@book{knuth:84, title = {The {TeX}book}, author = {Knuth, Donald}, year = {1984}, publisher = {Addison} }
"#;

    #[test]
    fn entries_are_read_and_found_without_case_or_accents() {
        let entries = parse_bib(BIB).unwrap();
        assert_eq!(entries.len(), 2);
        let first = entries.iter().find(|e| e.key == "goncalves2020").unwrap();
        assert_eq!(first.title, "Sobre as pérolas");
        assert_eq!(first.authors, "Maria Gonçalves, João Souza");
        assert_eq!(first.year, "2020");
        for query in ["", "GONCALVES", "pérolas", "perolas 2020", "souza maria"] {
            assert!(first.matches(query), "{query}");
        }
        for query in ["knuth", "perolas 1984"] {
            assert!(!first.matches(query), "{query}");
        }
    }

    // As doi.org sent it: an unquoted month and a dash in the pages.
    const FROM_DOI: &str = " @article{LeCun_2015, title={Deep learning}, volume={521}, ISSN={1476-4687}, url={http://dx.doi.org/10.1038/nature14539}, DOI={10.1038/nature14539}, number={7553}, journal={Nature}, publisher={Springer Science and Business Media LLC}, author={LeCun, Yann and Bengio, Yoshua and Hinton, Geoffrey}, year={2015}, month=May, pages={436–444} }\n";

    #[test]
    fn an_entry_from_doi_org_is_read_with_its_doi() {
        let entries = parse_bib(FROM_DOI).unwrap();
        assert_eq!(entries[0].key, "LeCun_2015");
        assert_eq!(entries[0].title, "Deep learning");
        assert_eq!(entries[0].doi.as_deref(), Some("10.1038/nature14539"));
        assert!(entries[0].matches("nature14539"));
    }

    #[test]
    fn appending_keeps_what_is_there_and_finds_a_free_key() {
        let file = std::env::temp_dir().join(format!("pearl-append-{}.bib", std::process::id()));
        fs::write(&file, BIB).unwrap();
        assert_eq!(append_entry(&file, FROM_DOI, &["goncalves2020"]), Ok("LeCun_2015".to_string()));
        assert_eq!(append_entry(&file, FROM_DOI, &["LeCun_2015"]), Ok("LeCun_2015-2".to_string()));
        let keys: Vec<String> = parse_bib(&fs::read_to_string(&file).unwrap()).unwrap().into_iter().map(|e| e.key).collect();
        assert_eq!(keys.len(), 4, "{keys:?}");
        assert!(keys.contains(&"goncalves2020".to_string()) && keys.contains(&"LeCun_2015-2".to_string()));
        for bad in ["", "<html>", "@article{", "@misc{a, title={A}} @misc{b, title={B}}"] {
            assert_eq!(append_entry(&file, bad, &[]), Err("the-doi-gave-no-usable-reference"), "{bad}");
        }
        fs::remove_file(file).unwrap();
    }

    #[test]
    fn citations_are_read_from_the_syntax() {
        let source = "@a e @b[p. 1] e @a.\n#cite(<c>, supplement: [x])\n#cite(label(\"d/e\"), form: \"year\")\n// @comentario\n`@bruto`\n$x$ <eq>\nmail@exemplo.com\n";
        let found = usage(source);
        // Typst reads the address as a citation too, and fails on it unless the `@` is escaped.
        assert_eq!(found.cited, ["a", "b", "c", "d/e", "exemplo.com"]);
        assert_eq!(found.labels, ["eq"]);
        assert!(!found.includes);
        assert!(usage("#include \"cap.typ\"").includes);
    }

    #[test]
    fn a_citation_is_missing_only_with_no_entry_and_no_label() {
        let found = usage("@knuth84 e @fig e @nada\n#figure([x]) <fig>\n");
        let entries = parse_bib(BIB).unwrap();
        let refs: Vec<&Entry> = entries.iter().collect();
        let book = "knuth:84";
        assert!(refs.iter().any(|e| e.key == book));
        assert_eq!(missing(&found, &refs), ["knuth84", "nada"]);
    }

    #[test]
    fn hostile_files_are_refused_and_nothing_breaks() {
        for text in ["@article{", "@article{a, title = {", "@@@@", "}{", "@string{x = {y}} @article{a, title = x # x # x"] {
            let _ = parse_bib(text);
        }
        let deep = format!("@article{{a, title = {{{}{}}}}}", "{".repeat(20_000), "}".repeat(20_000));
        let _ = parse_bib(&deep);
        assert_eq!(parse_bib("").map(|e| e.len()), Some(0));
    }

    #[test]
    fn keys_get_the_form_typst_can_read() {
        assert_eq!(citation("knuth:84"), "@knuth:84");
        assert_eq!(citation("silva-2020_a"), "@silva-2020_a");
        assert_eq!(citation("fim."), "#cite(label(\"fim.\"))");
        assert_eq!(citation("a/b"), "#cite(label(\"a/b\"))");
        assert_eq!(citation("a\"b"), "#cite(label(\"a\\\"b\"))");
    }

    #[test]
    fn the_documents_bibliography_is_read_from_its_syntax() {
        assert_eq!(used_files("Texto"), None);
        assert_eq!(used_files("// #bibliography(\"a.bib\")\n"), None, "a comment");
        assert_eq!(used_files("`#bibliography(\"a.bib\")`"), None, "raw text");
        assert_eq!(used_files("#bibliography(\"./a.bib\")"), Some(vec!["a.bib".to_string()]));
        let both = used_files("#bibliography((\"a.bib\", \"b.bib\"), style: \"apa\")").unwrap();
        assert!(both.contains(&"a.bib".to_string()) && both.contains(&"b.bib".to_string()));
    }

    #[test]
    fn only_the_files_of_the_bibliography_can_be_cited() {
        let file = |path: &str| BibFile { path: path.to_string(), entries: vec![], failed: false };
        let files = [file("a.bib"), file("b.bib")];
        let paths = |used: Option<&[String]>| citable(&files, used).iter().map(|f| f.path.clone()).collect::<Vec<_>>();
        assert_eq!(paths(None), ["a.bib", "b.bib"]);
        assert_eq!(paths(Some(&["b.bib".to_string()])), ["b.bib"]);
        assert_eq!(paths(Some(&["outro.bib".to_string()])), ["a.bib", "b.bib"], "what it names isn't here");
    }

    #[test]
    fn importing_copies_once_and_refuses_what_is_no_bibliography() {
        let root = std::env::temp_dir().join(format!("pearl-bib-{}", std::process::id()));
        let (doc, away) = (root.join("doc"), root.join("away"));
        fs::create_dir_all(&doc).unwrap();
        fs::create_dir_all(&away).unwrap();
        fs::write(away.join("refs.bib"), BIB).unwrap();
        assert_eq!(import(&doc, &away.join("refs.bib")), Ok(("refs.bib".to_string(), 2)));
        assert_eq!(import(&doc, &away.join("refs.bib")), Ok(("refs.bib".to_string(), 2)), "the same file again");
        assert_eq!(import(&doc, &doc.join("refs.bib")), Ok(("refs.bib".to_string(), 2)), "already inside");
        assert_eq!(fs::read_dir(&doc).unwrap().count(), 1);
        fs::write(away.join("refs.bib"), "@misc{outra, title = {Outra}}").unwrap();
        assert_eq!(import(&doc, &away.join("refs.bib")), Ok(("refs-2.bib".to_string(), 1)), "never over another file");
        assert!(fs::read_to_string(doc.join("refs.bib")).unwrap().contains("goncalves2020"));
        fs::write(away.join("nada.bib"), "").unwrap();
        assert_eq!(import(&doc, &away.join("nada.bib")), Err("the-bib-file-has-no-references"));
        fs::write(away.join("ruim.bib"), "@article{").unwrap();
        assert_eq!(import(&doc, &away.join("ruim.bib")), Err("the-bib-file-could-not-be-read"));
        assert_eq!(import(&doc, &away.join("x.txt")), Err("only-bib-files-can-be-imported"));
        fs::remove_dir_all(root).unwrap();
    }
}
