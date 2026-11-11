//! Session and preference persistence.
//!
//! Notepad++ remembers your open tabs, recent files and look-and-feel; NotepadSharp does
//! the same by writing a small JSON file. The core crate stays filesystem-agnostic — the
//! app decides *where* the file lives — so this module only needs explicit paths.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Color scheme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

impl Theme {
    pub const ALL: [Theme; 2] = [Theme::Dark, Theme::Light];

    pub fn label(self) -> &'static str {
        match self {
            Theme::Dark => "Dark",
            Theme::Light => "Light",
        }
    }
}

/// User preferences that survive restarts.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub theme: Theme,
    pub font_size: f32,
    pub word_wrap: bool,
    pub show_line_numbers: bool,
    pub show_status_bar: bool,
    pub highlight_current_line: bool,
    pub show_whitespace: bool,
    pub tab_width: usize,
    pub soft_tabs: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            font_size: 14.0,
            word_wrap: false,
            show_line_numbers: true,
            show_status_bar: true,
            highlight_current_line: true,
            show_whitespace: false,
            tab_width: 4,
            soft_tabs: true,
        }
    }
}

impl Prefs {
    /// The indentation inserted by Tab and by the indent command.
    pub fn indent_string(&self) -> String {
        if self.soft_tabs {
            " ".repeat(self.tab_width.max(1))
        } else {
            "\t".to_owned()
        }
    }

    /// Clamps values that could otherwise produce a broken UI.
    pub fn sanitize(&mut self) {
        if !(6.0..=48.0).contains(&self.font_size) {
            self.font_size = 14.0;
        }
        self.tab_width = self.tab_width.clamp(1, 16);
    }
}

/// Everything restored at startup.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    /// Files that should be reopened.
    pub open_files: Vec<PathBuf>,
    /// Index into `open_files` that was active.
    pub active: usize,
    /// Most-recently-used files, newest first.
    pub recent_files: Vec<PathBuf>,
    pub prefs: Prefs,
    /// Last window size in logical points.
    pub window_size: Option<(f32, f32)>,
}

impl Session {
    pub const RECENT_LIMIT: usize = 15;

    /// Loads a session, silently falling back to defaults when the file is missing or stale.
    pub fn load_from(path: &Path) -> Session {
        let Ok(contents) = std::fs::read_to_string(path) else {
            return Session::default();
        };
        let mut session: Session = serde_json::from_str(&contents).unwrap_or_default();
        session.prefs.sanitize();
        if session.active >= session.open_files.len() {
            session.active = 0;
        }
        session
    }

    /// Writes the session as pretty JSON, creating parent directories as needed.
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        std::fs::write(path, json)
    }

    /// Records a file as most recent, keeping the list de-duplicated and capped.
    pub fn push_recent(&mut self, path: &Path) {
        let path = path.to_path_buf();
        self.recent_files.retain(|existing| existing != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(Self::RECENT_LIMIT);
    }

    pub fn remove_recent(&mut self, path: &Path) {
        self.recent_files.retain(|existing| existing != path);
    }

    pub fn clear_recent(&mut self) {
        self.recent_files.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "notepadsharp-session-{}-{name}",
            std::process::id()
        ));
        path
    }

    #[test]
    fn round_trips_through_json() {
        let path = temp_path("session.json");
        let mut session = Session {
            open_files: vec![PathBuf::from("a.txt"), PathBuf::from("b.rs")],
            active: 1,
            window_size: Some((1280.0, 720.0)),
            ..Session::default()
        };
        session.prefs.theme = Theme::Light;
        session.prefs.font_size = 18.0;
        session.prefs.word_wrap = true;
        session.push_recent(Path::new("c.txt"));

        session.save_to(&path).unwrap();
        let loaded = Session::load_from(&path);
        assert_eq!(loaded.open_files, session.open_files);
        assert_eq!(loaded.active, 1);
        assert_eq!(loaded.prefs.theme, Theme::Light);
        assert_eq!(loaded.prefs.font_size, 18.0);
        assert!(loaded.prefs.word_wrap);
        assert_eq!(loaded.window_size, Some((1280.0, 720.0)));
        assert_eq!(loaded.recent_files, vec![PathBuf::from("c.txt")]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn recent_files_are_deduplicated_and_capped() {
        let mut session = Session::default();
        for index in 0..(Session::RECENT_LIMIT + 5) {
            session.push_recent(&PathBuf::from(format!("file{index}.txt")));
        }
        assert_eq!(session.recent_files.len(), Session::RECENT_LIMIT);
        assert_eq!(session.recent_files[0], PathBuf::from("file19.txt"));

        session.push_recent(Path::new("file19.txt"));
        assert_eq!(session.recent_files.len(), Session::RECENT_LIMIT);
        assert_eq!(
            session
                .recent_files
                .iter()
                .filter(|p| *p == Path::new("file19.txt"))
                .count(),
            1
        );

        session.remove_recent(Path::new("file19.txt"));
        assert_ne!(session.recent_files[0], PathBuf::from("file19.txt"));

        session.clear_recent();
        assert!(session.recent_files.is_empty());
    }

    #[test]
    fn missing_or_corrupt_files_fall_back_to_defaults() {
        let missing = temp_path("does-not-exist.json");
        let session = Session::load_from(&missing);
        assert!(session.open_files.is_empty());
        assert_eq!(session.prefs.theme, Theme::Dark);

        let corrupt = temp_path("corrupt.json");
        std::fs::write(&corrupt, "{ not json").unwrap();
        let session = Session::load_from(&corrupt);
        assert_eq!(session.prefs.font_size, Prefs::default().font_size);
        std::fs::remove_file(&corrupt).ok();
    }

    #[test]
    fn out_of_range_active_index_is_reset() {
        let path = temp_path("active.json");
        let session = Session {
            open_files: vec![PathBuf::from("only.txt")],
            active: 7,
            ..Session::default()
        };
        session.save_to(&path).unwrap();
        assert_eq!(Session::load_from(&path).active, 0);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn prefs_are_sanitized() {
        let mut prefs = Prefs {
            font_size: 900.0,
            tab_width: 0,
            ..Prefs::default()
        };
        prefs.sanitize();
        assert_eq!(prefs.font_size, 14.0);
        assert_eq!(prefs.tab_width, 1);
        assert_eq!(prefs.indent_string(), " ");
    }

    #[test]
    fn hard_tabs_come_from_prefs() {
        let prefs = Prefs {
            soft_tabs: false,
            ..Prefs::default()
        };
        assert_eq!(prefs.indent_string(), "\t");
    }
}
