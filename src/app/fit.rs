//! The window for fitting a document in a number of pages. Nothing is written until the difference has
//! been seen and accepted, and then as one undo step.

use egui::{RichText, TextureHandle, TextureOptions, Ui, vec2};

use super::App;
use super::docs::hash_of;
use crate::compiler::{FitAnswer, Job, Pixels};
use crate::fit::{Ending, Plan};
use crate::i18n::tx;
use crate::theme::{self, Tokens};
use crate::tr;

const THUMBNAIL_WIDTH: f32 = 150.0;
const MOST_PAGES: usize = 100_000;

struct Shown {
    answer: FitAnswer,
    before: Vec<TextureHandle>,
    after: Vec<TextureHandle>,
}

pub(super) struct FitDialog {
    target: usize,
    /// The search under way, if any.
    searching: Option<u64>,
    /// What the answer on show was found for.
    made_from: u64,
    target_of_answer: usize,
    answer: Option<Result<Shown, String>>,
}

impl App {
    /// Opens the window, aiming at one page fewer than now.
    pub fn open_fit_pages(&mut self) {
        let target = self.pages.len().saturating_sub(1).max(1);
        self.fit_dialog = Some(FitDialog { target, searching: None, made_from: 0, target_of_answer: 0, answer: None });
    }

    pub fn set_fit_target(&mut self, target: usize) {
        if let Some(dialog) = &mut self.fit_dialog {
            dialog.target = target.max(1);
        }
    }

    /// Starts the search, as the window's button does.
    pub fn search_fit_pages(&mut self) {
        if let Some(mut dialog) = self.fit_dialog.take() {
            self.search_fit(&mut dialog);
            self.fit_dialog = Some(dialog);
        }
    }

    /// Whether a search is under way, and what the last one found, while the window is open.
    pub fn fit_state(&self) -> Option<(bool, Option<Result<FitAnswer, String>>)> {
        let dialog = self.fit_dialog.as_ref()?;
        let answer = dialog.answer.as_ref().map(|found| found.as_ref().map(|shown| shown.answer.clone()).map_err(Clone::clone));
        Some((dialog.searching.is_some(), answer))
    }

    fn search_fit(&mut self, dialog: &mut FitDialog) {
        self.variants_asked += 1;
        dialog.searching = Some(self.variants_asked);
        dialog.made_from = hash_of(&self.text);
        dialog.target_of_answer = dialog.target;
        dialog.answer = None;
        let path = self.session.as_ref().and_then(crate::collab::Session::guest_path).or_else(|| self.path.clone());
        self.compiler.send(Job::Fit { request: self.variants_asked, text: self.text.clone(), path, target: dialog.target });
    }

    pub(super) fn receive_fit(&mut self, ctx: &egui::Context, request: u64, result: Result<FitAnswer, String>) {
        let Some(dialog) = self.fit_dialog.as_mut().filter(|d| d.searching == Some(request)) else { return };
        dialog.searching = None;
        let load = |name: &str, pixels: &[Pixels]| -> Vec<TextureHandle> {
            pixels
                .iter()
                .enumerate()
                .map(|(page, p)| {
                    let image = egui::ColorImage::from_rgba_unmultiplied(p.size, &p.rgba);
                    ctx.load_texture(format!("fit-{request}-{name}-{page}"), image, TextureOptions::LINEAR)
                })
                .collect()
        };
        dialog.answer = Some(result.map(|answer| Shown { before: load("before", &answer.before), after: load("after", &answer.after), answer }));
    }

    fn close_fit(&mut self) {
        self.fit_dialog = None;
        self.compiler.send(Job::CancelFit);
    }

    /// Writes what was found and closes the window, as its button does.
    fn accept_fit(&mut self, ctx: &egui::Context, plan: Plan) {
        self.close_fit();
        self.edit(ctx, |text, sel| plan.apply(text, sel));
    }

    pub(super) fn fit_window(&mut self, ctx: &egui::Context, t: &Tokens) {
        let Some(mut dialog) = self.fit_dialog.take() else { return };
        let (mut search, mut cancel, mut close, mut accept) = (false, false, false, None);
        let stale = hash_of(&self.text) != dialog.made_from;
        egui::Window::new(tx("fit-in-pages")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_width(4.0 * THUMBNAIL_WIDTH + 40.0);
            ui.colored_label(t.text_dim, tx("fit-only-the-layout-changes"));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(tx("fit-pages-wanted"));
                ui.add(egui::DragValue::new(&mut dialog.target).range(1..=MOST_PAGES));
                if dialog.searching.is_some() {
                    ui.spinner();
                    ui.label(tx("fit-searching"));
                    cancel = ui.button(tx("cancel")).clicked();
                } else {
                    search = ui.button(tx("fit-search")).clicked();
                }
            });
            ui.add_space(8.0);
            match &dialog.answer {
                Some(Err(message)) => {
                    ui.add(egui::Label::new(RichText::new(message).color(t.danger)).wrap());
                }
                Some(Ok(shown)) => {
                    Self::fit_answer(ui, shown, dialog.target_of_answer, t);
                    if stale {
                        ui.colored_label(t.warning_ink, tx("fit-the-text-has-changed"));
                    }
                }
                None => {}
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let found = dialog.answer.as_ref().and_then(|found| found.as_ref().ok()).map(|shown| &shown.answer);
                if let Some(found) = found.filter(|_| !stale)
                    && let Some(plan) = found.plan
                {
                    let words = if found.ending == Ending::Reached { tx("apply") } else { tx("fit-apply-anyway") };
                    if ui.button(words).clicked() {
                        accept = Some(plan);
                    }
                }
                close = ui.button(tx("close")).clicked();
            });
        });
        if search {
            self.search_fit(&mut dialog);
        }
        if cancel {
            dialog.searching = None;
            self.compiler.send(Job::CancelFit);
        }
        self.fit_dialog = Some(dialog);
        if let Some(plan) = accept {
            self.accept_fit(ctx, plan);
        } else if close {
            self.close_fit();
        }
    }

    fn fit_answer(ui: &mut Ui, shown: &Shown, target: usize, t: &Tokens) {
        let answer = &shown.answer;
        let pages = tr!("pages-count", n = answer.pages_after);
        let summary = match (answer.plan.is_some(), answer.ending) {
            (false, Ending::OutOfReach) => tr!("fit-nothing-left-to-take", pages = tr!("pages-count", n = answer.pages_before)),
            (false, Ending::Stopped) => tx("fit-stopped-with-nothing").to_string(),
            (false, Ending::Reached) => tr!("fit-already-fits", pages = tr!("pages-count", n = answer.pages_before)),
            (true, Ending::Reached) => tr!("fit-reached", pages = pages),
            (true, Ending::OutOfReach) => tr!("fit-out-of-reach", target = target, pages = pages),
            (true, Ending::Stopped) => tr!("fit-stopped", pages = pages),
        };
        ui.add(egui::Label::new(RichText::new(summary).font(theme::ui_bold(13.0))).wrap());
        let Some(now) = &answer.now else { return };
        if let Some(plan) = &answer.plan {
            for line in plan.describe(now) {
                ui.label(line);
            }
        }
        if answer.plan.is_none() {
            return;
        }
        ui.add_space(6.0);
        ui.horizontal_top(|ui| {
            for (title, textures, count) in
                [(tx("before-this-version"), &shown.before, answer.pages_before), (tx("fit-after"), &shown.after, answer.pages_after)]
            {
                ui.vertical(|ui| {
                    ui.label(RichText::new(format!("{title}: {}", tr!("pages-count", n = count))).font(theme::ui_bold(12.5)));
                    ui.horizontal_top(|ui| {
                        for texture in textures {
                            let size = texture.size_vec2();
                            let picture = ui.add(egui::Image::new((texture.id(), vec2(THUMBNAIL_WIDTH, THUMBNAIL_WIDTH * size.y / size.x.max(1.0)))));
                            ui.painter().rect_stroke(picture.rect, 0.0, egui::Stroke::new(1.0, t.separator), egui::StrokeKind::Outside);
                        }
                    });
                });
                ui.add_space(8.0);
            }
        });
    }
}
