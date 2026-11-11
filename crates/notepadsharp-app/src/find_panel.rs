//! The Find / Replace panel that docks at the bottom of the window.

use egui::{ComboBox, Key, RichText, Ui};
use notepadsharp_core::search::{SearchMatch, SearchMode};

/// All find/replace state. Matches are kept in both byte offsets (navigation) and char
/// ranges (painting), so the editor never has to re-scan the document.
pub struct FindState {
    pub open: bool,
    pub replace_mode: bool,
    pub query: String,
    pub replacement: String,
    pub mode: SearchMode,
    pub case_sensitive: bool,
    pub wrap: bool,
    pub matches: Vec<SearchMatch>,
    pub char_ranges: Vec<(usize, usize)>,
    pub active: usize,
    pub error: Option<String>,
    pub stale: bool,
    pub focus_requested: bool,
}

impl Default for FindState {
    fn default() -> Self {
        Self {
            open: false,
            replace_mode: false,
            query: String::new(),
            replacement: String::new(),
            mode: SearchMode::Literal,
            case_sensitive: false,
            wrap: true,
            matches: Vec::new(),
            char_ranges: Vec::new(),
            active: 0,
            error: None,
            stale: true,
            focus_requested: false,
        }
    }
}

impl FindState {
    pub fn reset_results(&mut self) {
        self.matches.clear();
        self.char_ranges.clear();
        self.active = 0;
        self.error = None;
    }

    pub fn active_match(&self) -> Option<(usize, usize)> {
        self.char_ranges.get(self.active).copied()
    }

    pub fn status_text(&self) -> String {
        if let Some(error) = &self.error {
            return error.clone();
        }
        if self.matches.is_empty() {
            return String::new();
        }
        format!("{}/{}", self.active + 1, self.matches.len())
    }
}

/// What the user asked for this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindAction {
    None,
    FindNext,
    FindPrevious,
    Replace,
    ReplaceAll,
    Refresh,
    Close,
}

/// Draws the panel. The caller applies the returned action.
pub fn show(ui: &mut Ui, state: &mut FindState) -> FindAction {
    let mut action = FindAction::None;

    ui.horizontal(|ui| {
        ui.label("Find:");
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.query)
                .desired_width(240.0)
                .hint_text("text or pattern"),
        );
        if state.focus_requested {
            response.request_focus();
            state.focus_requested = false;
        }
        if response.changed() {
            state.stale = true;
        }
        if response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
            action = FindAction::FindNext;
        }

        if ui
            .checkbox(&mut state.case_sensitive, "Match case")
            .changed()
        {
            state.stale = true;
        }
        if ui.checkbox(&mut state.wrap, "Wrap").changed() {
            state.stale = true;
        }

        ComboBox::from_id_salt("search_mode")
            .selected_text(state.mode.label())
            .show_ui(ui, |ui| {
                for mode in [
                    SearchMode::Literal,
                    SearchMode::WholeWord,
                    SearchMode::Regex,
                ] {
                    if ui
                        .selectable_value(&mut state.mode, mode, mode.label())
                        .clicked()
                    {
                        state.stale = true;
                    }
                }
            });

        if ui.button("Find next").clicked() {
            action = FindAction::FindNext;
        }
        if ui.button("Find previous").clicked() {
            action = FindAction::FindPrevious;
        }

        let status = state.status_text();
        if !status.is_empty() {
            let color = if state.error.is_some() {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().weak_text_color()
            };
            ui.label(RichText::new(status).color(color));
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("✕").on_hover_text("Close (Esc)").clicked() {
                action = FindAction::Close;
            }
        });
    });

    if state.replace_mode {
        ui.horizontal(|ui| {
            ui.label("Replace:");
            ui.add(
                egui::TextEdit::singleline(&mut state.replacement)
                    .desired_width(240.0)
                    .hint_text("replacement"),
            );
            if ui.button("Replace").clicked() {
                action = FindAction::Replace;
            }
            if ui.button("Replace all").clicked() {
                action = FindAction::ReplaceAll;
            }
        });
    }

    if state.stale {
        action = match action {
            FindAction::None => FindAction::Refresh,
            other => other,
        };
    }

    action
}
