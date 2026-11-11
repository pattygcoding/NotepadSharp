//! NotepadSharp — a Notepad++-style text editor built on `egui`/`eframe`.
//!
//! The window shell lives in [`app`], the editor widget in [`editor`]; everything that can
//! be reasoned about without a GUI lives in the `notepadsharp-core` crate.

mod app;
mod editor;
mod find_panel;
mod shortcuts;
mod tabs;
mod theme;

use notepadsharp_core::session::Session;

fn main() -> eframe::Result<()> {
    let session_path = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("NotepadSharp")
        .join("session.json");
    let mut session = Session::load_from(&session_path);

    // Files passed on the command line win over the restored session.
    let arguments: Vec<std::path::PathBuf> = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())
        .collect();
    if !arguments.is_empty() {
        session.open_files = arguments;
        session.active = 0;
    }

    let size = session
        .window_size
        .map(|(width, height)| egui::vec2(width.clamp(640.0, 1920.0), height.clamp(400.0, 1200.0)))
        .unwrap_or_else(|| egui::vec2(1100.0, 720.0));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("NotepadSharp")
            .with_app_id("NotepadSharp")
            .with_inner_size(size)
            .with_min_inner_size(egui::vec2(640.0, 400.0)),
        ..Default::default()
    };

    eframe::run_native(
        "NotepadSharp",
        options,
        Box::new(move |cc| Ok(Box::new(app::NotepadSharpApp::new(cc, session)))),
    )
}
