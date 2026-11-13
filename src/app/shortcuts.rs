use crate::LightBeatApp;
use crate::theme;
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

pub(crate) const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
pub(crate) const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
pub(crate) const QUIT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Q);
pub(crate) const UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
pub(crate) const REDO: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z);

enum Keys {
    Shortcut(KeyboardShortcut),
    Text(&'static str),
}

const REDO_Y: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Y);
const CTRL_D: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::D);
const CTRL_C: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::C);
const CTRL_V: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::V);

/// Only what the code does; keep in step with `widgets/nodes/graph/{mod,interact}.rs`.
fn sections() -> Vec<(&'static str, Vec<(Keys, &'static str)>)> {
    use Keys::{Shortcut, Text};
    vec![
        (
            "Application",
            vec![
                (Shortcut(SAVE), "Save the project and the setup"),
                (Shortcut(OPEN), "Open a project"),
                (Shortcut(QUIT), "Quit"),
                (Shortcut(UNDO), "Undo (project or setup, whichever changed last)"),
                (Shortcut(REDO), "Redo"),
                (Shortcut(REDO_Y), "Redo"),
            ],
        ),
        (
            "Canvas: view",
            vec![
                (Text("Mouse wheel"), "Zoom at the pointer"),
                (Text("Trackpad scroll"), "Pan"),
                (Text("Ctrl+scroll or pinch"), "Zoom"),
                (Text("Middle drag, Shift+drag or Space+drag"), "Pan"),
                (Text("F or Home"), "Fit the view to everything"),
                (Text("M"), "Show or hide the minimap"),
                (Text("Double-click empty canvas"), "Leave a subgraph"),
            ],
        ),
        (
            "Canvas: nodes",
            vec![
                (Text("Right-click empty canvas"), "Quick add: type to find a node, Up/Down to choose, Enter to add"),
                (Text("Drag on empty canvas"), "Select; dragging left selects everything touched, right only what is fully inside"),
                (Text("Ctrl+click or Shift+click"), "Add to or remove from the selection"),
                (Text("Delete or Backspace"), "Delete the selection"),
                (Shortcut(CTRL_D), "Duplicate the selected nodes"),
                (Shortcut(CTRL_C), "Copy the selected nodes"),
                (Shortcut(CTRL_V), "Paste at the pointer"),
                (Text("H"), "Hide or show unused ports of the selected nodes"),
                (Text("Double-click a subgraph"), "Open it"),
                (Text("Right-click a selected node or a port"), "More actions"),
            ],
        ),
        (
            "Canvas: wires",
            vec![
                (Text("Drag from a port"), "Draw a wire"),
                (Text("2 to 9, 0 while drawing"), "Connect that many ports in a row (0 = 10)"),
                (Text("Esc while drawing"), "Back to a single wire"),
            ],
        ),
    ]
}

impl LightBeatApp {
    pub(crate) fn show_shortcuts_window(&mut self, ctx: &egui::Context) {
        if !self.show_shortcuts {
            return;
        }
        let mut open = true;
        egui::Window::new("Keyboard shortcuts")
            .open(&mut open)
            .collapsible(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (title, rows) in sections() {
                        ui.add_space(4.0);
                        ui.colored_label(theme::TEXT_DIM, egui::RichText::new(title).strong());
                        egui::Grid::new(("shortcuts", title)).num_columns(2).spacing([16.0, 3.0]).show(ui, |ui| {
                            for (keys, what) in rows {
                                let keys = match keys {
                                    Keys::Shortcut(s) => ctx.format_shortcut(&s),
                                    Keys::Text(t) => t.to_string(),
                                };
                                ui.label(egui::RichText::new(keys).monospace());
                                ui.label(what);
                                ui.end_row();
                            }
                        });
                    }
                });
            });
        self.show_shortcuts = open;
    }
}
