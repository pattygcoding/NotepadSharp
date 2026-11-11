//! The per-line lexer.
//!
//! A single generic scanner handles every language, driven by the data in
//! [`crate::language::Syntax`]. It is deliberately small: it classifies identifiers,
//! numbers, strings, comments, operators, tags and `key = value` pairs, which is what
//! produces the Notepad++-style token coloring.

use super::{LineState, StrKind, TokenKind, TokenSpan};
use crate::language::{Syntax, SyntaxKind};

/// Tokenizes one line, given the state left behind by the previous line.
pub fn tokenize_line(syntax: &Syntax, line: &str, entry: LineState) -> (Vec<TokenSpan>, LineState) {
    if syntax.kind == SyntaxKind::Markdown {
        return tokenize_markdown_line(syntax, line, entry);
    }

    let mut tokens = Vec::new();
    let start = match resume_state(syntax, line, entry, &mut tokens) {
        Ok(start) => start,
        Err(state) => return (tokens, state),
    };

    match scan_inline(syntax, line, start, &mut tokens) {
        Some(state) => (tokens, state),
        None => (tokens, LineState::Normal),
    }
}

/// Finishes a block comment or string that started on a previous line.
///
/// `Ok(index)` means "carry on scanning from `index`"; `Err(state)` means the whole line
/// was consumed and the next line starts in `state`.
fn resume_state(
    syntax: &Syntax,
    line: &str,
    entry: LineState,
    tokens: &mut Vec<TokenSpan>,
) -> Result<usize, LineState> {
    match entry {
        LineState::Normal => Ok(0),
        LineState::BlockComment => match syntax.block_comment {
            Some((_, end)) if !end.is_empty() => match find_from(line, 0, end) {
                Some(position) => {
                    let stop = position + end.len();
                    push(tokens, 0, stop, TokenKind::Comment);
                    Ok(stop)
                }
                None => {
                    push(tokens, 0, line.len(), TokenKind::Comment);
                    Err(LineState::BlockComment)
                }
            },
            _ => Ok(0),
        },
        LineState::Str(kind) => match scan_string_body(line, 0, kind) {
            Some(stop) => {
                push(tokens, 0, stop, TokenKind::String);
                Ok(stop)
            }
            None => {
                push(tokens, 0, line.len(), TokenKind::String);
                Err(LineState::Str(kind))
            }
        },
    }
}

/// Scans generic tokens from `index`. Returns the state to carry into the next line when
/// the line ends inside a comment or string.
fn scan_inline(
    syntax: &Syntax,
    line: &str,
    mut index: usize,
    tokens: &mut Vec<TokenSpan>,
) -> Option<LineState> {
    let bytes = line.as_bytes();

    while index < bytes.len() {
        let ch = line[index..].chars().next()?;

        if ch.is_whitespace() {
            index += ch.len_utf8();
            continue;
        }

        // Line comments run to the end of the line.
        if starts_with(line, index, syntax.line_comment)
            || starts_with(line, index, syntax.line_comment_alt)
        {
            push(tokens, index, line.len(), TokenKind::Comment);
            return None;
        }

        // Block comments may stay open across lines.
        if let Some((start, end)) = syntax.block_comment {
            if !start.is_empty() && starts_with(line, index, Some(start)) {
                if let Some(position) = find_from(line, index + start.len(), end) {
                    let stop = position + end.len();
                    push(tokens, index, stop, TokenKind::Comment);
                    index = stop;
                    continue;
                }
                push(tokens, index, line.len(), TokenKind::Comment);
                return Some(LineState::BlockComment);
            }
        }

        if syntax.kind == SyntaxKind::Markup && ch == '<' {
            index = scan_markup_tag(line, index, tokens);
            continue;
        }

        if syntax.kind == SyntaxKind::Markdown && ch == '[' {
            if let Some(close) = line[index..].find(']') {
                let stop = index + close + 1;
                push(tokens, index, stop, TokenKind::Link);
                index = stop;
                continue;
            }
        }

        // Strings, including triple-quoted ones that span lines.
        if let Some((length, kind)) = match_quote(syntax, line, index) {
            let start = index;
            match scan_string_body(line, index + length, kind) {
                Some(stop) => {
                    let is_key = matches!(syntax.kind, SyntaxKind::KeyValue)
                        && next_significant(line, stop) == Some(b':');
                    push(
                        tokens,
                        start,
                        stop,
                        if is_key {
                            TokenKind::Key
                        } else {
                            TokenKind::String
                        },
                    );
                    index = stop;
                }
                None => {
                    push(tokens, start, line.len(), TokenKind::String);
                    return Some(LineState::Str(kind));
                }
            }
            continue;
        }

        // Numbers.
        if ch.is_ascii_digit() {
            let start = index;
            index += 1;
            while index < bytes.len() {
                let byte = bytes[index];
                if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' {
                    index += 1;
                } else {
                    break;
                }
            }
            push(tokens, start, index, TokenKind::Number);
            continue;
        }

        // Attributes (`@Override`, `#[derive(...)]`) and preprocessor macros.
        if ch == '#' && bytes.get(index + 1) == Some(&b'[') {
            let start = index;
            let mut cursor = index + 2;
            let mut depth = 1usize;
            while cursor < bytes.len() {
                match bytes[cursor] {
                    b'[' => depth += 1,
                    b']' => {
                        depth -= 1;
                        if depth == 0 {
                            cursor += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                cursor += 1;
            }
            push(tokens, start, cursor, TokenKind::Attribute);
            index = cursor;
            continue;
        }

        if ch == '#' || ch == '@' {
            let start = index;
            index += 1;
            while index < bytes.len() {
                let next = line[index..].chars().next().unwrap_or(' ');
                if is_ident_char(next) {
                    index += next.len_utf8();
                } else {
                    break;
                }
            }
            let kind = if ch == '#' {
                TokenKind::Macro
            } else {
                TokenKind::Attribute
            };
            push(tokens, start, index, kind);
            continue;
        }

        // Identifiers: keywords, types, literals, function names and keys.
        if is_ident_start(ch) {
            let start = index;
            while index < bytes.len() {
                let next = line[index..].chars().next().unwrap_or(' ');
                if is_ident_char(next) {
                    index += next.len_utf8();
                } else {
                    break;
                }
            }
            let word = &line[start..index];

            if let Some(kind) = classify_word(syntax, word) {
                push(tokens, start, index, kind);
                continue;
            }

            // Rust macro invocations: `println!(...)`.
            if bytes.get(index) == Some(&b'!') && bytes.get(index + 1) != Some(&b'=') {
                push(tokens, start, index + 1, TokenKind::Macro);
                index += 1;
                continue;
            }

            match next_significant(line, index) {
                Some(b'(') => push(tokens, start, index, TokenKind::Function),
                Some(b':') | Some(b'=') if syntax.kind == SyntaxKind::KeyValue => {
                    push(tokens, start, index, TokenKind::Key);
                }
                _ => {}
            }
            continue;
        }

        // Operators.
        if matches!(
            ch,
            '=' | '+' | '-' | '*' | '/' | '%' | '<' | '>' | '!' | '&' | '|' | '^' | '~' | '?'
        ) {
            push(tokens, index, index + 1, TokenKind::Operator);
            index += 1;
            continue;
        }

        index += ch.len_utf8();
    }

    None
}

/// Classifies a bare word against the language's keyword/type/literal tables.
fn classify_word(syntax: &Syntax, word: &str) -> Option<TokenKind> {
    if matches_word(syntax.keywords, word, syntax.case_insensitive) {
        return Some(TokenKind::Keyword);
    }
    if matches_word(syntax.literals, word, syntax.case_insensitive) {
        return Some(TokenKind::Literal);
    }
    if matches_word(syntax.types, word, syntax.case_insensitive) {
        return Some(TokenKind::Type);
    }
    None
}

fn matches_word(table: &[&str], word: &str, case_insensitive: bool) -> bool {
    if case_insensitive {
        table.iter().any(|entry| entry.eq_ignore_ascii_case(word))
    } else {
        table.contains(&word)
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_alphabetic() || ch == '_' || ch == '$'
}

fn is_ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '$'
}

fn push(tokens: &mut Vec<TokenSpan>, start: usize, end: usize, kind: TokenKind) {
    if end > start {
        tokens.push(TokenSpan::new(start, end, kind));
    }
}

fn starts_with(line: &str, index: usize, needle: Option<&str>) -> bool {
    match needle {
        Some(needle) if !needle.is_empty() => line[index..].starts_with(needle),
        _ => false,
    }
}

fn find_from(line: &str, from: usize, needle: &str) -> Option<usize> {
    if from > line.len() || needle.is_empty() {
        return None;
    }
    line[from..].find(needle).map(|position| from + position)
}

/// First non-whitespace byte at or after `index`.
fn next_significant(line: &str, index: usize) -> Option<u8> {
    line.as_bytes()
        .iter()
        .skip(index)
        .find(|byte| !byte.is_ascii_whitespace())
        .copied()
}

/// Recognises the start of a quoted string, preferring triple quotes where allowed.
fn match_quote(syntax: &Syntax, line: &str, index: usize) -> Option<(usize, StrKind)> {
    let ch = line[index..].chars().next()?;
    if !ch.is_ascii() || !syntax.quote_chars.contains(&ch) {
        return None;
    }
    if syntax.triple_quotes {
        let repeated = [ch as u8; 3];
        if line.as_bytes()[index..].starts_with(&repeated) {
            return Some((3, StrKind::from_triple_quote(ch)));
        }
    }
    Some((1, StrKind::from_quote(ch)))
}

/// Finds the closing delimiter of a string starting at `from`, returning the index just
/// past it. `None` means the string stays open at the end of the line.
fn scan_string_body(line: &str, from: usize, kind: StrKind) -> Option<usize> {
    let bytes = line.as_bytes();
    let delimiter = kind.delimiter().as_bytes();
    let escapes = kind.has_escapes();
    let mut index = from;

    while index < bytes.len() {
        if bytes[index..].starts_with(delimiter) {
            return Some(index + delimiter.len());
        }
        if escapes && bytes[index] == b'\\' {
            index += 2;
            continue;
        }
        index += 1;
    }
    None
}

/// Tokenizes `<tag attr="value">` style markup, returning the index after the tag.
fn scan_markup_tag(line: &str, index: usize, tokens: &mut Vec<TokenSpan>) -> usize {
    let bytes = line.as_bytes();
    let mut cursor = index + 1;
    let mut tag_end = cursor;
    if bytes.get(cursor) == Some(&b'/') {
        cursor += 1;
        tag_end = cursor;
    }
    push(tokens, index, tag_end, TokenKind::Tag);

    let name_start = cursor;
    while cursor < bytes.len() && is_name_byte(bytes[cursor]) {
        cursor += 1;
    }
    if cursor > name_start {
        push(tokens, name_start, cursor, TokenKind::TagName);
    }

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'>' => {
                push(tokens, cursor, cursor + 1, TokenKind::Tag);
                return cursor + 1;
            }
            b'/' if bytes.get(cursor + 1) == Some(&b'>') => {
                push(tokens, cursor, cursor + 2, TokenKind::Tag);
                return cursor + 2;
            }
            byte if byte.is_ascii_whitespace() => cursor += 1,
            b'=' => {
                push(tokens, cursor, cursor + 1, TokenKind::Operator);
                cursor += 1;
            }
            quote @ (b'"' | b'\'') => {
                let start = cursor;
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor] != quote {
                    cursor += 1;
                }
                if cursor < bytes.len() {
                    cursor += 1;
                }
                push(tokens, start, cursor, TokenKind::String);
            }
            _ => {
                let start = cursor;
                while cursor < bytes.len()
                    && (is_name_byte(bytes[cursor]) || matches!(bytes[cursor], b'-' | b':' | b'.'))
                {
                    cursor += 1;
                }
                if cursor > start {
                    push(tokens, start, cursor, TokenKind::Attribute);
                } else {
                    cursor += 1;
                }
            }
        }
    }
    cursor
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Markdown is line-oriented: headings, fenced code blocks, links and inline code.
fn tokenize_markdown_line(
    syntax: &Syntax,
    line: &str,
    entry: LineState,
) -> (Vec<TokenSpan>, LineState) {
    let mut tokens = Vec::new();

    // Inside a fenced code block.
    if let LineState::Str(kind) = entry {
        let fence = kind.delimiter();
        if let Some(position) = line.find(fence) {
            let stop = position + fence.len();
            push(&mut tokens, 0, stop, TokenKind::String);
            match scan_inline(syntax, line, stop, &mut tokens) {
                Some(state) => return (tokens, state),
                None => return (tokens, LineState::Normal),
            }
        }
        push(&mut tokens, 0, line.len(), TokenKind::String);
        return (tokens, LineState::Str(kind));
    }

    let trimmed_start = line.len() - line.trim_start().len();
    let trimmed = line[trimmed_start..].trim_end();

    if trimmed.starts_with("```") {
        push(&mut tokens, 0, line.len(), TokenKind::String);
        let closing = trimmed.matches("```").count() > 1;
        let state = if closing {
            LineState::Normal
        } else {
            LineState::Str(StrKind::TripleBacktick)
        };
        return (tokens, state);
    }

    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    if (1..=6).contains(&hashes) && (trimmed.len() == hashes || trimmed[hashes..].starts_with(' '))
    {
        push(&mut tokens, 0, line.len(), TokenKind::Heading);
        return (tokens, LineState::Normal);
    }

    match scan_inline(syntax, line, 0, &mut tokens) {
        Some(state) => (tokens, state),
        None => (tokens, LineState::Normal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::Language;

    fn tokenize(language: Language, line: &str) -> Vec<(TokenKind, String)> {
        let (tokens, _) = tokenize_line(language.syntax(), line, LineState::Normal);
        tokens
            .into_iter()
            .map(|span| (span.kind, line[span.start..span.end].to_owned()))
            .collect()
    }

    #[test]
    fn classifies_rust_line() {
        let tokens = tokenize(Language::Rust, "pub fn parse(input: &str) -> usize { 42 }");
        assert!(tokens.contains(&(TokenKind::Keyword, "pub".to_owned())));
        assert!(tokens.contains(&(TokenKind::Keyword, "fn".to_owned())));
        assert!(tokens.contains(&(TokenKind::Function, "parse".to_owned())));
        assert!(tokens.contains(&(TokenKind::Type, "usize".to_owned())));
        assert!(tokens.contains(&(TokenKind::Number, "42".to_owned())));
    }

    #[test]
    fn classifies_macro_and_attribute() {
        let tokens = tokenize(Language::Rust, "#[derive(Debug)] let x = vec![1];");
        assert!(tokens.contains(&(TokenKind::Attribute, "#[derive(Debug)]".to_owned())));
        assert!(tokens.contains(&(TokenKind::Macro, "vec!".to_owned())));

        let tokens = tokenize(Language::Java, "@Override public void run() {}");
        assert!(tokens.contains(&(TokenKind::Attribute, "@Override".to_owned())));
    }

    #[test]
    fn sql_is_case_insensitive() {
        let tokens = tokenize(Language::Sql, "SELECT id FROM users WHERE id = 1");
        assert!(tokens.contains(&(TokenKind::Keyword, "SELECT".to_owned())));
        assert!(tokens.contains(&(TokenKind::Keyword, "FROM".to_owned())));
    }

    #[test]
    fn yaml_and_json_keys() {
        let tokens = tokenize(Language::Yaml, "name: \"value\"");
        assert!(tokens.contains(&(TokenKind::Key, "name".to_owned())));
        assert!(tokens.contains(&(TokenKind::String, "\"value\"".to_owned())));

        let tokens = tokenize(Language::Json, "\"key\": 12");
        assert!(tokens.contains(&(TokenKind::Key, "\"key\"".to_owned())));
        assert!(tokens.contains(&(TokenKind::Number, "12".to_owned())));
    }

    #[test]
    fn html_tags_attributes_and_values() {
        let tokens = tokenize(Language::Html, "<a href=\"x\" title='y'>link</a>");
        assert!(tokens.contains(&(TokenKind::TagName, "a".to_owned())));
        assert!(tokens.contains(&(TokenKind::Attribute, "href".to_owned())));
        assert!(tokens.contains(&(TokenKind::String, "\"x\"".to_owned())));
    }

    #[test]
    fn markdown_headings_and_fences() {
        let tokens = tokenize(Language::Markdown, "## Heading");
        assert_eq!(tokens, vec![(TokenKind::Heading, "## Heading".to_owned())]);

        let (tokens, state) =
            tokenize_line(Language::Markdown.syntax(), "```rust", LineState::Normal);
        assert_eq!(state, LineState::Str(StrKind::TripleBacktick));
        assert_eq!(tokens.len(), 1);

        let (tokens, state) = tokenize_line(
            Language::Markdown.syntax(),
            "let x = 1;",
            LineState::Str(StrKind::TripleBacktick),
        );
        assert_eq!(state, LineState::Str(StrKind::TripleBacktick));
        assert_eq!(tokens[0].kind, TokenKind::String);
    }

    #[test]
    fn unterminated_string_reports_state() {
        let (tokens, state) =
            tokenize_line(Language::Rust.syntax(), "let s = \"open", LineState::Normal);
        assert_eq!(state, LineState::Str(StrKind::Double));
        assert_eq!(tokens.last().unwrap().kind, TokenKind::String);
    }

    #[test]
    fn c_preprocessor_lines_are_macros() {
        let tokens = tokenize(Language::C, "#include <stdio.h>");
        assert_eq!(tokens[0], (TokenKind::Macro, "#include".to_owned()));
    }

    #[test]
    fn unicode_text_never_panics() {
        for language in Language::ALL {
            let line = "let café = \"héllo ♫\" + 1; // ünïcode";
            let (tokens, _) = tokenize_line(language.syntax(), line, LineState::Normal);
            for span in tokens {
                assert!(span.end <= line.len(), "{language:?}");
                assert!(line.is_char_boundary(span.start), "{language:?}");
                assert!(line.is_char_boundary(span.end), "{language:?}");
            }
        }
    }
}
