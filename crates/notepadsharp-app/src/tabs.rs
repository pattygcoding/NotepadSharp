//! The document tab strip.

use egui::{RichText, Ui};

use crate::app::Tab;

/// What the user did to the tab strip this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TabAction {
    None,
    Select(usize),
    Close(usize),
    CloseOthers(usize),
    CloseAll,
    Save(usize),
    CopyPath(usize),
    NewTab,
}

/// Draws one button per open document.
pub fn show(ui: &mut Ui, tabs: &[Tab], active: usize) -> TabAction {
    let mut action = TabAction::None;

    egui::ScrollArea::horizontal()
        .id_salt("tab_strip")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (index, tab) in tabs.iter().enumerate() {
                    let dirty = tab.document.is_dirty();
                    let title = tab.document.title();
                    let label = if dirty {
                        RichText::new(format!("● {title}"))
                    } else {
                        RichText::new(format!("  {title}"))
                    };

                    let response = ui.selectable_label(index == active, label);
                    if response.clicked() {
                        action = TabAction::Select(index);
                    }
                    if response.middle_clicked() {
                        action = TabAction::Close(index);
                    }
                    response
                        .on_hover_text(tab.document.display_path())
                        .context_menu(|ui| {
                            if ui.button("Save").clicked() {
                                action = TabAction::Save(index);
                                ui.close_menu();
                            }
                            if ui.button("Close").clicked() {
                                action = TabAction::Close(index);
                                ui.close_menu();
                            }
                            if ui.button("Close others").clicked() {
                                action = TabAction::CloseOthers(index);
                                ui.close_menu();
                            }
                            if ui.button("Close all").clicked() {
                                action = TabAction::CloseAll;
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Copy full path").clicked() {
                                action = TabAction::CopyPath(index);
                                ui.close_menu();
                            }
                        });

                    if tabs.len() > 1 && ui.small_button("✕").clicked() {
                        action = TabAction::Close(index);
                    }
                    ui.add_space(2.0);
                }

                if ui.small_button("＋").on_hover_text("New file").clicked() {
                    action = TabAction::NewTab;
                }
            });
        });

    action
}
