//! Line-oriented editing commands, the muscle behind Notepad++'s Edit menu.
//!
//! Every command mutates the [`TextBuffer`] and returns the selection (byte range) the UI
//! should apply afterwards, so caret handling stays in one place.

use std::ops::Range;

use crate::buffer::TextBuffer;

/// Byte range to select after an operation (an empty range means "just the caret").
pub type Selection = Range<usize>;

/// Text case conversion modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseMode {
    Upper,
    Lower,
    Title,
}

/// Byte range of `line`, including its trailing newline when it has one.
pub fn line_span_including_newline(buffer: &TextBuffer, line: usize) -> Range<usize> {
    let start = buffer.line_start(line);
    let end = if line + 1 < buffer.line_count() {
        buffer.line_start(line + 1)
    } else {
        buffer.len()
    };
    start..end.max(start)
}

/// Line range touched by a selection: `(first_line, last_line)`, inclusive.
pub fn line_span_of_selection(buffer: &TextBuffer, selection: &Selection) -> (usize, usize) {
    let start = buffer.clamp_offset(selection.start);
    let end = buffer.clamp_offset(selection.end.max(selection.start));
    let first = buffer.line_of_offset(start);
    let mut last = buffer.line_of_offset(end);
    // A selection ending exactly at the start of a line does not include that line.
    if last > first && end == buffer.line_start(last) {
        last -= 1;
    }
    (first, last)
}

fn caret_column_bytes(buffer: &TextBuffer, caret: usize) -> usize {
    caret.saturating_sub(buffer.line_start(buffer.line_of_offset(caret)))
}

/// Duplicates the caret's line below itself and moves the caret into the copy.
pub fn duplicate_line(buffer: &mut TextBuffer, caret: usize) -> Selection {
    let caret = buffer.clamp_offset(caret);
    let line = buffer.line_of_offset(caret);
    let column = caret_column_bytes(buffer, caret);
    let span = line_span_including_newline(buffer, line);
    let line_text = buffer.line(line).to_owned();
    let has_newline = buffer.as_str()[span.clone()].ends_with('\n');

    let (insertion, copy_offset) = if has_newline {
        (format!("{line_text}\n"), span.end + column)
    } else {
        (format!("\n{line_text}"), span.end + 1 + column)
    };
    buffer.insert(span.end, &insertion);
    copy_offset..copy_offset
}

/// Deletes the caret's line (and its newline) and returns the caret position.
pub fn delete_line(buffer: &mut TextBuffer, caret: usize) -> Selection {
    let caret = buffer.clamp_offset(caret);
    let line = buffer.line_of_offset(caret);

    if buffer.line_count() == 1 {
        buffer.replace(0..buffer.len(), "");
        return 0..0;
    }

    let span = line_span_including_newline(buffer, line);
    let removal = if !buffer.as_str()[span.clone()].ends_with('\n') && line > 0 {
        // Last line without a trailing newline: also swallow the newline before it.
        buffer.line_end(line - 1)..span.end
    } else {
        span
    };
    buffer.remove(removal);

    let line = line.min(buffer.line_count().saturating_sub(1));
    let offset = buffer.line_start(line);
    offset..offset
}

/// Swaps the caret's line with the one above it.
pub fn move_line_up(buffer: &mut TextBuffer, caret: usize) -> Selection {
    let caret = buffer.clamp_offset(caret);
    let line = buffer.line_of_offset(caret);
    if line == 0 {
        return caret..caret;
    }
    let column = caret_column_bytes(buffer, caret);
    let current = line_span_including_newline(buffer, line);
    let previous = line_span_including_newline(buffer, line - 1);
    let current_text = buffer.as_str()[current.clone()].to_owned();
    let previous_text = buffer.as_str()[previous.clone()].to_owned();

    let combined = if current_text.ends_with('\n') {
        format!("{current_text}{previous_text}")
    } else {
        format!("{current_text}\n{}", previous_text.trim_end_matches('\n'))
    };
    buffer.replace(previous.start..current.end, &combined);

    let offset = (buffer.line_start(line - 1) + column).min(buffer.len());
    offset..offset
}

/// Swaps the caret's line with the one below it.
pub fn move_line_down(buffer: &mut TextBuffer, caret: usize) -> Selection {
    let caret = buffer.clamp_offset(caret);
    let line = buffer.line_of_offset(caret);
    if line + 1 >= buffer.line_count() {
        return caret..caret;
    }
    let column = caret_column_bytes(buffer, caret);
    let current = line_span_including_newline(buffer, line);
    let next = line_span_including_newline(buffer, line + 1);
    let current_text = buffer.as_str()[current.clone()].to_owned();
    let next_text = buffer.as_str()[next.clone()].to_owned();

    let combined = if next_text.ends_with('\n') {
        format!("{next_text}{current_text}")
    } else {
        format!("{next_text}\n{}", current_text.trim_end_matches('\n'))
    };
    buffer.replace(current.start..next.end, &combined);

    let offset = (buffer.line_start(line + 1) + column).min(buffer.len());
    offset..offset
}

/// Comments or uncomments every line touched by `selection`.
///
/// If every non-empty line already starts with `token`, the token is removed; otherwise
/// it is added. The affected lines stay selected.
pub fn toggle_comment(buffer: &mut TextBuffer, selection: Selection, token: &str) -> Selection {
    if token.is_empty() {
        return selection;
    }
    let start = buffer.clamp_offset(selection.start);
    let end = buffer.clamp_offset(selection.end.max(selection.start));
    let (first_line, last_line) = line_span_of_selection(buffer, &(start..end));

    let mut uncommenting = true;
    let mut any_content = false;
    for line in first_line..=last_line {
        let text = buffer.line(line);
        if text.trim().is_empty() {
            continue;
        }
        any_content = true;
        if !text.trim_start().starts_with(token) {
            uncommenting = false;
            break;
        }
    }
    if !any_content {
        return selection;
    }

    // Work bottom-up so earlier offsets stay valid.
    for line in (first_line..=last_line).rev() {
        let text = buffer.line(line).to_owned();
        if uncommenting {
            let indent_len = text.len() - text.trim_start().len();
            if text[indent_len..].starts_with(token) {
                let mut remove_len = token.len();
                if text[indent_len + token.len()..].starts_with(' ') {
                    remove_len += 1;
                }
                let line_start = buffer.line_start(line);
                buffer.remove(line_start + indent_len..line_start + indent_len + remove_len);
            }
        } else if !text.trim().is_empty() {
            let indent_len = text.len() - text.trim_start().len();
            let insert_at = buffer.line_start(line) + indent_len;
            buffer.insert(insert_at, &format!("{token} "));
        }
    }

    if start == end {
        let line = first_line.min(buffer.line_count().saturating_sub(1));
        let line_start = buffer.line_start(line);
        let adjusted = if uncommenting {
            start.saturating_sub(token.len() + 1)
        } else {
            start + token.len() + 1
        };
        let adjusted = adjusted.max(line_start).min(buffer.line_end(line));
        adjusted..adjusted
    } else {
        let last_line = last_line.min(buffer.line_count().saturating_sub(1));
        buffer.line_start(first_line)..buffer.line_end(last_line)
    }
}

/// Prepends `indent` to every line touched by the selection.
pub fn indent_lines(buffer: &mut TextBuffer, selection: Selection, indent: &str) -> Selection {
    if indent.is_empty() {
        return selection;
    }
    let start = buffer.clamp_offset(selection.start);
    let end = buffer.clamp_offset(selection.end.max(selection.start));
    let (first_line, last_line) = line_span_of_selection(buffer, &(start..end));

    for line in (first_line..=last_line).rev() {
        if buffer.line(line).is_empty() {
            continue;
        }
        buffer.insert(buffer.line_start(line), indent);
    }

    let caret = start + indent.len();
    if start == end {
        caret..caret
    } else {
        caret..end + indent.len() * (last_line - first_line + 1)
    }
}

/// Removes one level of leading indentation (a tab, or up to `width` spaces).
pub fn dedent_lines(buffer: &mut TextBuffer, selection: Selection, width: usize) -> Selection {
    let start = buffer.clamp_offset(selection.start);
    let end = buffer.clamp_offset(selection.end.max(selection.start));
    let (first_line, last_line) = line_span_of_selection(buffer, &(start..end));
    let mut removed_total = 0usize;
    let mut removed_first = 0usize;

    for line in (first_line..=last_line).rev() {
        let text = buffer.line(line).to_owned();
        let mut remove_len = 0usize;
        for ch in text.chars().take(width.max(1)) {
            if ch == ' ' {
                remove_len += 1;
            } else if ch == '\t' && remove_len == 0 {
                remove_len += 1;
                break;
            } else {
                break;
            }
        }
        if remove_len > 0 {
            let line_start = buffer.line_start(line);
            buffer.remove(line_start..line_start + remove_len);
            removed_total += remove_len;
            if line == first_line {
                removed_first = remove_len;
            }
        }
    }

    if start == end {
        let caret = start.saturating_sub(removed_first);
        caret..caret
    } else {
        start.saturating_sub(removed_first)..end.saturating_sub(removed_total)
    }
}

/// Removes trailing spaces and tabs from every line. Returns the number of changed lines.
pub fn trim_trailing_whitespace(buffer: &mut TextBuffer) -> usize {
    let mut changed = 0usize;
    for line in (0..buffer.line_count()).rev() {
        let text = buffer.line(line).to_owned();
        let trimmed_len = text.trim_end_matches([' ', '\t']).len();
        if trimmed_len != text.len() {
            let line_start = buffer.line_start(line);
            buffer.remove(line_start + trimmed_len..line_start + text.len());
            changed += 1;
        }
    }
    changed
}

/// Sorts the lines touched by the selection alphabetically.
pub fn sort_lines(buffer: &mut TextBuffer, selection: Selection, descending: bool) -> Selection {
    let start = buffer.clamp_offset(selection.start);
    let end = buffer.clamp_offset(selection.end.max(selection.start));
    let (first_line, last_line) = line_span_of_selection(buffer, &(start..end));
    if first_line == last_line {
        return selection;
    }

    let mut lines: Vec<String> = (first_line..=last_line)
        .map(|line| buffer.line(line).to_owned())
        .collect();
    lines.sort_by(|a, b| {
        let ordering = a.to_lowercase().cmp(&b.to_lowercase());
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });

    let replacement = lines.join("\n");
    let range = buffer.line_start(first_line)..buffer.line_end(last_line);
    buffer.replace(range, &replacement);
    buffer.line_start(first_line)..buffer.line_end(last_line)
}

/// Changes the case of the selection, or of the whole line when there is no selection.
pub fn convert_case(buffer: &mut TextBuffer, selection: Selection, mode: CaseMode) -> Selection {
    let start = buffer.clamp_offset(selection.start);
    let end = buffer.clamp_offset(selection.end.max(selection.start));
    let range = if start == end {
        let line = buffer.line_of_offset(start);
        buffer.line_range(line)
    } else {
        start..end
    };
    if range.is_empty() {
        return selection;
    }

    let original = buffer.as_str()[range.clone()].to_owned();
    let converted = match mode {
        CaseMode::Upper => original.to_uppercase(),
        CaseMode::Lower => original.to_lowercase(),
        CaseMode::Title => original
            .split_inclusive([' ', '\t', '\n'])
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => {
                        let mut out = first.to_uppercase().to_string();
                        out.push_str(&chars.as_str().to_lowercase());
                        out
                    }
                    None => String::new(),
                }
            })
            .collect(),
    };
    buffer.replace(range.clone(), &converted);
    range.start..range.start + converted.len()
}

/// Converts leading tabs to `width` spaces, document-wide. Returns changed line count.
pub fn tabs_to_spaces(buffer: &mut TextBuffer, width: usize) -> usize {
    convert_indentation(buffer, width, true)
}

/// Converts leading groups of `width` spaces to tabs, document-wide.
pub fn spaces_to_tabs(buffer: &mut TextBuffer, width: usize) -> usize {
    convert_indentation(buffer, width, false)
}

fn convert_indentation(buffer: &mut TextBuffer, width: usize, tabs_to_spaces: bool) -> usize {
    let width = width.max(1);
    let mut converted = 0usize;

    for line in (0..buffer.line_count()).rev() {
        let text = buffer.line(line).to_owned();
        let indent_chars = text
            .chars()
            .take_while(|ch| *ch == '\t' || *ch == ' ')
            .count();
        let indent = &text[..indent_chars];

        if (tabs_to_spaces && !indent.contains('\t')) || (!tabs_to_spaces && !indent.contains(' '))
        {
            continue;
        }

        let replacement = if tabs_to_spaces {
            indent
                .chars()
                .map(|ch| {
                    if ch == '\t' {
                        " ".repeat(width)
                    } else {
                        " ".to_string()
                    }
                })
                .collect::<String>()
        } else {
            expand_spaces_to_tabs(indent, width)
        };

        let line_start = buffer.line_start(line);
        buffer.replace(line_start..line_start + indent.len(), &replacement);
        converted += 1;
    }

    converted
}

fn expand_spaces_to_tabs(indent: &str, width: usize) -> String {
    let mut out = String::new();
    let mut pending_spaces = 0usize;
    for ch in indent.chars() {
        match ch {
            ' ' => {
                pending_spaces += 1;
                if pending_spaces == width {
                    out.push('\t');
                    pending_spaces = 0;
                }
            }
            _ => {
                out.push_str(&" ".repeat(pending_spaces));
                pending_spaces = 0;
                out.push(ch);
            }
        }
    }
    out.push_str(&" ".repeat(pending_spaces));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(text: &str) -> TextBuffer {
        TextBuffer::from_string(text.to_owned())
    }

    #[test]
    fn duplicate_line_copies_below_and_moves_caret() {
        let mut buffer = buffer("one\ntwo\nthree");
        let selection = duplicate_line(&mut buffer, 4); // start of "two"
        assert_eq!(buffer.as_str(), "one\ntwo\ntwo\nthree");
        assert_eq!(selection.start, 8, "caret moves into the duplicate");
        assert_eq!(buffer.line_of_offset(selection.start), 2);
    }

    #[test]
    fn duplicate_line_on_last_line_without_newline() {
        let mut buffer = buffer("only");
        let selection = duplicate_line(&mut buffer, 2);
        assert_eq!(buffer.as_str(), "only\nonly");
        assert_eq!(selection.start, 7);
        assert_eq!(buffer.line_of_offset(selection.start), 1);
    }

    #[test]
    fn delete_line_removes_line_and_newline() {
        let mut buffer = buffer("one\ntwo\nthree");
        let offset = delete_line(&mut buffer, 4);
        assert_eq!(buffer.as_str(), "one\nthree");
        assert_eq!(offset.start, 4);

        let offset = delete_line(&mut buffer, 4);
        assert_eq!(buffer.as_str(), "one");
        assert_eq!(offset.start, 0);
    }

    #[test]
    fn delete_last_line_swallows_previous_newline() {
        let mut buffer = buffer("one\ntwo");
        delete_line(&mut buffer, 5);
        assert_eq!(buffer.as_str(), "one");
    }

    #[test]
    fn deleting_the_only_line_empties_the_buffer() {
        let mut buffer = buffer("only");
        let offset = delete_line(&mut buffer, 2);
        assert_eq!(buffer.as_str(), "");
        assert_eq!(offset.start, 0);
    }

    #[test]
    fn move_line_up_and_down() {
        let mut buffer = buffer("one\ntwo\nthree");
        let caret = move_line_up(&mut buffer, 5).start; // inside "two"
        assert_eq!(buffer.as_str(), "two\none\nthree");
        assert_eq!(buffer.line_of_offset(caret), 0);

        let caret = move_line_down(&mut buffer, caret).start;
        assert_eq!(buffer.as_str(), "one\ntwo\nthree");
        assert_eq!(buffer.line_of_offset(caret), 1);
    }

    #[test]
    fn move_last_line_up_keeps_newlines_valid() {
        let mut buffer = buffer("one\ntwo");
        move_line_up(&mut buffer, 5);
        assert_eq!(buffer.as_str(), "two\none");
        assert_eq!(buffer.line_count(), 2);
    }

    #[test]
    fn toggle_comment_adds_and_removes() {
        let mut buffer = buffer("fn a() {}\nfn b() {}\n");
        let selection = toggle_comment(&mut buffer, 0..0, "//");
        assert_eq!(buffer.as_str(), "// fn a() {}\nfn b() {}\n");
        assert!(selection.start > 0);

        toggle_comment(&mut buffer, 0..0, "//");
        assert_eq!(buffer.as_str(), "fn a() {}\nfn b() {}\n");
    }

    #[test]
    fn toggle_comment_handles_ranges_and_blank_lines() {
        let mut buffer = buffer("a\n\nb\n");
        toggle_comment(&mut buffer, 0..4, "#");
        assert_eq!(buffer.as_str(), "# a\n\n# b\n");
        toggle_comment(&mut buffer, 0..8, "#");
        assert_eq!(buffer.as_str(), "a\n\nb\n");
    }

    #[test]
    fn indent_and_dedent_selection() {
        let mut buffer = buffer("a\nb\n");
        let selection = indent_lines(&mut buffer, 0..4, "  ");
        assert_eq!(buffer.as_str(), "  a\n  b\n");
        assert!(selection.start >= 2);

        dedent_lines(&mut buffer, 0..8, 2);
        assert_eq!(buffer.as_str(), "a\nb\n");
    }

    #[test]
    fn trim_trailing_whitespace_counts_lines() {
        let mut buffer = buffer("a   \nb\t\nc\n");
        assert_eq!(trim_trailing_whitespace(&mut buffer), 2);
        assert_eq!(buffer.as_str(), "a\nb\nc\n");
    }

    #[test]
    fn sort_lines_is_case_insensitive() {
        let mut buffer = buffer("banana\nApple\ncherry\n");
        sort_lines(&mut buffer, 0..19, false);
        assert_eq!(buffer.as_str(), "Apple\nbanana\ncherry\n");

        sort_lines(&mut buffer, 0..19, true);
        assert_eq!(buffer.as_str(), "cherry\nbanana\nApple\n");
    }

    #[test]
    fn convert_case_modes() {
        let mut buffer = buffer("hello world");
        convert_case(&mut buffer, 0..11, CaseMode::Upper);
        assert_eq!(buffer.as_str(), "HELLO WORLD");

        convert_case(&mut buffer, 0..11, CaseMode::Lower);
        assert_eq!(buffer.as_str(), "hello world");

        convert_case(&mut buffer, 0..11, CaseMode::Title);
        assert_eq!(buffer.as_str(), "Hello World");
    }

    #[test]
    fn tab_and_space_conversion_round_trips() {
        let mut buffer = buffer("\tx\n\ty\n");
        assert_eq!(tabs_to_spaces(&mut buffer, 4), 2);
        assert_eq!(buffer.as_str(), "    x\n    y\n");

        assert_eq!(spaces_to_tabs(&mut buffer, 4), 2);
        assert_eq!(buffer.as_str(), "\tx\n\ty\n");
    }

    #[test]
    fn selection_line_span_excludes_next_line() {
        let buffer = buffer("a\nb\nc");
        assert_eq!(line_span_of_selection(&buffer, &(0..4)), (0, 1));
        assert_eq!(line_span_of_selection(&buffer, &(0..0)), (0, 0));
        assert_eq!(line_span_of_selection(&buffer, &(0..1)), (0, 0));
    }
}
