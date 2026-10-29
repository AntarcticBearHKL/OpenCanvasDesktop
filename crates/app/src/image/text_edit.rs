//! In-canvas text editing for the Image Studio board.
//!
//! Web parity (`ps-canvas.tsx`): a double-click on a text layer swaps its
//! rendered glyphs for a textarea positioned over the layer box, pre-filled
//! with the layer's content; Enter commits, Shift+Enter inserts a newline,
//! Escape cancels and a click elsewhere commits. The draft lives here so the
//! commit writes exactly one "Edit Text" history entry and Escape can close the
//! editor without touching the document.
//! Owns the in-canvas text draft (`TextEdit`).

/// One open text draft, scoped to the layer that owns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub layer_id: String,
    pub original: String,
    pub draft: String,
}

/// What a commit hands back when the text actually changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedText {
    pub layer_id: String,
    pub text: String,
}

/// The app's single in-canvas text editor slot.
#[derive(Debug, Default)]
pub struct TextEditState {
    edit: Option<TextEdit>,
}

impl TextEditState {
    pub fn open(&self) -> Option<&TextEdit> {
        self.edit.as_ref()
    }

    pub fn is_editing(&self) -> bool {
        self.edit.is_some()
    }

    pub fn is_editing_layer(&self, layer_id: &str) -> bool {
        self.open().is_some_and(|edit| edit.layer_id == layer_id)
    }

    /// Opens (or re-seeds) the editor for `layer_id`; returns whether a sync is
    /// needed so the overlay mounts with the seed text.
    pub fn begin(&mut self, layer_id: &str, content: &str) -> bool {
        let same = self.open().is_some_and(|edit| {
            edit.layer_id == layer_id && edit.draft == content && edit.original == content
        });
        if same {
            return false;
        }
        self.edit = Some(TextEdit {
            layer_id: layer_id.to_string(),
            original: content.to_string(),
            draft: content.to_string(),
        });
        true
    }

    /// Applies a keystroke to the draft; edits for any other layer are dropped
    /// (a stale overlay cannot write around a cancel).
    pub fn change(&mut self, layer_id: &str, text: &str) -> bool {
        match self.edit.as_mut() {
            Some(edit) if edit.layer_id == layer_id => {
                edit.draft = text.to_string();
                true
            }
            _ => false,
        }
    }

    /// Closes the draft and returns the document write. `final_text` is the
    /// textarea's current text (the overlay owns it while typing); an untouched
    /// or unchanged draft returns `None` so no history entry is created.
    pub fn commit(&mut self, final_text: Option<&str>) -> Option<CommittedText> {
        let mut edit = self.edit.take()?;
        if let Some(text) = final_text {
            edit.draft = text.to_string();
        }
        if edit.draft == edit.original {
            return None;
        }
        Some(CommittedText {
            layer_id: edit.layer_id,
            text: edit.draft,
        })
    }

    /// Closes the draft without a write (Escape, view switch, node deleted).
    /// Returns the layer whose render must be restored.
    pub fn cancel(&mut self) -> Option<String> {
        self.edit.take().map(|edit| edit.layer_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), &'static str>;

    #[test]
    fn begin_prefills_and_commit_writes_once() -> TestResult {
        let mut state = TextEditState::default();
        assert!(state.begin("text-1", "Hello"));
        assert!(state.is_editing_layer("text-1"));
        assert_eq!(state.open().map(|edit| edit.draft.as_str()), Some("Hello"));

        assert!(state.change("text-1", "Hello world"));
        let committed = state
            .commit(Some("Hello world"))
            .ok_or("a dirty draft commits")?;
        assert_eq!(committed.layer_id, "text-1");
        assert_eq!(committed.text, "Hello world");
        assert!(!state.is_editing(), "commit closes the editor");
        assert!(state.commit(None).is_none(), "no double write");
        Ok(())
    }

    #[test]
    fn unchanged_drafts_do_not_write() {
        let mut state = TextEditState::default();
        assert!(state.begin("text-1", "Hello"));
        assert!(state.commit(Some("Hello")).is_none(), "same text, no write");
        assert!(state.begin("text-1", "Hello"));
        assert!(state.commit(None).is_none(), "no keystroke, no write");
    }

    #[test]
    fn escape_cancels_without_a_write_and_blocks_late_input() {
        let mut state = TextEditState::default();
        assert!(state.begin("text-1", "Hello"));
        assert!(state.change("text-1", "Hello world"));
        assert_eq!(state.cancel().as_deref(), Some("text-1"));
        assert!(!state.is_editing());
        assert!(state.commit(Some("late")).is_none());
        assert!(!state.change("text-1", "late keystroke"));
    }

    #[test]
    fn the_final_text_wins_over_the_last_keystroke() -> TestResult {
        let mut state = TextEditState::default();
        assert!(state.begin("text-1", "old"));
        assert!(state.change("text-1", "partial"));
        let committed = state.commit(Some("final text")).ok_or("commit")?;
        assert_eq!(committed.text, "final text");
        Ok(())
    }

    #[test]
    fn drafts_are_scoped_to_one_layer() {
        let mut state = TextEditState::default();
        assert!(state.begin("text-1", "one"));
        assert!(!state.change("text-2", "hijack"));
        assert!(state.is_editing_layer("text-1"));
        assert!(!state.is_editing_layer("text-2"));
        assert!(state.begin("text-2", "two"), "switching re-seeds");
        assert!(!state.is_editing_layer("text-1"));
    }
}
