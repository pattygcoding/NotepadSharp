//! A `String` plus an incrementally maintained index of line start offsets.
//!
//! Offsets handed out by this type are **byte** offsets (what `String` slicing wants);
//! `egui` speaks in *character* indices, so the helpers at the bottom convert between
//! the two. Internally, documents are always stored with `\n` line endings — the
//! original line ending is remembered by [`crate::Document`] and re-applied on save.

use std::ops::Range;

/// A growable text buffer with a cheap "which line am I on?" lookup.
#[derive(Clone, Debug, Default)]
pub struct TextBuffer {
    text: String,
    /// Byte offset of the first character of every line. Always starts with `0`.
    line_starts: Vec<usize>,
    /// Bumped on every mutation; lets callers cache derived state (dirty flags, layout).
    revision: u64,
}

impl TextBuffer {
    pub fn new() -> Self {
        Self::from_string(String::new())
    }

    pub fn from_string(text: String) -> Self {
        let mut buffer = Self {
            text,
            line_starts: Vec::new(),
            revision: 0,
        };
        buffer.rebuild_line_starts();
        buffer
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Monotonically increasing counter, bumped on every edit.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Number of lines. A trailing `\n` produces a final empty line, like every editor.
    pub fn line_count(&self) -> usize {
        self.line_starts.len().max(1)
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.line_starts
            .get(line)
            .copied()
            .unwrap_or(self.text.len())
    }

    /// Offset of the line end, *excluding* the trailing newline.
    pub fn line_end(&self, line: usize) -> usize {
        match self.line_starts.get(line + 1) {
            Some(next) => next.saturating_sub(1),
            None => self.text.len(),
        }
    }

    pub fn line_range(&self, line: usize) -> Range<usize> {
        self.line_start(line)..self.line_end(line)
    }

    pub fn line(&self, line: usize) -> &str {
        let range = self.line_range(line);
        &self.text[range]
    }

    /// Line index containing `offset` (byte offset).
    pub fn line_of_offset(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(insert) => insert.saturating_sub(1),
        }
    }

    /// `(line, column-in-chars)` for a byte offset.
    pub fn position_of_offset(&self, offset: usize) -> (usize, usize) {
        let offset = self.clamp_offset(offset);
        let line = self.line_of_offset(offset);
        let line_start = self.line_start(line);
        let col = self.text[line_start..offset].chars().count();
        (line, col)
    }

    /// Byte offset for a `(line, column-in-chars)` pair, clamped to the line.
    pub fn offset_of_position(&self, line: usize, col: usize) -> usize {
        let start = self.line_start(line);
        let end = self.line_end(line);
        self.text[start..end]
            .char_indices()
            .nth(col)
            .map(|(index, _)| start + index)
            .unwrap_or(end)
    }

    pub fn insert(&mut self, offset: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        let offset = self.clamp_offset(offset);
        assert!(
            self.text.is_char_boundary(offset),
            "insert offset {offset} is not a char boundary"
        );
        self.text.insert_str(offset, text);
        self.refresh_line_starts_from(offset);
        self.revision += 1;
    }

    /// Removes and returns the text in `range`.
    pub fn remove(&mut self, range: Range<usize>) -> String {
        let range = self.clamp_range(range);
        if range.is_empty() {
            return String::new();
        }
        let removed: String = self.text.drain(range.clone()).collect();
        self.refresh_line_starts_from(range.start);
        self.revision += 1;
        removed
    }

    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        let range = self.clamp_range(range);
        if self.text[range.clone()] == *text {
            return;
        }
        self.text.replace_range(range.clone(), text);
        self.refresh_line_starts_from(range.start);
        self.revision += 1;
    }

    pub fn set_text(&mut self, text: String) {
        if self.text == text {
            return;
        }
        self.text = text;
        self.rebuild_line_starts();
        self.revision += 1;
    }

    /// Mutable access to the raw text, for undo/redo.
    ///
    /// Callers **must** call [`TextBuffer::refresh`] once they are done editing.
    pub fn text_mut(&mut self) -> &mut String {
        &mut self.text
    }

    /// Rebuilds the line index after [`TextBuffer::text_mut`] was used to edit the text.
    pub fn refresh(&mut self) {
        self.rebuild_line_starts();
        self.revision += 1;
    }

    /// Number of `char`s before `offset` — conversion into `egui`'s cursor space.
    pub fn char_index_of_offset(&self, offset: usize) -> usize {
        let offset = self.clamp_offset(offset);
        self.text[..offset].chars().count()
    }

    /// Byte offset of the `char_index`-th character — conversion out of `egui`'s cursor space.
    pub fn offset_of_char_index(&self, char_index: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_index)
            .map(|(offset, _)| offset)
            .unwrap_or(self.text.len())
    }

    pub fn clamp_offset(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.text.len());
        while offset > 0 && !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    fn clamp_range(&self, range: Range<usize>) -> Range<usize> {
        let start = self.clamp_offset(range.start);
        let end = self.clamp_offset(range.end.max(start));
        start..end
    }

    fn rebuild_line_starts(&mut self) {
        self.line_starts.clear();
        self.line_starts.push(0);
        for (index, byte) in self.text.bytes().enumerate() {
            if byte == b'\n' {
                self.line_starts.push(index + 1);
            }
        }
    }

    /// Re-indexes line starts from the line that contains `from_byte` onwards.
    fn refresh_line_starts_from(&mut self, from_byte: usize) {
        if self.line_starts.is_empty() {
            self.rebuild_line_starts();
            return;
        }
        let line = self.line_of_offset(from_byte.min(self.text.len()));
        self.line_starts.truncate(line + 1);
        let start = self.line_starts[line].min(self.text.len());
        for (index, byte) in self.text.as_bytes()[start..].iter().enumerate() {
            if *byte == b'\n' {
                self.line_starts.push(start + index + 1);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(text: &str) -> TextBuffer {
        TextBuffer::from_string(text.to_owned())
    }

    #[test]
    fn empty_buffer_has_one_line() {
        let buffer = TextBuffer::new();
        assert_eq!(buffer.line_count(), 1);
        assert_eq!(buffer.line(0), "");
        assert_eq!(buffer.line_range(0), 0..0);
    }

    #[test]
    fn trailing_newline_creates_final_empty_line() {
        let buffer = buffer("a\nb\n");
        assert_eq!(buffer.line_count(), 3);
        assert_eq!(buffer.line(0), "a");
        assert_eq!(buffer.line(1), "b");
        assert_eq!(buffer.line(2), "");
        assert_eq!(buffer.line_range(2), 4..4);
    }

    #[test]
    fn line_lookup_matches_brute_force() {
        let text = "alpha\nbeta\r\ngamma\n\nomega";
        let buffer = buffer(text);
        for offset in 0..=text.len() {
            let expected = text[..offset].matches('\n').count();
            assert_eq!(buffer.line_of_offset(offset), expected, "offset {offset}");
        }
    }

    #[test]
    fn position_and_offset_round_trip() {
        let buffer = buffer("héllo\nwörld\n");
        assert_eq!(buffer.position_of_offset(4), (0, 3)); // the second 'l'
        assert_eq!(buffer.offset_of_position(0, 3), 4);
        assert_eq!(buffer.offset_of_position(1, 99), 13); // clamped to line end
        assert_eq!(buffer.offset_of_position(9, 0), buffer.len());
    }

    #[test]
    fn insert_and_remove_update_line_index() {
        let mut buffer = buffer("one\ntwo");
        buffer.insert(3, "\ninserted");
        assert_eq!(buffer.line_count(), 3);
        assert_eq!(buffer.line(1), "inserted");
        assert_eq!(buffer.line(2), "two");

        let removed = buffer.remove(0..4);
        assert_eq!(removed, "one\n");
        assert_eq!(buffer.as_str(), "inserted\ntwo");
        assert_eq!(buffer.line_count(), 2);

        buffer.insert(buffer.len(), "\nthree\n");
        assert_eq!(buffer.line_count(), 4);
        assert_eq!(buffer.line(3), "");
    }

    #[test]
    fn revision_tracks_edits() {
        let mut buffer = buffer("abc");
        let start = buffer.revision();
        buffer.insert(0, "x");
        assert!(buffer.revision() > start);
        let after_insert = buffer.revision();
        buffer.replace(0..1, "x");
        assert_eq!(buffer.revision(), after_insert, "no-op edit must not bump");
    }

    #[test]
    fn char_index_conversions_are_utf8_safe() {
        let buffer = buffer("åäö\n♫ note");
        let offset = buffer.offset_of_position(1, 2);
        assert_eq!(buffer.char_index_of_offset(offset), 6);
        assert_eq!(buffer.offset_of_char_index(6), offset);
        assert_eq!(buffer.offset_of_char_index(999), buffer.len());
    }

    #[test]
    fn out_of_range_offsets_are_clamped() {
        let mut buffer = buffer("abc");
        buffer.insert(999, "!");
        assert_eq!(buffer.as_str(), "abc!");
        let removed = buffer.remove(2..999);
        assert_eq!(removed, "c!");
    }
}
