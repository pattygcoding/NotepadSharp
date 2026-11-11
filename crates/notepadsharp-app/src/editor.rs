//! The editor pane: syntax-highlighted text, a line-number gutter, current-line and
//! search-match overlays.
//!
//! `egui`'s [`TextEdit`] does the hard caret/selection/IME work; this module supplies a
//! cached highlighting layouter, records undo steps as precise deltas, and paints the
//! Notepad++-style decorations on top.

use std::ops::Range;
use std::sync::Arc;

use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use egui::{
    Align, Color32, CornerRadius, FontId, Galley, Id, Margin, Pos2, Rect, ScrollArea, Sense,
    Stroke, TextEdit, Ui, Vec2,
};
use notepadsharp_core::{Document, EditDelta, Highlighter};

use crate::theme::Palette;

/// Everything the editor needs for one frame.
pub struct EditorInput<'a> {
    pub id: Id,
    pub document: &'a mut Document,
    pub highlighter: &'a mut Highlighter,
    pub palette: &'a Palette,
    pub font_size: f32,
    pub word_wrap: bool,
    pub show_line_numbers: bool,
    pub highlight_current_line: bool,
    /// Search matches as document-wide **char** ranges, for highlighting.
    pub matches: &'a [(usize, usize)],
    pub active_match: Option<(usize, usize)>,
    /// Line index to tint as "current".
    pub current_line: usize,
    /// Byte range to select (and scroll to) this frame.
    pub pending_selection: Option<(usize, usize)>,
}

/// What the editor reports back.
pub struct EditorOutput {
    pub response: egui::Response,
    /// Selection as byte offsets `(min, max)`.
    pub selection: Option<(usize, usize)>,
    /// Primary caret as a byte offset.
    pub caret: usize,
}

struct RawOutput {
    response: egui::Response,
    /// `(primary, min, max)` char indices of the selection.
    cursor: Option<(usize, usize, usize)>,
}

/// Adapts the core (byte-offset) buffer to `egui`'s char-index `TextBuffer` trait, so every
/// edit egui makes is recorded as a precise undo step.
struct EditorBuffer<'a> {
    document: &'a mut Document,
}

pub fn show(ui: &mut Ui, input: EditorInput<'_>) -> EditorOutput {
    let EditorInput {
        id,
        document,
        highlighter,
        palette,
        font_size,
        word_wrap,
        show_line_numbers,
        highlight_current_line,
        matches,
        active_match,
        current_line,
        pending_selection,
    } = input;

    let font_id = FontId::monospace(font_size);
    let digits = document.line_count().max(1).to_string().len().max(2);
    let digit_width = ui.fonts(|fonts| fonts.glyph_width(&font_id, '0'));
    let gutter_width = if show_line_numbers {
        digit_width * digits as f32 + 20.0
    } else {
        0.0
    };

    // Move the caret before the widget runs so this very frame shows the jump.
    if let Some((start, end)) = pending_selection {
        let mut state =
            egui::widgets::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
        let char_start = document.buffer.char_index_of_offset(start);
        let char_end = document.buffer.char_index_of_offset(end);
        state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(char_start),
            CCursor::new(char_end),
        )));
        state.store(ui.ctx(), id);
        ui.memory_mut(|memory| memory.request_focus(id));
    }

    let mut layouter = |ui: &Ui, text: &str, wrap_width: f32| -> Arc<Galley> {
        highlighter.update(text);
        let job = build_job(text, highlighter, palette, &font_id, word_wrap, wrap_width);
        ui.fonts(|fonts| fonts.layout_job(job))
    };

    let scroll = if word_wrap {
        ScrollArea::vertical()
    } else {
        ScrollArea::both()
    };

    let raw = scroll
        .id_salt(("editor_body", id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                if show_line_numbers {
                    ui.allocate_exact_size(Vec2::new(gutter_width, 0.0), Sense::hover());
                }
                let desired_width = if word_wrap {
                    ui.available_width()
                } else {
                    f32::INFINITY
                };

                let output = {
                    let mut buffer = EditorBuffer {
                        document: &mut *document,
                    };
                    TextEdit::multiline(&mut buffer)
                        .id(id)
                        .font(font_id.clone())
                        .desired_width(desired_width)
                        .desired_rows(1)
                        .frame(false)
                        .background_color(Color32::TRANSPARENT)
                        .margin(Margin::symmetric(6, 4))
                        .layouter(&mut layouter)
                        .show(ui)
                };

                #[allow(clippy::too_many_arguments)]
                paint(
                    ui,
                    &output,
                    palette,
                    &font_id,
                    gutter_width,
                    show_line_numbers,
                    highlight_current_line,
                    matches,
                    active_match,
                    current_line,
                );

                if pending_selection.is_some() {
                    let index = output
                        .cursor_range
                        .map(|range| range.primary.ccursor.index)
                        .unwrap_or(0);
                    let caret_rect = output
                        .galley
                        .pos_from_ccursor(CCursor::new(index))
                        .translate(output.galley_pos.to_vec2());
                    ui.scroll_to_rect(caret_rect, Some(Align::Center));
                }

                let cursor = output.cursor_range.map(|range| {
                    let [min, max] = range.sorted_cursors();
                    (
                        range.primary.ccursor.index,
                        min.ccursor.index,
                        max.ccursor.index,
                    )
                });

                RawOutput {
                    response: output.response,
                    cursor,
                }
            })
            .inner
        })
        .inner;

    let selection = raw.cursor.map(|(_, min, max)| {
        let first = document.buffer.offset_of_char_index(min);
        let second = document.buffer.offset_of_char_index(max);
        (first.min(second), first.max(second))
    });
    let caret = raw
        .cursor
        .map(|(primary, _, _)| document.buffer.offset_of_char_index(primary))
        .unwrap_or(0);

    EditorOutput {
        response: raw.response,
        selection,
        caret,
    }
}

impl egui::TextBuffer for EditorBuffer<'_> {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        self.document.buffer.as_str()
    }

    fn insert_text(&mut self, text: &str, char_index: usize) -> usize {
        if text.is_empty() {
            return 0;
        }
        let offset = self.document.buffer.offset_of_char_index(char_index);
        self.document.history.record(EditDelta::new(
            offset..offset,
            String::new(),
            text.to_owned(),
        ));
        self.document.buffer.insert(offset, text);
        text.chars().count()
    }

    fn delete_char_range(&mut self, char_range: Range<usize>) {
        let start = self.document.buffer.offset_of_char_index(char_range.start);
        let end = self
            .document
            .buffer
            .offset_of_char_index(char_range.end.max(char_range.start));
        if end > start {
            let removed = self.document.buffer.as_str()[start..end].to_owned();
            self.document
                .history
                .record(EditDelta::new(start..end, removed, String::new()));
            self.document.buffer.remove(start..end);
        }
    }

    fn clear(&mut self) {
        let chars = self.document.buffer.as_str().chars().count();
        self.delete_char_range(0..chars);
    }

    fn replace_with(&mut self, text: &str) {
        self.clear();
        self.insert_text(text, 0);
    }
}

/// Builds the syntax-highlighted layout for one frame of text.
fn build_job(
    text: &str,
    highlighter: &Highlighter,
    palette: &Palette,
    font_id: &FontId,
    word_wrap: bool,
    wrap_width: f32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = if word_wrap { wrap_width } else { f32::INFINITY };

    let format = |color: Color32| TextFormat {
        font_id: font_id.clone(),
        color,
        ..Default::default()
    };

    let lines: Vec<&str> = text.split('\n').collect();
    let last = lines.len().saturating_sub(1);

    for (index, line) in lines.iter().enumerate() {
        let line_len = line.len();
        let mut cursor = 0usize;

        for span in highlighter.tokens_for_line(index) {
            let start = clamp_boundary(line, span.start);
            let end = clamp_boundary(line, span.end);
            if start > cursor {
                job.append(&line[cursor..start], 0.0, format(palette.text));
            }
            if end > start.max(cursor) {
                job.append(
                    &line[start.max(cursor)..end],
                    0.0,
                    format(palette.token(span.kind)),
                );
            }
            cursor = cursor.max(end);
        }

        if cursor < line_len {
            job.append(&line[cursor..line_len], 0.0, format(palette.text));
        }
        if index != last {
            job.append("\n", 0.0, format(palette.text));
        }
    }

    job
}

fn clamp_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Screen rectangle covering the character range `start..end`.
fn match_rect(galley: &Galley, origin: Pos2, start: usize, end: usize) -> Option<Rect> {
    let from = galley.pos_from_ccursor(CCursor::new(start));
    let to = galley.pos_from_ccursor(CCursor::new(end));

    let left = origin.x + from.left();
    let right = if (from.top() - to.top()).abs() < 0.5 {
        origin.x + to.left()
    } else {
        // A match spanning rows: highlight the first row only, which is what is readable.
        origin.x + from.right() + 4.0
    };
    let top = origin.y + from.top();
    let bottom = origin.y + from.bottom();
    if bottom <= top {
        return None;
    }
    Some(Rect::from_min_max(
        Pos2::new(left, top),
        Pos2::new(right.max(left + 2.0), bottom),
    ))
}

#[allow(clippy::too_many_arguments)]
fn paint(
    ui: &Ui,
    output: &egui::widgets::text_edit::TextEditOutput,
    palette: &Palette,
    font_id: &FontId,
    gutter_width: f32,
    show_line_numbers: bool,
    highlight_current_line: bool,
    matches: &[(usize, usize)],
    active_match: Option<(usize, usize)>,
    current_line: usize,
) {
    let painter = ui.painter();
    let origin = output.galley_pos;
    let clip = ui.clip_rect();

    // First row of every logical line, in screen space.
    let mut line_tops: Vec<(usize, Rect)> = Vec::new();
    let mut line = 0usize;
    let mut at_line_start = true;
    for row in &output.galley.rows {
        if at_line_start {
            line_tops.push((line, row.rect.translate(origin.to_vec2())));
            at_line_start = false;
        }
        if row.ends_with_newline {
            line += 1;
            at_line_start = true;
        }
    }

    if highlight_current_line {
        if let Some((_, rect)) = line_tops.iter().find(|(index, _)| *index == current_line) {
            let bottom = line_tops
                .iter()
                .find(|(index, _)| *index == current_line + 1)
                .map(|(_, next)| next.top())
                .unwrap_or(rect.bottom());
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(clip.left(), rect.top()),
                    Pos2::new(clip.right(), bottom.max(rect.top() + 1.0)),
                ),
                CornerRadius::ZERO,
                palette.current_line,
            );
        }
    }

    if show_line_numbers {
        let gutter_right = origin.x - 6.0;
        let gutter_left = gutter_right - gutter_width;
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(gutter_left, clip.top()),
                Pos2::new(gutter_right, clip.bottom()),
            ),
            CornerRadius::ZERO,
            palette.gutter_background,
        );
        painter.line_segment(
            [
                Pos2::new(gutter_right + 2.0, clip.top()),
                Pos2::new(gutter_right + 2.0, clip.bottom()),
            ],
            Stroke::new(1.0_f32, palette.gutter_text.gamma_multiply(0.4)),
        );

        for (index, rect) in &line_tops {
            if rect.bottom() < clip.top() || rect.top() > clip.bottom() {
                continue;
            }
            painter.text(
                Pos2::new(gutter_right - 4.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                (index + 1).to_string(),
                font_id.clone(),
                palette.gutter_text,
            );
        }
    }

    for (start, end) in matches {
        if let Some(rect) = match_rect(&output.galley, origin, *start, *end) {
            painter.rect_filled(rect, CornerRadius::same(2), palette.match_fill);
        }
    }
    if let Some((start, end)) = active_match {
        if let Some(rect) = match_rect(&output.galley, origin, start, end) {
            painter.rect_filled(rect, CornerRadius::same(2), palette.active_match_fill);
        }
    }
}
