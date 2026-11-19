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

impl LightBeatApp {
    pub(crate) fn toggle_view(&mut self) {
        self.view = self.view.other();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_toggle_goes_back_and_forth() {
        assert_eq!(View::Patch.other(), View::Desk);
        assert_eq!(View::Desk.other().other(), View::Desk);
    }
}
