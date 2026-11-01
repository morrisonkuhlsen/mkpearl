//! The shared text: a CRDT, so simultaneous edits merge the same way on every machine. Offsets are
//! bytes of UTF-8, as the document's `String` has them.

use std::sync::{Arc, Mutex};

use yrs::undo::{Options as UndoOptions, UndoManager};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{
    Assoc, Doc, GetString, IndexedSequence, Origin, ReadTxn, StateVector, StickyIndex, Text, TextRef, Transact, TransactionMut, Update, UpdateEvent,
};

const LOCAL: &str = "local";
const REMOTE: &str = "remote";

/// The smallest edit turning `old` into `new`: a byte range of `old` and what replaces it, on
/// character boundaries of both.
pub fn diff<'a>(old: &str, new: &'a str) -> (usize, usize, &'a str) {
    let (a, b) = (old.as_bytes(), new.as_bytes());
    let mut start = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
        start -= 1;
    }
    let room = a.len().min(b.len()) - start;
    let mut suffix = a.iter().rev().zip(b.iter().rev()).take(room).take_while(|(x, y)| x == y).count();
    while !old.is_char_boundary(a.len() - suffix) || !new.is_char_boundary(b.len() - suffix) {
        suffix -= 1;
    }
    (start, a.len() - suffix, new.get(start..b.len() - suffix).unwrap_or_default())
}

pub struct SharedText {
    doc: Doc,
    text: TextRef,
    /// The text as of the last sync, to tell what the editor changed.
    known: String,
    undo: UndoManager<()>,
    /// Encoded changes made here, waiting to be sent.
    outbox: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl SharedText {
    pub fn new(initial: &str) -> Result<SharedText, String> {
        let doc = Doc::new();
        let text = doc.get_or_insert_text("source");
        if !initial.is_empty() {
            // Before anything observes it: this isn't an edit to undo, and it reaches guests in
            // the Welcome message.
            text.insert(&mut doc.transact_mut_with("initial"), 0, initial);
        }
        let outbox: Arc<Mutex<Vec<Vec<u8>>>> = Arc::default();
        let queue = Arc::clone(&outbox);
        doc.observe_update_v1("outbox", move |txn: &TransactionMut, event: &UpdateEvent| {
            if txn.origin() != Some(&Origin::from(REMOTE))
                && let Ok(mut queue) = queue.lock()
            {
                queue.push(event.update.clone());
            }
        })
        .map_err(|e| e.to_string())?;
        let mut undo = UndoManager::with_options(UndoOptions::default());
        undo.expand_scope(&doc, &text);
        undo.include_origin(LOCAL);
        Ok(SharedText { doc, text, known: initial.to_string(), undo, outbox })
    }

    pub fn text(&self) -> &str {
        &self.known
    }

    fn refresh(&mut self) {
        self.known = self.text.get_string(&self.doc.transact());
    }

    /// Takes in what the editor now shows; true when it had changed.
    pub fn apply_local(&mut self, new: &str) -> bool {
        if new == self.known {
            return false;
        }
        let (start, end, inserted) = diff(&self.known, new);
        {
            let mut txn = self.doc.transact_mut_with(LOCAL);
            if end > start {
                self.text.remove_range(&mut txn, start as u32, (end - start) as u32);
            }
            if !inserted.is_empty() {
                self.text.insert(&mut txn, start as u32, inserted);
            }
        }
        self.refresh();
        true
    }

    /// Merges a change from elsewhere; a malformed one is refused. True when the text changed.
    pub fn apply_remote(&mut self, update: &[u8]) -> Result<bool, String> {
        let decoded = Update::decode_v1(update).map_err(|e| e.to_string())?;
        // The library's checks are for well-meant peers: a corrupt update must not bring down an
        // editor holding unsaved text.
        let applied = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.doc.transact_mut_with(REMOTE).apply_update(decoded)));
        applied.map_err(|_| "atualização inválida".to_string())?.map_err(|e| e.to_string())?;
        let before = std::mem::take(&mut self.known);
        self.refresh();
        Ok(before != self.known)
    }

    /// The whole state, for someone joining.
    pub fn state(&self) -> Vec<u8> {
        self.doc.transact().encode_state_as_update_v1(&StateVector::default())
    }

    /// The changes made here since the last call, to send.
    pub fn take_outgoing(&mut self) -> Vec<Vec<u8>> {
        self.outbox.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
    }

    /// Undoes this person's last change only. True when there was one.
    pub fn undo(&mut self) -> bool {
        let done = self.undo.undo_blocking();
        self.refresh();
        done
    }

    pub fn redo(&mut self) -> bool {
        let done = self.undo.redo_blocking();
        self.refresh();
        done
    }

    /// Ties a byte offset to the text around it, so it moves with later edits.
    pub fn pin(&self, byte: usize) -> Option<StickyIndex> {
        let byte = byte.min(self.known.len()) as u32;
        let txn = self.doc.transact();
        // Past the last character there's nothing to the right to hold on to: the end of the text
        // ties to what's before it, and an empty text to the text itself.
        self.text
            .sticky_index(&txn, byte, Assoc::After)
            .or_else(|| self.text.sticky_index(&txn, byte, Assoc::Before))
            .or_else(|| Some(StickyIndex::from_type(&txn, &self.text, Assoc::After)))
    }

    /// Where a pinned position is now.
    pub fn locate(&self, pin: &StickyIndex) -> Option<usize> {
        let offset = pin.get_offset(&self.doc.transact())?.index as usize;
        Some(offset.min(self.known.len()))
    }

    pub fn encode_pin(pin: &StickyIndex) -> Vec<u8> {
        pin.encode_v1()
    }

    pub fn decode_pin(bytes: &[u8]) -> Option<StickyIndex> {
        StickyIndex::decode_v1(bytes).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(old: &str, (start, end, new): (usize, usize, &str)) -> String {
        format!("{}{new}{}", old.get(..start).unwrap(), old.get(end..).unwrap())
    }

    #[test]
    fn diff_reproduces_the_new_text_on_character_boundaries() {
        for (old, new) in [
            ("", ""),
            ("", "ação"),
            ("ação", ""),
            ("ação", "ação"),
            ("aaa", "aa"),
            ("aa", "aaa"),
            ("á", "à"),
            ("açã", "aço"),
            ("日本語", "日本人語"),
            ("🙂x", "🙃x"),
            ("x🙂", "x🙃"),
            ("abcabc", "abc"),
        ] {
            assert_eq!(apply(old, diff(old, new)), new, "{old} -> {new}");
        }
    }

    #[test]
    fn diff_is_small() {
        assert_eq!(diff("olá mundo", "olá lindo mundo"), (5, 5, "lindo "));
    }

    fn sync(from: &mut SharedText, to: &mut SharedText) {
        for u in from.take_outgoing() {
            to.apply_remote(&u).unwrap();
        }
    }

    fn pair(initial: &str) -> (SharedText, SharedText) {
        let a = SharedText::new(initial).unwrap();
        let mut b = SharedText::new("").unwrap();
        b.apply_remote(&a.state()).unwrap();
        (a, b)
    }

    #[test]
    fn a_joiner_gets_the_text_with_accents_intact() {
        let (_, b) = pair("ação 日本語 🙂");
        assert_eq!(b.text(), "ação 日本語 🙂");
    }

    #[test]
    fn simultaneous_edits_merge_the_same_on_both_sides() {
        let (mut a, mut b) = pair("meio");
        a.apply_local("início meio");
        b.apply_local("meio fim");
        sync(&mut a, &mut b);
        sync(&mut b, &mut a);
        assert_eq!(a.text(), b.text());
        assert_eq!(a.text(), "início meio fim");
    }

    #[test]
    fn typing_at_the_same_spot_keeps_both_texts() {
        let (mut a, mut b) = pair("x");
        a.apply_local("xAÇÃO");
        b.apply_local("xbbb");
        sync(&mut a, &mut b);
        sync(&mut b, &mut a);
        assert_eq!(a.text(), b.text());
        assert!(a.text().contains("AÇÃO") && a.text().contains("bbb"));
    }

    #[test]
    fn undo_takes_back_only_your_own_change() {
        let (mut a, mut b) = pair("base");
        a.apply_local("base A");
        sync(&mut a, &mut b);
        b.apply_local("B base A");
        sync(&mut b, &mut a);
        assert!(a.undo());
        assert_eq!(a.text(), "B base");
        sync(&mut a, &mut b);
        assert_eq!(b.text(), "B base");
        assert!(a.redo());
        sync(&mut a, &mut b);
        assert_eq!(b.text(), "B base A");
    }

    #[test]
    fn the_initial_text_cannot_be_undone() {
        let mut a = SharedText::new("base").unwrap();
        assert!(!a.undo());
        assert_eq!(a.text(), "base");
    }

    #[test]
    fn a_pinned_position_follows_the_text_on_another_machine() {
        let (mut a, mut b) = pair("olá mundo");
        let at_mundo = a.text().find("mundo").unwrap();
        let wire = SharedText::encode_pin(&a.pin(at_mundo).unwrap());
        b.apply_local("ei, olá mundo");
        sync(&mut b, &mut a);
        let pin = SharedText::decode_pin(&wire).unwrap();
        let at = b.locate(&pin).unwrap();
        assert_eq!(b.text().get(at..), Some("mundo"));
        assert_eq!(a.locate(&pin), Some(at));
    }

    #[test]
    fn a_position_at_the_very_end_is_kept() {
        let (a, b) = pair("fim");
        let pin = a.pin(a.text().len()).unwrap();
        assert_eq!(b.locate(&pin), Some(3));
    }

    #[test]
    fn a_position_in_an_empty_text_is_kept() {
        let a = SharedText::new("").unwrap();
        let pin = a.pin(0).unwrap();
        assert_eq!(a.locate(&pin), Some(0));
    }

    #[test]
    fn typing_at_the_end_keeps_the_other_persons_caret_at_the_end_of_their_text() {
        let (mut a, mut b) = pair("ab");
        let pin = a.pin(2).unwrap();
        b.apply_local("xab");
        sync(&mut b, &mut a);
        assert_eq!(a.locate(&pin), Some(3));
    }

    #[test]
    fn corrupt_updates_are_refused_and_change_nothing() {
        let (mut a, _) = pair("base");
        for bad in [&[][..], &[255; 64], &[0, 1, 2, 3], b"not an update"] {
            let _ = a.apply_remote(bad);
        }
        assert_eq!(a.text(), "base");
    }
}
