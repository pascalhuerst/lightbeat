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

    #[test]
    fn the_toggle_goes_back_and_forth() {
        assert_eq!(View::Patch.other(), View::Desk);
        assert_eq!(View::Desk.other().other(), View::Desk);
    }
}
