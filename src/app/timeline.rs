//! The timeline of a document's writing: a slider over its versions, their pages, and what each says
//! that the document no longer does. It only looks: the one thing that changes the document is bringing
//! a passage back, as one undo step. Versions are read and compiled off this thread, one at a time.

use std::collections::{HashMap, HashSet};

use egui::{RichText, TextureHandle, TextureOptions, Ui, vec2};

use super::{App, Layout};
use crate::compiler::{Job, TimelineView};
use crate::history::{self, Store, Version};
use crate::i18n::tx;
use crate::theme::{self, Tokens};
use crate::timeline::{self, Passage};
use crate::tr;

const PAGE_WIDTH: f32 = 220.0;
const PASSAGES_WIDTH: f32 = 300.0;
/// How long the slider rests on a version before it's compiled: dragging passes over many.
const REST_SECONDS: f64 = 0.12;
/// Versions kept to go back to at once; a version's pictures are large.
const KEPT_VERSIONS: usize = 3;

struct Shown {
    text: String,
    pages: Result<(usize, Vec<TextureHandle>), String>,
}

pub(super) struct Timeline {
    store: Store,
    /// Oldest first, as the slider goes.
    versions: Vec<Version>,
    index: usize,
    /// Which way it last moved, to go on past a damaged version.
    forward: bool,
    moved_at: f64,
    in_flight: Option<u64>,
    shown: HashMap<u64, Shown>,
    damaged: HashSet<u64>,
    /// The passages on show, and the version and the document they're for.
    lost: Option<(u64, u64, Vec<Passage>)>,
}

impl Timeline {
    fn current(&self) -> Option<&Version> {
        self.versions.get(self.index)
    }

    /// Moves past the versions known to be damaged, the way it was going, or else the other.
    fn skip_damaged(&mut self) {
        let good = |t: &Self, i: &usize| t.versions.get(*i).is_some_and(|v| !t.damaged.contains(&v.stamp));
        let ahead = (self.index..self.versions.len()).find(|i| good(self, i));
        let behind = (0..=self.index).rev().find(|i| good(self, i));
        let found = if self.forward { ahead.or(behind) } else { behind.or(ahead) };
        if let Some(found) = found {
            self.index = found;
        }
    }
}

impl App {
    /// Opens the timeline on the newest version.
    pub fn open_timeline(&mut self) {
        let Some(store) = self.store() else { return };
        let mut versions = store.list();
        versions.reverse();
        if versions.is_empty() {
            return;
        }
        if self.previewing {
            self.preview_version(false);
        }
        if self.layout == Layout::Source {
            self.layout = Layout::Both;
        }
        self.comparing = None;
        let index = versions.len() - 1;
        self.timeline = Some(Timeline {
            store,
            versions,
            index,
            forward: false,
            moved_at: f64::NEG_INFINITY,
            in_flight: None,
            shown: HashMap::new(),
            damaged: HashSet::new(),
            lost: None,
        });
    }

    pub fn close_timeline(&mut self) {
        self.timeline = None;
    }

    /// Moves the timeline to this version, oldest first, as its slider does.
    pub fn move_timeline(&mut self, index: usize) {
        if let Some(timeline) = &mut self.timeline {
            timeline.forward = index >= timeline.index;
            timeline.index = index.min(timeline.versions.len().saturating_sub(1));
            timeline.moved_at = f64::NEG_INFINITY;
        }
    }

    /// Where it is, how many versions there are, and how many were found damaged.
    pub fn timeline_position(&self) -> Option<(usize, usize, usize)> {
        let timeline = self.timeline.as_ref()?;
        Some((timeline.index, timeline.versions.len(), timeline.damaged.len()))
    }

    /// The passages on show, once the version on show has been read.
    pub fn timeline_passages(&self) -> Option<Vec<String>> {
        let timeline = self.timeline.as_ref()?;
        let (stamp, _, lost) = timeline.lost.as_ref()?;
        (timeline.current().map(|v| v.stamp) == Some(*stamp)).then(|| lost.iter().map(|p| p.text.clone()).collect())
    }

    /// Whether the version on show has been read and compiled, and how many pages it has if it compiles.
    pub fn timeline_pages(&self) -> Option<Option<usize>> {
        let timeline = self.timeline.as_ref()?;
        let shown = timeline.shown.get(&timeline.current()?.stamp)?;
        Some(shown.pages.as_ref().ok().map(|(pages, _)| *pages))
    }

    /// Puts a passage on show back in the document, as its button does.
    pub fn bring_back_passage(&mut self, ctx: &egui::Context, index: usize) {
        let passage = self.timeline.as_ref().and_then(|t| t.lost.as_ref()).and_then(|(_, _, lost)| lost.get(index)).cloned();
        if let Some(passage) = passage {
            self.edit(ctx, |text, sel| timeline::bring_back(text, sel, &passage));
        }
    }

    pub(super) fn receive_timeline(&mut self, ctx: &egui::Context, request: u64, stamp: u64, result: Option<TimelineView>) {
        let Some(timeline) = self.timeline.as_mut().filter(|t| t.in_flight == Some(request)) else { return };
        timeline.in_flight = None;
        let Some(view) = result else {
            timeline.damaged.insert(stamp);
            timeline.skip_damaged();
            return;
        };
        let pages = view.pages.map(|(count, pixels)| {
            let textures = pixels
                .iter()
                .enumerate()
                .map(|(page, p)| {
                    let image = egui::ColorImage::from_rgba_unmultiplied(p.size, &p.rgba);
                    ctx.load_texture(format!("timeline-{stamp}-{page}"), image, TextureOptions::LINEAR)
                })
                .collect();
            (count, textures)
        });
        let on_show = timeline.current().map(|v| v.stamp);
        if timeline.shown.len() >= KEPT_VERSIONS
            && let Some(old) = timeline.shown.keys().copied().find(|key| Some(*key) != on_show)
        {
            timeline.shown.remove(&old);
        }
        timeline.shown.insert(stamp, Shown { text: view.text, pages });
    }

    /// Asks for the version on show, when the slider has rested on it and nothing else is under way.
    fn ask_for_version(&mut self, now: f64) {
        let path = self.path.clone();
        self.variants_asked += 1;
        let request = self.variants_asked;
        let Some(timeline) = &mut self.timeline else { return };
        let Some(version) = timeline.current().cloned() else { return };
        if timeline.in_flight.is_some()
            || timeline.shown.contains_key(&version.stamp)
            || timeline.damaged.contains(&version.stamp)
            || now - timeline.moved_at < REST_SECONDS
        {
            return;
        }
        timeline.in_flight = Some(request);
        self.compiler.send(Job::TimelineVersion { request, store: timeline.store.clone(), version, path });
    }

    /// Keeps the passages on show true to the version and to the document, as it's written in.
    fn refresh_passages(&mut self) {
        let now_text = egui::epaint::util::hash(&self.text);
        let text = &self.text;
        let Some(timeline) = &mut self.timeline else { return };
        let Some(stamp) = timeline.current().map(|v| v.stamp) else { return };
        let fresh = timeline.lost.as_ref().is_some_and(|(s, t, _)| *s == stamp && *t == now_text);
        if !fresh && let Some(shown) = timeline.shown.get(&stamp) {
            timeline.lost = Some((stamp, now_text, timeline::lost_passages(&shown.text, text)));
        }
    }

    pub(super) fn timeline_view(&mut self, ui: &mut Ui, t: &Tokens) {
        let now = ui.input(|i| i.time);
        self.ask_for_version(now);
        self.refresh_passages();
        if self.timeline.as_ref().is_some_and(|tl| {
            tl.in_flight.is_none() && tl.current().is_some_and(|v| !tl.shown.contains_key(&v.stamp) && !tl.damaged.contains(&v.stamp))
        }) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
        }
        if self.timeline_bar(ui, now, t) {
            self.timeline = None;
            return;
        }
        let mut bring = None;
        let Some(timeline) = &self.timeline else { return };
        let on_show = timeline.current().and_then(|v| timeline.shown.get(&v.stamp));
        ui.horizontal_top(|ui| {
            egui::Frame::new().inner_margin(8.0).show(ui, |ui| {
                ui.set_width(PASSAGES_WIDTH);
                bring = Self::timeline_passages_list(ui, timeline, t);
            });
            ui.separator();
            egui::Frame::new().inner_margin(8.0).show(ui, |ui| {
                match on_show {
                    Some(shown) => Self::timeline_pages_grid(ui, shown, t),
                    None => {
                        ui.colored_label(t.text_dim, tx("compiling"));
                    }
                };
            });
        });
        if let Some(index) = bring {
            let ctx = ui.ctx().clone();
            self.bring_back_passage(&ctx, index);
        }
    }

    /// The slider and what goes with it. Whether it asked to close.
    fn timeline_bar(&mut self, ui: &mut Ui, now: f64, t: &Tokens) -> bool {
        let Some(timeline) = &mut self.timeline else { return true };
        let mut close = false;
        egui::Frame::new().fill(t.ribbon).inner_margin(6.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let total = timeline.versions.len();
            ui.horizontal_wrapped(|ui| {
                if let Some(version) = timeline.current() {
                    ui.label(RichText::new(history::describe(version.stamp, history::now_ms())).strong().color(t.text));
                    ui.colored_label(t.text_dim, version.kind.label());
                }
                ui.colored_label(t.text_dim, tr!("timeline-position", n = timeline.index + 1, total = total));
                if let Some(Ok((pages, _))) =
                    timeline.current().and_then(|v| timeline.shown.get(&v.stamp)).map(|s| s.pages.as_ref().map(|(p, x)| (*p, x)))
                {
                    ui.colored_label(t.text_dim, tr!("pages-count", n = pages));
                }
                close = ui.button(tx("close")).clicked();
            });
            ui.horizontal(|ui| {
                let mut wanted = None;
                if ui.add_enabled(timeline.index > 0, egui::Button::new("‹")).on_hover_text(tx("timeline-older")).clicked() {
                    wanted = Some((timeline.index - 1, false));
                }
                ui.spacing_mut().slider_width = (ui.available_width() - 40.0).max(60.0);
                let mut index = timeline.index;
                if total > 1 && ui.add(egui::Slider::new(&mut index, 0..=total - 1).show_value(false)).changed() {
                    wanted = Some((index, index >= timeline.index));
                }
                if ui.add_enabled(timeline.index + 1 < total, egui::Button::new("›")).on_hover_text(tx("timeline-newer")).clicked() {
                    wanted = Some((timeline.index + 1, true));
                }
                if let Some((index, forward)) = wanted {
                    timeline.index = index;
                    timeline.forward = forward;
                    timeline.moved_at = now;
                    timeline.skip_damaged();
                }
            });
            if !timeline.damaged.is_empty() {
                ui.colored_label(t.warning_ink, tr!("timeline-damaged", n = timeline.damaged.len()));
            }
        });
        close
    }

    fn timeline_passages_list(ui: &mut Ui, timeline: &Timeline, t: &Tokens) -> Option<usize> {
        let mut bring = None;
        ui.label(RichText::new(tx("timeline-passages")).font(theme::ui_bold(12.5)));
        ui.add_space(4.0);
        let lost = timeline.lost.as_ref().filter(|(stamp, _, _)| timeline.current().map(|v| v.stamp) == Some(*stamp)).map(|(_, _, lost)| lost);
        match lost {
            None => {
                ui.colored_label(t.text_dim, tx("compiling"));
            }
            Some(lost) if lost.is_empty() => {
                ui.colored_label(t.text_dim, tx("timeline-nothing-lost"));
            }
            Some(lost) => {
                egui::ScrollArea::vertical().id_salt("timeline-passages").show(ui, |ui| {
                    for (index, passage) in lost.iter().enumerate() {
                        ui.push_id(index, |ui| {
                            let shown: String = passage.text.chars().take(240).collect();
                            ui.add(egui::Label::new(RichText::new(shown).font(egui::FontId::monospace(11.5))).wrap());
                            if ui.small_button(tx("timeline-bring-back")).clicked() {
                                bring = Some(index);
                            }
                        });
                        ui.add_space(8.0);
                    }
                });
            }
        }
        bring
    }

    fn timeline_pages_grid(ui: &mut Ui, shown: &Shown, t: &Tokens) {
        match &shown.pages {
            Err(error) => {
                ui.add(egui::Label::new(RichText::new(tr!("timeline-does-not-compile", error = error.clone())).color(t.danger)).wrap());
            }
            Ok((count, textures)) => {
                egui::ScrollArea::both().id_salt("timeline-pages").auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                        for texture in textures {
                            let size = texture.size_vec2();
                            let picture = ui.add(egui::Image::new((texture.id(), vec2(PAGE_WIDTH, PAGE_WIDTH * size.y / size.x.max(1.0)))));
                            ui.painter().rect_stroke(picture.rect, 0.0, egui::Stroke::new(1.0, t.separator), egui::StrokeKind::Outside);
                        }
                    });
                    if *count > textures.len() {
                        ui.colored_label(t.text_dim, tr!("timeline-more-pages", n = *count - textures.len()));
                    }
                });
            }
        }
    }
}
