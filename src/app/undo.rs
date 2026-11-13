/// The two documents with their own undo history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UndoDoc {
    Project,
    Setup,
}

/// Which document an Undo (or Redo) acts on: the one changed most recently, so
/// repeated presses walk back through it. When that one has nothing to step
/// to, the other is used, so a press is never wasted while history remains.
pub(crate) fn pick_target(
    last_changed: Option<UndoDoc>,
    can_project: bool,
    can_setup: bool,
) -> Option<UndoDoc> {
    let can = |d: UndoDoc| match d {
        UndoDoc::Project => can_project,
        UndoDoc::Setup => can_setup,
    };
    let first = last_changed.unwrap_or(UndoDoc::Project);
    let second = match first {
        UndoDoc::Project => UndoDoc::Setup,
        UndoDoc::Setup => UndoDoc::Project,
    };
    [first, second].into_iter().find(|d| can(*d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_changed_document_wins_when_both_can_step() {
        assert_eq!(pick_target(Some(UndoDoc::Setup), true, true), Some(UndoDoc::Setup));
        assert_eq!(pick_target(Some(UndoDoc::Project), true, true), Some(UndoDoc::Project));
    }

    #[test]
    fn falls_back_to_the_other_document() {
        assert_eq!(pick_target(Some(UndoDoc::Setup), true, false), Some(UndoDoc::Project));
        assert_eq!(pick_target(Some(UndoDoc::Project), false, true), Some(UndoDoc::Setup));
    }

    #[test]
    fn nothing_to_step_to_gives_none() {
        assert_eq!(pick_target(Some(UndoDoc::Setup), false, false), None);
        assert_eq!(pick_target(None, false, false), None);
    }

    #[test]
    fn without_a_change_marker_the_project_comes_first() {
        assert_eq!(pick_target(None, true, true), Some(UndoDoc::Project));
        assert_eq!(pick_target(None, false, true), Some(UndoDoc::Setup));
    }
}
