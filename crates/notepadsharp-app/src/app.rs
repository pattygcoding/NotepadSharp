//! The application: window layout, menus, shortcuts and command dispatch.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Context, Key, Modifiers, RichText, ViewportCommand};
use notepadsharp_core::lineops::{self, CaseMode};
use notepadsharp_core::search::{SearchMatch, SearchQuery};
use notepadsharp_core::session::{Prefs, Session, Theme};
use notepadsharp_core::{Document, Encoding, Eol, Highlighter, Language, TextBuffer};

use crate::editor::{self, EditorInput};
use crate::find_panel::{self, FindAction, FindState};
use crate::shortcuts::Shortcuts;
use crate::tabs::{self, TabAction};
use crate::theme::{self, Palette};

/// One open document plus its view state.
pub struct Tab {
    pub document: Document,
    pub highlighter: Highlighter,
    pub caret: usize,
    pub selection: Option<(usize, usize)>,
    /// Byte range the editor should select (and scroll to) on the next frame.
    pub pending_selection: Option<(usize, usize)>,
}

impl Tab {
    pub fn new(document: Document) -> Self {
        let highlighter = Highlighter::new(document.language);
        Self {
            document,
            highlighter,
            caret: 0,
            selection: None,
            pending_selection: None,
        }
    }

    fn select(&mut self, start: usize, end: usize) {
        self.pending_selection = Some((start, end.max(start)));
    }

    fn put_caret(&mut self, offset: usize) {
        self.pending_selection = Some((offset, offset));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CloseTarget {
    Tab(usize),
    All,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dialog {
    None,
    Unsaved(CloseTarget),
    About,
}

pub struct NotepadSharpApp {
    tabs: Vec<Tab>,
    active: usize,
    prefs: Prefs,
    session: Session,
    session_path: PathBuf,
    find: FindState,
    dialog: Dialog,
    shortcuts: Shortcuts,
    palette: Palette,
    status: Option<(String, Instant)>,
    goto_line: Option<String>,
    closed_documents: Vec<Document>,
    /// Document revision the cached search results belong to.
    search_revision: u64,
    applied_style: Option<(Theme, f32)>,
    session_dirty: bool,
    last_session_save: Instant,
    allow_close: bool,
    pending_quit: bool,
    window_size: Option<(f32, f32)>,
}

impl NotepadSharpApp {
    pub fn new(cc: &eframe::CreationContext<'_>, session: Session) -> Self {
        let palette = Palette::for_theme(session.prefs.theme);
        let theme_preference = session.prefs.theme;
        let font_size = session.prefs.font_size;
        theme::apply(&cc.egui_ctx, theme_preference, font_size);

        let mut app = Self {
            tabs: Vec::new(),
            active: 0,
            session_path: Self::session_path(),
            prefs: session.prefs.clone(),
            session,
            find: FindState::default(),
            dialog: Dialog::None,
            shortcuts: Shortcuts::default(),
            palette,
            status: None,
            goto_line: None,
            closed_documents: Vec::new(),
            search_revision: 0,
            applied_style: Some((theme_preference, font_size)),
            session_dirty: false,
            last_session_save: Instant::now(),
            allow_close: false,
            pending_quit: false,
            window_size: None,
        };

        let files = app.session.open_files.clone();
        for path in files {
            app.open_path(path, false);
        }
        if app.tabs.is_empty() {
            app.new_tab();
        }
        app.active = app.session.active.min(app.tabs.len().saturating_sub(1));
        app
    }

    fn session_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("NotepadSharp")
            .join("session.json")
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), Instant::now()));
    }

    fn new_tab(&mut self) {
        self.tabs.push(Tab::new(Document::untitled()));
        self.active = self.tabs.len() - 1;
        self.session_dirty = true;
    }

    /// Opens a file, reusing an existing tab when the path is already open.
    fn open_path(&mut self, path: PathBuf, focus: bool) -> bool {
        if let Some(index) = self
            .tabs
            .iter()
            .position(|tab| tab.document.path.as_deref() == Some(path.as_path()))
        {
            if focus {
                self.active = index;
            }
            return true;
        }
        match Document::open(&path) {
            Ok(document) => {
                self.tabs.push(Tab::new(document));
                if focus {
                    self.active = self.tabs.len() - 1;
                }
                self.session.push_recent(&path);
                self.session_dirty = true;
                self.set_status(format!("Opened {}", path.display()));
                true
            }
            Err(error) => {
                self.set_status(format!("Could not open {}: {error}", path.display()));
                false
            }
        }
    }

    fn open_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title("Open file")
            .add_filter("All files", &["*"])
            .pick_files();
        if let Some(paths) = picked {
            for path in paths {
                self.open_path(path, true);
            }
        }
    }

    /// Saves the tab at `index`, asking for a path when it has none.
    fn save_tab(&mut self, index: usize) -> bool {
        let Some(path) = self
            .tabs
            .get(index)
            .and_then(|tab| tab.document.path.clone())
        else {
            return self.save_tab_as(index);
        };
        let result = match self.tabs.get_mut(index) {
            Some(tab) => tab.document.save_to(path.clone()),
            None => return false,
        };
        match result {
            Ok(()) => {
                self.session.push_recent(&path);
                self.session_dirty = true;
                self.set_status(format!("Saved {}", path.display()));
                true
            }
            Err(error) => {
                self.set_status(format!("Save failed: {error}"));
                false
            }
        }
    }

    fn save_tab_as(&mut self, index: usize) -> bool {
        let Some(tab) = self.tabs.get(index) else {
            return false;
        };
        let mut dialog = rfd::FileDialog::new().set_title("Save as");
        if let Some(path) = &tab.document.path {
            if let Some(parent) = path.parent() {
                dialog = dialog.set_directory(parent);
            }
            if let Some(name) = path.file_name() {
                dialog = dialog.set_file_name(name.to_string_lossy());
            }
        }
        let Some(path) = dialog.save_file() else {
            return false;
        };

        let result = match self.tabs.get_mut(index) {
            Some(tab) => {
                let result = tab.document.save_to(path.clone());
                if result.is_ok() {
                    tab.highlighter.set_language(tab.document.language);
                }
                result
            }
            None => return false,
        };

        match result {
            Ok(()) => {
                self.session.push_recent(&path);
                self.session_dirty = true;
                self.set_status(format!("Saved {}", path.display()));
                true
            }
            Err(error) => {
                self.set_status(format!("Save failed: {error}"));
                false
            }
        }
    }

    fn save_active(&mut self) -> bool {
        self.save_tab(self.active)
    }

    fn save_all(&mut self) {
        for index in 0..self.tabs.len() {
            if self.tabs[index].document.is_dirty() {
                self.save_tab(index);
            }
        }
    }

    fn request_close_tab(&mut self, index: usize) {
        match self.tabs.get(index) {
            Some(tab) if tab.document.is_dirty() => {
                self.dialog = Dialog::Unsaved(CloseTarget::Tab(index));
            }
            Some(_) => self.close_tab_now(index),
            None => {}
        }
    }

    fn close_tab_now(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        let tab = self.tabs.remove(index);
        if tab.document.path.is_some() {
            self.closed_documents.push(tab.document);
            if self.closed_documents.len() > 20 {
                self.closed_documents.remove(0);
            }
        }
        if self.tabs.is_empty() {
            self.new_tab();
            self.active = 0;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if self.active > index {
            self.active -= 1;
        }
        self.session_dirty = true;
    }

    fn close_all_tabs(&mut self) {
        for index in (0..self.tabs.len()).rev() {
            self.close_tab_now(index);
        }
    }

    fn reopen_closed_tab(&mut self) {
        match self.closed_documents.pop() {
            Some(document) => {
                let path = document.path.clone();
                self.tabs.push(Tab::new(document));
                self.active = self.tabs.len() - 1;
                if let Some(path) = path {
                    self.session.push_recent(&path);
                }
                self.session_dirty = true;
            }
            None => self.set_status("No closed tab to reopen"),
        }
    }

    fn any_dirty(&self) -> bool {
        self.tabs.iter().any(|tab| tab.document.is_dirty())
    }

    fn request_quit(&mut self, ctx: &Context) {
        if self.any_dirty() {
            self.dialog = Dialog::Unsaved(CloseTarget::All);
        } else {
            self.finish_quit(ctx);
        }
    }

    fn finish_quit(&mut self, ctx: &Context) {
        self.save_session();
        self.allow_close = true;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    fn save_session(&mut self) {
        self.session.open_files = self
            .tabs
            .iter()
            .filter_map(|tab| tab.document.path.clone())
            .collect();
        self.session.active = self.active;
        self.session.prefs = self.prefs.clone();
        self.session.window_size = self.window_size;
        if let Err(error) = self.session.save_to(&self.session_path) {
            eprintln!("could not save session: {error}");
        }
        self.session_dirty = false;
        self.last_session_save = Instant::now();
    }

    // ----------------------------------------------------------------- editing

    /// Runs an operation on a scratch copy of the active document, applies the result as a
    /// single undoable edit, and restores the selection the operation asks for.
    fn edit_active<F>(&mut self, op: F)
    where
        F: FnOnce(&mut TextBuffer, std::ops::Range<usize>) -> std::ops::Range<usize>,
    {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let selection = tab.selection.unwrap_or((tab.caret, tab.caret));
        let mut working = TextBuffer::from_string(tab.document.text().to_owned());
        let result = op(&mut working, selection.0..selection.1);
        if working.as_str() != tab.document.text() {
            tab.document.replace_all(working.as_str().to_owned());
            tab.pending_selection = Some((result.start, result.end));
        }
    }

    fn undo(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        match tab.document.undo() {
            Some(caret) => tab.put_caret(caret),
            None => self.set_status("Nothing to undo"),
        }
    }

    fn redo(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        match tab.document.redo() {
            Some(caret) => tab.put_caret(caret),
            None => self.set_status("Nothing to redo"),
        }
    }

    fn duplicate_line(&mut self) {
        self.edit_active(|buffer, range| lineops::duplicate_line(buffer, range.start));
    }

    fn delete_line(&mut self) {
        self.edit_active(|buffer, range| lineops::delete_line(buffer, range.start));
    }

    fn move_line_up(&mut self) {
        self.edit_active(|buffer, range| lineops::move_line_up(buffer, range.start));
    }

    fn move_line_down(&mut self) {
        self.edit_active(|buffer, range| lineops::move_line_down(buffer, range.start));
    }

    fn toggle_comment(&mut self) {
        let token = self
            .tabs
            .get(self.active)
            .map(|tab| tab.document.language.comment_token())
            .unwrap_or("");
        if token.is_empty() {
            self.set_status("This language has no line comment");
            return;
        }
        self.edit_active(move |buffer, range| lineops::toggle_comment(buffer, range, token));
    }

    fn sort_lines(&mut self) {
        self.edit_active(|buffer, range| lineops::sort_lines(buffer, range, false));
    }

    fn convert_case(&mut self, mode: CaseMode) {
        self.edit_active(move |buffer, range| lineops::convert_case(buffer, range, mode));
    }

    fn trim_trailing_whitespace(&mut self) {
        self.edit_active(|buffer, range| {
            lineops::trim_trailing_whitespace(buffer);
            range
        });
    }

    fn tabs_to_spaces(&mut self) {
        let width = self.prefs.tab_width;
        self.edit_active(move |buffer, range| {
            lineops::tabs_to_spaces(buffer, width);
            range
        });
    }

    fn spaces_to_tabs(&mut self) {
        let width = self.prefs.tab_width;
        self.edit_active(move |buffer, range| {
            lineops::spaces_to_tabs(buffer, width);
            range
        });
    }

    /// Tab / Shift+Tab inside the editor, honoring soft tabs.
    fn change_indentation(&mut self, dedent: bool) {
        let indent = self.prefs.indent_string();
        let width = self.prefs.tab_width;
        self.edit_active(move |buffer, range| {
            if dedent {
                lineops::dedent_lines(buffer, range, width)
            } else if range.is_empty() {
                buffer.insert(range.start, &indent);
                let caret = range.start + indent.len();
                caret..caret
            } else {
                lineops::indent_lines(buffer, range, &indent)
            }
        });
    }

    fn set_eol(&mut self, eol: Eol) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.document.set_eol(eol);
            self.set_status(format!("Line endings: {}", eol.label()));
        }
    }

    fn set_encoding(&mut self, encoding: Encoding) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.document.set_encoding(encoding);
            self.set_status(format!("Encoding: {}", encoding.label()));
        }
    }

    fn set_language(&mut self, language: Language) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.document.set_language(language);
            tab.highlighter.set_language(language);
            self.set_status(format!("Language: {}", language.label()));
        }
    }

    fn goto_line_number(&mut self, line: usize) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let last = tab.document.line_count().saturating_sub(1);
        let offset = tab.document.buffer.line_start(line.min(last));
        tab.put_caret(offset);
    }

    // ------------------------------------------------------------------- search

    fn build_query(&self) -> SearchQuery {
        SearchQuery::new(
            self.find.query.clone(),
            self.find.mode,
            self.find.case_sensitive,
        )
    }

    fn refresh_search(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let text = tab.document.text();
        let query = self.build_query();
        let revision = tab.document.revision();
        match query.find_all(text) {
            Ok(matches) => {
                self.find.char_ranges = char_ranges(text, &matches);
                self.find.matches = matches;
                self.find.error = None;
                if self.find.active >= self.find.matches.len() {
                    self.find.active = 0;
                }
            }
            Err(error) => {
                self.find.reset_results();
                self.find.error = Some(error.to_string());
            }
        }
        self.find.stale = false;
        self.search_revision = revision;
    }

    fn find_step(&mut self, forward: bool) {
        if self.find.query.is_empty() {
            self.set_status("Type something to find");
            return;
        }
        let revision = self.tabs[self.active].document.revision();
        if self.find.stale || self.search_revision != revision {
            self.refresh_search();
        }
        if self.find.matches.is_empty() {
            self.set_status("No matches");
            return;
        }

        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let text = tab.document.text().to_owned();
        let caret = tab.caret;
        let query = self.build_query();
        let found = if forward {
            query.find_next(&text, caret.saturating_add(1), self.find.wrap)
        } else {
            query.find_previous(&text, caret, self.find.wrap)
        };

        match found {
            Ok(Some(found)) => {
                if let Some(index) = self
                    .find
                    .matches
                    .iter()
                    .position(|candidate| *candidate == found)
                {
                    self.find.active = index;
                }
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.select(found.start, found.end);
                }
            }
            Ok(None) => self.set_status(if forward {
                "No further matches"
            } else {
                "No earlier matches"
            }),
            Err(error) => self.set_status(error.to_string()),
        }
    }

    fn replace_current(&mut self) {
        if self.find.matches.is_empty() {
            return;
        }
        let query = self.build_query();
        let replacement = self.find.replacement.clone();
        let Some(current) = self.find.matches.get(self.find.active).copied() else {
            return;
        };
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let text = tab.document.text().to_owned();
        match query.replace_in(&text, current.start..current.end, &replacement) {
            Ok((new_text, count)) if count > 0 => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.document.replace_all(new_text);
                    tab.put_caret(current.start);
                }
                self.find.stale = true;
                self.refresh_search();
            }
            Ok(_) => {}
            Err(error) => self.set_status(error.to_string()),
        }
    }

    fn replace_all(&mut self) {
        let query = self.build_query();
        let replacement = self.find.replacement.clone();
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let text = tab.document.text().to_owned();
        match query.replace_all(&text, &replacement) {
            Ok((new_text, count)) => {
                if count > 0 {
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        tab.document.replace_all(new_text);
                    }
                    self.set_status(format!("Replaced {count} occurrence(s)"));
                } else {
                    self.set_status("No matches");
                }
                self.find.stale = true;
                self.refresh_search();
            }
            Err(error) => self.set_status(error.to_string()),
        }
    }
}

/// Converts byte-offset matches into document-wide char ranges for painting.
fn char_ranges(text: &str, matches: &[SearchMatch]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::with_capacity(matches.len());
    let mut byte_index = 0usize;
    let mut char_index = 0usize;

    for found in matches {
        while byte_index < found.start {
            match text[byte_index..].chars().next() {
                Some(ch) => {
                    byte_index += ch.len_utf8();
                    char_index += 1;
                }
                None => break,
            }
        }
        let start_char = char_index;
        while byte_index < found.end {
            match text[byte_index..].chars().next() {
                Some(ch) => {
                    byte_index += ch.len_utf8();
                    char_index += 1;
                }
                None => break,
            }
        }
        ranges.push((start_char, char_index));
    }
    ranges
}

/// A menu row that reports whether it was clicked and closes the menu when it was.
fn menu_item(ui: &mut egui::Ui, label: &str, shortcut: &str) -> bool {
    let clicked = ui
        .add(egui::Button::new(label).shortcut_text(shortcut))
        .clicked();
    if clicked {
        ui.close_menu();
    }
    clicked
}

impl NotepadSharpApp {
    fn menu_bar(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| self.file_menu(ui, ctx));
            ui.menu_button("Edit", |ui| self.edit_menu(ui, ctx));
            ui.menu_button("Search", |ui| self.search_menu(ui));
            ui.menu_button("View", |ui| self.view_menu(ui));
            ui.menu_button("Language", |ui| self.language_menu(ui));
            ui.menu_button("Encoding", |ui| self.encoding_menu(ui));
            ui.menu_button("Help", |ui| {
                if menu_item(ui, "About NotepadSharp", "") {
                    self.dialog = Dialog::About;
                }
            });
        });
    }

    fn file_menu(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        if menu_item(ui, "New", "Ctrl+N") {
            self.new_tab();
        }
        if menu_item(ui, "Open…", "Ctrl+O") {
            self.open_dialog();
        }
        if menu_item(ui, "Reopen closed tab", "Ctrl+Shift+T") {
            self.reopen_closed_tab();
        }

        let recent = self.session.recent_files.clone();
        ui.menu_button("Recent files", |ui| {
            if recent.is_empty() {
                ui.label("(nothing yet)");
            }
            for path in recent {
                let label = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                if ui
                    .button(label)
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    self.open_path(path, true);
                    ui.close_menu();
                }
            }
            ui.separator();
            if ui.button("Clear list").clicked() {
                self.session.clear_recent();
                ui.close_menu();
            }
        });

        ui.separator();
        if menu_item(ui, "Save", "Ctrl+S") {
            self.save_active();
        }
        if menu_item(ui, "Save as…", "Ctrl+Shift+S") {
            self.save_tab_as(self.active);
        }
        if menu_item(ui, "Save all", "Ctrl+Alt+S") {
            self.save_all();
        }

        ui.separator();
        if menu_item(ui, "Close tab", "Ctrl+W") {
            self.request_close_tab(self.active);
        }
        if menu_item(ui, "Close all tabs", "") {
            if self.any_dirty() {
                self.dialog = Dialog::Unsaved(CloseTarget::All);
            } else {
                self.close_all_tabs();
            }
        }

        ui.separator();
        if menu_item(ui, "Exit", "Ctrl+Shift+Q") {
            self.request_quit(ctx);
        }
    }
}

impl NotepadSharpApp {
    fn selected_text(&self) -> Option<String> {
        let tab = self.tabs.get(self.active)?;
        let (start, end) = tab.selection?;
        if end <= start {
            return None;
        }
        tab.document
            .text()
            .get(start..end)
            .map(|text| text.to_owned())
    }

    fn copy(&mut self, ctx: &Context) {
        match self.selected_text() {
            Some(text) => {
                ctx.copy_text(text);
                self.set_status("Copied to clipboard");
            }
            None => self.set_status("Nothing selected"),
        }
    }

    fn cut(&mut self, ctx: &Context) {
        let Some(text) = self.selected_text() else {
            self.set_status("Nothing selected");
            return;
        };
        ctx.copy_text(text);
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if let Some((start, end)) = tab.selection {
                let caret = tab.document.apply_edit(start..end, "");
                tab.put_caret(caret);
            }
        }
        self.set_status("Cut to clipboard");
    }

    fn edit_menu(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let can_undo = self.tabs[self.active].document.history.can_undo();
        let can_redo = self.tabs[self.active].document.history.can_redo();

        if ui
            .add_enabled(can_undo, egui::Button::new("Undo").shortcut_text("Ctrl+Z"))
            .clicked()
        {
            self.undo();
            ui.close_menu();
        }
        if ui
            .add_enabled(can_redo, egui::Button::new("Redo").shortcut_text("Ctrl+Y"))
            .clicked()
        {
            self.redo();
            ui.close_menu();
        }

        ui.separator();
        if menu_item(ui, "Cut", "Ctrl+X") {
            self.cut(ctx);
        }
        if menu_item(ui, "Copy", "Ctrl+C") {
            self.copy(ctx);
        }
        if menu_item(ui, "Select all", "Ctrl+A") {
            let length = self.tabs[self.active].document.buffer.len();
            self.tabs[self.active].select(0, length);
        }

        ui.separator();
        if menu_item(ui, "Duplicate line", "Ctrl+D") {
            self.duplicate_line();
        }
        if menu_item(ui, "Delete line", "Ctrl+Shift+K") {
            self.delete_line();
        }
        if menu_item(ui, "Move line up", "Ctrl+Shift+Up") {
            self.move_line_up();
        }
        if menu_item(ui, "Move line down", "Ctrl+Shift+Down") {
            self.move_line_down();
        }
        if menu_item(ui, "Toggle comment", "Ctrl+/") {
            self.toggle_comment();
        }
        if menu_item(ui, "Indent", "Tab") {
            self.change_indentation(false);
        }
        if menu_item(ui, "Unindent", "Shift+Tab") {
            self.change_indentation(true);
        }

        ui.separator();
        ui.menu_button("Convert case", |ui| {
            if menu_item(ui, "UPPERCASE", "") {
                self.convert_case(CaseMode::Upper);
            }
            if menu_item(ui, "lowercase", "") {
                self.convert_case(CaseMode::Lower);
            }
            if menu_item(ui, "Title Case", "") {
                self.convert_case(CaseMode::Title);
            }
        });
        if menu_item(ui, "Sort lines", "Ctrl+Alt+L") {
            self.sort_lines();
        }
        if menu_item(ui, "Trim trailing whitespace", "") {
            self.trim_trailing_whitespace();
        }
        ui.menu_button("Tabs and spaces", |ui| {
            if menu_item(ui, "Tabs to spaces", "") {
                self.tabs_to_spaces();
            }
            if menu_item(ui, "Spaces to tabs", "") {
                self.spaces_to_tabs();
            }
        });
    }
}

impl NotepadSharpApp {
    fn search_menu(&mut self, ui: &mut egui::Ui) {
        if menu_item(ui, "Find…", "Ctrl+F") {
            self.find.open = true;
            self.find.replace_mode = false;
            self.find.focus_requested = true;
        }
        if menu_item(ui, "Replace…", "Ctrl+H") {
            self.find.open = true;
            self.find.replace_mode = true;
            self.find.focus_requested = true;
        }
        ui.separator();
        if menu_item(ui, "Find next", "F3") {
            self.find_step(true);
        }
        if menu_item(ui, "Find previous", "Shift+F3") {
            self.find_step(false);
        }
        if menu_item(ui, "Go to line…", "Ctrl+G") {
            self.goto_line = Some(String::new());
        }
        if menu_item(ui, "Count matches", "") {
            self.refresh_search();
            let count = self.find.matches.len();
            self.set_status(format!("{count} match(es)"));
        }
    }

    fn view_menu(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        changed |= ui
            .checkbox(&mut self.prefs.word_wrap, "Word wrap")
            .changed();
        changed |= ui
            .checkbox(&mut self.prefs.show_line_numbers, "Line numbers")
            .changed();
        changed |= ui
            .checkbox(
                &mut self.prefs.highlight_current_line,
                "Highlight current line",
            )
            .changed();
        changed |= ui
            .checkbox(&mut self.prefs.show_status_bar, "Status bar")
            .changed();
        changed |= ui
            .checkbox(&mut self.prefs.soft_tabs, "Soft tabs")
            .changed();
        if changed {
            self.session_dirty = true;
        }

        ui.separator();
        ui.menu_button("Theme", |ui| {
            for theme in Theme::ALL {
                if ui
                    .selectable_label(self.prefs.theme == theme, theme.label())
                    .clicked()
                {
                    self.prefs.theme = theme;
                    self.session_dirty = true;
                    ui.close_menu();
                }
            }
        });

        ui.separator();
        if menu_item(ui, "Zoom in", "Ctrl++") {
            self.prefs.font_size = (self.prefs.font_size + 1.0).min(48.0);
            self.session_dirty = true;
        }
        if menu_item(ui, "Zoom out", "Ctrl+-") {
            self.prefs.font_size = (self.prefs.font_size - 1.0).max(6.0);
            self.session_dirty = true;
        }
        if menu_item(ui, "Reset zoom", "Ctrl+0") {
            self.prefs.font_size = 14.0;
            self.session_dirty = true;
        }
    }

    fn language_menu(&mut self, ui: &mut egui::Ui) {
        let current = self.tabs[self.active].document.language;
        egui::ScrollArea::vertical()
            .max_height(400.0)
            .show(ui, |ui| {
                for language in Language::ALL {
                    if ui
                        .selectable_label(current == language, language.label())
                        .clicked()
                    {
                        self.set_language(language);
                        ui.close_menu();
                    }
                }
            });
    }

    fn encoding_menu(&mut self, ui: &mut egui::Ui) {
        let (current_encoding, current_eol) = {
            let tab = &self.tabs[self.active];
            (tab.document.encoding, tab.document.eol)
        };

        for encoding in Encoding::ALL {
            if ui
                .selectable_label(current_encoding == encoding, encoding.label())
                .clicked()
            {
                self.set_encoding(encoding);
                ui.close_menu();
            }
        }

        ui.separator();
        ui.menu_button("Line endings", |ui| {
            for eol in Eol::ALL {
                if ui
                    .selectable_label(current_eol == eol, eol.name())
                    .clicked()
                {
                    self.set_eol(eol);
                    ui.close_menu();
                }
            }
        });
    }
}

fn consume(ctx: &Context, shortcut: &egui::KeyboardShortcut) -> bool {
    ctx.input_mut(|input| input.consume_shortcut(shortcut))
}

impl NotepadSharpApp {
    /// Global keyboard handling. Runs before the editor is drawn so shortcuts such as
    /// Ctrl+Z are consumed and never reach `egui`'s own undo stack.
    fn handle_shortcuts(&mut self, ctx: &Context) {
        let keys = self.shortcuts.clone();

        // Undo/redo first: they must beat the TextEdit's built-in handling.
        if consume(ctx, &keys.undo) {
            self.undo();
        }
        if consume(ctx, &keys.redo) || consume(ctx, &keys.redo_alt) {
            self.redo();
        }

        if consume(ctx, &keys.new_file) {
            self.new_tab();
        }
        if consume(ctx, &keys.open) {
            self.open_dialog();
        }
        if consume(ctx, &keys.save) {
            self.save_active();
        }
        if consume(ctx, &keys.save_as) {
            self.save_tab_as(self.active);
        }
        if consume(ctx, &keys.save_all) {
            self.save_all();
        }
        if consume(ctx, &keys.close_tab) {
            self.request_close_tab(self.active);
        }
        if consume(ctx, &keys.reopen_tab) {
            self.reopen_closed_tab();
        }
        if consume(ctx, &keys.quit) {
            self.request_quit(ctx);
        }

        if consume(ctx, &keys.find) {
            self.find.open = true;
            self.find.replace_mode = false;
            self.find.focus_requested = true;
        }
        if consume(ctx, &keys.replace) {
            self.find.open = true;
            self.find.replace_mode = true;
            self.find.focus_requested = true;
        }
        if consume(ctx, &keys.find_next) {
            self.find_step(true);
        }
        if consume(ctx, &keys.find_previous) {
            self.find_step(false);
        }
        if consume(ctx, &keys.goto_line) {
            self.goto_line = Some(String::new());
        }
        if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) && self.find.open
        {
            self.find.open = false;
            self.find.reset_results();
        }

        if consume(ctx, &keys.duplicate_line) {
            self.duplicate_line();
        }
        if consume(ctx, &keys.delete_line) {
            self.delete_line();
        }
        if consume(ctx, &keys.move_line_up) {
            self.move_line_up();
        }
        if consume(ctx, &keys.move_line_down) {
            self.move_line_down();
        }
        if consume(ctx, &keys.toggle_comment) {
            self.toggle_comment();
        }
        if consume(ctx, &keys.sort_lines) {
            self.sort_lines();
        }

        if consume(ctx, &keys.zoom_in) {
            self.prefs.font_size = (self.prefs.font_size + 1.0).min(48.0);
            self.session_dirty = true;
        }
        if consume(ctx, &keys.zoom_out) {
            self.prefs.font_size = (self.prefs.font_size - 1.0).max(6.0);
            self.session_dirty = true;
        }
        if consume(ctx, &keys.zoom_reset) {
            self.prefs.font_size = 14.0;
            self.session_dirty = true;
        }

        // Ctrl+wheel zoom, handled by egui's zoom delta.
        let zoom = ctx.input(|input| input.zoom_delta());
        if (zoom - 1.0).abs() > 0.001 {
            self.prefs.font_size = (self.prefs.font_size * zoom).clamp(6.0, 48.0);
            self.session_dirty = true;
        }

        if consume(ctx, &keys.next_tab) && self.tabs.len() > 1 {
            let next = (self.active + 1) % self.tabs.len();
            self.select_tab(next);
        }
        if consume(ctx, &keys.previous_tab) && self.tabs.len() > 1 {
            let previous = (self.active + self.tabs.len() - 1) % self.tabs.len();
            self.select_tab(previous);
        }

        // Tab / Shift+Tab indent, but only while the editor has focus.
        let editor_id = egui::Id::new(("editor", self.active));
        if ctx.memory(|memory| memory.has_focus(editor_id)) {
            if consume(ctx, &keys.select_all) {
                let length = self.tabs[self.active].document.buffer.len();
                self.tabs[self.active].select(0, length);
            }
            if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Tab)) {
                self.change_indentation(false);
            }
            if ctx.input_mut(|input| input.consume_key(Modifiers::SHIFT, Key::Tab)) {
                self.change_indentation(true);
            }
        }
    }
}

impl NotepadSharpApp {
    fn select_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.active = index;
        let caret = self.tabs[index].caret;
        self.tabs[index].put_caret(caret);
        self.session_dirty = true;
    }

    fn handle_tab_action(&mut self, action: TabAction) {
        match action {
            TabAction::None => {}
            TabAction::Select(index) => self.select_tab(index),
            TabAction::Close(index) => self.request_close_tab(index),
            TabAction::NewTab => self.new_tab(),
            TabAction::Save(index) => {
                self.save_tab(index);
            }
            TabAction::CopyPath(index) => {
                if let Some(path) = self
                    .tabs
                    .get(index)
                    .and_then(|tab| tab.document.path.clone())
                {
                    self.set_status(path.display().to_string());
                }
            }
            TabAction::CloseOthers(index) => {
                let dirty = self
                    .tabs
                    .iter()
                    .enumerate()
                    .any(|(other, tab)| other != index && tab.document.is_dirty());
                if dirty {
                    self.set_status("Save or close the modified tabs first");
                } else if index < self.tabs.len() {
                    let kept = self.tabs.remove(index);
                    self.tabs.clear();
                    self.tabs.push(kept);
                    self.active = 0;
                    self.session_dirty = true;
                }
            }
            TabAction::CloseAll => {
                if self.any_dirty() {
                    self.set_status("Save or close the modified tabs first");
                } else {
                    self.close_all_tabs();
                }
            }
        }
    }

    fn show_tab_strip(&mut self, ui: &mut egui::Ui) {
        let action = tabs::show(ui, &self.tabs, self.active);
        self.handle_tab_action(action);
    }

    fn show_status_bar(&mut self, ui: &mut egui::Ui) {
        let tab = &self.tabs[self.active];
        let (line, column) = tab.document.buffer.position_of_offset(tab.caret);
        let selected_chars = tab
            .selection
            .filter(|(start, end)| end > start)
            .map(|(start, end)| tab.document.text()[start..end].chars().count())
            .unwrap_or(0);

        let status_message = self
            .status
            .as_ref()
            .filter(|(_, when)| when.elapsed() < Duration::from_secs(6))
            .map(|(message, _)| message.clone());
        let error_color = self.palette.error;
        let font_size = self.prefs.font_size;

        ui.horizontal(|ui| {
            if let Some(message) = status_message {
                ui.label(
                    RichText::new(message)
                        .color(ui.visuals().hyperlink_color)
                        .small(),
                );
                ui.separator();
            }
            ui.label(format!("Ln {}, Col {}", line + 1, column + 1));
            if selected_chars > 0 {
                ui.label(format!("Sel {selected_chars}"));
            }
            ui.separator();
            ui.label(format!("{} chars", tab.document.char_count()));
            ui.label(format!("{} words", tab.document.word_count()));
            ui.label(format!("{} lines", tab.document.line_count()));
            ui.separator();
            ui.label(if tab.document.is_dirty() {
                RichText::new("modified").color(error_color)
            } else {
                RichText::new("saved").weak()
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!(
                    "{}  |  {}  |  {}  |  {font_size}pt",
                    tab.document.encoding.label(),
                    tab.document.eol.label(),
                    tab.document.language.label()
                ));
            });
        });
    }
}

impl NotepadSharpApp {
    fn show_find_panel(&mut self, ui: &mut egui::Ui) {
        let action = find_panel::show(ui, &mut self.find);
        match action {
            FindAction::None => {}
            FindAction::Refresh => self.refresh_search(),
            FindAction::FindNext => self.find_step(true),
            FindAction::FindPrevious => self.find_step(false),
            FindAction::Replace => self.replace_current(),
            FindAction::ReplaceAll => self.replace_all(),
            FindAction::Close => {
                self.find.open = false;
                self.find.reset_results();
            }
        }
    }

    fn show_editor_panel(&mut self, ui: &mut egui::Ui) {
        let index = self.active;
        if index >= self.tabs.len() {
            return;
        }

        let revision = self.tabs[index].document.revision();
        if self.find.open
            && !self.find.query.is_empty()
            && (self.find.stale || self.search_revision != revision)
        {
            self.refresh_search();
        }

        let palette = self.palette.clone();
        let prefs = self.prefs.clone();
        let matches = if self.find.open {
            self.find.char_ranges.clone()
        } else {
            Vec::new()
        };
        let active_match = if self.find.open && !self.find.matches.is_empty() {
            self.find.active_match()
        } else {
            None
        };

        let editor_id = egui::Id::new(("editor", index));
        let changed = {
            let tab = &mut self.tabs[index];
            let current_line = tab.document.buffer.line_of_offset(tab.caret);
            let pending = tab.pending_selection.take();
            let output = editor::show(
                ui,
                EditorInput {
                    id: editor_id,
                    document: &mut tab.document,
                    highlighter: &mut tab.highlighter,
                    palette: &palette,
                    font_size: prefs.font_size,
                    word_wrap: prefs.word_wrap,
                    show_line_numbers: prefs.show_line_numbers,
                    highlight_current_line: prefs.highlight_current_line,
                    matches: &matches,
                    active_match,
                    current_line,
                    pending_selection: pending,
                },
            );
            tab.caret = output.caret;
            tab.selection = output.selection;
            output.response.changed()
        };

        if changed {
            self.find.stale = true;
            self.session_dirty = true;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Decision {
    Save,
    Discard,
    Cancel,
}

impl NotepadSharpApp {
    fn show_dialogs(&mut self, ctx: &Context) {
        self.show_goto_line_dialog(ctx);
        self.show_unsaved_dialog(ctx);
        self.show_about_dialog(ctx);
    }

    fn show_goto_line_dialog(&mut self, ctx: &Context) {
        let Some(mut input) = self.goto_line.take() else {
            return;
        };
        let mut keep_open = true;
        let mut submitted = false;

        egui::Window::new("Go to line")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Line number:");
                    let response =
                        ui.add(egui::TextEdit::singleline(&mut input).desired_width(100.0));
                    response.request_focus();
                    submitted = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Go").clicked() {
                        submitted = true;
                    }
                    if ui.button("Cancel").clicked() {
                        keep_open = false;
                    }
                });
            });

        if submitted {
            if let Ok(line) = input.trim().parse::<usize>() {
                self.goto_line_number(line.saturating_sub(1));
            }
            keep_open = false;
        }
        if keep_open {
            self.goto_line = Some(input);
        }
    }

    fn show_unsaved_dialog(&mut self, ctx: &Context) {
        let Dialog::Unsaved(target) = self.dialog else {
            return;
        };

        let message = match target {
            CloseTarget::Tab(index) => self
                .tabs
                .get(index)
                .map(|tab| {
                    format!(
                        "Save changes to \"{}\" before closing?",
                        tab.document.title()
                    )
                })
                .unwrap_or_default(),
            CloseTarget::All => {
                let count = self
                    .tabs
                    .iter()
                    .filter(|tab| tab.document.is_dirty())
                    .count();
                format!("{count} document(s) have unsaved changes. Save them before closing?")
            }
        };

        let mut decision = None;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(message);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        decision = Some(Decision::Save);
                    }
                    if ui.button("Don't save").clicked() {
                        decision = Some(Decision::Discard);
                    }
                    if ui.button("Cancel").clicked() {
                        decision = Some(Decision::Cancel);
                    }
                });
            });

        let Some(decision) = decision else {
            return;
        };

        match (decision, target) {
            (Decision::Cancel, _) => {
                self.dialog = Dialog::None;
                self.pending_quit = false;
            }
            (Decision::Save, CloseTarget::Tab(index)) => {
                if self.save_tab(index) {
                    self.dialog = Dialog::None;
                    self.close_tab_now(index);
                }
            }
            (Decision::Save, CloseTarget::All) => {
                self.save_all();
                if !self.any_dirty() {
                    self.finish_pending(ctx);
                }
            }
            (Decision::Discard, CloseTarget::Tab(index)) => {
                self.dialog = Dialog::None;
                self.close_tab_now(index);
            }
            (Decision::Discard, CloseTarget::All) => self.finish_pending(ctx),
        }
    }

    fn finish_pending(&mut self, ctx: &Context) {
        self.dialog = Dialog::None;
        if self.pending_quit {
            self.pending_quit = false;
            self.finish_quit(ctx);
        } else {
            self.close_all_tabs();
        }
    }
}

impl NotepadSharpApp {
    fn show_about_dialog(&mut self, ctx: &Context) {
        if self.dialog != Dialog::About {
            return;
        }
        let mut open = true;
        egui::Window::new("About NotepadSharp")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.heading("NotepadSharp");
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.label("A Notepad++-style text editor written in Rust with egui.");
                ui.separator();
                ui.label(RichText::new("Keyboard").strong());
                for (keys, action) in [
                    ("Ctrl+N / Ctrl+O", "New / open file"),
                    ("Ctrl+S / Ctrl+Shift+S", "Save / save as"),
                    ("Ctrl+W", "Close tab"),
                    ("Ctrl+F / Ctrl+H", "Find / replace"),
                    ("F3 / Shift+F3", "Find next / previous"),
                    ("Ctrl+G", "Go to line"),
                    ("Ctrl+D", "Duplicate line"),
                    ("Ctrl+Shift+K", "Delete line"),
                    ("Ctrl+Shift+Up / Down", "Move line"),
                    ("Ctrl+/", "Toggle comment"),
                    ("Ctrl+Z / Ctrl+Y", "Undo / redo"),
                    ("Ctrl++ / Ctrl+-", "Zoom"),
                ] {
                    ui.label(format!("{keys}  —  {action}"));
                }
                ui.separator();
                ui.label("Limitation: one caret at a time — no column/block selection yet.");
            });
        if !open {
            self.dialog = Dialog::None;
        }
    }

    fn update_window_title(&self, ctx: &Context) {
        let tab = &self.tabs[self.active];
        let marker = if tab.document.is_dirty() { "*" } else { "" };
        let title = format!("{marker}{} - NotepadSharp", tab.document.display_path());
        ctx.send_viewport_cmd(ViewportCommand::Title(title));
    }
}

impl eframe::App for NotepadSharpApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let style = (self.prefs.theme, self.prefs.font_size);
        if self.applied_style != Some(style) {
            theme::apply(ctx, style.0, style.1);
            self.palette = Palette::for_theme(style.0);
            self.applied_style = Some(style);
        }

        if ctx.input(|input| input.viewport().close_requested()) && !self.allow_close {
            if self.any_dirty() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.pending_quit = true;
                self.dialog = Dialog::Unsaved(CloseTarget::All);
            } else {
                self.save_session();
                self.allow_close = true;
            }
        }

        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });
        for path in dropped {
            self.open_path(path, true);
        }

        self.handle_shortcuts(ctx);

        if let Some(rect) = ctx.input(|input| input.viewport().inner_rect) {
            self.window_size = Some((rect.width(), rect.height()));
        }

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| self.menu_bar(ui, ctx));
        egui::TopBottomPanel::top("tab_strip").show(ctx, |ui| self.show_tab_strip(ui));
        if self.prefs.show_status_bar {
            egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| self.show_status_bar(ui));
        }
        if self.find.open {
            egui::TopBottomPanel::bottom("find_panel").show(ctx, |ui| self.show_find_panel(ui));
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.palette.background))
            .show(ctx, |ui| self.show_editor_panel(ui));

        self.show_dialogs(ctx);
        self.update_window_title(ctx);

        if self.session_dirty && self.last_session_save.elapsed() > Duration::from_secs(2) {
            self.save_session();
        }
    }
}
