//! Find and replace.
//!
//! All three search modes (literal, whole word, regex) compile to a [`regex::Regex`], which
//! gives correct Unicode handling, case-insensitive matching and capture groups in
//! replacements for free. `regex` guarantees matches land on char boundaries.

use std::ops::Range;

use regex::{NoExpand, Regex};

/// How the pattern should be interpreted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchMode {
    #[default]
    Literal,
    WholeWord,
    Regex,
}

impl SearchMode {
    pub fn label(self) -> &'static str {
        match self {
            SearchMode::Literal => "Literal",
            SearchMode::WholeWord => "Whole word",
            SearchMode::Regex => "Regex",
        }
    }
}

/// A compiled match, as byte offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    pub start: usize,
    pub end: usize,
}

impl SearchMatch {
    pub fn range(self) -> Range<usize> {
        self.start..self.end
    }

    pub fn len(self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start >= self.end
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchError {
    EmptyPattern,
    InvalidPattern(String),
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchError::EmptyPattern => write!(formatter, "nothing to search for"),
            SearchError::InvalidPattern(message) => write!(formatter, "invalid regex: {message}"),
        }
    }
}

impl std::error::Error for SearchError {}

/// A find/replace request. Compiled lazily with [`SearchQuery::compile`].
#[derive(Clone, Debug, Default)]
pub struct SearchQuery {
    pub pattern: String,
    pub mode: SearchMode,
    pub case_sensitive: bool,
}

impl SearchQuery {
    pub fn new(pattern: impl Into<String>, mode: SearchMode, case_sensitive: bool) -> Self {
        Self {
            pattern: pattern.into(),
            mode,
            case_sensitive,
        }
    }

    /// Builds the regex backing this query.
    pub fn compile(&self) -> Result<Regex, SearchError> {
        if self.pattern.is_empty() {
            return Err(SearchError::EmptyPattern);
        }
        let mut source = String::new();
        if !self.case_sensitive {
            source.push_str("(?i)");
        }
        match self.mode {
            SearchMode::Literal => source.push_str(&regex::escape(&self.pattern)),
            SearchMode::WholeWord => {
                source.push_str(r"\b");
                source.push_str(&regex::escape(&self.pattern));
                source.push_str(r"\b");
            }
            SearchMode::Regex => source.push_str(&self.pattern),
        }
        Regex::new(&source).map_err(|error| SearchError::InvalidPattern(error.to_string()))
    }

    /// Every match in `text`, in document order.
    pub fn find_all(&self, text: &str) -> Result<Vec<SearchMatch>, SearchError> {
        let end = text.len();
        self.find_in(text, 0..end)
    }

    /// Every match inside `range`.
    pub fn find_in(
        &self,
        text: &str,
        range: Range<usize>,
    ) -> Result<Vec<SearchMatch>, SearchError> {
        let regex = self.compile()?;
        let end = range.end.min(text.len());
        let start = range.start.min(end);
        let mut matches = Vec::new();
        for found in regex.find_iter(&text[start..end]) {
            matches.push(SearchMatch {
                start: start + found.start(),
                end: start + found.end(),
            });
        }
        Ok(matches)
    }

    /// Number of matches in `text`.
    pub fn count(&self, text: &str) -> Result<usize, SearchError> {
        Ok(self.compile()?.find_iter(text).count())
    }

    /// The first match starting at or after `from`, wrapping to the top when `wrap` is set.
    pub fn find_next(
        &self,
        text: &str,
        from: usize,
        wrap: bool,
    ) -> Result<Option<SearchMatch>, SearchError> {
        let matches = self.find_all(text)?;
        let from = from.min(text.len());
        if let Some(found) = matches.iter().find(|found| found.start >= from) {
            return Ok(Some(*found));
        }
        if wrap {
            return Ok(matches.first().copied());
        }
        Ok(None)
    }

    /// The last match ending at or before `from`, wrapping to the bottom when `wrap` is set.
    pub fn find_previous(
        &self,
        text: &str,
        from: usize,
        wrap: bool,
    ) -> Result<Option<SearchMatch>, SearchError> {
        let matches = self.find_all(text)?;
        let from = from.min(text.len());
        if let Some(found) = matches.iter().rev().find(|found| found.end <= from) {
            return Ok(Some(*found));
        }
        if wrap {
            return Ok(matches.last().copied());
        }
        Ok(None)
    }

    /// Replaces every match. `$1`-style capture references are expanded in regex mode only;
    /// literal modes insert `replacement` verbatim.
    pub fn replace_all(
        &self,
        text: &str,
        replacement: &str,
    ) -> Result<(String, usize), SearchError> {
        let regex = self.compile()?;
        let count = regex.find_iter(text).count();
        let replaced = match self.mode {
            SearchMode::Regex => regex.replace_all(text, replacement).into_owned(),
            _ => regex.replace_all(text, NoExpand(replacement)).into_owned(),
        };
        Ok((replaced, count))
    }

    /// Replaces matches inside `range`, leaving the rest of the text untouched.
    pub fn replace_in(
        &self,
        text: &str,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<(String, usize), SearchError> {
        let matches = self.find_in(text, range)?;
        if matches.is_empty() {
            return Ok((text.to_owned(), 0));
        }
        let regex = self.compile()?;
        let mut output = String::with_capacity(text.len());
        let mut cursor = 0usize;
        for found in &matches {
            output.push_str(&text[cursor..found.start]);
            let expanded = match self.mode {
                SearchMode::Regex => regex
                    .replace(&text[found.start..found.end], replacement)
                    .into_owned(),
                _ => replacement.to_owned(),
            };
            output.push_str(&expanded);
            cursor = found.end;
        }
        output.push_str(&text[cursor..]);
        Ok((output, matches.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "let cat = 1;\nlet concat = 2;\nlet CAT = 3;\n";

    #[test]
    fn literal_search_is_case_sensitive_by_default() {
        let query = SearchQuery::new("cat", SearchMode::Literal, true);
        let matches = query.find_all(TEXT).unwrap();
        assert_eq!(matches.len(), 2, "cat and concat");
        assert_eq!(&TEXT[matches[0].start..matches[0].end], "cat");
    }

    #[test]
    fn case_insensitive_search_finds_all_casings() {
        let query = SearchQuery::new("cat", SearchMode::Literal, false);
        assert_eq!(query.count(TEXT).unwrap(), 3);
    }

    #[test]
    fn whole_word_skips_substrings() {
        let query = SearchQuery::new("cat", SearchMode::WholeWord, true);
        assert_eq!(query.find_all(TEXT).unwrap().len(), 1);

        let insensitive = SearchQuery::new("cat", SearchMode::WholeWord, false);
        assert_eq!(insensitive.count(TEXT).unwrap(), 2);
    }

    #[test]
    fn regex_mode_and_capture_groups() {
        let query = SearchQuery::new(r"let (\w+) =", SearchMode::Regex, true);
        assert_eq!(query.count(TEXT).unwrap(), 3);

        let (replaced, count) = query.replace_all(TEXT, "var $1 :=").unwrap();
        assert_eq!(count, 3);
        assert!(replaced.contains("var cat :="));
        assert!(replaced.contains("var CAT :="));
    }

    #[test]
    fn literal_replace_does_not_expand_dollars() {
        let query = SearchQuery::new("cat", SearchMode::Literal, true);
        let (replaced, count) = query.replace_all(TEXT, "$1dog").unwrap();
        assert_eq!(count, 2);
        assert!(replaced.contains("$1dog"));
        assert!(replaced.contains("con$1dog"));
    }

    #[test]
    fn next_and_previous_wrap_around() {
        let query = SearchQuery::new("let", SearchMode::Literal, true);
        let first = query.find_next(TEXT, 0, false).unwrap().unwrap();
        assert_eq!(first.start, 0);

        let second = query.find_next(TEXT, 1, false).unwrap().unwrap();
        assert!(second.start > first.start);

        let wrapped = query.find_next(TEXT, TEXT.len(), true).unwrap().unwrap();
        assert_eq!(wrapped.start, first.start);
        assert_eq!(query.find_next(TEXT, TEXT.len(), false).unwrap(), None);

        let previous = query
            .find_previous(TEXT, TEXT.len(), false)
            .unwrap()
            .unwrap();
        assert!(previous.start > second.start);
    }

    #[test]
    fn empty_and_invalid_patterns_are_errors() {
        assert!(matches!(
            SearchQuery::new("", SearchMode::Literal, true).compile(),
            Err(SearchError::EmptyPattern)
        ));
        let invalid = SearchQuery::new("(unclosed", SearchMode::Regex, true);
        assert!(matches!(
            invalid.compile(),
            Err(SearchError::InvalidPattern(_))
        ));
    }

    #[test]
    fn replace_in_selection_leaves_the_rest_alone() {
        let query = SearchQuery::new("let", SearchMode::Literal, true);
        let (replaced, count) = query.replace_in(TEXT, 0..12, "var").unwrap();
        assert_eq!(count, 1);
        assert!(replaced.starts_with("var cat"));
        assert!(replaced.contains("\nlet concat"));
    }

    #[test]
    fn unicode_patterns_match_on_boundaries() {
        let text = "café au lait, café noir";
        let query = SearchQuery::new("café", SearchMode::WholeWord, false);
        let matches = query.find_all(text).unwrap();
        assert_eq!(matches.len(), 2);
        for found in matches {
            assert!(text.is_char_boundary(found.start));
            assert!(text.is_char_boundary(found.end));
        }
    }
}
