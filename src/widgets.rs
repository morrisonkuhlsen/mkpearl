//! The toolbar's controls: MK Matrix's single row of one-size icons, its groups named on hover.
//! Metrics and painting adapted from MK Matrix (MIT OR Apache-2.0); see NOTICE.

use egui::{Align2, Color32, Rect, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::icons::{self, Icon};
use crate::theme::{self, Tokens};

/// The height of the toolbar's single row of controls.
pub const TOOL_ROW_H: f32 = 30.0;

fn group_start_id() -> egui::Id {
    egui::Id::unique("ribbon_group_start")
}

/// Marks where a tab's first group starts.
pub fn group_begin(ui: &Ui) {
    let x = ui.cursor().min.x;
    ui.ctx().data_mut(|d| d.insert_temp(group_start_id(), x));
}

/// Ends the group that started at the last mark. Its name isn't written under it, which would cost
/// the toolbar a second row: it shows when the pointer rests on the group's own ground, between its
/// controls. A screen reader has each control's own name, which often is the group's.
pub fn group_end(ui: &mut Ui, name: &str) {
    let start = ui.ctx().data(|d| d.get_temp::<f32>(group_start_id())).unwrap_or(ui.max_rect().left());
    let end = ui.cursor().min.x;
    let top = ui.max_rect().top();
    let group = Rect::from_min_max(pos2(start, top), pos2(end.max(start), top + TOOL_ROW_H));
    let resp = ui.interact(group, ui.scope_id().with(("ribbon-group", name)), Sense::hover());
    resp.on_hover_text(name);
}

/// Ends a group, named on hover, with a short, light line to the next.
pub fn sep(ui: &mut Ui, group: &str) {
    let t = Tokens::get(ui.ctx());
    group_end(ui, group);
    let (r, _) = ui.allocate_exact_size(vec2(13.0, 28.0), Sense::hover());
    ui.painter().line_segment([pos2(r.center().x, r.top() + 6.0), pos2(r.center().x, r.bottom() - 6.0)], Stroke::new(1.0, t.separator));
    group_begin(ui);
}

fn feedback(ui: &Ui, rect: Rect, resp: &Response, on: bool, t: &Tokens) {
    if resp.is_pointer_button_down_on() || on {
        ui.painter().rect_filled(rect, theme::RADIUS, t.pressed);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, theme::RADIUS, t.hover);
    }
    if on {
        ui.painter().rect_stroke(rect, theme::RADIUS, Stroke::new(1.0, t.separator), StrokeKind::Inside);
    }
}

/// A frameless icon button in one colour (the title bar's).
pub fn icon_button_mono(ui: &mut Ui, icon: Icon, color: Color32, tip: &str, size: egui::Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let t = Tokens::get(ui.ctx());
    feedback(ui, rect, &resp, false, &t);
    icons::paint_mono(ui.painter(), Rect::from_center_size(rect.center(), vec2(16.0, 16.0)), icon, color);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    resp.on_hover_text(tip)
}

fn button(ui: &mut Ui, icon: Icon, label: &str, tip: &str, dropdown: bool, on: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let font = theme::ui_font(12.5);
    let tw = if label.is_empty() { 0.0 } else { ui.painter().layout_no_wrap(label.to_string(), font.clone(), t.text).size().x + 6.0 };
    let w = 24.0 + tw + if dropdown { 12.0 } else { 0.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 23.0), Sense::click());
    feedback(ui, rect, &resp, on, &t);
    icons::paint(ui.painter(), Rect::from_center_size(pos2(rect.left() + 12.0, rect.center().y), vec2(16.0, 16.0)), icon, t.text);
    if !label.is_empty() {
        ui.painter().text(pos2(rect.left() + 24.0, rect.center().y), Align2::LEFT_CENTER, label, font, t.text);
    }
    if dropdown {
        chevron(ui, pos2(rect.right() - 7.0, rect.center().y + 1.0), &t);
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    resp.on_hover_text(tip)
}

fn chevron(ui: &Ui, center: egui::Pos2, t: &Tokens) {
    icons::paint(ui.painter(), Rect::from_center_size(center, vec2(8.0, 8.0)), Icon::Chevron, t.text_dim.gamma_multiply(0.7));
}

/// An icon of the toolbar's size; its name lives in the tooltip.
pub fn tool(ui: &mut Ui, icon: Icon, tip: &str) -> Response {
    button(ui, icon, "", tip, false, false)
}

/// An icon with its name beside it, where the icon alone can't tell two buttons apart.
pub fn labelled(ui: &mut Ui, icon: Icon, label: &str, tip: &str) -> Response {
    button(ui, icon, label, tip, false, false)
}

/// An icon with its name that stays pressed while `on`.
pub fn toggle(ui: &mut Ui, icon: Icon, label: &str, tip: &str, on: bool) -> Response {
    button(ui, icon, label, tip, false, on)
}

/// An icon with the arrow that says it opens something.
pub fn tool_with_arrow(ui: &mut Ui, icon: Icon, tip: &str) -> Response {
    button(ui, icon, "", tip, true, false)
}

/// An icon that opens a menu.
pub fn tool_menu(ui: &mut Ui, icon: Icon, tip: &str, menu: impl FnOnce(&mut Ui)) {
    let resp = button(ui, icon, "", tip, true, false);
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(180.0);
        menu(ui);
    });
}

/// A button of the toolbar's size showing characters, for what the icon set has no drawing of.
pub fn glyphs(ui: &mut Ui, text: &str, tip: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    let font = theme::ui_font(12.5);
    let w = ui.painter().layout_no_wrap(text.to_string(), font.clone(), t.text).size().x + 10.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w.max(24.0), 23.0), Sense::click());
    feedback(ui, rect, &resp, false, &t);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font, t.text);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    resp.on_hover_text(tip)
}

/// A field showing `text` that opens a list: the font and size boxes.
pub fn field_menu(ui: &mut Ui, text: &str, width: f32, tip: &str, menu: impl FnOnce(&mut Ui)) {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 23.0), Sense::click());
    ui.painter().rect(
        rect,
        theme::RADIUS,
        if resp.hovered() { t.field_hover } else { t.input_bg },
        Stroke::new(1.0, t.input_border),
        StrokeKind::Inside,
    );
    let area = Rect::from_min_max(rect.min, pos2(rect.right() - 14.0, rect.bottom()));
    ui.painter().with_clip_rect(area.intersect(ui.clip_rect())).text(
        pos2(area.left() + 7.0, area.center().y),
        Align2::LEFT_CENTER,
        text,
        theme::ui_font(12.5),
        t.text,
    );
    chevron(ui, pos2(rect.right() - 8.0, rect.center().y + 1.0), &t);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    let resp = resp.on_hover_text(tip);
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(width.max(120.0));
        menu(ui);
    });
}
