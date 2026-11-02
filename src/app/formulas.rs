//! Ready-made formulas, in the Symbols pane: a preview of each, a search, and a click to insert.

use egui::{CollapsingHeader, RichText, Sense, Ui, vec2};

use super::docs::hash_of;
use super::{App, Side};
use crate::compiler::Job;
use crate::edit;
use crate::formulas::{Formula, catalog};
use crate::i18n::tx;
use crate::theme::Tokens;

/// The room kept for a preview not yet made, so that scrolling doesn't jump.
const PREVIEW_ROOM: f32 = 36.0;

#[derive(Default)]
pub(super) struct FormulaPane {
    /// On show instead of the symbols.
    on: bool,
    search: String,
}

impl App {
    /// The pane's two views, and whether it's the formulas'.
    pub(super) fn formulas_tabs(&mut self, ui: &mut Ui) -> bool {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.formulas.on, true, tx("formulas"));
            ui.selectable_value(&mut self.formulas.on, false, tx("symbols"));
        });
        ui.add_space(4.0);
        self.formulas.on
    }

    pub fn open_formulas(&mut self) {
        self.formulas.on = true;
        self.side = Some(Side::Symbols);
    }

    /// Searches the formulas, as the pane's field does.
    pub fn search_formulas(&mut self, query: &str) {
        query.clone_into(&mut self.formulas.search);
    }

    pub(super) fn pane_formulas(&mut self, ui: &mut Ui, t: &Tokens) {
        let ctx = ui.ctx().clone();
        ui.add(egui::TextEdit::singleline(&mut self.formulas.search).hint_text(tx("search-the-formulas")).desired_width(f32::INFINITY));
        ui.add_space(4.0);
        ui.colored_label(t.text_dim, tx("click-a-formula-to-insert-it"));
        ui.add_space(4.0);
        let all = catalog();
        let mut chosen = None;
        if self.formulas.search.trim().is_empty() {
            for category in 0..all.category_count() {
                CollapsingHeader::new(RichText::new(all.category(category)).strong())
                    .id_salt(("formulas", category))
                    .default_open(category == 0)
                    .show(ui, |ui| {
                        for formula in all.formulas.iter().filter(|f| f.category == category) {
                            chosen = self.formula_row(ui, t, formula).or(chosen.take());
                        }
                    });
            }
        } else {
            let query = self.formulas.search.clone();
            let found: Vec<&Formula> = all.formulas.iter().filter(|f| f.matches(&query)).collect();
            if found.is_empty() {
                ui.colored_label(t.text_dim, tx("no-results"));
            }
            for formula in found {
                chosen = self.formula_row(ui, t, formula).or(chosen.take());
            }
        }
        if let Some(code) = chosen {
            let block = format!("{code}\n");
            self.edit(&ctx, |text, sel| edit::insert_block(text, sel, &block));
        }
    }

    /// One formula: its name and picture, both of which insert it. Made only when scrolled into view.
    fn formula_row(&mut self, ui: &mut Ui, t: &Tokens, formula: &Formula) -> Option<String> {
        let code = formula.code();
        let mut clicked = ui.add(egui::Button::new(RichText::new(formula.name()).color(t.text)).frame_when_inactive(false).truncate()).clicked();
        let request = hash_of(&code);
        match self.docs.renders.get(&request) {
            Some(Ok((texture, size))) => {
                let width = (size[0] as f32 / 2.0).min(ui.available_width());
                let height = width * size[1] as f32 / size[0].max(1) as f32;
                let picture = egui::Image::new((texture.id(), vec2(width, height))).sense(Sense::click());
                clicked |= ui.add(picture).on_hover_text(&code).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
            }
            Some(Err(message)) => {
                ui.add(egui::Label::new(RichText::new(message).small().color(t.danger)).wrap());
            }
            None => {
                let (room, _) = ui.allocate_exact_size(vec2(ui.available_width(), PREVIEW_ROOM), Sense::hover());
                if ui.is_rect_visible(room) && self.docs.asked.insert(request) {
                    self.compiler.send(Job::Example { request, text: code.clone() });
                }
            }
        }
        ui.add_space(4.0);
        clicked.then_some(code)
    }
}
