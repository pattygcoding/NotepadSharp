//! `notepadsharp-core` — the GUI-free text editing engine behind NotepadSharp.
//!
//! Everything that is worth unit testing lives here: the text buffer and line index,
//! encodings, line endings, the incremental syntax highlighter, undo/redo, search,
//! line operations and the serialized session/preferences model.
//!
//! The `notepadsharp-app` crate is a thin `egui`/`eframe` front-end on top of this.

pub mod buffer;
pub mod document;
pub mod encoding;
pub mod eol;
pub mod highlight;
pub mod history;
pub mod language;
pub mod lineops;
pub mod search;
pub mod session;

pub use buffer::TextBuffer;
pub use document::Document;
pub use encoding::Encoding;
pub use eol::Eol;
pub use highlight::{Highlighter, LineState, TokenKind, TokenSpan};
pub use history::{EditDelta, History};
pub use language::Language;
pub use search::{SearchError, SearchMatch, SearchMode, SearchQuery};
pub use session::{Prefs, Session, Theme};
