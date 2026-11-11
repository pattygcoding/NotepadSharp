//! A document: the text buffer plus everything that makes it a *file* — path, encoding,
//! line endings, language, and undo history.

use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

use crate::buffer::TextBuffer;
use crate::encoding::Encoding;
use crate::eol::{self, Eol};
use crate::history::{EditDelta, History};
use crate::language::Language;

/// One open file (or one untitled scratch buffer).
#[derive(Debug)]
pub struct Document {
    pub path: Option<PathBuf>,
    pub encoding: Encoding,
    pub eol: Eol,
    pub language: Language,
    pub buffer: TextBuffer,
    pub history: History,
    saved_hash: u64,
    dirty_cache: Cell<Option<(u64, bool)>>,
}

impl Document {
    /// A new, empty, untitled document.
    pub fn untitled() -> Self {
        Self {
            path: None,
            encoding: Encoding::default(),
            eol: Eol::default(),
            language: Language::PlainText,
            buffer: TextBuffer::new(),
            history: History::new(),
            saved_hash: hash_text(""),
            dirty_cache: Cell::new(Some((0, false))),
        }
    }

    /// Decodes `bytes` into a document. Line endings are normalized to `\n` and remembered.
    pub fn from_bytes(bytes: &[u8], path: Option<PathBuf>) -> Self {
        let (text, encoding) = Encoding::decode(bytes);
        let eol = Eol::detect(&text);
        let normalized = eol::normalize_to_lf(&text);
        let mut language = path
            .as_deref()
            .map(Language::from_path)
            .unwrap_or(Language::PlainText);
        if language == Language::PlainText {
            let first_line = normalized.split('\n').next().unwrap_or("");
            language = Language::from_shebang(first_line).unwrap_or(Language::PlainText);
        }

        let buffer = TextBuffer::from_string(normalized);
        let saved_hash = hash_text(buffer.as_str());
        let revision = buffer.revision();
        Self {
            path,
            encoding,
            eol,
            language,
            buffer,
            history: History::new(),
            saved_hash,
            dirty_cache: Cell::new(Some((revision, false))),
        }
    }

    /// Opens a file from disk.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let bytes = std::fs::read(&path)?;
        Ok(Self::from_bytes(&bytes, Some(path)))
    }

    pub fn text(&self) -> &str {
        self.buffer.as_str()
    }

    pub fn revision(&self) -> u64 {
        self.buffer.revision()
    }

    pub fn line_count(&self) -> usize {
        self.buffer.line_count()
    }

    pub fn char_count(&self) -> usize {
        self.buffer.as_str().chars().count()
    }

    pub fn word_count(&self) -> usize {
        self.buffer.as_str().split_whitespace().count()
    }

    /// True when the buffer differs from what is on disk.
    pub fn is_dirty(&self) -> bool {
        let revision = self.buffer.revision();
        if let Some((cached_revision, dirty)) = self.dirty_cache.get() {
            if cached_revision == revision {
                return dirty;
            }
        }
        let dirty = hash_text(self.buffer.as_str()) != self.saved_hash;
        self.dirty_cache.set(Some((revision, dirty)));
        dirty
    }

    /// File name shown on the tab.
    pub fn title(&self) -> String {
        match &self.path {
            Some(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned()),
            None => "Untitled".to_owned(),
        }
    }

    /// Full path, or `"Untitled"`.
    pub fn display_path(&self) -> String {
        match &self.path {
            Some(path) => path.to_string_lossy().into_owned(),
            None => "Untitled".to_owned(),
        }
    }

    pub fn set_language(&mut self, language: Language) {
        self.language = language;
    }

    pub fn set_encoding(&mut self, encoding: Encoding) {
        self.encoding = encoding;
    }

    pub fn set_eol(&mut self, eol: Eol) {
        self.eol = eol;
    }

    /// Replaces the whole buffer, recording a minimal undo step.
    pub fn replace_all(&mut self, new_text: String) {
        let old = self.buffer.as_str().to_owned();
        if old == new_text {
            return;
        }
        let (prefix, old_end, new_end) = diff_bounds(&old, &new_text);
        self.history.record(EditDelta::new(
            prefix..old_end,
            old[prefix..old_end].to_owned(),
            new_text[prefix..new_end].to_owned(),
        ));
        self.buffer.set_text(new_text);
    }

    /// Saves back to the current path.
    pub fn save(&mut self) -> io::Result<()> {
        match self.path.clone() {
            Some(path) => self.save_to(path),
            None => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "document has no path; use save as",
            )),
        }
    }

    /// Saves to `path` (applying the document's encoding and line endings) and adopts it.
    pub fn save_to(&mut self, path: impl Into<PathBuf>) -> io::Result<()> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let encoded = self.encoded_bytes();
        std::fs::write(&path, encoded)?;

        let adopting_language = self.language == Language::PlainText;
        let detected = Language::from_path(&path);
        if adopting_language && detected != Language::PlainText {
            self.language = detected;
        }
        self.path = Some(path);
        self.mark_saved();
        Ok(())
    }

    /// The bytes that would be written to disk.
    pub fn encoded_bytes(&self) -> Vec<u8> {
        let text = eol::apply_eol(self.buffer.as_str(), self.eol);
        self.encoding.encode(&text)
    }

    /// Undoes the last edit, returning the caret position to restore.
    pub fn undo(&mut self) -> Option<usize> {
        let text = self.buffer.text_mut();
        let caret = self.history.undo(text);
        self.buffer.refresh();
        caret
    }

    /// Redoes the last undone edit, returning the caret position to restore.
    pub fn redo(&mut self) -> Option<usize> {
        let text = self.buffer.text_mut();
        let caret = self.history.redo(text);
        self.buffer.refresh();
        caret
    }

    /// Applies a text replacement as a single undoable edit.
    pub fn apply_edit(&mut self, range: std::ops::Range<usize>, replacement: &str) -> usize {
        let start = self.buffer.clamp_offset(range.start);
        let end = self.buffer.clamp_offset(range.end.max(start));
        let removed = self.buffer.as_str()[start..end].to_owned();
        self.history
            .record(EditDelta::new(start..end, removed, replacement.to_owned()));
        self.buffer.replace(start..end, replacement);
        start + replacement.len()
    }

    fn mark_saved(&mut self) {
        self.saved_hash = hash_text(self.buffer.as_str());
        self.dirty_cache.set(Some((self.buffer.revision(), false)));
    }
}

fn hash_text(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Common prefix/suffix diff, used to record a minimal undo step for whole-buffer replaces.
fn diff_bounds(old: &str, new: &str) -> (usize, usize, usize) {
    let old_bytes = old.as_bytes();
    let new_bytes = new.as_bytes();

    let max_prefix = old_bytes.len().min(new_bytes.len());
    let mut prefix = 0usize;
    while prefix < max_prefix && old_bytes[prefix] == new_bytes[prefix] {
        prefix += 1;
    }
    while prefix > 0 && !(old.is_char_boundary(prefix) && new.is_char_boundary(prefix)) {
        prefix -= 1;
    }

    let max_suffix = max_prefix - prefix;
    let mut suffix = 0usize;
    while suffix < max_suffix
        && old_bytes[old_bytes.len() - 1 - suffix] == new_bytes[new_bytes.len() - 1 - suffix]
    {
        suffix += 1;
    }
    while suffix > 0
        && !(old.is_char_boundary(old.len() - suffix) && new.is_char_boundary(new.len() - suffix))
    {
        suffix -= 1;
    }

    (prefix, old.len() - suffix, new.len() - suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("notepadsharp-test-{}-{name}", std::process::id()));
        path
    }

    #[test]
    fn untitled_document_is_clean() {
        let document = Document::untitled();
        assert_eq!(document.title(), "Untitled");
        assert_eq!(document.display_path(), "Untitled");
        assert!(!document.is_dirty());
        assert_eq!(document.line_count(), 1);
        assert_eq!(document.language, Language::PlainText);
    }

    #[test]
    fn detects_language_eol_and_encoding() {
        let path = PathBuf::from("src/lib.rs");
        let document = Document::from_bytes(b"fn main() {}\r\n", Some(path));
        assert_eq!(document.language, Language::Rust);
        assert_eq!(document.eol, Eol::CrLf);
        assert_eq!(document.encoding, Encoding::Utf8);
        assert_eq!(document.text(), "fn main() {}\n", "normalized in memory");
        assert_eq!(document.title(), "lib.rs");
    }

    #[test]
    fn detects_language_from_shebang() {
        let document = Document::from_bytes(b"#!/usr/bin/env python3\nprint(1)\n", None);
        assert_eq!(document.language, Language::Python);
    }

    #[test]
    fn dirty_flag_follows_edits_and_saves() {
        let path = temp_path("dirty.txt");
        let mut document = Document::from_bytes(b"hello", Some(path.clone()));
        assert!(!document.is_dirty());

        document.buffer.insert(5, " world");
        assert!(document.is_dirty());

        document.save().unwrap();
        assert!(!document.is_dirty());
        assert_eq!(std::fs::read(&path).unwrap(), b"hello world");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn save_preserves_crlf_line_endings() {
        let path = temp_path("crlf.txt");
        let mut document = Document::from_bytes(b"one\r\ntwo\r\n", Some(path.clone()));
        assert_eq!(document.eol, Eol::CrLf);

        document.buffer.insert(0, "zero\n");
        document.save().unwrap();

        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"zero\r\none\r\ntwo\r\n",
            "the original CRLF style is restored on save"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn save_preserves_utf16_encoding() {
        let path = temp_path("utf16.txt");
        let original = "héllo ♫";
        let bytes = Encoding::Utf16Le.encode(original);
        let mut document = Document::from_bytes(&bytes, Some(path.clone()));
        assert_eq!(document.encoding, Encoding::Utf16Le);

        document.buffer.insert(0, "> ");
        document.save().unwrap();

        let (text, encoding) = Encoding::decode(&std::fs::read(&path).unwrap());
        assert_eq!(encoding, Encoding::Utf16Le);
        assert_eq!(text, "> héllo ♫");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn save_as_adopts_path_and_language() {
        let path = temp_path("adopted.rs");
        let mut document = Document::untitled();
        document.buffer.insert(0, "// hi\n");
        document.save_to(path.clone()).unwrap();

        assert_eq!(document.language, Language::Rust);
        assert_eq!(document.path.as_deref(), Some(path.as_path()));
        assert!(!document.is_dirty());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn saving_untitled_without_a_path_errors() {
        let mut document = Document::untitled();
        assert!(document.save().is_err());
    }

    #[test]
    fn replace_all_is_undoable() {
        let mut document = Document::from_bytes(b"abcd", None);
        document.replace_all("abXYcd".to_owned());
        assert_eq!(document.text(), "abXYcd");

        assert_eq!(document.undo(), Some(2));
        assert_eq!(document.text(), "abcd");
        assert_eq!(document.redo(), Some(4));
        assert_eq!(document.text(), "abXYcd");
    }

    #[test]
    fn eol_change_only_applies_on_save() {
        let path = temp_path("eol-convert.txt");
        let mut document = Document::from_bytes(b"a\nb\n", Some(path.clone()));
        assert_eq!(document.eol, Eol::Lf);

        document.set_eol(Eol::CrLf);
        assert_eq!(document.text(), "a\nb\n", "buffer stays normalized");
        document.save().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"a\r\nb\r\n");
        std::fs::remove_file(&path).ok();
    }
}
