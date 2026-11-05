//! The colour picker of the ribbon: a palette and the latest colours used on one side, and on the other
//! any colour at all, by its look, its hex code or its numbers, with its opacity.

use crate::i18n::tx;
use crate::theme::{self, Tokens};
use egui::epaint::{Mesh, Shape};
use egui::{Color32, Rect, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

const WIDTH: f32 = 236.0;
const COLUMNS: usize = 10;
const SWATCH: f32 = 20.0;
const RECENT_FILE: &str = "colours.txt";
const MAX_RECENT: usize = 10;

/// The hues of the palette; each gives a column, from its lightest tint to its deepest shade.
const HUES: [u32; COLUMNS] = [0x1F2328, 0x475569, 0xA0522D, 0xC0392B, 0xE67E22, 0xF2B705, 0x2E8B57, 0x0F8B8D, 0x2563EB, 0x7C3AED];
const VIVID: [u32; COLUMNS] = [0xFF0000, 0xFF7F00, 0xFFD400, 0x7FD000, 0x00B050, 0x00C8C8, 0x0090FF, 0x0040D0, 0x8000C0, 0xFF00A0];
/// How far each row of the palette goes towards white (above 0) or black (below 0).
const TINTS: [f32; 6] = [0.8, 0.6, 0.4, 0.0, -0.25, -0.5];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colour {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Colour {
    const fn from_rgb(rgb: u32) -> Colour {
        Colour { r: (rgb >> 16) as u8, g: (rgb >> 8) as u8, b: rgb as u8, a: 255 }
    }

    /// Reads `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`, the `#` optional, in either case.
    pub fn parse(text: &str) -> Option<Colour> {
        let code = text.trim();
        let code = code.strip_prefix('#').unwrap_or(code);
        // Alone, the number parser below would also take a sign.
        if !code.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let pair = |at: usize| code.get(at..at + 2).and_then(|p| u8::from_str_radix(p, 16).ok());
        let single = |at: usize| code.get(at..=at).and_then(|p| u8::from_str_radix(p, 16).ok()).map(|v| v * 17);
        match code.len() {
            3 => Some(Colour { r: single(0)?, g: single(1)?, b: single(2)?, a: 255 }),
            4 => Some(Colour { r: single(0)?, g: single(1)?, b: single(2)?, a: single(3)? }),
            6 => Some(Colour { r: pair(0)?, g: pair(2)?, b: pair(4)?, a: 255 }),
            8 => Some(Colour { r: pair(0)?, g: pair(2)?, b: pair(4)?, a: pair(6)? }),
            _ => None,
        }
    }

    /// `#rrggbb`, with its opacity only where it isn't full.
    pub fn hex(self) -> String {
        let Colour { r, g, b, a } = self;
        if a == 255 { format!("#{r:02x}{g:02x}{b:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") }
    }

    /// As Typst writes it.
    pub fn typst(self) -> String {
        format!("rgb(\"{}\")", self.hex())
    }

    pub fn color32(self) -> Color32 {
        Color32::from_rgba_unmultiplied(self.r, self.g, self.b, self.a)
    }

    /// Hue, saturation and value, each from 0 to 1.
    pub fn hsv(self) -> (f32, f32, f32) {
        let (r, g, b) = (f32::from(self.r) / 255.0, f32::from(self.g) / 255.0, f32::from(self.b) / 255.0);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let delta = max - min;
        let hue = if delta <= 0.0 {
            0.0
        } else if max == r {
            ((g - b) / delta).rem_euclid(6.0) / 6.0
        } else if max == g {
            ((b - r) / delta + 2.0) / 6.0
        } else {
            ((r - g) / delta + 4.0) / 6.0
        };
        (hue, if max <= 0.0 { 0.0 } else { delta / max }, max)
    }

    pub fn from_hsv(hue: f32, saturation: f32, value: f32, a: u8) -> Colour {
        let (s, v) = (saturation.clamp(0.0, 1.0), value.clamp(0.0, 1.0));
        let h = hue.rem_euclid(1.0) * 6.0;
        let c = v * s;
        let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let byte = |channel: f32| ((channel + v - c) * 255.0).round() as u8;
        Colour { r: byte(r), g: byte(g), b: byte(b), a }
    }

    /// Mixed with white (`amount` above 0) or black (below 0).
    fn tinted(self, amount: f32) -> Colour {
        let target = if amount > 0.0 { 255.0 } else { 0.0 };
        let mix = |channel: u8| (f32::from(channel) + (target - f32::from(channel)) * amount.abs()).round() as u8;
        Colour { r: mix(self.r), g: mix(self.g), b: mix(self.b), a: self.a }
    }
}

/// The saved list. It's a file on disk, so what isn't a colour is passed over.
pub fn parse_recent(text: &str) -> Vec<Colour> {
    let mut list = Vec::new();
    for colour in text.lines().filter_map(Colour::parse) {
        if !list.contains(&colour) && list.len() < MAX_RECENT {
            list.push(colour);
        }
    }
    list
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Palette,
    Custom,
}

/// What stays between one opening and the next. Hue and saturation are kept apart from the colour:
/// in black every hue is the same, and the handle mustn't jump.
pub struct Picker {
    tab: Tab,
    hue: f32,
    saturation: f32,
    value: f32,
    alpha: u8,
    /// What the hex field says, which may be half typed.
    hex: String,
    recent: Vec<Colour>,
}

impl Default for Picker {
    fn default() -> Picker {
        let mut picker = Picker {
            tab: Tab::Palette,
            hue: 0.0,
            saturation: 0.0,
            value: 0.0,
            alpha: 255,
            hex: String::new(),
            recent: crate::recent::read_setting(RECENT_FILE).map_or_else(Vec::new, |text| parse_recent(&text)),
        };
        picker.set(Colour::from_rgb(HUES[3]));
        picker
    }
}

impl Picker {
    fn current(&self) -> Colour {
        Colour::from_hsv(self.hue, self.saturation, self.value, self.alpha)
    }

    fn set(&mut self, colour: Colour) {
        let (hue, saturation, value) = colour.hsv();
        // Grey and black have no hue of their own to give.
        if saturation > 0.0 && value > 0.0 {
            self.hue = hue;
        }
        if value > 0.0 {
            self.saturation = saturation;
        }
        self.value = value;
        self.alpha = colour.a;
        self.hex = colour.hex();
    }

    fn remember(&mut self, colour: Colour) {
        self.recent.retain(|c| *c != colour);
        self.recent.insert(0, colour);
        self.recent.truncate(MAX_RECENT);
        let text: String = self.recent.iter().map(|c| c.hex() + "\n").collect();
        crate::recent::write_setting(RECENT_FILE, &text);
    }

    /// Draws the picker. Returns the colour once one is chosen.
    pub fn show(&mut self, ui: &mut Ui) -> Option<Colour> {
        let t = Tokens::get(ui.ctx());
        ui.set_width(WIDTH);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, Tab::Palette, tx("palette"));
            ui.selectable_value(&mut self.tab, Tab::Custom, tx("custom-colour"));
        });
        ui.add_space(6.0);
        let chosen = match self.tab {
            Tab::Palette => self.palette(ui, &t),
            Tab::Custom => self.custom(ui, &t),
        };
        if let Some(colour) = chosen {
            self.set(colour);
            self.remember(colour);
        }
        chosen
    }

    fn palette(&mut self, ui: &mut Ui, t: &Tokens) -> Option<Colour> {
        let mut chosen = None;
        let current = self.current();
        let greys: Vec<Colour> = (0..COLUMNS).map(|i| Colour::from_rgb(0xFFFFFF).tinted(-(i as f32) / (COLUMNS - 1) as f32)).collect();
        swatch_row(ui, &greys, current, t, &mut chosen);
        ui.add_space(6.0);
        for amount in TINTS {
            let row: Vec<Colour> = HUES.iter().map(|hue| Colour::from_rgb(*hue).tinted(amount)).collect();
            swatch_row(ui, &row, current, t, &mut chosen);
        }
        ui.add_space(6.0);
        swatch_row(ui, &VIVID.map(Colour::from_rgb), current, t, &mut chosen);
        if !self.recent.is_empty() {
            ui.add_space(8.0);
            ui.colored_label(t.text_dim, egui::RichText::new(tx("recent")).font(theme::ui_font(11.5)));
            ui.add_space(2.0);
            swatch_row(ui, &self.recent, current, t, &mut chosen);
        }
        chosen
    }

    fn custom(&mut self, ui: &mut Ui, t: &Tokens) -> Option<Colour> {
        self.square(ui);
        ui.add_space(8.0);
        self.hue_bar(ui);
        ui.add_space(6.0);
        self.alpha_bar(ui, t);
        ui.add_space(10.0);
        let mut apply = false;
        // The ribbon's fields have an edge; egui's would melt into the menu.
        let widgets = &mut ui.visuals_mut().widgets;
        for look in [&mut widgets.inactive, &mut widgets.hovered] {
            look.bg_stroke = Stroke::new(1.0, t.input_border);
            look.weak_bg_fill = t.input_bg;
            look.bg_fill = t.input_bg;
        }
        ui.visuals_mut().extreme_bg_color = t.input_bg;
        ui.allocate_ui_with_layout(vec2(WIDTH, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(44.0, 22.0), Sense::hover());
            checkers(ui, rect);
            ui.painter().rect_filled(rect, 0.0, self.current().color32());
            ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, t.input_border), StrokeKind::Inside);
            ui.colored_label(t.text_dim, tx("colour-hex"));
            let field = ui
                .add(egui::TextEdit::singleline(&mut self.hex).desired_width(ui.available_width()).char_limit(9).font(egui::FontId::monospace(12.5)));
            if field.changed()
                && let Some(colour) = Colour::parse(&self.hex)
            {
                let typed = self.hex.clone();
                self.set(colour);
                self.hex = typed;
            }
            if field.lost_focus() {
                apply = ui.input(|i| i.key_pressed(egui::Key::Enter)) && Colour::parse(&self.hex).is_some();
                self.hex = self.current().hex();
            }
        });
        ui.add_space(6.0);
        ui.allocate_ui_with_layout(vec2(WIDTH, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let mut colour = self.current();
            let mut changed = false;
            for (name, channel) in [("R", &mut colour.r), ("G", &mut colour.g), ("B", &mut colour.b)] {
                ui.colored_label(t.text_dim, name);
                changed |= ui.add(egui::DragValue::new(channel).range(0..=255).speed(1.0)).changed();
            }
            if changed {
                self.set(colour);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut percent = (f32::from(self.alpha) / 255.0 * 100.0).round();
                let opacity = ui.add(egui::DragValue::new(&mut percent).range(0.0..=100.0).speed(1.0).suffix(" %")).on_hover_text(tx("opacity"));
                if opacity.changed() {
                    self.alpha = (percent / 100.0 * 255.0).round() as u8;
                    self.hex = self.current().hex();
                }
            });
        });
        ui.add_space(10.0);
        let button = egui::Button::new(egui::RichText::new(tx("apply")).color(t.on_accent)).fill(t.accent).stroke(Stroke::NONE);
        if ui.add_sized(vec2(WIDTH, 26.0), button).clicked() {
            apply = true;
        }
        apply.then(|| self.current())
    }

    /// Saturation across and value down, for the hue chosen.
    fn square(&mut self, ui: &mut Ui) {
        const STEPS: (usize, usize) = (24, 16);
        let (rect, resp) = ui.allocate_exact_size(vec2(WIDTH, 148.0), Sense::click_and_drag());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, tx("custom-colour")));
        if let Some(at) = resp.interact_pointer_pos() {
            self.saturation = ((at.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            self.value = 1.0 - ((at.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            self.hex = self.current().hex();
        }
        let mut mesh = Mesh::default();
        for row in 0..=STEPS.1 {
            for column in 0..=STEPS.0 {
                let (s, down) = (column as f32 / STEPS.0 as f32, row as f32 / STEPS.1 as f32);
                let at = pos2(rect.left() + s * rect.width(), rect.top() + down * rect.height());
                mesh.colored_vertex(at, Colour::from_hsv(self.hue, s, 1.0 - down, 255).color32());
            }
        }
        let across = STEPS.0 as u32 + 1;
        for row in 0..STEPS.1 as u32 {
            for column in 0..STEPS.0 as u32 {
                let corner = row * across + column;
                mesh.add_triangle(corner, corner + 1, corner + across);
                mesh.add_triangle(corner + 1, corner + across, corner + across + 1);
            }
        }
        ui.painter().add(Shape::mesh(mesh));
        handle(ui, pos2(rect.left() + self.saturation * rect.width(), rect.top() + (1.0 - self.value) * rect.height()));
    }

    fn hue_bar(&mut self, ui: &mut Ui) {
        const STEPS: usize = 36;
        let (rect, resp) = ui.allocate_exact_size(vec2(WIDTH, 12.0), Sense::click_and_drag());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, tx("colour-hue")));
        let resp = resp.on_hover_text(tx("colour-hue"));
        if let Some(at) = resp.interact_pointer_pos() {
            self.hue = ((at.x - rect.left()) / rect.width()).clamp(0.0, 0.9999);
            self.hex = self.current().hex();
        }
        let colours: Vec<Color32> = (0..=STEPS).map(|i| Colour::from_hsv(i as f32 / STEPS as f32, 1.0, 1.0, 255).color32()).collect();
        gradient(ui, rect, &colours);
        handle(ui, pos2(rect.left() + self.hue * rect.width(), rect.center().y));
    }

    fn alpha_bar(&mut self, ui: &mut Ui, t: &Tokens) {
        let (rect, resp) = ui.allocate_exact_size(vec2(WIDTH, 12.0), Sense::click_and_drag());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, tx("opacity")));
        let resp = resp.on_hover_text(tx("opacity"));
        if let Some(at) = resp.interact_pointer_pos() {
            self.alpha = (((at.x - rect.left()) / rect.width()).clamp(0.0, 1.0) * 255.0).round() as u8;
            self.hex = self.current().hex();
        }
        checkers(ui, rect);
        let solid = Colour { a: 255, ..self.current() };
        gradient(ui, rect, &[Colour { a: 0, ..solid }.color32(), solid.color32()]);
        ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, t.input_border), StrokeKind::Inside);
        handle(ui, pos2(rect.left() + f32::from(self.alpha) / 255.0 * rect.width(), rect.center().y));
    }
}

fn swatch_row(ui: &mut Ui, colours: &[Colour], current: Colour, t: &Tokens, chosen: &mut Option<Colour>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2((WIDTH - COLUMNS as f32 * SWATCH) / (COLUMNS - 1) as f32, 2.0);
        for colour in colours {
            if swatch(ui, *colour, *colour == current, t).clicked() {
                *chosen = Some(*colour);
            }
        }
    });
}

fn swatch(ui: &mut Ui, colour: Colour, on: bool, t: &Tokens) -> Response {
    let hex = colour.hex();
    let (rect, resp) = ui.allocate_exact_size(vec2(SWATCH, SWATCH), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &hex));
    if colour.a < 255 {
        checkers(ui, rect);
    }
    ui.painter().rect_filled(rect, 2.0, colour.color32());
    // The light ones must not melt into the menu.
    ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, Color32::from_black_alpha(36)), StrokeKind::Inside);
    if resp.hovered() || on {
        ui.painter().rect_stroke(rect.expand(1.5), 3.0, Stroke::new(1.5, if on { t.accent } else { t.text }), StrokeKind::Outside);
    }
    resp.on_hover_text(hex)
}

/// A band through `colours`, left to right.
fn gradient(ui: &Ui, rect: Rect, colours: &[Color32]) {
    let mut mesh = Mesh::default();
    let last = colours.len().saturating_sub(1).max(1) as f32;
    for (i, colour) in colours.iter().enumerate() {
        let x = rect.left() + i as f32 / last * rect.width();
        mesh.colored_vertex(pos2(x, rect.top()), *colour);
        mesh.colored_vertex(pos2(x, rect.bottom()), *colour);
    }
    for i in 0..colours.len().saturating_sub(1) as u32 {
        mesh.add_triangle(2 * i, 2 * i + 1, 2 * i + 2);
        mesh.add_triangle(2 * i + 1, 2 * i + 2, 2 * i + 3);
    }
    ui.painter().add(Shape::mesh(mesh));
}

/// What shows through a colour that isn't fully opaque.
fn checkers(ui: &Ui, rect: Rect) {
    const CELL: f32 = 5.0;
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    let (columns, rows) = ((rect.width() / CELL).ceil() as u32, (rect.height() / CELL).ceil() as u32);
    for row in 0..rows {
        for column in (row % 2..columns).step_by(2) {
            let at = rect.min + vec2(column as f32 * CELL, row as f32 * CELL);
            painter.rect_filled(Rect::from_min_size(at, vec2(CELL, CELL)), 0.0, Color32::from_gray(205));
        }
    }
}

/// Where a colour stands: light and dark together, to show on any colour.
fn handle(ui: &Ui, at: egui::Pos2) {
    ui.painter().circle_stroke(at, 6.0, Stroke::new(1.0, Color32::from_black_alpha(140)));
    ui.painter().circle_stroke(at, 5.0, Stroke::new(2.0, Color32::WHITE));
}

/// A tool of the ribbon that opens the picker, with a bar under its icon in the colour last used.
pub fn tool(ui: &mut Ui, icon: crate::icons::Icon, tip: &str, last: Option<Colour>, picker: &mut Picker) -> Option<Colour> {
    let resp = crate::widgets::tool_with_arrow(ui, icon, tip);
    if let Some(colour) = last {
        let bar = Rect::from_center_size(pos2(resp.rect.left() + 12.0, resp.rect.bottom() - 2.5), vec2(14.0, 3.0));
        ui.painter().rect_filled(bar, 0.0, colour.color32());
    }
    let mut chosen = None;
    egui::Popup::menu(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        chosen = picker.show(ui);
        if chosen.is_some() {
            ui.close();
        }
    });
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rgb(r: u8, g: u8, b: u8) -> Colour {
        Colour { r, g, b, a: 255 }
    }

    #[test]
    fn codes_are_read_in_every_form() {
        assert_eq!(Colour::parse("#a0522d"), Some(rgb(0xA0, 0x52, 0x2D)));
        assert_eq!(Colour::parse(" A0522D "), Some(rgb(0xA0, 0x52, 0x2D)));
        assert_eq!(Colour::parse("#f80"), Some(rgb(0xFF, 0x88, 0x00)));
        assert_eq!(Colour::parse("#f808"), Some(Colour { r: 0xFF, g: 0x88, b: 0, a: 0x88 }));
        assert_eq!(Colour::parse("#a0522d80"), Some(Colour { r: 0xA0, g: 0x52, b: 0x2D, a: 0x80 }));
    }

    #[test]
    fn what_is_not_a_code_is_refused() {
        for text in ["", "#", "#12", "#12345", "#1234567", "#123456789", "#gggggg", "#ééé", "#éa", "+1+2+3", "é1234", "#+1+2+3+4"] {
            assert_eq!(Colour::parse(text), None, "{text}");
        }
    }

    #[test]
    fn codes_are_written_for_typst() {
        assert_eq!(rgb(0xA0, 0x52, 0x2D).hex(), "#a0522d");
        assert_eq!(rgb(1, 2, 3).typst(), "rgb(\"#010203\")");
        assert_eq!(Colour { a: 128, ..rgb(1, 2, 3) }.typst(), "rgb(\"#01020380\")");
    }

    #[test]
    fn every_colour_comes_back_from_hsv() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let colour = rgb(r, g, b);
                    let (h, s, v) = colour.hsv();
                    assert_eq!(Colour::from_hsv(h, s, v, 255), colour);
                }
            }
        }
    }

    #[test]
    fn wild_numbers_still_make_a_colour() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -3.0, 7.0] {
            let _ = Colour::from_hsv(value, value, value, 255);
        }
        assert_eq!(Colour::from_hsv(1.0, 1.0, 1.0, 255), rgb(255, 0, 0), "the hue wraps round");
    }

    #[test]
    fn tints_go_towards_white_and_shades_towards_black() {
        assert_eq!(rgb(100, 100, 100).tinted(1.0), rgb(255, 255, 255));
        assert_eq!(rgb(100, 100, 100).tinted(-1.0), rgb(0, 0, 0));
        assert_eq!(rgb(100, 100, 100).tinted(0.0), rgb(100, 100, 100));
    }

    #[test]
    fn a_saved_list_is_hostile_input() {
        let many: String = (0..50).map(|i| format!("#0000{i:02x}\n")).collect();
        assert_eq!(parse_recent(&many).len(), MAX_RECENT);
        assert_eq!(parse_recent("lixo\n#ff0000\n\n#FF0000\n#é\n"), [rgb(255, 0, 0)]);
    }

    #[test]
    fn black_keeps_the_hue_it_was_given() {
        let mut picker = Picker { hue: 0.5, saturation: 0.7, value: 1.0, alpha: 255, hex: String::new(), recent: Vec::new(), tab: Tab::Palette };
        picker.set(rgb(0, 0, 0));
        assert_eq!((picker.hue, picker.saturation, picker.value), (0.5, 0.7, 0.0));
        picker.set(rgb(128, 128, 128));
        assert_eq!(picker.hue, 0.5);
        assert_eq!(picker.saturation, 0.0);
    }
}
