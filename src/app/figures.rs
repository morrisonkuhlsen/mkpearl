//! Figures on the pages: a click selects an image, its corner resizes it, dragging it moves its block,
//! and Delete takes it out. The source is the only thing that changes, and each is one undo step.

use egui::{Color32, CursorIcon, Key, Modifiers, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::App;
use crate::compiler::{Job, PlacedImage};
use crate::edit;
use crate::i18n::tx;
use crate::ruler;
use crate::theme::{self, Tokens};

const HANDLE: f32 = 11.0;
/// The least width a drag gives, in points: about half a centimetre.
const LEAST_WIDTH: f32 = 14.0;

#[derive(Default)]
pub(super) struct Figures {
    pub(super) placed: Vec<PlacedImage>,
    /// The one selected, by its place in `placed`.
    pub(super) selected: Option<usize>,
}

/// What the pointer did to an image.
pub(super) enum Action {
    Select(usize),
    Clear,
    Resize(usize, f32),
    /// Dropped with the pointer here on screen.
    Move(usize, egui::Pos2),
}

fn on_screen(image: &PlacedImage, page: Rect, zoom: f32) -> Rect {
    Rect::from_min_size(page.min + vec2(image.x, image.y) * zoom, vec2(image.width, image.height) * zoom)
}

/// The images on one page: draws what's selected, and reports what the pointer does.
pub(super) fn on_page(ui: &mut Ui, figures: &Figures, page: usize, rect: Rect, zoom: f32, page_width: f32, t: &Tokens) -> Option<Action> {
    let mut action = None;
    let mut pressed_on_image = false;
    let pressed_at = ui.input(|i| i.pointer.press_origin().filter(|_| i.pointer.any_pressed()));
    for (k, image) in figures.placed.iter().enumerate().filter(|(_, image)| image.page == page) {
        let area = on_screen(image, rect, zoom);
        let selected = figures.selected == Some(k);
        let handle_rect = Rect::from_center_size(area.max, vec2(HANDLE, HANDLE));
        pressed_on_image |= pressed_at.is_some_and(|at| area.contains(at) || (selected && handle_rect.contains(at)));
        let body = ui.interact(area, ui.scope_id().with(("figure", k)), Sense::click_and_drag());
        if body.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
        if body.clicked() {
            action = Some(Action::Select(k));
        }
        if body.dragged() {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            let moved = ui.input(|i| i.pointer.interact_pos().zip(i.pointer.press_origin()).map_or(egui::Vec2::ZERO, |(now, from)| now - from));
            let ghost = area.translate(moved);
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, ui.scope_id().with("figure-ghost")));
            painter.rect_filled(ghost, 0.0, t.accent.gamma_multiply(0.25));
            painter.rect_stroke(ghost, 0.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Outside);
        }
        if body.drag_stopped()
            && let Some(at) = body.interact_pointer_pos()
        {
            action = Some(Action::Move(k, at));
        }
        if !selected {
            continue;
        }
        ui.painter().rect_stroke(area, 0.0, Stroke::new(1.5, t.accent), egui::StrokeKind::Outside);
        let handle = ui.interact(handle_rect, ui.scope_id().with(("figure-handle", k)), Sense::drag());
        if handle.hovered() || handle.dragged() {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeNwSe);
        }
        ui.painter().rect_filled(handle_rect, 1.0, Color32::WHITE);
        ui.painter().rect_stroke(handle_rect, 1.0, Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
        let width = |at: egui::Pos2| ((at.x - area.left()) / zoom).clamp(LEAST_WIDTH, page_width);
        if handle.dragged()
            && let Some(at) = handle.interact_pointer_pos()
        {
            let new = width(at);
            let ratio = new / image.width.max(1.0);
            let outline = Rect::from_min_size(area.min, area.size() * ratio);
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, ui.scope_id().with("figure-size")));
            painter.rect_stroke(outline, 0.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Outside);
            painter.text(pos2(outline.left(), outline.top() - 4.0), egui::Align2::LEFT_BOTTOM, ruler::in_cm(new), theme::ui_font(12.0), t.accent);
        }
        if handle.drag_stopped()
            && let Some(at) = handle.interact_pointer_pos()
        {
            action = Some(Action::Resize(k, width(at)));
        }
    }
    // A press anywhere else on the page lets go of the image.
    if action.is_none() && figures.selected.is_some() && !pressed_on_image && pressed_at.is_some_and(|at| rect.contains(at)) {
        action = Some(Action::Clear);
    }
    action
}

impl App {
    pub(super) fn figures_after_pages(&mut self, ctx: &egui::Context, action: Option<Action>) {
        // A press off the pages, in the source or the ribbon, lets go too.
        let elsewhere = ctx
            .input(|i| i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(|at| !self.page_rects.iter().any(|page| page.contains(at))));
        if elsewhere {
            self.figures.selected = None;
        }
        match action {
            Some(Action::Select(k)) => self.figures.selected = Some(k),
            Some(Action::Clear) => self.figures.selected = None,
            Some(Action::Resize(k, width)) => {
                let Some(byte) = self.figures.placed.get(k).map(|image| image.byte) else { return };
                let value = ruler::in_cm(width);
                self.edit(ctx, |text, sel| edit::set_image_width(text, sel, byte, &value));
            }
            Some(Action::Move(k, at)) => self.ask_to_move(k, at),
            None => {}
        }
    }

    fn block_of(&self, k: usize) -> Option<std::ops::Range<usize>> {
        let image = self.figures.placed.get(k)?;
        edit::image_call_at(&self.text, image.byte)?.block
    }

    fn ask_to_move(&mut self, k: usize, at: egui::Pos2) {
        let Some(block) = self.block_of(k) else {
            self.status = tx("this-image-is-part-of-a-line").into();
            return;
        };
        // The page under the pointer, or the closest.
        let distance = |page: &Rect| page.distance_to_pos(at);
        let page = self.page_rects.iter().enumerate().min_by(|a, b| distance(a.1).total_cmp(&distance(b.1)));
        let Some((index, rect)) = page else { return };
        let zoom = rect.width() / self.pages.get(index).map_or(1.0, |p| p.width_pt).max(1.0);
        let inside = (at - rect.min) / zoom;
        self.figures.selected = None;
        self.compiler.send(Job::Drop { page: index, x: f64::from(inside.x), y: f64::from(inside.y), block });
    }

    pub(super) fn finish_move(&mut self, ctx: &egui::Context, block: std::ops::Range<usize>, byte: usize, above: bool) {
        // The source may have changed while the compiler looked.
        let same = self.figures.placed.iter().any(|image| edit::image_call_at(&self.text, image.byte).and_then(|c| c.block).as_ref() == Some(&block));
        if !same {
            return;
        }
        self.edit(ctx, |text, sel| match edit::move_block(text, block, byte, above) {
            Some(at) => (at, at),
            None => sel,
        });
    }

    /// Delete and Esc for the image selected, before the editor sees them.
    pub(super) fn figure_keys(&mut self, ctx: &egui::Context) {
        let Some(k) = self.figures.selected else { return };
        if self.figures.placed.get(k).is_none() {
            self.figures.selected = None;
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) || ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_))))
        {
            self.figures.selected = None;
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)) {
            match self.block_of(k) {
                Some(block) => {
                    self.figures.selected = None;
                    self.edit(ctx, |text, _| {
                        let at = edit::delete_block(text, block);
                        (at, at)
                    });
                }
                None => self.status = tx("this-image-is-part-of-a-line").into(),
            }
        }
    }

    /// The images on the pages, for tests.
    pub fn placed_images(&self) -> &[PlacedImage] {
        &self.figures.placed
    }

    pub fn selected_image(&self) -> Option<usize> {
        self.figures.selected
    }
}
