//! Keyboard shortcuts, defined once and consumed by the app.

use egui::{Key, KeyboardShortcut, Modifiers};

/// Every shortcut NotepadSharp understands.
#[derive(Clone, Debug)]
pub struct Shortcuts {
    pub new_file: KeyboardShortcut,
    pub open: KeyboardShortcut,
    pub save: KeyboardShortcut,
    pub save_as: KeyboardShortcut,
    pub save_all: KeyboardShortcut,
    pub close_tab: KeyboardShortcut,
    pub reopen_tab: KeyboardShortcut,
    pub quit: KeyboardShortcut,
    pub undo: KeyboardShortcut,
    pub redo: KeyboardShortcut,
    pub redo_alt: KeyboardShortcut,
    pub select_all: KeyboardShortcut,
    pub find: KeyboardShortcut,
    pub replace: KeyboardShortcut,
    pub find_next: KeyboardShortcut,
    pub find_previous: KeyboardShortcut,
    pub goto_line: KeyboardShortcut,
    pub duplicate_line: KeyboardShortcut,
    pub delete_line: KeyboardShortcut,
    pub move_line_up: KeyboardShortcut,
    pub move_line_down: KeyboardShortcut,
    pub toggle_comment: KeyboardShortcut,
    pub sort_lines: KeyboardShortcut,
    pub zoom_in: KeyboardShortcut,
    pub zoom_out: KeyboardShortcut,
    pub zoom_reset: KeyboardShortcut,
    pub next_tab: KeyboardShortcut,
    pub previous_tab: KeyboardShortcut,
}

impl Default for Shortcuts {
    fn default() -> Self {
        let ctrl = Modifiers::CTRL;
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        let ctrl_alt = Modifiers::CTRL | Modifiers::ALT;
        Self {
            new_file: KeyboardShortcut::new(ctrl, Key::N),
            open: KeyboardShortcut::new(ctrl, Key::O),
            save: KeyboardShortcut::new(ctrl, Key::S),
            save_as: KeyboardShortcut::new(ctrl_shift, Key::S),
            save_all: KeyboardShortcut::new(ctrl_alt, Key::S),
            close_tab: KeyboardShortcut::new(ctrl, Key::W),
            reopen_tab: KeyboardShortcut::new(ctrl_shift, Key::T),
            quit: KeyboardShortcut::new(ctrl_shift, Key::Q),
            undo: KeyboardShortcut::new(ctrl, Key::Z),
            redo: KeyboardShortcut::new(ctrl, Key::Y),
            redo_alt: KeyboardShortcut::new(ctrl_shift, Key::Z),
            select_all: KeyboardShortcut::new(ctrl, Key::A),
            find: KeyboardShortcut::new(ctrl, Key::F),
            replace: KeyboardShortcut::new(ctrl, Key::H),
            find_next: KeyboardShortcut::new(Modifiers::NONE, Key::F3),
            find_previous: KeyboardShortcut::new(Modifiers::SHIFT, Key::F3),
            goto_line: KeyboardShortcut::new(ctrl, Key::G),
            duplicate_line: KeyboardShortcut::new(ctrl, Key::D),
            delete_line: KeyboardShortcut::new(ctrl_shift, Key::K),
            move_line_up: KeyboardShortcut::new(ctrl_shift, Key::ArrowUp),
            move_line_down: KeyboardShortcut::new(ctrl_shift, Key::ArrowDown),
            toggle_comment: KeyboardShortcut::new(ctrl, Key::Slash),
            sort_lines: KeyboardShortcut::new(ctrl_alt, Key::L),
            zoom_in: KeyboardShortcut::new(ctrl, Key::Equals),
            zoom_out: KeyboardShortcut::new(ctrl, Key::Minus),
            zoom_reset: KeyboardShortcut::new(ctrl, Key::Num0),
            next_tab: KeyboardShortcut::new(ctrl, Key::Tab),
            previous_tab: KeyboardShortcut::new(ctrl_shift, Key::Tab),
        }
    }
}
