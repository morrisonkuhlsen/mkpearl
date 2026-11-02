//! Comparing two versions on their pages: before and now, side by side, with what changed marked. It
//! looks and never touches: the document stays as it is.

use std::collections::HashMap;

use egui::{Color32, Rect, RichText, Sense, TextureHandle, TextureOptions, Ui, pos2, vec2};

use super::{App, Layout};
use crate::compare::Kind;
use crate::compiler::{Comparison, Job, Mark, Page};
use crate::history;
use crate::i18n::tx;
use crate::theme::Tokens;
use crate::tr;

const GAP: f32 = 12.0;
const PAD: f32 = 14.0;

pub(super) struct Comparing {
    old_text: String,
    when: String,
    /// The compile the comparison asked for was made against.
    asked: Option<u64>,
    result: Option<Result<Comparison, String>>,
    textures: HashMap<u128, TextureHandle>,
    /// The change on show, among all of them in order down the pages.
    current: Option<usize>,
    scroll_to_current: bool,
}

fn color(kind: Kind, solid: bool) -> Color32 {
    let (r, g, b) = match kind {
        Kind::Added => (34, 160, 80),
        Kind::Removed => (220, 50, 50),
        Kind::Moved => (50, 110, 230),
    };
    if solid { Color32::from_rgb(r, g, b) } else { Color32::from_rgba_unmultiplied(r, g, b, 70) }
}

/// Which side a change is on, and which change on it.
#[derive(Clone, Copy, PartialEq)]
enum Side {
    Old,
    New,
}

impl App {
    /// Compares the version looked at in the Versions pane with the document.
    pub fn compare_viewed(&mut self) {
        let Some(viewed) = &self.viewed else { return };
        let when = history::describe(viewed.version.stamp, history::now_ms());
        let old_text = viewed.text.clone();
        self.start_comparing(old_text, when);
    }

    /// Compares this text with the document, for tests and pictures of the window.
    pub fn compare_with_text(&mut self, old: &str) {
        self.start_comparing(old.to_string(), String::new());
    }

    fn start_comparing(&mut self, old_text: String, when: String) {
        if self.previewing {
            self.preview_version(false);
        }
        if self.layout == Layout::Source {
            self.layout = Layout::Both;
        }
        self.timeline = None;
        self.comparing =
            Some(Comparing { old_text, when, asked: None, result: None, textures: HashMap::new(), current: None, scroll_to_current: false });
    }

    pub fn stop_comparing(&mut self) {
        self.comparing = None;
    }

    /// The marks before and now, and how many changes in all, once it's worked out.
    pub fn comparison_summary(&self) -> Option<(usize, usize, usize)> {
        let result = self.comparing.as_ref()?.result.as_ref()?.as_ref().ok()?;
        Some((result.old_marks.len(), result.new_marks.len(), result.unseen))
    }

    pub(super) fn receive_comparison(&mut self, ctx: &egui::Context, request: u64, result: Result<Comparison, String>) {
        let Some(comparing) = &mut self.comparing else { return };
        if comparing.asked != Some(request) {
            return;
        }
        if let Ok(found) = &result {
            for page in &found.old_pages {
                comparing.textures.entry(page.hash).or_insert_with(|| {
                    let image = egui::ColorImage::from_rgba_unmultiplied(page.pixels.size, &page.pixels.rgba);
                    ctx.load_texture(format!("compare-{:x}", page.hash), image, TextureOptions::LINEAR)
                });
            }
            comparing.textures.retain(|hash, _| found.old_pages.iter().any(|page| page.hash == *hash));
        }
        comparing.current = None;
        comparing.result = Some(result);
    }

    pub(super) fn compare_view(&mut self, ui: &mut Ui, t: &Tokens) {
        // Asked again as the document changes, once its compile is done.
        let compiles = self.compiles;
        let mut ask = None;
        if let Some(comparing) = &mut self.comparing
            && comparing.asked != Some(compiles)
            && compiles > 0
        {
            comparing.asked = Some(compiles);
            ask = Some(comparing.old_text.clone());
        }
        if let Some(old) = ask {
            self.compiler.send(Job::Compare { old, request: compiles });
        }
        let close = self.compare_bar(ui, t);
        if close {
            self.comparing = None;
            return;
        }
        self.compare_pages(ui, t);
    }

    /// The bar above the pages. Whether it asked to close.
    fn compare_bar(&mut self, ui: &mut Ui, t: &Tokens) -> bool {
        let Some(comparing) = &mut self.comparing else { return true };
        let mut close = false;
        egui::Frame::new().fill(t.ribbon).inner_margin(6.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                if !comparing.when.is_empty() {
                    ui.label(RichText::new(tr!("comparing-with", when = comparing.when.clone())).strong().color(t.text));
                }
                for (kind, label) in [(Kind::Added, tx("came-in")), (Kind::Removed, tx("went-out")), (Kind::Moved, tx("changed-place"))] {
                    let (dot, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                    ui.painter().rect_filled(dot, 2.0, color(kind, true));
                    ui.colored_label(t.text, label);
                }
                let total = comparing.result.as_ref().and_then(|r| r.as_ref().ok()).map_or(0, |found| found.old_marks.len() + found.new_marks.len());
                if total > 0 {
                    let mut step = None;
                    if ui.button(tx("previous-change")).clicked() {
                        step = Some(comparing.current.map_or(total - 1, |c| (c + total - 1) % total));
                    }
                    if ui.button(tx("next-change")).clicked() {
                        step = Some(comparing.current.map_or(0, |c| (c + 1) % total));
                    }
                    if step.is_some() {
                        comparing.current = step;
                        comparing.scroll_to_current = true;
                    }
                }
                close = ui.button(tx("close-the-comparison")).clicked();
            });
            match &comparing.result {
                None => {
                    ui.colored_label(t.text_dim, tx("comparing-the-versions"));
                }
                Some(Err(error)) => {
                    ui.colored_label(t.danger, tr!("the-old-version-does-not-compile", error = error.clone()));
                }
                Some(Ok(found)) => {
                    if found.old_marks.is_empty() && found.new_marks.is_empty() {
                        ui.colored_label(t.text_dim, tx("no-changes-on-the-pages"));
                    }
                    if found.unseen > 0 {
                        ui.colored_label(t.text_dim, tr!("changes-in-code-only", count = found.unseen.to_string()));
                    }
                }
            }
        });
        close
    }

    fn compare_pages(&mut self, ui: &mut Ui, t: &Tokens) {
        let App { comparing, pages, textures, .. } = self;
        let Some(comparing) = comparing else { return };
        let Some(Ok(found)) = &comparing.result else { return };
        let width = ((ui.available_width() - GAP - 2.0 * PAD - 14.0) / 2.0).max(60.0);
        // All the changes in order down the pages, for going through them.
        let mut order: Vec<(Side, Mark)> =
            found.old_marks.iter().map(|m| (Side::Old, *m)).chain(found.new_marks.iter().map(|m| (Side::New, *m))).collect();
        order.sort_by(|a, b| (a.1.page, a.1.y).partial_cmp(&(b.1.page, b.1.y)).unwrap_or(std::cmp::Ordering::Equal));
        let current = comparing.current.and_then(|c| order.get(c).copied());
        let scroll = std::mem::take(&mut comparing.scroll_to_current);
        let count = found.old_pages.len().max(pages.len());
        egui::ScrollArea::vertical().id_salt("compare-scroll").auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(PAD);
            for index in 0..count {
                let heights = |page: Option<&Page>| page.map_or(0.0, |p| width * p.height_pt / p.width_pt.max(1.0));
                let (old, new) = (found.old_pages.get(index), pages.get(index));
                let height = heights(old).max(heights(new)).max(60.0) + 18.0;
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
                let left = pos2(row.left() + PAD, row.top());
                let right = pos2(row.left() + PAD + width + GAP, row.top());
                for (side, at, page, texture, marks, label) in [
                    (Side::Old, left, old, old.and_then(|p| comparing.textures.get(&p.hash)), &found.old_marks, tx("before-this-version")),
                    (Side::New, right, new, new.and_then(|p| textures.get(&p.hash)), &found.new_marks, tx("now-the-document")),
                ] {
                    ui.painter().text(at, egui::Align2::LEFT_TOP, label, crate::theme::ui_font(11.5), t.text_dim);
                    let top = at + vec2(0.0, 16.0);
                    let Some(page) = page else {
                        ui.painter().text(top + vec2(0.0, 4.0), egui::Align2::LEFT_TOP, tx("no-page-here"), crate::theme::ui_font(12.0), t.text_dim);
                        continue;
                    };
                    let zoom = width / page.width_pt.max(1.0);
                    let rect = Rect::from_min_size(top, vec2(width, page.height_pt * zoom));
                    if !ui.is_rect_visible(rect.expand(200.0)) && !scroll {
                        continue;
                    }
                    if let Some(texture) = texture {
                        ui.painter().image(texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    for mark in marks.iter().filter(|m| m.page == index) {
                        let area = Rect::from_min_size(rect.min + vec2(mark.x, mark.y) * zoom, vec2(mark.width, mark.height) * zoom);
                        ui.painter().rect_filled(area, 1.0, color(mark.kind, false));
                        let this = current.is_some_and(|(s, m)| s == side && m == *mark);
                        if this {
                            ui.painter().rect_stroke(
                                area.expand(2.0),
                                2.0,
                                egui::Stroke::new(1.5, color(mark.kind, true)),
                                egui::StrokeKind::Outside,
                            );
                            if scroll {
                                ui.scroll_to_rect(area.expand(60.0), Some(egui::Align::Center));
                            }
                        }
                    }
                }
            }
            ui.add_space(PAD);
        });
    }
}
