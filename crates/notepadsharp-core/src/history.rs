//! Undo / redo history.
//!
//! Every mutation is recorded as a single text replacement (`range` → `inserted`), which
//! makes undo and redo symmetric and keeps line operations, pastes and typing on one
//! stack. Consecutive keystrokes are coalesced so `Ctrl+Z` removes a word-sized chunk
//! instead of a single character.

use std::ops::Range;
use std::time::{Duration, Instant};

/// How long two adjacent edits may be apart and still merge into one undo step.
const COALESCE_WINDOW: Duration = Duration::from_millis(600);
/// Upper bound on stored undo steps.
const MAX_ENTRIES: usize = 1000;

/// A single text replacement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditDelta {
    /// Byte range in the document *before* the edit.
    pub range: Range<usize>,
    /// Text that was replaced (needed to undo).
    pub removed: String,
    /// Text that replaced it (needed to redo).
    pub inserted: String,
}

impl EditDelta {
    pub fn new(range: Range<usize>, removed: String, inserted: String) -> Self {
        Self {
            range,
            removed,
            inserted,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.inserted.is_empty()
    }

    /// Caret position after redoing this edit.
    pub fn caret_after(&self) -> usize {
        self.range.start + self.inserted.len()
    }
}

#[derive(Clone, Debug)]
struct Entry {
    delta: EditDelta,
    stamp: Instant,
}

/// Undo/redo stacks for one document.
#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// Records an edit. Clears the redo stack, like every editor.
    pub fn record(&mut self, delta: EditDelta) {
        if delta.is_empty() {
            return;
        }
        self.redo.clear();

        if let Some(last) = self.undo.last_mut() {
            if last.stamp.elapsed() < COALESCE_WINDOW {
                if let Some(merged) = merge(&last.delta, &delta) {
                    last.delta = merged;
                    last.stamp = Instant::now();
                    return;
                }
            }
        }

        self.undo.push(Entry {
            delta,
            stamp: Instant::now(),
        });
        if self.undo.len() > MAX_ENTRIES {
            self.undo.remove(0);
        }
    }

    /// Reverts the last edit, returning the caret position to restore.
    pub fn undo(&mut self, text: &mut String) -> Option<usize> {
        let entry = self.undo.pop()?;
        let delta = entry.delta;
        // The edit inserted `inserted` at `range.start`; replace it back with `removed`.
        let start = delta.range.start;
        apply(text, &(start..start + delta.inserted.len()), &delta.removed);
        let caret = start;
        self.redo.push(Entry {
            delta,
            stamp: Instant::now(),
        });
        Some(caret)
    }

    /// Re-applies the last undone edit, returning the caret position to restore.
    pub fn redo(&mut self, text: &mut String) -> Option<usize> {
        let entry = self.redo.pop()?;
        let delta = entry.delta;
        let start = delta.range.start;
        apply(text, &(start..start + delta.removed.len()), &delta.inserted);
        let caret = delta.caret_after();
        self.undo.push(Entry {
            delta,
            stamp: Instant::now(),
        });
        Some(caret)
    }
}

/// Applies a replacement, clamped to valid char boundaries.
fn apply(text: &mut String, range: &Range<usize>, replacement: &str) {
    let start = clamp_to_boundary(text, range.start.min(text.len()));
    let end = clamp_to_boundary(text, range.end.min(text.len())).max(start);
    text.replace_range(start..end, replacement);
}

fn clamp_to_boundary(text: &str, mut offset: usize) -> usize {
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// Merges two adjacent edits of the same kind, so typing or holding backspace is one step.
fn merge(older: &EditDelta, newer: &EditDelta) -> Option<EditDelta> {
    let both_inserts = older.removed.is_empty() && newer.removed.is_empty();
    if both_inserts && newer.range.start == older.range.start + older.inserted.len() {
        let mut inserted = older.inserted.clone();
        inserted.push_str(&newer.inserted);
        return Some(EditDelta {
            range: older.range.start..older.range.start,
            removed: String::new(),
            inserted,
        });
    }

    let both_deletes = older.inserted.is_empty() && newer.inserted.is_empty();
    if both_deletes {
        // Backspace: the newer deletion is immediately *before* the older one.
        if newer.range.end == older.range.start {
            let mut removed = newer.removed.clone();
            removed.push_str(&older.removed);
            return Some(EditDelta {
                range: newer.range.start..newer.range.start,
                removed,
                inserted: String::new(),
            });
        }
        // Delete key: the newer deletion starts where the older one started.
        if newer.range.start == older.range.start {
            let mut removed = older.removed.clone();
            removed.push_str(&newer.removed);
            return Some(EditDelta {
                range: older.range.start..older.range.start,
                removed,
                inserted: String::new(),
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(offset: usize, text: &str) -> EditDelta {
        EditDelta::new(offset..offset, String::new(), text.to_owned())
    }

    fn delete(range: Range<usize>, removed: &str) -> EditDelta {
        EditDelta::new(range, removed.to_owned(), String::new())
    }

    #[test]
    fn undo_and_redo_round_trip() {
        let mut text = String::from("hello");
        let mut history = History::new();
        text.insert_str(5, " world");
        history.record(insert(5, " world"));

        assert_eq!(history.undo(&mut text), Some(5));
        assert_eq!(text, "hello");
        assert!(history.can_redo());

        assert_eq!(history.redo(&mut text), Some(11));
        assert_eq!(text, "hello world");
    }

    #[test]
    fn coalesces_consecutive_typing() {
        let mut text = String::new();
        let mut history = History::new();
        for (index, ch) in "abc".chars().enumerate() {
            text.push(ch);
            history.record(insert(index, &ch.to_string()));
        }
        assert_eq!(history.undo_depth(), 1, "typing coalesces into one step");

        history.undo(&mut text);
        assert_eq!(text, "");
    }

    #[test]
    fn coalesces_backspaces() {
        let mut text = String::from("abc");
        let mut history = History::new();
        text.remove(2);
        history.record(delete(2..3, "c"));
        text.remove(1);
        history.record(delete(1..2, "b"));

        assert_eq!(history.undo_depth(), 1);
        history.undo(&mut text);
        assert_eq!(text, "abc");
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut text = String::from("a");
        let mut history = History::new();
        text.push('b');
        history.record(insert(1, "b"));
        history.undo(&mut text);
        assert!(history.can_redo());

        text.push('c');
        history.record(insert(1, "c"));
        assert!(!history.can_redo());
    }

    #[test]
    fn empty_edits_are_ignored() {
        let mut history = History::new();
        history.record(EditDelta::new(0..0, String::new(), String::new()));
        assert!(!history.can_undo());
        assert_eq!(history.undo(&mut String::new()), None);
        assert_eq!(history.redo(&mut String::new()), None);
    }

    #[test]
    fn replacements_do_not_coalesce() {
        let mut text = String::from("abc");
        let mut history = History::new();
        text.replace_range(0..1, "x");
        history.record(EditDelta::new(0..1, "a".to_owned(), "x".to_owned()));
        text.replace_range(1..2, "y");
        history.record(EditDelta::new(1..2, "b".to_owned(), "y".to_owned()));
        assert_eq!(history.undo_depth(), 2);
    }

    #[test]
    fn undo_survives_out_of_range_edits() {
        let mut text = String::from("ab");
        let mut history = History::new();
        history.record(delete(50..60, "ghost"));
        // Out-of-range edits are clamped rather than panicking.
        assert_eq!(history.undo(&mut text), Some(50));
        assert_eq!(text, "abghost");
    }

    #[test]
    fn clear_drops_both_stacks() {
        let mut text = String::from("a");
        let mut history = History::new();
        text.push('b');
        history.record(insert(1, "b"));
        history.undo(&mut text);
        history.clear();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }
}
