//! The References pane: what the document can cite, what's wrong with its citations and, when asked,
//! what the outside world knows about them.

use std::path::{Path, PathBuf};

use egui::{Key, RichText, Ui};

use super::App;
use crate::bib::{self, BibFile, Entry};
use crate::collab::{self, Session};
use crate::doi::{self, Servers, Verdict};
use crate::edit;
use crate::i18n::tx;
use crate::theme::Tokens;
use crate::tr;

const MAX_SHOWN: usize = 200;
/// The file a first reference goes in; the ribbon's bibliography names the same.
const NEW_FILE: &str = "refs.bib";

impl App {
    pub(super) fn pane_references(&mut self, ui: &mut Ui, t: &Tokens) {
        let ctx = ui.ctx().clone();
        let guest = self.session.as_ref().is_some_and(|s| !s.is_host());
        let document = self.session.as_ref().and_then(Session::guest_path).or_else(|| self.path.clone());
        self.references.tick(&ctx, document.as_deref().and_then(Path::parent).map(Path::to_path_buf));
        self.receive_lookups(&ctx);

        // A guest has only the files the host's document uses, and can't send any.
        if !guest {
            self.adding_row(ui, &ctx);
        }
        let search = ui.add(
            egui::TextEdit::singleline(&mut self.reference_search).hint_text(tx("search-by-title-author-key-or-year")).desired_width(f32::INFINITY),
        );
        let enter = search.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        ui.add_space(4.0);

        let used = bib::used_files(&self.text);
        let usage = bib::usage(&self.text);
        let files = bib::citable(&self.references.files, used.as_deref());
        let everything: Vec<&Entry> = files.iter().flat_map(|file| &file.entries).collect();
        let matching: Vec<(&str, &Entry)> = files
            .iter()
            .flat_map(|file| file.entries.iter().map(|entry| (file.path.as_str(), entry)))
            .filter(|(_, entry)| entry.matches(&self.reference_search))
            .collect();

        let missing = bib::missing(&usage, &everything);
        if !missing.is_empty() {
            ui.colored_label(t.danger, tr!("citations-with-no-reference", keys = missing.join(", ")));
            ui.add_space(4.0);
        }
        if usage.includes {
            ui.colored_label(t.text_dim, tx("only-this-file-is-checked"));
            ui.add_space(4.0);
        }

        let mut check = false;
        if !everything.is_empty() {
            let busy = self.checking.is_some();
            let label = if busy {
                tr!(
                    "checking-references",
                    done = self.checked_count(&everything).to_string(),
                    total = everything.len().min(doi::MAX_CHECKED).to_string()
                )
            } else {
                tx("check-online").to_string()
            };
            check = ui.add_enabled(!busy, egui::Button::new(label)).on_hover_text(tx("check-online-sends")).clicked();
            ui.add_space(4.0);
        }
        if self.references.files.is_empty() {
            let empty = if document.is_none() { tx("save-the-document-to-see-its-references") } else { tx("no-bib-files-in-the-folder") };
            ui.colored_label(t.text_dim, empty);
        } else if matching.is_empty() {
            ui.colored_label(t.text_dim, tx("no-reference-matches"));
        } else {
            ui.colored_label(t.text_dim, tx("click-a-reference-to-cite-it"));
            ui.add_space(4.0);
        }

        let mut chosen = None;
        if enter && let Some((file, entry)) = matching.first() {
            chosen = Some((file.to_string(), entry.key.clone()));
        }
        for (file, entry) in matching.iter().take(MAX_SHOWN) {
            let title = if entry.title.is_empty() { &entry.key } else { &entry.title };
            let button = egui::Button::new(RichText::new(title).color(t.text)).frame_when_inactive(false).truncate();
            let detail: Vec<&str> = [entry.authors.as_str(), entry.year.as_str(), entry.key.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
            let detail = detail.join(" · ");
            if ui.add(button).on_hover_text(format!("{title}\n{detail}\n{file}")).clicked() {
                chosen = Some((file.to_string(), entry.key.clone()));
            }
            ui.add(egui::Label::new(RichText::new(detail).small().color(t.text_dim)).truncate());
            if !usage.cited.contains(&entry.key) {
                ui.label(RichText::new(tx("never-cited")).small().color(t.text_dim));
            }
            if let Some(verdict) = self.verdicts.get(&entry.identity()) {
                let (text, color) = match verdict {
                    Verdict::Found => (tx("found-online").to_string(), t.text_dim),
                    Verdict::OtherTitle(other) => (tr!("the-doi-belongs-to", title = other.clone()), t.danger),
                    Verdict::NotFound => (tx("not-found-online").to_string(), t.danger),
                    Verdict::Failed => (tx("could-not-check").to_string(), t.text_dim),
                    Verdict::NothingToAsk => (tx("nothing-to-check").to_string(), t.text_dim),
                };
                ui.add(egui::Label::new(RichText::new(text).small().color(color)).wrap());
            }
            ui.add_space(3.0);
        }
        if matching.len() > MAX_SHOWN {
            ui.colored_label(t.text_dim, tr!("n-more-references", count = (matching.len() - MAX_SHOWN).to_string()));
        }
        ui.add_space(6.0);
        if self.verdicts.values().any(|v| matches!(v, Verdict::NotFound)) {
            ui.colored_label(t.text_dim, tx("not-found-proves-nothing"));
        }
        let outside: Vec<&str> =
            self.references.files.iter().filter(|f| !files.iter().any(|shown| shown.path == f.path)).map(|f| f.path.as_str()).collect();
        if !outside.is_empty() {
            ui.colored_label(t.text_dim, tr!("bib-files-outside-the-bibliography", files = outside.join(", ")));
        }
        for failed in self.references.files.iter().filter(|f| f.failed) {
            ui.colored_label(t.danger, tr!("bib-file-could-not-be-read", file = failed.path.clone()));
        }

        let to_check: Vec<Entry> = if check { everything.iter().map(|entry| (*entry).clone()).collect() } else { Vec::new() };
        if check {
            self.verdicts.clear();
            self.checking = doi::check_all(&ctx, self.servers.clone(), to_check);
        }
        if let Some((file, key)) = chosen {
            self.cite(&ctx, &file, &key);
        }
    }

    fn checked_count(&self, entries: &[&Entry]) -> usize {
        entries.iter().filter(|entry| self.verdicts.contains_key(&entry.identity())).count()
    }

    /// The buttons that bring references in: a whole file, or one by its DOI.
    fn adding_row(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if ui.button(tx("import-a-bib-file")).clicked() {
            self.ask_to_import(ctx);
        }
        ui.add_space(4.0);
        let busy = self.doi_lookup.is_some();
        let mut add = false;
        ui.horizontal(|ui| {
            let button = ui.add_enabled(!busy, egui::Button::new(if busy { tx("looking-up-the-doi") } else { tx("add-by-doi") }));
            add = button.on_hover_text(tx("add-by-doi-sends")).clicked();
            let field = ui.add(egui::TextEdit::singleline(&mut self.doi_input).hint_text("10.1038/nature14539").desired_width(f32::INFINITY));
            add |= !busy && field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        });
        ui.add_space(4.0);
        if add {
            self.add_by_doi(ctx);
        }
    }

    fn add_by_doi(&mut self, ctx: &egui::Context) {
        let Some(found) = doi::extract(&self.doi_input) else {
            self.status = tx("that-is-not-a-doi").into();
            return;
        };
        let already = self
            .references
            .files
            .iter()
            .flat_map(|file| &file.entries)
            .find(|entry| entry.doi.as_deref().is_some_and(|d| d.eq_ignore_ascii_case(&found)));
        if let Some(entry) = already {
            self.status = tr!("the-doi-is-already-here", key = entry.key.clone());
            return;
        }
        let Some(_) = self.saved_folder(tx("save-the-document-before-importing-references")) else { return };
        let servers = self.servers.clone();
        self.doi_lookup = doi::in_background(ctx, move || doi::fetch_bibtex(&servers, &found));
    }

    /// Takes in what the DOI lookup found, and how the check is going.
    fn receive_lookups(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.doi_lookup
            && let Ok(result) = rx.try_recv()
        {
            self.doi_lookup = None;
            match result.and_then(|bibtex| self.keep_fetched(&bibtex)) {
                Ok(key) => {
                    self.status = tr!("reference-added", key = key);
                    self.doi_input.clear();
                    self.references.refresh();
                    ctx.request_repaint();
                }
                Err(id) => self.status = tx(id).into(),
            }
        }
        if let Some(rx) = &self.checking {
            let mut ended = false;
            loop {
                match rx.try_recv() {
                    Ok((identity, verdict)) => {
                        self.verdicts.insert(identity, verdict);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        ended = true;
                        break;
                    }
                }
            }
            if ended {
                self.checking = None;
            }
        }
    }

    fn keep_fetched(&self, bibtex: &str) -> Result<String, &'static str> {
        let root = self.path.as_deref().and_then(Path::parent).ok_or("the-reference-could-not-be-saved")?;
        let used = bib::used_files(&self.text);
        let file = target_file(root, used.as_deref(), &self.references.files);
        let keys: Vec<&str> = self.references.files.iter().flat_map(|f| &f.entries).map(|e| e.key.as_str()).collect();
        bib::append_entry(&file, bibtex, &keys)
    }

    fn ask_to_import(&mut self, ctx: &egui::Context) {
        let Some(dir) = self.saved_folder(tx("save-the-document-before-importing-references")) else { return };
        if let Some(file) = rfd::FileDialog::new().add_filter("BibTeX", &["bib"]).pick_file() {
            self.import_references(ctx, &dir, &file);
        }
    }

    fn import_references(&mut self, ctx: &egui::Context, dir: &Path, file: &Path) {
        match bib::import(dir, file) {
            Ok((path, count)) => {
                self.status = tr!("references-imported", file = path, count = count.to_string());
                self.references.refresh();
                ctx.request_repaint();
            }
            Err(id) => self.status = tx(id).into(),
        }
    }

    /// Imports a file into the document's folder, as the pane's button does after its dialog.
    pub fn import_references_from(&mut self, ctx: &egui::Context, file: &Path) {
        if let Some(dir) = self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf) {
            self.import_references(ctx, &dir, file);
        }
    }

    pub fn search_references(&mut self, query: &str) {
        query.clone_into(&mut self.reference_search);
    }

    /// Asks these servers instead of the real ones. For tests.
    pub fn use_servers(&mut self, servers: Servers) {
        self.servers = servers;
    }

    /// Types a DOI in the pane's field.
    pub fn set_doi_input(&mut self, text: &str) {
        text.clone_into(&mut self.doi_input);
    }

    /// Puts a citation at the caret, as one change with the bibliography it needs: without one in
    /// the document, a citation is an error.
    pub(super) fn cite(&mut self, ctx: &egui::Context, file: &str, key: &str) {
        let citation = bib::citation(key);
        let bibliography = bib::used_files(&self.text).is_none().then(|| edit::file_snippet(file));
        let added = bibliography.is_some();
        self.edit(ctx, |text, sel| {
            let next = edit::insert_citation(text, sel, &citation);
            if let Some(bibliography) = &bibliography {
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(bibliography);
            }
            next
        });
        if added {
            self.status = tx("bibliography-added-at-the-end").into();
        }
    }
}

/// Where a new reference goes: the file the document's bibliography names, one already here, or a new one.
fn target_file(root: &Path, used: Option<&[String]>, files: &[BibFile]) -> PathBuf {
    let named =
        used.unwrap_or_default().iter().filter(|name| name.to_lowercase().ends_with(".bib")).find_map(|name| collab::wire::safe_path(root, name));
    let existing = || bib::citable(files, used).first().and_then(|file| collab::wire::safe_path(root, &file.path));
    named.or_else(existing).unwrap_or_else(|| root.join(NEW_FILE))
}
