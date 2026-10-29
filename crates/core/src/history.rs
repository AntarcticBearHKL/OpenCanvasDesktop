//! Undo/redo history, mirroring `web/src/pages/canvas/hooks/use-canvas-history.ts`.
//!
//! Snapshots hold only `{ nodes, connections }`. A change is committed with a fixed
//! debounce so rapid edits coalesce into one entry; the past stack is capped at 50
//! entries by dropping the oldest. Applying an entry (undo/redo) clears the pending
//! commit; the caller is responsible for clearing selection.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::node::{CanvasConnection, CanvasNodeData};

/// Debounce window before a change is committed to `past` (use-canvas-history.ts:69).
pub const HISTORY_DEBOUNCE_MS: u64 = 180;

/// Maximum number of `past` entries kept (use-canvas-history.ts:64).
pub const HISTORY_CAP: usize = 50;

/// The undo/redo payload: the node and connection lists only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub nodes: Vec<CanvasNodeData>,
    pub connections: Vec<CanvasConnection>,
}

impl Snapshot {
    pub fn new(nodes: Vec<CanvasNodeData>, connections: Vec<CanvasConnection>) -> Self {
        Self { nodes, connections }
    }
}

#[derive(Debug, Clone)]
struct Pending {
    snapshot: Snapshot,
    deadline_ms: u64,
}

/// Snapshot undo/redo stack driven by an injected millisecond clock (no async runtime).
#[derive(Debug, Default)]
pub struct History {
    past: VecDeque<Snapshot>,
    future: Vec<Snapshot>,
    last: Option<Snapshot>,
    pending: Option<Pending>,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start from a known baseline (project load), discarding all history.
    pub fn reset(&mut self, snapshot: Snapshot) {
        self.past.clear();
        self.future.clear();
        self.pending = None;
        self.last = Some(snapshot);
    }

    /// Report a state change observed at `now_ms`. Schedules (or reschedules) a commit;
    /// a no-op when the state equals the last committed or already-pending snapshot.
    pub fn record(&mut self, snapshot: Snapshot, now_ms: u64) {
        if self.last.as_ref() == Some(&snapshot) {
            return;
        }
        if let Some(pending) = &self.pending {
            if pending.snapshot == snapshot {
                return;
            }
        }
        self.pending = Some(Pending {
            snapshot,
            deadline_ms: now_ms + HISTORY_DEBOUNCE_MS,
        });
    }

    /// Advance the clock, committing the pending entry once its debounce elapsed.
    /// Returns whether a commit happened.
    pub fn tick(&mut self, now_ms: u64) -> bool {
        if let Some(pending) = self.pending.take() {
            if now_ms >= pending.deadline_ms {
                self.commit(pending.snapshot);
                return true;
            }
            self.pending = Some(pending);
        }
        false
    }

    /// Commit immediately, ignoring the remaining debounce (e.g. on drag end).
    pub fn flush(&mut self) -> bool {
        match self.pending.take() {
            Some(pending) => {
                self.commit(pending.snapshot);
                true
            }
            None => false,
        }
    }

    fn commit(&mut self, snapshot: Snapshot) {
        if let Some(last) = self.last.take() {
            if self.past.len() >= HISTORY_CAP {
                self.past.pop_front();
            }
            self.past.push_back(last);
        }
        self.future.clear();
        self.pending = None;
        self.last = Some(snapshot);
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    pub fn current(&self) -> Option<&Snapshot> {
        self.last.as_ref()
    }

    pub fn past_len(&self) -> usize {
        self.past.len()
    }

    pub fn future_len(&self) -> usize {
        self.future.len()
    }

    /// Move to the previous entry; returns the snapshot to apply.
    pub fn undo(&mut self) -> Option<Snapshot> {
        let previous = self.past.pop_back()?;
        let current = self.last.take()?;
        self.pending = None;
        self.future.push(current);
        self.last = Some(previous.clone());
        Some(previous)
    }

    /// Move to the next entry; returns the snapshot to apply.
    pub fn redo(&mut self) -> Option<Snapshot> {
        let next = self.future.pop()?;
        let current = self.last.take()?;
        self.pending = None;
        self.past.push_back(current);
        self.last = Some(next.clone());
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{CanvasNodeType, Position};

    fn snapshot(tag: u64) -> Snapshot {
        let node = CanvasNodeData {
            id: format!("n{tag}"),
            node_type: CanvasNodeType::Text,
            title: format!("n{tag}"),
            position: Position::new(tag as f64, 0.0),
            width: 340.0,
            height: 240.0,
            metadata: None,
        };
        Snapshot::new(vec![node], Vec::new())
    }

    #[test]
    fn change_within_debounce_then_commit_moves_baseline_to_past() {
        let mut history = History::new();
        history.reset(snapshot(0));

        history.record(snapshot(1), 1_000);
        assert_eq!(history.past_len(), 0);
        // Not yet past the debounce window.
        assert!(!history.tick(1_100));
        assert_eq!(history.past_len(), 0);
        // At 180ms the previous baseline is committed.
        assert!(history.tick(1_180));
        assert_eq!(history.past_len(), 1);
        assert_eq!(history.current(), Some(&snapshot(1)));
    }

    #[test]
    fn rapid_changes_coalesce_into_one_entry() {
        let mut history = History::new();
        history.reset(snapshot(0));
        history.record(snapshot(1), 0);
        history.record(snapshot(2), 100);
        history.record(snapshot(3), 150);
        // The final deadline is 150 + 180 = 330.
        assert!(!history.tick(300));
        assert!(history.tick(330));
        assert_eq!(history.past_len(), 1);
        assert_eq!(history.current(), Some(&snapshot(3)));
    }

    #[test]
    fn recording_the_current_state_is_a_no_op() {
        let mut history = History::new();
        history.reset(snapshot(0));
        history.record(snapshot(0), 0);
        assert!(!history.tick(1_000));
        assert_eq!(history.past_len(), 0);
    }

    #[test]
    fn past_is_capped_at_fifty_dropping_the_oldest() {
        let mut history = History::new();
        history.reset(snapshot(0));
        for i in 1..=60u64 {
            history.record(snapshot(i), i * 1_000);
            history.tick(i * 1_000 + HISTORY_DEBOUNCE_MS);
        }
        assert_eq!(history.past_len(), HISTORY_CAP);
        // The oldest reachable entry is #10 (entries 0..=9 were dropped).
        assert_eq!(history.current(), Some(&snapshot(60)));
        let mut first_undo = None;
        for _ in 0..HISTORY_CAP {
            first_undo = history.undo();
        }
        assert_eq!(first_undo, Some(snapshot(10)));
    }

    #[test]
    fn undo_and_redo_move_entries_between_stacks() {
        let mut history = History::new();
        history.reset(snapshot(0));
        history.record(snapshot(1), 0);
        history.flush();
        history.record(snapshot(2), 1_000);
        history.flush();

        assert!(history.can_undo());
        assert!(!history.can_redo());

        assert_eq!(history.undo(), Some(snapshot(1)));
        assert_eq!(history.current(), Some(&snapshot(1)));
        assert!(history.can_redo());

        assert_eq!(history.undo(), Some(snapshot(0)));
        assert_eq!(history.current(), Some(&snapshot(0)));
        assert!(!history.can_undo());

        assert_eq!(history.undo(), None);

        assert_eq!(history.redo(), Some(snapshot(1)));
        assert_eq!(history.redo(), Some(snapshot(2)));
        assert_eq!(history.redo(), None);
        assert_eq!(history.current(), Some(&snapshot(2)));
    }

    #[test]
    fn a_new_change_clears_the_redo_stack() {
        let mut history = History::new();
        history.reset(snapshot(0));
        history.record(snapshot(1), 0);
        history.flush();
        history.undo();
        assert!(history.can_redo());

        history.record(snapshot(5), 2_000);
        history.flush();
        assert!(!history.can_redo());
        assert_eq!(history.future_len(), 0);
    }

    #[test]
    fn reset_discards_history() {
        let mut history = History::new();
        history.reset(snapshot(0));
        history.record(snapshot(1), 0);
        history.flush();
        history.reset(snapshot(9));
        assert!(!history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.current(), Some(&snapshot(9)));
    }
}
