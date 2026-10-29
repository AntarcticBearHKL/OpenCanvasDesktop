//! Document ↔ `CanvasProject` conversion (a further `impl Doc` block, like
//! `nav.rs`/`doc_edit.rs`): the editor owns `nodes`/`connections`/`viewport` as
//! plain state, persistence and the library exchange full `CanvasProject`s.

use open_canvas_core::document::{CanvasProject, ViewportTransform};

use crate::coords::Viewport;
use crate::doc::Doc;

/// The open editor's identity (content lives in [`Doc`]).
///
/// It lives next to [`Doc::to_project`], the only consumer of these five
/// strings: the save shape is domain data, not library UI. `crates/app`'s
/// `library` module re-exports it, so `crate::library::EditorMeta` keeps
/// working for the editor/sidebar code that also needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorMeta {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub group_id: Option<String>,
}

impl Doc {
    /// An empty document (a freshly created project).
    pub fn empty() -> Self {
        Self::assemble(Vec::new(), Vec::new())
    }

    /// Loads a stored project: nodes, connections and the saved viewport; the
    /// undo history restarts at it (web `resetHistory` on project load).
    pub fn from_project(project: &CanvasProject) -> Self {
        let mut doc = Self::assemble(project.nodes.clone(), project.connections.clone());
        doc.viewport = Viewport {
            x: project.viewport.x,
            y: project.viewport.y,
            k: project.viewport.k,
        };
        doc
    }

    /// Snapshots the editor into the persisted document; `meta.updated_at` is
    /// the timestamp this save carries.
    pub fn to_project(&self, meta: &EditorMeta) -> CanvasProject {
        CanvasProject {
            id: meta.id.clone(),
            title: meta.title.clone(),
            created_at: meta.created_at.clone(),
            updated_at: meta.updated_at.clone(),
            group_id: meta.group_id.clone(),
            nodes: self.nodes.clone(),
            connections: self.connections.clone(),
            viewport: ViewportTransform {
                x: self.viewport.x,
                y: self.viewport.y,
                k: self.viewport.k,
            },
            scope: None,
        }
    }
}
