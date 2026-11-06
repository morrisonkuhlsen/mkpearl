//! A folder opened as a project: its tree in a pane beside the rail, the document chosen from it,
//! and Typst's paths starting at its top (see `world`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::{App, Pending};
use crate::i18n::tx;
use crate::icons::Icon;
use crate::project::{Entry, Follower, Tree};
use crate::theme::{self, Tokens};
use crate::tr;
use crate::widgets::icon_button_mono;

pub(super) const WIDTH: f32 = 250.0;
const HEADER: f32 = 34.0;
const ROW: f32 = 24.0;

#[derive(Default)]
pub(super) struct ProjectPane {
    pub(super) open: bool,
    pub(super) root: Option<PathBuf>,
    tree: Tree,
    /// Whether `tree` has been read for `root` yet.
    read: bool,
    /// Folders shown open.
    expanded: HashSet<PathBuf>,
    follower: Option<Follower>,
    /// A folder given on the command line, opened on the first frame.
    pub(super) at_start: Option<PathBuf>,
}

impl App {
    pub fn project_open(&self) -> bool {
        self.project.open
    }

    pub fn toggle_project(&mut self) {
        self.project.open = !self.project.open;
    }

    /// The folder opened as a project, if any.
    pub fn project_folder(&self) -> Option<&Path> {
        self.project.root.as_deref()
    }

    /// The tree as last read, for tests.
    pub fn project_entries(&self) -> Vec<PathBuf> {
        self.project.tree.entries.iter().map(|e| e.path.clone()).collect()
    }

    /// Opens `dir` as the project: its tree on show, Typst's paths from its top, and its main
    /// document, if it has one, in the editor. The document there now is saved first.
    pub fn open_folder(&mut self, ctx: &egui::Context, dir: PathBuf) {
        let dir = std::path::absolute(&dir).unwrap_or(dir);
        if !dir.is_dir() {
            self.status = tr!("the-folder-is-gone", folder = dir.display().to_string());
            return;
        }
        let tree = crate::project::read(&dir);
        let main = crate::project::main_file(&tree).map(|m| dir.join(m));
        let current_inside = self.path.as_deref().is_some_and(|p| std::path::absolute(p).is_ok_and(|p| p.starts_with(&dir)));
        crate::world::set_project(Some(dir.clone()));
        self.project.root = Some(dir.clone());
        self.project.expanded.clear();
        self.project.tree = tree;
        self.project.read = true;
        self.project.open = true;
        self.project.follower.get_or_insert_with(Follower::new).follow(ctx, Some(dir.clone()));
        if crate::recent::push(&mut self.recent, &dir) {
            crate::recent::save(&self.recent);
        }
        // The document there now stays if it's in the folder; otherwise the folder's own comes in,
        // once what's in the editor is saved.
        match main.filter(|_| !current_inside) {
            Some(main) => self.open_project_file(ctx, main),
            None => self.compile(),
        }
        self.expand_to_document();
        self.status = tr!("opened-the-folder", folder = dir.display().to_string());
    }

    /// Closes the project; the document stays, its paths from its own folder again.
    pub fn close_folder(&mut self, ctx: &egui::Context) {
        crate::world::set_project(None);
        self.project.root = None;
        self.project.tree = Tree::default();
        self.project.read = false;
        if let Some(follower) = &self.project.follower {
            follower.follow(ctx, None);
        }
        self.compile();
    }

    /// Opens another of the project's documents. What's in the editor is saved first; if it can't
    /// be, it stays, and the status says why.
    pub fn open_project_file(&mut self, ctx: &egui::Context, path: PathBuf) {
        if self.path.as_deref() == Some(path.as_path()) {
            return;
        }
        if self.leave_document(ctx, &path) {
            self.session = None;
            self.open_path(ctx, path);
            self.expand_to_document();
        }
    }

    /// Saves what must be saved before `next` takes the editor's place. False when the switch has
    /// to wait: saving failed, or a document with no file goes through the usual question.
    fn leave_document(&mut self, ctx: &egui::Context, next: &Path) -> bool {
        if !self.dirty {
            return true;
        }
        if self.path.is_none() {
            self.request(ctx, Pending::Recent(next.to_path_buf()));
            return false;
        }
        if self.save(false) {
            return true;
        }
        if self.status.is_empty() {
            self.status = tx("the-document-couldnt-be-saved-so-it-stays").to_string();
        }
        false
    }

    fn expand_to_document(&mut self) {
        let (Some(root), Some(path)) = (self.project.root.clone(), self.path.clone()) else { return };
        let Ok(relative) = std::path::absolute(&path).unwrap_or(path).strip_prefix(&root).map(Path::to_path_buf) else { return };
        let mut folder = relative.parent();
        while let Some(f) = folder.filter(|f| !f.as_os_str().is_empty()) {
            self.project.expanded.insert(f.to_path_buf());
            folder = f.parent();
        }
    }

    /// Takes in the tree read again; a change to any file compiles again, as one the document reads
    /// shows on the pages.
    pub(super) fn project_tick(&mut self, ctx: &egui::Context) {
        if let Some(dir) = self.project.at_start.take() {
            self.open_folder(ctx, dir);
        }
        let Some(follower) = &self.project.follower else { return };
        let mut changed = false;
        for (root, tree) in follower.read() {
            if self.project.root.as_ref() != Some(&root) {
                continue;
            }
            changed |= self.project.read && tree.fingerprint != self.project.tree.fingerprint;
            self.project.tree = tree;
            self.project.read = true;
        }
        if changed {
            self.compile();
        }
    }

    pub(super) fn project_pane(&mut self, ui: &mut Ui, rect: Rect, t: &Tokens) {
        let ctx = ui.ctx().clone();
        ui.painter().rect_filled(rect, 0.0, t.ribbon);
        ui.painter().vline(rect.right() - 1.0, rect.y_range(), Stroke::new(1.0, t.separator));
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER)).shrink2(vec2(8.0, 4.0));
        let title =
            self.project.root.as_deref().and_then(Path::file_name).map_or_else(|| tx("project").to_string(), |n| n.to_string_lossy().into_owned());
        let mut close = false;
        ui.scope_builder(egui::UiBuilder::new().max_rect(header).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
            if icon_button_mono(ui, Icon::Left, t.text_dim, tx("hide-the-project"), vec2(24.0, 22.0)).clicked() {
                self.project.open = false;
            }
            if self.project.root.is_some() && icon_button_mono(ui, Icon::Delete, t.text_dim, tx("close-the-folder"), vec2(24.0, 22.0)).clicked() {
                close = true;
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                crate::icons::paint(ui.painter(), r, Icon::Folder, t.text);
                let label = ui.add(egui::Label::new(egui::RichText::new(&title).font(theme::ui_bold(13.0)).color(t.text)).truncate());
                if let Some(root) = &self.project.root {
                    label.on_hover_text(root.display().to_string());
                }
            });
        });
        if close {
            self.close_folder(&ctx);
        }
        let body = Rect::from_min_max(pos2(rect.left(), rect.top() + HEADER), pos2(rect.right() - 1.0, rect.bottom()));
        ui.scope_builder(egui::UiBuilder::new().max_rect(body.shrink2(vec2(6.0, 4.0))), |ui| {
            if self.project.root.is_none() {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(tx("open-a-folder-to-see-its-files")).color(t.text_dim));
                ui.add_space(8.0);
                if ui.button(tx("open-folder")).clicked() {
                    self.request(&ctx, Pending::OpenFolder);
                }
                return;
            }
            egui::ScrollArea::vertical().id_salt("project-tree").auto_shrink([false, false]).show(ui, |ui| self.project_rows(ui, t));
        });
    }

    fn project_rows(&mut self, ui: &mut Ui, t: &Tokens) {
        let ctx = ui.ctx().clone();
        let Some(root) = self.project.root.clone() else { return };
        let current = self.path.as_deref().map(|p| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()));
        let mut hidden_under: Option<PathBuf> = None;
        let mut toggle = None;
        let mut open = None;
        let mut insert = None;
        if self.project.tree.entries.is_empty() {
            ui.label(egui::RichText::new(tx("the-folder-is-empty")).color(t.text_dim));
        }
        for entry in &self.project.tree.entries {
            if let Some(folder) = &hidden_under {
                if entry.path.starts_with(folder) {
                    continue;
                }
                hidden_under = None;
            }
            let expanded = entry.dir && self.project.expanded.contains(&entry.path);
            if entry.dir && !expanded {
                hidden_under = Some(entry.path.clone());
            }
            let here = current.as_deref() == Some(root.join(&entry.path).as_path());
            let resp = row(ui, entry, expanded, here, t);
            if entry.dir {
                if resp.clicked() {
                    toggle = Some(entry.path.clone());
                }
            } else if entry.is_typst() {
                if resp.on_hover_text(tx("open-this-document")).clicked() {
                    open = Some(root.join(&entry.path));
                }
            } else if resp.on_hover_text(tx("click-to-insert-at-the-caret")).clicked() {
                insert = Some(entry.path.clone());
            }
        }
        if self.project.tree.cut {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(tr!("the-folder-is-too-large", count = crate::project::MAX_ENTRIES.to_string()))
                    .font(theme::ui_font(11.5))
                    .color(t.text_dim),
            );
        }
        if let Some(folder) = toggle
            && !self.project.expanded.remove(&folder)
        {
            self.project.expanded.insert(folder);
        }
        if let Some(path) = open {
            self.open_project_file(&ctx, path);
        }
        if let Some(path) = insert {
            // From the project's top, which Typst writes with a leading slash.
            let from_top = format!("/{}", path.to_string_lossy().replace('\\', "/"));
            let snippet = crate::edit::file_snippet(&from_top);
            self.edit(&ctx, |text, sel| crate::edit::insert_plain(text, sel, &snippet));
        }
    }
}

fn row(ui: &mut Ui, entry: &Entry, expanded: bool, here: bool, t: &Tokens) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
    let name = entry.name();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, here, &name));
    if here {
        ui.painter().rect_filled(rect, theme::RADIUS, t.accent_soft);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, theme::RADIUS, t.hover);
    }
    let x = rect.left() + 14.0 + 14.0 * entry.depth as f32;
    let icon_rect = Rect::from_center_size(pos2(x + 8.0, rect.center().y), vec2(14.0, 14.0));
    if entry.dir {
        let chevron = if expanded { "▾" } else { "▸" };
        ui.painter().text(pos2(x - 1.0, rect.center().y), Align2::RIGHT_CENTER, chevron, theme::ui_font(10.0), t.text_dim);
        crate::icons::paint_mono(ui.painter(), icon_rect, Icon::Folder, t.text_dim);
    } else {
        crate::icons::paint_mono(ui.painter(), icon_rect, Icon::FileName, if entry.is_typst() { t.text } else { t.text_dim });
    }
    let ink = if entry.dir || entry.is_typst() { t.text } else { t.text_dim };
    let font = if here { theme::ui_bold(12.5) } else { theme::ui_font(12.5) };
    let clip = ui.painter().with_clip_rect(rect);
    clip.text(pos2(x + 20.0, rect.center().y), Align2::LEFT_CENTER, name, font, ink);
    resp
}
