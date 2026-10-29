//! Inline node editing: the body/title draft state machine behind the card's
//! in-place editors.
//!
//! Web parity (`canvas-node.tsx`): a double-click on a Text node opens the body
//! textarea (`isEditingContent`, line 150), a double-click on the title opens the
//! rename input (`isEditingTitle`, line 151); the rename commits on blur/Enter
//! with `trim() || old title` (lines 205-210), Escape restores the old title
//! (lines 375-378). The desktop keeps the draft here so a commit writes exactly
//! one history entry instead of one per keystroke, and so Escape can cancel
//! without touching the document.
//!
//! Commit-on-blur safety: Escape clears the edit, so the follow-up blur callback
//! finds no open draft and commits nothing — cancel can never leak a write.

/// Which in-place editor owns the draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditTarget {
    Body,
    Title,
}

impl EditTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Title => "title",
        }
    }
}

/// The one open draft (a card edits either its body or its title, never both).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineEdit {
    pub target: EditTarget,
    pub node_id: String,
    pub original: String,
    pub draft: String,
}

/// What [`EditState::commit`] hands back for the document write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedEdit {
    pub target: EditTarget,
    pub node_id: String,
    /// The text to store (title commits are trimmed, never empty).
    pub text: String,
}

/// The app's single inline-edit slot.
#[derive(Debug, Default)]
pub struct EditState {
    edit: Option<InlineEdit>,
}

impl EditState {
    pub fn open(&self) -> Option<&InlineEdit> {
        self.edit.as_ref()
    }

    pub fn is_editing_body(&self, node_id: &str) -> bool {
        self.open()
            .is_some_and(|edit| edit.target == EditTarget::Body && edit.node_id == node_id)
    }

    pub fn is_editing_title(&self, node_id: &str) -> bool {
        self.open()
            .is_some_and(|edit| edit.target == EditTarget::Title && edit.node_id == node_id)
    }

    /// The draft shown by the body editor of `node_id` ("" when not editing).
    pub fn body_draft(&self, node_id: &str) -> String {
        match self.open() {
            Some(edit) if edit.target == EditTarget::Body && edit.node_id == node_id => {
                edit.draft.clone()
            }
            _ => String::new(),
        }
    }

    /// The draft shown by the title editor of `node_id` ("" when not editing).
    pub fn title_draft(&self, node_id: &str) -> String {
        match self.open() {
            Some(edit) if edit.target == EditTarget::Title && edit.node_id == node_id => {
                edit.draft.clone()
            }
            _ => String::new(),
        }
    }

    /// Opens (or re-seeds) the body editor. Returns `true` when the caller must
    /// resync the row so the input mounts with the seed text.
    pub fn begin_body(&mut self, node_id: &str, current: &str) -> bool {
        self.begin(EditTarget::Body, node_id, current)
    }

    /// Opens (or re-seeds) the title editor.
    pub fn begin_title(&mut self, node_id: &str, current: &str) -> bool {
        self.begin(EditTarget::Title, node_id, current)
    }

    fn begin(&mut self, target: EditTarget, node_id: &str, current: &str) -> bool {
        let same = self.open().is_some_and(|edit| {
            edit.target == target && edit.node_id == node_id && edit.draft == current
        });
        if same {
            return false;
        }
        self.edit = Some(InlineEdit {
            target,
            node_id: node_id.to_string(),
            original: current.to_string(),
            draft: current.to_string(),
        });
        true
    }

    /// Applies a keystroke to the open draft; edits for any other node are
    /// dropped (a stale input cannot write around an Escape).
    pub fn change(&mut self, node_id: &str, text: &str) -> bool {
        match self.edit.as_mut() {
            Some(edit) if edit.node_id == node_id => {
                edit.draft = text.to_string();
                true
            }
            _ => false,
        }
    }

    /// Closes the draft and returns the document write, or `None` when nothing
    /// changed (an untouched editor must not dirty the document).
    pub fn commit(&mut self) -> Option<CommittedEdit> {
        let edit = self.edit.take()?;
        let text = match edit.target {
            EditTarget::Title => {
                let trimmed = edit.draft.trim();
                if trimmed.is_empty() {
                    edit.original.clone()
                } else {
                    trimmed.to_string()
                }
            }
            EditTarget::Body => edit.draft.clone(),
        };
        if text == edit.original {
            return None;
        }
        Some(CommittedEdit {
            target: edit.target,
            node_id: edit.node_id,
            text,
        })
    }

    /// Closes the draft without a write (Escape, node deleted, view switch).
    /// Returns the node whose editor must be re-seeded.
    pub fn cancel(&mut self) -> Option<(EditTarget, String)> {
        let edit = self.edit.take()?;
        Some((edit.target, edit.node_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), &'static str>;

    fn body_state(node_id: &str, original: &str) -> Result<EditState, &'static str> {
        let mut state = EditState::default();
        if !state.begin_body(node_id, original) {
            return Err("the body editor must open");
        }
        Ok(state)
    }

    #[test]
    fn body_edit_commits_exactly_once_after_changes() -> TestResult {
        let mut state = body_state("text-1", "hello")?;
        assert!(state.is_editing_body("text-1"));
        assert!(state.change("text-1", "hello world"));
        let committed = state.commit().ok_or("a dirty draft commits")?;
        assert_eq!(committed.target, EditTarget::Body);
        assert_eq!(committed.node_id, "text-1");
        assert_eq!(committed.text, "hello world");
        assert!(state.open().is_none(), "commit closes the editor");
        assert!(state.commit().is_none(), "no double write");
        Ok(())
    }

    #[test]
    fn untouched_or_unchanged_drafts_do_not_write() -> TestResult {
        let mut state = body_state("text-1", "hello")?;
        assert!(state.commit().is_none(), "no change, no doc write");

        assert!(state.begin_body("text-1", "hello"));
        assert!(state.change("text-1", "hello"));
        assert!(state.commit().is_none(), "same text, no doc write");
        Ok(())
    }

    #[test]
    fn escape_cancels_without_a_write_and_blocks_late_blur() -> TestResult {
        let mut state = body_state("text-1", "hello")?;
        assert!(state.change("text-1", "hello world"));
        let (target, node_id) = state.cancel().ok_or("cancel reports the editor")?;
        assert_eq!(target, EditTarget::Body);
        assert_eq!(node_id, "text-1");
        assert!(state.open().is_none());
        assert!(
            state.commit().is_none(),
            "the blur after an Escape writes nothing"
        );
        assert!(!state.change("text-1", "late keystroke"));
        Ok(())
    }

    #[test]
    fn title_commit_trims_and_falls_back_to_the_old_title() -> TestResult {
        let mut state = EditState::default();
        assert!(state.begin_title("image-1", "Cover"));
        assert!(state.change("image-1", "   "));
        assert!(
            state.commit().is_none(),
            "an empty title keeps the old one, so nothing changes"
        );

        assert!(state.begin_title("image-1", "Cover"));
        assert!(state.change("image-1", "  Hero  "));
        let committed = state.commit().ok_or("a trimmed title differs")?;
        assert_eq!(committed.text, "Hero");
        Ok(())
    }

    #[test]
    fn drafts_are_scoped_to_one_node_and_one_editor() {
        let mut state = EditState::default();
        assert!(state.begin_body("a", "a-body"));
        assert_eq!(state.body_draft("a"), "a-body");
        assert_eq!(state.body_draft("b"), "", "other nodes have no draft");
        assert!(!state.change("b", "hijack"), "stale inputs are dropped");
        assert!(state.change("a", "a-body-2"));
        assert!(!state.is_editing_title("a"));
        assert!(
            state.begin_title("a", "A title"),
            "switching editor re-seeds"
        );
        assert!(!state.is_editing_body("a"));
        assert_eq!(state.title_draft("a"), "A title");
    }

    #[test]
    fn reseeding_the_same_editor_is_a_no_op() {
        let mut state = EditState::default();
        assert!(state.begin_title("a", "T"));
        assert!(
            !state.begin_title("a", "T"),
            "same seed: no resync needed, the input keeps focus"
        );
        assert!(state.begin_title("a", "Other"));
    }
}
