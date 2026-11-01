//! The File view: a full-window page in place of everything else until Esc or the back arrow,
//! in the layout of MK Matrix's Backstage (MIT OR Apache-2.0; see NOTICE).

use egui::{Align2, Color32, Key, Rect, RichText, Sense, Ui, pos2, vec2};

use super::{App, Pending};
use crate::icons::{self, Icon};
use crate::recent;
use crate::splash;
use crate::templates::TEMPLATES;
use crate::theme::{self, Tokens};

const NAV_W: f32 = 196.0;
const CONTENT_W: f32 = 760.0;
const MODIFIED_W: f32 = 150.0;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Page {
    Home,
    New,
    Open,
    Info,
    SaveAs,
    Export,
    Options,
}

impl Page {
    fn label(self) -> &'static str {
        match self {
            Page::Home => "Início",
            Page::New => "Novo",
            Page::Open => "Abrir",
            Page::Info => "Informações",
            Page::SaveAs => "Salvar como",
            Page::Export => "Exportar",
            Page::Options => "Opções",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Page::Home => Icon::Home,
            Page::New => Icon::NewFile,
            Page::Open => Icon::OpenFolder,
            Page::Info => Icon::Info,
            Page::SaveAs => Icon::Save,
            Page::Export => Icon::Share,
            Page::Options => Icon::Settings,
        }
    }
}

fn nav_item(ui: &mut Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let sel = Color32::from_white_alpha(26);
    let (rect, resp) = ui.allocate_exact_size(vec2(NAV_W, 38.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, label));
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 0.0, sel);
        p.rect_filled(Rect::from_min_size(rect.min, vec2(4.0, rect.height())), 0.0, splash::BRAND);
    } else if resp.hovered() {
        p.rect_filled(rect, 0.0, sel.gamma_multiply(0.55));
    }
    icons::paint_mono(p, Rect::from_center_size(pos2(rect.left() + 30.0, rect.center().y), vec2(17.0, 17.0)), icon, Color32::WHITE);
    p.text(pos2(rect.left() + 50.0, rect.center().y), Align2::LEFT_CENTER, label, theme::ui_font(14.0), Color32::WHITE);
    resp
}

fn section(ui: &mut Ui, title: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(6.0);
    ui.label(RichText::new(title).font(theme::ui_bold(15.0)).color(t.text));
    ui.add_space(8.0);
}

fn dim(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(text).font(theme::ui_font(12.5)).color(t.text_dim));
}

/// A new-document tile: a small page drawn in code over its name. `kind` varies the lines.
fn tile(ui: &mut Ui, label: &str, kind: Option<usize>) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(150.0, 140.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect.expand(5.0), 6.0, t.hover);
    }
    let thumb = Rect::from_min_size(rect.min, vec2(rect.width(), 104.0));
    p.rect_filled(thumb, 4.0, t.grid_bg);
    let page = Rect::from_center_size(thumb.center(), vec2(66.0, 90.0));
    p.rect_filled(page, 0.0, Color32::WHITE);
    if let Some(i) = kind {
        let inner = page.shrink(8.0);
        let ink = Color32::from_gray(150);
        // A title in the accent, and lines of text whose lengths differ from one template to the next.
        p.rect_filled(Rect::from_center_size(pos2(inner.center().x, inner.top() + 3.0), vec2(30.0, 3.0)), 0.0, t.accent);
        for r in 0..8_usize {
            let share = 0.55 + ((r * (i + 3) * 7 + i * 5) % 10) as f32 / 22.0;
            let y = inner.top() + 12.0 + r as f32 * 8.0;
            p.rect_filled(Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width() * share.min(1.0), 2.0)), 0.0, ink);
        }
    }
    let galley = p.layout(label.to_string(), theme::ui_font(12.5), t.text, rect.width());
    p.galley(pos2(rect.left(), thumb.bottom() + 6.0), galley, t.text);
    resp
}

fn action_row(ui: &mut Ui, icon: Icon, title: &str, detail: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let width = ui.available_width().min(CONTENT_W);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 56.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 4.0, t.hover);
    }
    icons::paint_mono(p, Rect::from_center_size(pos2(rect.left() + 24.0, rect.center().y), vec2(24.0, 24.0)), icon, t.text);
    p.text(pos2(rect.left() + 52.0, rect.center().y - 9.0), Align2::LEFT_CENTER, title, theme::ui_bold(13.5), t.text);
    p.text(pos2(rect.left() + 52.0, rect.center().y + 10.0), Align2::LEFT_CENTER, detail, theme::ui_font(12.0), t.text_dim);
    resp
}

fn square_button(ui: &mut Ui, icon: Icon, label: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(96.0, 88.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let p = ui.painter();
    p.rect_filled(rect, 6.0, if resp.hovered() { t.hover } else { t.ribbon });
    icons::paint_mono(p, Rect::from_center_size(pos2(rect.center().x, rect.top() + 32.0), vec2(30.0, 30.0)), icon, t.text);
    let galley = p.layout(label.to_string(), theme::ui_font(12.0), t.text, rect.width() - 8.0);
    p.galley(pos2(rect.center().x - galley.size().x / 2.0, rect.top() + 56.0), galley, t.text);
    resp
}

impl App {
    pub(super) fn open_backstage(&mut self, page: Page) {
        self.backstage = Some(page);
        self.find = None;
        self.ask_modified();
    }

    /// Asks when the recent documents last changed, on a thread of its own: a folder on a slow disk
    /// or a network mount must not hold up drawing.
    fn ask_modified(&self) {
        let paths = self.recent.clone();
        let found = std::sync::Arc::clone(&self.recent_ages);
        std::thread::spawn(move || {
            for path in paths {
                let age = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|m| m.elapsed().ok()).map(|e| e.as_secs());
                if let Ok(mut found) = found.lock() {
                    found.insert(path, age);
                }
            }
        });
    }

    pub(super) fn backstage_ui(&mut self, ui: &mut Ui) {
        let Some(mut page) = self.backstage else { return };
        let ctx = ui.ctx().clone();
        let t = Tokens::get(&ctx);
        let mut close = false;
        egui::Panel::left("backstage_nav")
            .exact_size(NAV_W)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(splash::INK).inner_margin(egui::Margin { left: 0, right: 0, top: 10, bottom: 10 }))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                close |= nav_item(ui, Icon::Back, "Voltar", false).clicked();
                let (rect, _) = ui.allocate_exact_size(vec2(NAV_W, 58.0), Sense::hover());
                splash::pearl(ui.painter(), pos2(rect.left() + 32.0, rect.center().y), 13.0);
                ui.painter().text(pos2(rect.left() + 54.0, rect.center().y), Align2::LEFT_CENTER, "Pearl", theme::ui_font(18.0), Color32::WHITE);
                for item in [Page::Home, Page::New, Page::Open] {
                    if nav_item(ui, item.icon(), item.label(), page == item).clicked() {
                        page = item;
                    }
                }
                ui.add_space(17.0);
                if nav_item(ui, Page::Info.icon(), Page::Info.label(), page == Page::Info).clicked() {
                    page = Page::Info;
                }
                if nav_item(ui, Icon::Clock, "Histórico de versões", false).clicked() {
                    self.side = Some(super::Side::Versions);
                    close = true;
                }
                if nav_item(ui, Icon::Save, "Salvar", false).clicked() {
                    if self.path.is_some() {
                        close |= self.save(false);
                    } else {
                        page = Page::SaveAs;
                    }
                }
                for item in [Page::SaveAs, Page::Export] {
                    if nav_item(ui, item.icon(), item.label(), page == item).clicked() {
                        page = item;
                    }
                }
                if nav_item(ui, Icon::Close, "Fechar", false).on_hover_text("Fecha o documento e começa um em branco").clicked() {
                    self.request(&ctx, Pending::New);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    if nav_item(ui, Page::Options.icon(), Page::Options.label(), page == Page::Options).clicked() {
                        page = Page::Options;
                    }
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.ribbon)).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::Frame::NONE.inner_margin(egui::Margin { left: 44, right: 32, top: 26, bottom: 32 }).show(ui, |ui| {
                    ui.set_max_width(CONTENT_W);
                    ui.label(RichText::new(page.label()).font(theme::ui_font(28.0)).color(t.text));
                    ui.add_space(18.0);
                    match page {
                        Page::Home => {
                            section(ui, "Novo");
                            self.new_tiles(ui, &ctx);
                            ui.add_space(18.0);
                            section(ui, "Recentes");
                            self.recent_list(ui, &ctx, 8);
                        }
                        Page::New => self.new_tiles(ui, &ctx),
                        Page::Open => {
                            if square_button(ui, Icon::OpenFolder, "Procurar…").clicked() {
                                self.request(&ctx, Pending::Open);
                            }
                            ui.add_space(18.0);
                            section(ui, "Recentes");
                            self.recent_list(ui, &ctx, usize::MAX);
                        }
                        Page::Info => self.pane_details(ui, &t),
                        Page::SaveAs => {
                            dim(ui, "Imagens e bibliografias são lidas da pasta onde o documento está salvo.");
                            ui.add_space(12.0);
                            if square_button(ui, Icon::Folder, "Procurar…").clicked() && self.save(true) {
                                close = true;
                            }
                        }
                        Page::Export => {
                            if action_row(ui, Icon::Print, "Exportar como PDF", "As páginas, prontas para imprimir ou enviar").clicked() {
                                self.export_pdf();
                            }
                            if action_row(ui, Icon::Save, "Salvar uma cópia", "O código Typst em outro arquivo, sem trocar o documento aberto").clicked() {
                                self.save_copy();
                            }
                        }
                        Page::Options => self.options(ui, &ctx),
                    }
                });
            });
        });
        let free = self.confirm.is_none() && !egui::Popup::is_any_open(&ctx);
        if free && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            close = true;
        }
        // An action on a page may already have closed the view (a template, a recent document).
        if self.backstage.is_some() {
            self.backstage = Some(page);
        }
        if close {
            self.backstage = None;
            self.want_focus = true;
        }
    }

    fn new_tiles(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(18.0, 14.0);
            if tile(ui, "Documento em branco", None).clicked() {
                self.request(ctx, Pending::New);
            }
            for (i, template) in TEMPLATES.iter().enumerate() {
                if tile(ui, template.name, Some(i)).clicked() {
                    self.request(ctx, Pending::Template(i));
                }
            }
        });
    }

    fn recent_list(&mut self, ui: &mut Ui, ctx: &egui::Context, limit: usize) {
        let t = Tokens::get(ctx);
        if self.recent.is_empty() {
            dim(ui, "Nenhum documento recente ainda.");
            return;
        }
        let width = ui.available_width().min(CONTENT_W);
        let (head, _) = ui.allocate_exact_size(vec2(width, 20.0), Sense::hover());
        ui.painter().text(pos2(head.right() - MODIFIED_W, head.center().y), Align2::LEFT_CENTER, "Modificado", theme::ui_font(11.5), t.text_dim);
        let ages = self.recent_ages.lock().map(|a| a.clone()).unwrap_or_default();
        let mut chosen = None;
        for (i, path) in self.recent.iter().take(limit).enumerate() {
            let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
            let folder = path.parent().map_or_else(String::new, |p| p.display().to_string());
            let (rect, resp) = ui.allocate_exact_size(vec2(width, 44.0), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
            if resp.hovered() {
                ui.painter().rect_filled(rect, 4.0, t.hover);
            }
            icons::paint_mono(ui.painter(), Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), vec2(20.0, 20.0)), Icon::FileName, t.text);
            let text_right = rect.right() - MODIFIED_W - 8.0;
            let clip = ui.painter().with_clip_rect(Rect::from_min_max(rect.min, pos2(text_right, rect.bottom())));
            clip.text(pos2(rect.left() + 44.0, rect.center().y - 9.0), Align2::LEFT_CENTER, &name, theme::ui_bold(13.0), t.text);
            clip.text(pos2(rect.left() + 44.0, rect.center().y + 9.0), Align2::LEFT_CENTER, &folder, theme::ui_font(11.5), t.text_dim);
            let when = match ages.get(path) {
                Some(Some(seconds)) => recent::age(*seconds),
                Some(None) => "Não encontrado".to_string(),
                None => String::new(),
            };
            ui.painter().text(pos2(rect.right() - MODIFIED_W, rect.center().y), Align2::LEFT_CENTER, when, theme::ui_font(12.0), t.text_dim);
            if resp.on_hover_text(path.display().to_string()).clicked() {
                chosen = Some(i);
            }
        }
        if let Some(i) = chosen {
            self.request(ctx, Pending::Recent(i));
        }
    }

    fn options(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        section(ui, "Salvamento");
        ui.checkbox(&mut self.autosave, "Salvamento automático");
        dim(ui, "Grava no arquivo cerca de um segundo depois de cada alteração, em documentos que já têm um arquivo.");
        ui.add_space(14.0);
        section(ui, "Aparência");
        for choice in theme::ThemeChoice::ALL {
            if ui.radio(self.theme == choice, choice.label()).clicked() {
                self.choose_theme(ctx, choice);
            }
        }
        ui.add_space(14.0);
        section(ui, "Editor");
        ui.checkbox(&mut self.line_numbers, "Mostrar os números de linha");
    }
}
