//! Several carets in the editor, as in VS Code: Alt+click adds one, Shift+Alt+drag makes a box,
//! Ctrl+Alt or Shift+Alt with Up or Down adds one on the next line, and Esc goes back to one.
//! egui's editor has a single caret, which stays the main one here; while there are others, what
//! the keyboard does is taken before it sees it. See `carets` for what that does to the text.

use super::*;
use crate::carets::{Caret, Carets, Move};

/// The carets while there's more than one. They're places in one exact text: anything else
/// changing it, a peer in a session or an undo, ends them rather than leave them in the wrong place.
pub(super) struct MultiCaret {
    pub(super) carets: Carets,
    text_hash: u64,
    /// The main caret is being dragged, and follows the editor's own selection.
    pub(super) dragging: bool,
}

/// What a key does with several carets.
enum Action {
    Text(String),
    Paste(String),
    Copy,
    Cut,
    Backspace {
        word: bool,
    },
    Delete {
        word: bool,
    },
    Go {
        way: Move,
        extend: bool,
    },
    /// Back to one caret, with this event going on to the editor.
    Collapse(egui::Event),
}

fn action_of(event: &egui::Event) -> Option<Action> {
    match event {
        egui::Event::Text(text) if text != "\n" && text != "\r" => Some(Action::Text(text.clone())),
        egui::Event::Ime(egui::ImeEvent::Commit(text)) if !text.is_empty() => Some(Action::Text(text.clone())),
        egui::Event::Paste(text) => Some(Action::Paste(text.clone())),
        egui::Event::Copy => Some(Action::Copy),
        egui::Event::Cut => Some(Action::Cut),
        egui::Event::Key { key, pressed: true, modifiers, .. } => key_action(*key, *modifiers, event),
        _ => None,
    }
}

fn key_action(key: Key, modifiers: Modifiers, event: &egui::Event) -> Option<Action> {
    let word = if cfg!(target_os = "macos") { modifiers.alt } else { modifiers.command };
    let go = |way| Some(Action::Go { way, extend: modifiers.shift });
    match key {
        Key::Backspace => Some(Action::Backspace { word }),
        Key::Delete => Some(Action::Delete { word }),
        Key::Enter => Some(Action::Text("\n".into())),
        Key::Tab if !modifiers.shift => Some(Action::Text("\t".into())),
        Key::ArrowLeft => go(if word { Move::WordLeft } else { Move::Left }),
        Key::ArrowRight => go(if word { Move::WordRight } else { Move::Right }),
        Key::ArrowUp if !modifiers.command => go(Move::Up),
        Key::ArrowDown if !modifiers.command => go(Move::Down),
        Key::Home if !modifiers.command => go(Move::LineStart),
        Key::End if !modifiers.command => go(Move::LineEnd),
        // To the ends of the text, undo and select all are about the whole document, for one caret.
        Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End => Some(Action::Collapse(event.clone())),
        Key::Z | Key::Y | Key::A if modifiers.command => Some(Action::Collapse(event.clone())),
        _ => None,
    }
}

impl App {
    fn text_hash(&self) -> u64 {
        egui::epaint::util::hash(&self.text)
    }

    /// The carets to work on: those there are, or the one the editor has.
    fn carets_or_one(&mut self, ctx: &egui::Context) -> Carets {
        self.multi.take().map_or_else(|| Carets::one(self.selection(ctx)), |m| m.carets)
    }

    /// Keeps these carets, or goes back to the editor's own when only one is left.
    pub(super) fn keep_carets(&mut self, ctx: &egui::Context, carets: Carets, dragging: bool) {
        self.set_selection(ctx, carets.primary().sel());
        self.multi = (carets.len() > 1 || dragging).then(|| MultiCaret { carets, text_hash: self.text_hash(), dragging });
    }

    /// An edit at every caret, as one undo step.
    fn edit_carets(&mut self, ctx: &egui::Context, mut carets: Carets, change: impl FnOnce(&mut String, &mut Carets)) {
        self.edit(ctx, |text, _| {
            change(text, &mut carets);
            carets.primary().sel()
        });
        self.keep_carets(ctx, carets, false);
    }

    /// What the keyboard does to the carets, before the editor sees it.
    pub(super) fn caret_keys(&mut self, ctx: &egui::Context) {
        if self.multi.as_ref().is_some_and(|m| m.text_hash != self.text_hash()) {
            self.multi = None;
        }
        // egui lets go of the focus on Esc before anyone can take the key, so it's looked for first.
        let focused = ctx.memory(egui::Memory::focused);
        if let Some(multi) = &self.multi
            && (focused.is_none() || focused == Some(Id::unique(EDITOR)))
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            let main = multi.carets.primary().sel();
            self.multi = None;
            self.set_selection(ctx, main);
            self.want_focus = true;
            return;
        }
        let waiting = self.waiting_for_session();
        if waiting || !ctx.memory(|m| m.has_focus(Id::unique(EDITOR))) {
            return;
        }
        // Linux desktops often keep Ctrl+Alt with the arrows for themselves, and VS Code uses
        // Shift+Alt there: both work everywhere.
        let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        let (ctrl_alt, shift_alt) = (Modifiers::COMMAND | Modifiers::ALT, Modifiers::SHIFT | Modifiers::ALT);
        let up = pressed(ctrl_alt, Key::ArrowUp) || pressed(shift_alt, Key::ArrowUp);
        let down = pressed(ctrl_alt, Key::ArrowDown) || pressed(shift_alt, Key::ArrowDown);
        if up || down {
            let mut carets = self.carets_or_one(ctx);
            carets.add_line(&self.text, up);
            self.completions = None;
            self.scroll_caret = true;
            self.keep_carets(ctx, carets, false);
        }
        if self.multi.is_none() {
            return;
        }
        self.completions = None;
        let events = ctx.input_mut(|i| std::mem::take(&mut i.events));
        let mut left = Vec::with_capacity(events.len());
        for event in events {
            let Some(multi) = self.multi.take() else {
                left.push(event);
                continue;
            };
            let carets = multi.carets;
            let action = action_of(&event);
            let composing = matches!(event, egui::Event::Ime(egui::ImeEvent::Preedit { .. } | egui::ImeEvent::Commit(_)));
            match action {
                Some(action) => self.caret_action(ctx, carets, action, &mut left),
                None => {
                    // What's being composed shows nowhere rather than at the main caret alone.
                    if !composing {
                        left.push(event);
                    }
                    self.multi = Some(MultiCaret { carets, text_hash: multi.text_hash, dragging: multi.dragging });
                }
            }
        }
        ctx.input_mut(|i| i.events = left);
    }

    fn caret_action(&mut self, ctx: &egui::Context, mut carets: Carets, action: Action, left: &mut Vec<egui::Event>) {
        match action {
            Action::Text(typed) => {
                let mut chars = typed.chars();
                let single = chars.next().filter(|_| chars.next().is_none());
                self.edit_carets(ctx, carets, |text, carets| match single {
                    Some(c) if c != '\n' && c != '\t' => carets.type_char(text, c),
                    _ => carets.insert(text, &typed),
                });
            }
            Action::Paste(pasted) => self.edit_carets(ctx, carets, |text, carets| carets.paste(text, &pasted)),
            Action::Copy => {
                ctx.copy_text(carets.copied(&self.text));
                self.keep_carets(ctx, carets, false);
            }
            Action::Cut => {
                ctx.copy_text(carets.copied(&self.text));
                self.edit_carets(ctx, carets, |text, carets| carets.cut(text));
            }
            Action::Backspace { word } => self.edit_carets(ctx, carets, |text, carets| carets.backspace(text, word)),
            Action::Delete { word } => self.edit_carets(ctx, carets, |text, carets| carets.delete_forward(text, word)),
            Action::Go { way, extend } => {
                carets.go(&self.text, way, extend);
                self.scroll_caret = true;
                self.keep_carets(ctx, carets, false);
            }
            Action::Collapse(event) => {
                self.set_selection(ctx, carets.primary().sel());
                left.push(event);
                self.want_focus = true;
            }
        }
    }

    /// Whether a box selection is being dragged, starting one when it's asked for. The editor's own
    /// dragging stays out of it meanwhile.
    pub(super) fn column_drag_begins(&mut self, ctx: &egui::Context) -> bool {
        let (down, pressed, modifiers) = ctx.input(|i| (i.pointer.primary_down(), i.pointer.primary_pressed(), i.modifiers));
        if self.column_from.is_some() && !down && !pressed {
            self.column_from = None;
        }
        if pressed && modifiers.alt && modifiers.shift && self.editor_hovered && self.column_from.is_none() {
            let head = self.multi.as_ref().map_or(self.selection(ctx).1, |m| m.carets.primary().head);
            self.column_from = Some(Carets::line_and_column(&self.text, head));
            self.want_focus = true;
        }
        self.column_from.is_some()
    }

    /// What the pointer did on the editor this frame. `before` is the selection before the editor
    /// took the click, `place` the char under the pointer, `column` the column its x falls on, and
    /// `own` the editor's selection now, as anchor and head.
    pub(super) fn caret_pointer(&mut self, ctx: &egui::Context, before: Sel, place: Option<usize>, column: usize, own: Option<(usize, usize)>) {
        let (pressed, down, modifiers) = ctx.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.modifiers));
        if let (Some(from), Some(place)) = (self.column_from, place) {
            let line = Carets::line_and_column(&self.text, place).0;
            let carets = Carets::column(&self.text, from, (line, column));
            self.keep_carets(ctx, carets, false);
            return;
        }
        if pressed && self.editor_hovered && !self.waiting_for_session() {
            if modifiers.alt {
                if let Some(place) = place {
                    self.completions = None;
                    let mut carets = self.multi.take().map_or_else(|| Carets::one(before), |m| m.carets);
                    let count = carets.len();
                    carets.toggle(place);
                    let added = carets.len() > count;
                    self.keep_carets(ctx, carets, added);
                }
                return;
            }
            self.multi = None;
            return;
        }
        let Some(multi) = &mut self.multi else { return };
        if multi.dragging {
            if down {
                if let Some((anchor, head)) = own {
                    multi.carets.set_primary(Caret::between(anchor, head));
                }
                return;
            }
            multi.dragging = false;
            if multi.carets.len() < 2 {
                self.multi = None;
                return;
            }
        }
        // The editor's own caret stays the main one, whatever it did with a click just taken away.
        let main = multi.carets.primary().sel();
        if own.map(|(a, b)| (a.min(b), a.max(b))) != Some(main) && !down {
            self.set_selection(ctx, main);
        }
    }

    fn waiting_for_session(&self) -> bool {
        self.session.as_ref().is_some_and(Session::is_waiting)
    }
}
