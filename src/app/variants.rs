//! The window of layout variants: the document set in a few other ways, side by side on its own pages.
//! Nothing in the document changes until one is chosen, and then as one undo step.

use egui::{RichText, TextureHandle, TextureOptions, Ui, vec2};

use super::App;
use super::docs::hash_of;
use crate::compiler::{Job, VariantView};
use crate::i18n::tx;
use crate::theme::{self, Tokens};
use crate::tr;
use crate::variants::PRESETS;

const TILE_WIDTH: f32 = 190.0;
/// How long the text rests before the variants are made again: they're six compiles.
const REST_SECONDS: f64 = 0.8;

/// A variant as far as it's come: nothing while it's being made, its pages and whether it looks like the
/// document now, or why it failed.
pub type VariantState = Option<Result<(usize, bool), String>>;

struct Tile {
    pages: usize,
    same_as_now: bool,
    /// Its first pages.
    textures: Vec<TextureHandle>,
}

pub(super) struct VariantsDialog {
    request: u64,
    /// What the variants on show were made from.
    made_from: u64,
    tiles: Vec<Option<Result<Tile, String>>>,
    /// The page on show in all of them, from zero.
    page: usize,
}

impl App {
    /// Opens the window on the document as it is now.
    pub fn open_layout_variants(&mut self) {
        let mut dialog = VariantsDialog { request: 0, made_from: 0, tiles: Vec::new(), page: 0 };
        self.ask_for_variants(&mut dialog);
        self.variants_dialog = Some(dialog);
    }

    fn ask_for_variants(&mut self, dialog: &mut VariantsDialog) {
        self.variants_asked += 1;
        dialog.request = self.variants_asked;
        dialog.made_from = hash_of(&self.text);
        dialog.tiles = PRESETS.iter().map(|_| None).collect();
        let path = self.session.as_ref().and_then(crate::collab::Session::guest_path).or_else(|| self.path.clone());
        self.compiler.send(Job::Variants { request: dialog.request, text: self.text.clone(), path, variants: PRESETS.to_vec() });
    }

    pub(super) fn receive_variant(&mut self, ctx: &egui::Context, request: u64, index: usize, result: Result<VariantView, String>) {
        let Some(dialog) = self.variants_dialog.as_mut().filter(|d| d.request == request) else { return };
        let Some(slot) = dialog.tiles.get_mut(index) else { return };
        *slot = Some(result.map(|view| {
            Tile {
                pages: view.pages,
                same_as_now: view.same_as_now,
                textures: view
                    .thumbnails
                    .iter()
                    .enumerate()
                    .map(|(page, pixels)| {
                        let image = egui::ColorImage::from_rgba_unmultiplied(pixels.size, &pixels.rgba);
                        ctx.load_texture(format!("variant-{request}-{index}-{page}"), image, TextureOptions::LINEAR)
                    })
                    .collect(),
            }
        }));
    }

    /// Each variant so far, while the window is open.
    pub fn layout_variants(&self) -> Option<Vec<VariantState>> {
        let dialog = self.variants_dialog.as_ref()?;
        Some(dialog.tiles.iter().map(|tile| tile.as_ref().map(|r| r.as_ref().map(|t| (t.pages, t.same_as_now)).map_err(Clone::clone))).collect())
    }

    /// Writes this variant in the document and closes the window, as its button does.
    pub fn choose_layout_variant(&mut self, ctx: &egui::Context, index: usize) {
        let Some(variant) = PRESETS.get(index).copied() else { return };
        self.close_variants();
        self.edit(ctx, |text, sel| variant.apply(text, sel));
    }

    fn close_variants(&mut self) {
        self.variants_dialog = None;
        self.compiler.send(Job::CancelVariants);
    }

    pub(super) fn variants_window(&mut self, ctx: &egui::Context, t: &Tokens) {
        let Some(mut dialog) = self.variants_dialog.take() else { return };
        // The writer can go on typing with it open: the variants follow once the text rests.
        let now = ctx.input(|i| i.time);
        if now - self.changed_at >= REST_SECONDS && hash_of(&self.text) != dialog.made_from {
            self.ask_for_variants(&mut dialog);
        }
        let (mut chosen, mut close) = (None, false);
        let height = (ctx.content_rect().height() * 0.85).max(200.0);
        egui::Window::new(tx("layout-variants")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(
            ctx,
            |ui| {
                egui::ScrollArea::vertical().max_height(height - 110.0).show(ui, |ui| {
                    chosen = Self::variant_tiles(ui, &dialog, t);
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    close = ui.button(tx("close")).clicked();
                    Self::page_stepper(ui, &mut dialog);
                });
            },
        );
        self.variants_dialog = Some(dialog);
        if let Some(index) = chosen {
            self.choose_layout_variant(ctx, index);
        } else if close {
            self.close_variants();
        }
    }

    fn page_stepper(ui: &mut Ui, dialog: &mut VariantsDialog) {
        let shown = dialog.tiles.iter().flatten().flatten().map(|tile| tile.textures.len()).max().unwrap_or(0);
        if shown < 2 {
            dialog.page = 0;
            return;
        }
        ui.add_space(12.0);
        if ui.add_enabled(dialog.page > 0, egui::Button::new("‹")).on_hover_text(tx("previous-page")).clicked() {
            dialog.page -= 1;
        }
        ui.label(tr!("page-number", n = dialog.page + 1));
        if ui.add_enabled(dialog.page + 1 < shown, egui::Button::new("›")).on_hover_text(tx("next-page")).clicked() {
            dialog.page += 1;
        }
    }

    fn variant_tiles(ui: &mut Ui, dialog: &VariantsDialog, t: &Tokens) -> Option<usize> {
        let current = dialog.tiles.first().and_then(|tile| tile.as_ref()).and_then(|r| r.as_ref().ok()).map(|tile| tile.pages);
        let mut chosen = None;
        egui::Grid::new("variants").num_columns(3).spacing(vec2(14.0, 14.0)).show(ui, |ui| {
            for (index, (variant, tile)) in PRESETS.iter().zip(&dialog.tiles).enumerate() {
                ui.vertical(|ui| {
                    ui.set_width(TILE_WIDTH);
                    ui.label(RichText::new(tx(variant.name)).font(theme::ui_bold(13.0)));
                    let height = TILE_WIDTH * std::f32::consts::SQRT_2;
                    match tile {
                        None => {
                            Self::empty_page(ui, height, tx("compiling"), t);
                        }
                        Some(Err(message)) => {
                            ui.add(egui::Label::new(RichText::new(message).small().color(t.danger)).wrap());
                        }
                        Some(Ok(tile)) => {
                            match tile.textures.get(dialog.page) {
                                Some(texture) => {
                                    let size = texture.size_vec2();
                                    let shown = ui.add(egui::Image::new((texture.id(), vec2(TILE_WIDTH, TILE_WIDTH * size.y / size.x.max(1.0)))));
                                    // White paper on a white window: the edge says where it ends.
                                    ui.painter().rect_stroke(shown.rect, 0.0, egui::Stroke::new(1.0, t.separator), egui::StrokeKind::Outside);
                                }
                                None => Self::empty_page(ui, height, tx("no-such-page"), t),
                            }
                            let pages = tr!("pages-count", n = tile.pages);
                            let difference = current.map_or(0, |now| tile.pages.cast_signed() - now.cast_signed());
                            let words = if difference == 0 { pages } else { format!("{pages} ({difference:+})") };
                            ui.label(RichText::new(words).font(theme::ui_font(12.0)).color(t.text_dim));
                            if index > 0 {
                                if tile.same_as_now {
                                    ui.colored_label(t.text_dim, tx("looks-the-same-as-now"));
                                } else if ui.button(tx("use-this-layout")).clicked() {
                                    chosen = Some(index);
                                }
                            }
                        }
                    }
                });
                if index % 3 == 2 {
                    ui.end_row();
                }
            }
        });
        chosen
    }

    fn empty_page(ui: &mut Ui, height: f32, words: &str, t: &Tokens) {
        let (rect, _) = ui.allocate_exact_size(vec2(TILE_WIDTH, height), egui::Sense::hover());
        ui.painter().rect_filled(rect, 1.0, t.hover);
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, words, theme::ui_font(12.0), t.text_dim);
    }
}
