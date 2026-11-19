//! The two views of the window: the canvas for patching and the Desk for
//! playing. Which one is open is state of the window, not of the project.

use crate::LightBeatApp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum View {
    Patch,
    Desk,
}

impl View {
    pub(crate) fn other(self) -> View {
        match self {
            View::Patch => View::Desk,
            View::Desk => View::Patch,
        }
    }
}

/// The Add Nodes panel is for patching.
pub(crate) fn add_nodes_panel_allowed(view: View) -> bool {
    view == View::Patch
}

/// A locked Desk has the whole window; an unlocked one keeps the inspector.
pub(crate) fn inspector_allowed(view: View, desk_locked: bool) -> bool {
    view == View::Patch || !desk_locked
}

/// Takes a plain Tab out of the frame's raw input and says whether it was
/// meant to switch views. It has to happen before egui sees the events: on
/// the first pass egui turns a Tab into a focus move and hands focus to the
/// first widget drawn, so by the time the shortcuts run a text field "has
/// focus" that nobody edits. A Tab while a text field has focus (it moves to
/// the next field) and Shift/Ctrl+Tab are left alone.
pub(crate) fn take_view_switch(events: &mut Vec<egui::Event>, text_has_focus: bool) -> bool {
    if text_has_focus {
        return false;
    }
    let plain_tab = |e: &egui::Event| {
        matches!(e, egui::Event::Key { key: egui::Key::Tab, modifiers, .. } if modifiers.is_none())
    };
    let pressed = events.iter().any(|e| {
        plain_tab(e) && matches!(e, egui::Event::Key { pressed: true, repeat: false, .. })
    });
    if pressed {
        events.retain(|e| !plain_tab(e));
    }
    pressed
}

/// Find lives on the canvas, which the Desk does not draw; a text field that
/// is being edited keeps Ctrl+F.
pub(crate) fn find_allowed(view: View, text_has_focus: bool) -> bool {
    view == View::Patch && !text_has_focus
}

impl LightBeatApp {
    pub(crate) fn toggle_view(&mut self) {
        self.view = self.view.other();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_side_panels_give_way_to_a_locked_desk() {
        assert!(add_nodes_panel_allowed(View::Patch));
        assert!(!add_nodes_panel_allowed(View::Desk));
        assert!(inspector_allowed(View::Patch, true), "the lock is the Desk's, the Patch is unaffected");
        assert!(inspector_allowed(View::Desk, false));
        assert!(!inspector_allowed(View::Desk, true));
    }

    fn tab(pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn tab_with_nothing_focused_switches_and_is_hidden_from_egui() {
        let mut ev = vec![tab(true, egui::Modifiers::NONE), tab(false, egui::Modifiers::NONE)];
        assert!(take_view_switch(&mut ev, false));
        assert!(ev.is_empty(), "egui must not see the Tab, or it moves focus");
    }

    #[test]
    fn tab_in_a_text_field_stays_with_egui() {
        let mut ev = vec![tab(true, egui::Modifiers::NONE)];
        assert!(!take_view_switch(&mut ev, true));
        assert_eq!(ev.len(), 1);
    }

    #[test]
    fn shift_tab_and_other_keys_are_not_a_switch() {
        let mut ev = vec![tab(true, egui::Modifiers::SHIFT)];
        assert!(!take_view_switch(&mut ev, false));
        assert_eq!(ev.len(), 1);
        let mut none: Vec<egui::Event> = vec![egui::Event::Text("a".into())];
        assert!(!take_view_switch(&mut none, false));
    }

    #[test]
    fn find_is_for_the_patch_and_not_while_typing() {
        assert!(find_allowed(View::Patch, false));
        assert!(!find_allowed(View::Patch, true));
        assert!(!find_allowed(View::Desk, false));
    }

    #[test]
    fn the_toggle_goes_back_and_forth() {
        assert_eq!(View::Patch.other(), View::Desk);
        assert_eq!(View::Desk.other().other(), View::Desk);
    }
}
