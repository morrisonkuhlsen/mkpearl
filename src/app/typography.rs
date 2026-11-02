//! The typography watch in the window: what it found, in the Problems pane and on the pages. What the
//! writer chooses to ignore stays ignored while the passage is the same, also in later runs.

use std::collections::HashSet;

use egui::{Rect, RichText, Sense, Stroke, StrokeKind, Ui, vec2};

use super::App;
use crate::i18n::tx;
use crate::recent;
use crate::theme::{self, Tokens};
use crate::tr;
use crate::typography::Finding;

const IGNORED_FILE: &str = "typography-ignored.txt";
/// The file is small, and hostile input like any other.
const MAX_IGNORED: usize = 5000;

pub(super) struct Watch {
    pub(super) findings: Vec<Finding>,
    ignored: HashSet<u64>,
}

impl Watch {
    pub(super) fn load() -> Self {
        let text = recent::read_setting(IGNORED_FILE).unwrap_or_default();
        let ignored = text.lines().filter_map(|line| u64::from_str_radix(line.trim(), 16).ok()).take(MAX_IGNORED).collect();
        Self { findings: Vec::new(), ignored }
    }

    fn save(&self) {
        let lines: Vec<String> = self.ignored.iter().map(|key| format!("{key:016x}")).collect();
        recent::write_setting(IGNORED_FILE, &lines.join("\n"));
    }

    pub(super) fn visible(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| !self.ignored.contains(&f.key))
    }

    fn hidden(&self) -> usize {
        self.findings.len() - self.visible().count()
    }

    /// Marks on one page, drawn over it.
    pub(super) fn on_page(&self, ui: &Ui, page: usize, rect: Rect, zoom: f32, t: &Tokens) {
        for found in self.visible().filter(|f| f.page == page) {
            let at = rect.min + vec2(found.x, found.y) * zoom;
            // What goes off the paper is marked up to its edge.
            let mark = Rect::from_min_size(at, vec2(found.width, found.height) * zoom).expand(2.0).intersect(rect);
            ui.painter().rect_filled(mark, 2.0, t.warning.gamma_multiply(0.18));
            ui.painter().rect_stroke(mark, 2.0, Stroke::new(1.0, t.warning), StrokeKind::Outside);
        }
    }
}

impl App {
    /// What the watch found and wasn't told to ignore.
    pub fn typography_findings(&self) -> Vec<Finding> {
        self.watch.visible().copied().collect()
    }

    pub(super) fn typography_pane(&mut self, ui: &mut Ui) {
        if self.watch.findings.is_empty() {
            return;
        }
        let (mut jump, mut ignore) = (None, None);
        for (name, accessibility) in [(tx("typography"), false), (tx("accessibility"), true)] {
            let group = || self.watch.visible().filter(|f| f.kind.is_accessibility() == accessibility);
            if group().next().is_none() {
                continue;
            }
            ui.add_space(6.0);
            ui.label(RichText::new(name).font(theme::ui_bold(12.5)));
            ui.add_space(2.0);
            for (index, found) in group().enumerate() {
                ui.push_id((name, index), |ui| {
                    let message = egui::Label::new(found.message()).wrap().sense(Sense::click());
                    if ui.add(message).on_hover_text(tx("go-to-the-passage")).clicked() {
                        jump = Some(*found);
                    }
                    if ui.small_button(tx("ignore")).clicked() {
                        ignore = Some(found.key);
                    }
                });
                ui.add_space(4.0);
            }
        }
        let hidden = self.watch.hidden();
        let restore = hidden > 0 && ui.small_button(tr!("show-what-was-ignored", n = hidden)).clicked();
        if let Some(found) = jump {
            self.jump_to(found.byte);
            self.scroll_page = Some((found.page, found.y));
        }
        if let Some(key) = ignore {
            if self.watch.ignored.len() >= MAX_IGNORED
                && let Some(old) = self.watch.ignored.iter().next().copied()
            {
                self.watch.ignored.remove(&old);
            }
            self.watch.ignored.insert(key);
            self.watch.save();
        }
        if restore {
            let keys: Vec<u64> = self.watch.findings.iter().map(|f| f.key).collect();
            for key in keys {
                self.watch.ignored.remove(&key);
            }
            self.watch.save();
        }
    }
}
