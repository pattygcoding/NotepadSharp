//! Syntax highlighting: token kinds, a per-line cache, and the incremental highlighter.
//!
//! The highlighter works like Scintilla's line state machine: every line is tokenized
//! independently, given the state left behind by the line above it (e.g. "inside a block
//! comment"). Cached lines are only re-tokenized when their text — or the state flowing
//! into them — changed, so typing at the bottom of a file does not re-highlight the top.

pub mod lexer;

use crate::language::Language;

/// What a span of text represents, used by the UI to pick a color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Keyword,
    Type,
    Literal,
    String,
    Number,
    Comment,
    Function,
    Macro,
    Attribute,
    Operator,
    Tag,
    TagName,
    Heading,
    Link,
    Key,
}

/// A highlighted span, as byte offsets relative to the start of its line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenSpan {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
}

impl TokenSpan {
    pub fn new(start: usize, end: usize, kind: TokenKind) -> Self {
        Self { start, end, kind }
    }
}

/// Which style of string a line ended inside of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrKind {
    Single,
    Double,
    Backtick,
    TripleSingle,
    TripleDouble,
    TripleBacktick,
}

impl StrKind {
    pub fn delimiter(self) -> &'static str {
        match self {
            StrKind::Single => "'",
            StrKind::Double => "\"",
            StrKind::Backtick => "`",
            StrKind::TripleSingle => "'''",
            StrKind::TripleDouble => "\"\"\"",
            StrKind::TripleBacktick => "```",
        }
    }

    /// Whether `\` escapes the next character inside this string.
    pub fn has_escapes(self) -> bool {
        !matches!(self, StrKind::TripleSingle | StrKind::TripleDouble)
    }

    /// String kind for a single quote character.
    pub fn from_quote(ch: char) -> StrKind {
        match ch {
            '\'' => StrKind::Single,
            '`' => StrKind::Backtick,
            _ => StrKind::Double,
        }
    }

    /// Triple-quoted variant of a single quote character.
    pub fn from_triple_quote(ch: char) -> StrKind {
        match ch {
            '\'' => StrKind::TripleSingle,
            '`' => StrKind::TripleBacktick,
            _ => StrKind::TripleDouble,
        }
    }
}

/// Lexer state carried from one line to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineState {
    #[default]
    Normal,
    BlockComment,
    Str(StrKind),
}

#[derive(Clone, Debug)]
struct LineCache {
    hash: u64,
    start: LineState,
    end: LineState,
    tokens: Vec<TokenSpan>,
}

/// Caches per-line tokenization for a single document.
#[derive(Clone, Debug)]
pub struct Highlighter {
    language: Language,
    lines: Vec<LineCache>,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new(Language::PlainText)
    }
}

impl Highlighter {
    pub fn new(language: Language) -> Self {
        Self {
            language,
            lines: Vec::new(),
        }
    }

    pub fn language(&self) -> Language {
        self.language
    }

    /// Switches language. Clears the cache so everything is re-tokenized once.
    pub fn set_language(&mut self, language: Language) {
        if self.language != language {
            self.language = language;
            self.lines.clear();
        }
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Re-tokenizes whatever changed in `text`. Cheap when nothing changed.
    pub fn update(&mut self, text: &str) {
        if self.language == Language::PlainText {
            self.update_plain(text);
            return;
        }

        let syntax = self.language.syntax();
        let mut state = LineState::Normal;
        let mut index = 0usize;

        for raw_line in text.split('\n') {
            let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
            let hash = hash_line(line);

            let reusable = match self.lines.get(index) {
                Some(cache) => cache.hash == hash && cache.start == state,
                None => false,
            };

            if reusable {
                state = self.lines[index].end;
            } else {
                let (tokens, end) = lexer::tokenize_line(syntax, line, state);
                let cache = LineCache {
                    hash,
                    start: state,
                    end,
                    tokens,
                };
                if index < self.lines.len() {
                    self.lines[index] = cache;
                } else {
                    self.lines.push(cache);
                }
                state = end;
            }

            index += 1;
        }

        self.lines.truncate(index);
    }

    /// Plain text has no tokens; we still keep per-line entries so line counts stay in sync.
    fn update_plain(&mut self, text: &str) {
        let mut index = 0usize;
        for raw_line in text.split('\n') {
            let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
            let hash = hash_line(line);
            let reusable = matches!(
                self.lines.get(index),
                Some(cache) if cache.hash == hash && cache.tokens.is_empty()
            );
            if !reusable {
                let cache = LineCache {
                    hash,
                    start: LineState::Normal,
                    end: LineState::Normal,
                    tokens: Vec::new(),
                };
                if index < self.lines.len() {
                    self.lines[index] = cache;
                } else {
                    self.lines.push(cache);
                }
            }
            index += 1;
        }
        self.lines.truncate(index);
    }

    /// Tokens for `line`, or an empty slice when the line is not cached yet.
    pub fn tokens_for_line(&self, line: usize) -> &[TokenSpan] {
        self.lines
            .get(line)
            .map(|cache| cache.tokens.as_slice())
            .unwrap_or(&[])
    }

    /// State a line starts in — exposed for tests and diagnostics.
    pub fn state_at(&self, line: usize) -> LineState {
        self.lines
            .get(line)
            .map(|cache| cache.start)
            .unwrap_or_default()
    }
}

fn hash_line(line: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    line.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(highlighter: &Highlighter, line: usize, text: &str) -> Vec<(TokenKind, String)> {
        highlighter
            .tokens_for_line(line)
            .iter()
            .map(|span| (span.kind, text[span.start..span.end].to_owned()))
            .collect()
    }

    #[test]
    fn rust_keywords_and_functions() {
        let text = "fn main() { let x = 1; }";
        let mut highlighter = Highlighter::new(Language::Rust);
        highlighter.update(text);
        let tokens = kinds(&highlighter, 0, text);
        assert!(tokens.contains(&(TokenKind::Keyword, "fn".to_owned())));
        assert!(tokens.contains(&(TokenKind::Function, "main".to_owned())));
        assert!(tokens.contains(&(TokenKind::Keyword, "let".to_owned())));
        assert!(tokens.contains(&(TokenKind::Number, "1".to_owned())));
    }

    #[test]
    fn line_comment_runs_to_end_of_line() {
        let text = "let x = 1; // trailing";
        let mut highlighter = Highlighter::new(Language::Rust);
        highlighter.update(text);
        let tokens = kinds(&highlighter, 0, text);
        assert_eq!(
            tokens.last().unwrap(),
            &(TokenKind::Comment, "// trailing".to_owned())
        );
    }

    #[test]
    fn block_comment_spans_lines_and_recovers() {
        let text = "let a = 1;\n/* open\nstill comment\nclose */ let b = 2;";
        let mut highlighter = Highlighter::new(Language::Rust);
        highlighter.update(text);
        assert_eq!(highlighter.state_at(1), LineState::Normal);
        assert_eq!(highlighter.state_at(2), LineState::BlockComment);
        assert_eq!(highlighter.state_at(3), LineState::BlockComment);
        assert_eq!(highlighter.state_at(4), LineState::Normal);

        let line3 = "close */ let b = 2;";
        let tokens = kinds(&highlighter, 3, line3);
        assert_eq!(tokens[0], (TokenKind::Comment, "close */".to_owned()));
        assert!(tokens.contains(&(TokenKind::Keyword, "let".to_owned())));
    }

    #[test]
    fn triple_quoted_python_string_keeps_state() {
        let text = "def f():\n    \"\"\"doc\n    more\n    \"\"\"\n    return 1";
        let mut highlighter = Highlighter::new(Language::Python);
        highlighter.update(text);
        assert_eq!(highlighter.state_at(1), LineState::Normal);
        assert_eq!(
            highlighter.state_at(2),
            LineState::Str(StrKind::TripleDouble)
        );
        assert_eq!(
            highlighter.state_at(3),
            LineState::Str(StrKind::TripleDouble)
        );
        assert_eq!(highlighter.state_at(4), LineState::Normal);
        let tokens = kinds(&highlighter, 2, "    more");
        assert_eq!(tokens, vec![(TokenKind::String, "    more".to_owned())]);
    }

    #[test]
    fn escaped_quotes_do_not_close_strings() {
        let text = "let s = \"a \\\" b\"; let t = 2;";
        let mut highlighter = Highlighter::new(Language::Rust);
        highlighter.update(text);
        let tokens = kinds(&highlighter, 0, text);
        assert!(tokens.contains(&(TokenKind::String, "\"a \\\" b\"".to_owned())));
        assert!(tokens.contains(&(TokenKind::Number, "2".to_owned())));
    }

    #[test]
    fn shrinking_text_drops_cached_lines() {
        let mut highlighter = Highlighter::new(Language::Rust);
        highlighter.update("let a = 1;\nlet b = 2;\nlet c = 3;");
        assert_eq!(highlighter.line_count(), 3);

        highlighter.update("let a = 1;");
        assert_eq!(highlighter.line_count(), 1);
        let tokens = kinds(&highlighter, 0, "let a = 1;");
        assert!(tokens.contains(&(TokenKind::Keyword, "let".to_owned())));
    }

    #[test]
    fn changing_language_clears_cache() {
        let mut highlighter = Highlighter::new(Language::Rust);
        highlighter.update("-- not a rust comment");
        assert_eq!(highlighter.line_count(), 1);
        highlighter.set_language(Language::Sql);
        assert_eq!(highlighter.line_count(), 0);
        highlighter.update("-- sql comment");
        let tokens = kinds(&highlighter, 0, "-- sql comment");
        assert_eq!(tokens[0], (TokenKind::Comment, "-- sql comment".to_owned()));
    }

    #[test]
    fn plain_text_produces_no_tokens() {
        let text = "just words and 123 numbers";
        let mut highlighter = Highlighter::new(Language::PlainText);
        highlighter.update(text);
        assert!(highlighter.tokens_for_line(0).is_empty());
        assert!(highlighter.tokens_for_line(99).is_empty());
    }
}
