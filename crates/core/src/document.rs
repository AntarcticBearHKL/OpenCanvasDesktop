//! The persisted project document and the `"open-canvas"` v3 export envelope,
//! mirroring `CanvasProject` (web/src/stores/canvas/use-canvas-store.ts:9-19) and
//! `CanvasExportFile` (web/src/types/canvas-export.ts).

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::node::{CanvasConnection, CanvasNodeData, CanvasScope};

/// Export envelope discriminator (`app: "open-canvas"`).
pub const EXPORT_APP: &str = "open-canvas";

/// Supported export schema version (`version: 3`).
pub const EXPORT_VERSION: u32 = 3;

#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported export app {found:?} (expected {expected:?})", expected = EXPORT_APP)]
    InvalidApp { found: String },
    #[error("unsupported export version {found} (expected {expected})", expected = EXPORT_VERSION)]
    InvalidVersion { found: u32 },
    #[error("unsupported library app {found:?} (expected {expected:?})", expected = LIBRARY_APP)]
    InvalidLibraryApp { found: String },
    #[error("unsupported library version {found} (expected {expected})", expected = LIBRARY_VERSION)]
    InvalidLibraryVersion { found: u32 },
}

/// `ViewportTransform` (canvas.ts:10-14). `k` is the zoom factor.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewportTransform {
    pub x: f64,
    pub y: f64,
    pub k: f64,
}

impl Default for ViewportTransform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        }
    }
}

/// `CanvasProject` (use-canvas-store.ts:9-19) — the drawable document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasProject {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    pub nodes: Vec<CanvasNodeData>,
    pub connections: Vec<CanvasConnection>,
    pub viewport: ViewportTransform,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<CanvasScope>,
}

impl CanvasProject {
    /// A new empty project with the default viewport (`{ x: 0, y: 0, k: 1 }`).
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        created_at: impl Into<String>,
        updated_at: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            created_at: created_at.into(),
            updated_at: updated_at.into(),
            group_id: None,
            nodes: Vec::new(),
            connections: Vec::new(),
            viewport: ViewportTransform::default(),
            scope: None,
        }
    }
}

/// Discriminator of the desktop library index (`library.json`).
pub const LIBRARY_APP: &str = "open-canvas-desktop";

/// Schema version of the desktop library index.
pub const LIBRARY_VERSION: u32 = 1;

/// `CanvasGroup` (use-canvas-store.ts:21-25) — a project-library folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasGroup {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

/// Library row kinds: which workspace a project opens into.
pub const PROJECT_KIND_CANVAS: &str = "canvas";

/// The library's index row for one project (`CanvasProjectRow` metadata): enough
/// to render the library grid without loading every project document. `kind`
/// selects the workspace (canvas editor vs the image studio); older index files
/// without the field read back as canvas projects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIndexEntry {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(default = "default_project_kind")]
    pub kind: String,
}

fn default_project_kind() -> String {
    PROJECT_KIND_CANVAS.to_string()
}

/// The desktop library index (`library.json`): groups plus one row per project.
/// Each project document lives beside it as a full [`CanvasProject`] file, so the
/// per-project JSON stays byte-compatible with the web store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryIndex {
    pub app: String,
    pub version: u32,
    pub groups: Vec<CanvasGroup>,
    pub projects: Vec<ProjectIndexEntry>,
}

impl Default for LibraryIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl LibraryIndex {
    /// An empty index carrying the current `app`/`version` stamps.
    pub fn new() -> Self {
        Self {
            app: LIBRARY_APP.to_string(),
            version: LIBRARY_VERSION,
            groups: Vec::new(),
            projects: Vec::new(),
        }
    }

    pub fn group(&self, id: &str) -> Option<&CanvasGroup> {
        self.groups.iter().find(|group| group.id == id)
    }

    pub fn project(&self, id: &str) -> Option<&ProjectIndexEntry> {
        self.projects.iter().find(|row| row.id == id)
    }

    /// Rejects index files that were not written by this app.
    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.app != LIBRARY_APP {
            return Err(DocumentError::InvalidLibraryApp {
                found: self.app.clone(),
            });
        }
        if self.version != LIBRARY_VERSION {
            return Err(DocumentError::InvalidLibraryVersion {
                found: self.version,
            });
        }
        Ok(())
    }

    /// Inserts (at the head, newest first — the web store's array order) or
    /// refreshes the row of `project`.
    pub fn upsert_project(&mut self, project: &CanvasProject) {
        self.upsert_entry(ProjectIndexEntry {
            id: project.id.clone(),
            title: project.title.clone(),
            created_at: project.created_at.clone(),
            updated_at: project.updated_at.clone(),
            group_id: project.group_id.clone(),
            kind: PROJECT_KIND_CANVAS.to_string(),
        });
    }

    /// Inserts or refreshes any library row (canvas or pixel project).
    pub fn upsert_entry(&mut self, row: ProjectIndexEntry) {
        match self.projects.iter().position(|entry| entry.id == row.id) {
            Some(index) => self.projects[index] = row,
            None => self.projects.insert(0, row),
        }
    }

    /// Drops one project row; `true` when it was present.
    pub fn remove_project(&mut self, id: &str) -> bool {
        let before = self.projects.len();
        self.projects.retain(|entry| entry.id != id);
        self.projects.len() != before
    }
}

/// `CanvasExportAsset` (canvas-export.ts:16-21).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasExportAsset {
    pub storage_key: String,
    pub path: String,
    pub mime_type: String,
    pub bytes: u64,
}

/// `CanvasProjectExportItem` (canvas-export.ts:10-14).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasProjectExportItem {
    pub project: CanvasProject,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    pub files: Vec<CanvasExportAsset>,
}

/// `CanvasExportFile` (canvas-export.ts:3-8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasExportFile {
    pub app: String,
    pub version: u32,
    pub exported_at: String,
    pub projects: Vec<CanvasProjectExportItem>,
}

impl CanvasExportFile {
    /// Reject envelopes that are not a supported `"open-canvas"` v3 file.
    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.app != EXPORT_APP {
            return Err(DocumentError::InvalidApp {
                found: self.app.clone(),
            });
        }
        if self.version != EXPORT_VERSION {
            return Err(DocumentError::InvalidVersion {
                found: self.version,
            });
        }
        Ok(())
    }
}

/// Serialize any document value to JSON.
pub fn to_json<T: Serialize>(value: &T) -> Result<String, DocumentError> {
    Ok(serde_json::to_string(value)?)
}

/// Deserialize any document value from JSON.
pub fn from_json<T: DeserializeOwned>(json: &str) -> Result<T, DocumentError> {
    Ok(serde_json::from_str(json)?)
}

/// Deserialize an export envelope and validate `app`/`version`.
pub fn deserialize_export(json: &str) -> Result<CanvasExportFile, DocumentError> {
    let file: CanvasExportFile = from_json(json)?;
    file.validate()?;
    Ok(file)
}

/// Deserialize a library index and validate `app`/`version`.
pub fn deserialize_library(json: &str) -> Result<LibraryIndex, DocumentError> {
    let index: LibraryIndex = from_json(json)?;
    index.validate()?;
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{
        CanvasGenerationMode, CanvasNodeMetadata, CanvasNodeStatus, CanvasNodeType, Position,
    };

    fn sample_project() -> CanvasProject {
        let metadata = CanvasNodeMetadata {
            status: Some(CanvasNodeStatus::Idle),
            prompt: Some("draw a fox".into()),
            generation_mode: Some(CanvasGenerationMode::Image),
            count: Some(2),
            references: Some(vec!["image:a".into(), "media:b".into()]),
            locked: Some(true),
            ..Default::default()
        };

        let node = CanvasNodeData {
            id: "image-generation-1".into(),
            node_type: CanvasNodeType::ImageGeneration,
            title: "Image Generation".into(),
            position: Position::new(120.0, 240.0),
            width: 412.0,
            height: 576.0,
            metadata: Some(metadata),
        };
        let connection = CanvasConnection {
            id: "conn-1".into(),
            from_node_id: "prompt-1".into(),
            to_node_id: "image-generation-1".into(),
            relation: None,
            kind: None,
            reason: None,
            changes: None,
            agent_run_id: None,
        };
        CanvasProject {
            nodes: vec![node],
            connections: vec![connection],
            ..CanvasProject::new(
                "proj-1",
                "My Canvas",
                "2026-01-01T00:00:00Z",
                "2026-01-02T00:00:00Z",
            )
        }
    }

    #[test]
    fn project_uses_camel_case_keys_and_default_viewport() {
        let json = to_json(&sample_project()).unwrap();
        assert!(json.contains("\"createdAt\":\"2026-01-01T00:00:00Z\""));
        assert!(json.contains("\"updatedAt\""));
        assert!(json.contains("\"viewport\":{\"x\":0.0,\"y\":0.0,\"k\":1.0}"));
        assert!(json.contains("\"fromNodeId\":\"prompt-1\""));
        assert!(json.contains("\"generationMode\":\"image\""));
        // groupId is absent, not null.
        assert!(!json.contains("groupId"));
    }

    #[test]
    fn project_round_trips_losslessly() {
        let original = sample_project();
        let json = to_json(&original).unwrap();
        let back: CanvasProject = from_json(&json).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn export_envelope_round_trips_and_validates() {
        let export = CanvasExportFile {
            app: EXPORT_APP.to_string(),
            version: EXPORT_VERSION,
            exported_at: "2026-01-02T00:00:00Z".into(),
            projects: vec![CanvasProjectExportItem {
                project: sample_project(),
                group_name: Some("Folder".into()),
                files: vec![CanvasExportAsset {
                    storage_key: "image:a".into(),
                    path: "projects/proj-1/files/a.png".into(),
                    mime_type: "image/png".into(),
                    bytes: 2048,
                }],
            }],
        };
        let json = to_json(&export).unwrap();
        assert!(json.contains("\"app\":\"open-canvas\""));
        assert!(json.contains("\"version\":3"));
        assert!(json.contains("\"groupName\":\"Folder\""));

        let parsed = deserialize_export(&json).unwrap();
        assert_eq!(parsed, export);
    }

    #[test]
    fn export_validation_rejects_wrong_app_and_version() {
        let mut export = CanvasExportFile {
            app: "other-app".into(),
            version: EXPORT_VERSION,
            exported_at: "now".into(),
            projects: Vec::new(),
        };
        assert!(matches!(
            export.validate(),
            Err(DocumentError::InvalidApp { .. })
        ));

        export.app = EXPORT_APP.to_string();
        export.version = 2;
        assert!(matches!(
            export.validate(),
            Err(DocumentError::InvalidVersion { .. })
        ));
    }

    #[test]
    fn deserializing_an_unknown_node_type_and_extra_metadata_does_not_fail() {
        let json = r#"{
            "id": "proj-x",
            "title": "Plugin Canvas",
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z",
            "nodes": [{
                "id": "plugin-1",
                "type": "acme:sticker",
                "title": "Sticker",
                "position": { "x": 0.0, "y": 0.0 },
                "width": 100.0,
                "height": 100.0,
                "metadata": { "pluginPayload": { "nested": true } }
            }],
            "connections": [],
            "viewport": { "x": 10.0, "y": 20.0, "k": 1.5 }
        }"#;
        let project: CanvasProject = from_json(json).unwrap();
        assert_eq!(
            project.nodes[0].node_type,
            CanvasNodeType::Plugin("acme:sticker".into())
        );
        assert_eq!(project.viewport.k, 1.5);
    }

    #[test]
    fn library_index_round_trips_with_camel_case_keys() {
        let mut index = LibraryIndex::new();
        index.groups.push(CanvasGroup {
            id: "group-1".into(),
            name: "Product".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        });
        let mut project = sample_project();
        project.group_id = Some("group-1".into());
        index.upsert_project(&project);

        let json = to_json(&index).unwrap();
        assert!(json.contains("\"app\":\"open-canvas-desktop\""));
        assert!(json.contains("\"version\":1"));
        assert!(json.contains("\"createdAt\":\"2026-01-01T00:00:00Z\""));
        assert!(json.contains("\"groupId\":\"group-1\""));

        let back = deserialize_library(&json).unwrap();
        assert_eq!(back, index);
    }

    #[test]
    fn upsert_refreshes_the_row_in_place_and_remove_drops_it() {
        let mut index = LibraryIndex::new();
        let mut first = sample_project();
        first.id = "p1".into();
        let mut second = sample_project();
        second.id = "p2".into();
        index.upsert_project(&first);
        index.upsert_project(&second);
        // Newest first (web store order).
        assert_eq!(index.projects[0].id, "p2");

        first.title = "Renamed".into();
        first.updated_at = "2026-02-02T00:00:00Z".into();
        index.upsert_project(&first);
        assert_eq!(index.projects.len(), 2);
        assert_eq!(
            index.project("p1").map(|row| row.title.as_str()),
            Some("Renamed")
        );
        assert_eq!(
            index.project("p1").map(|row| row.updated_at.as_str()),
            Some("2026-02-02T00:00:00Z")
        );

        assert!(index.remove_project("p2"));
        assert!(!index.remove_project("p2"));
        assert_eq!(index.projects.len(), 1);
    }

    #[test]
    fn library_validation_rejects_foreign_files() {
        let mut index = LibraryIndex::new();
        index.app = "other-app".into();
        assert!(matches!(
            index.validate(),
            Err(DocumentError::InvalidLibraryApp { .. })
        ));
        index.app = LIBRARY_APP.to_string();
        index.version = 9;
        assert!(matches!(
            index.validate(),
            Err(DocumentError::InvalidLibraryVersion { .. })
        ));
    }

    #[test]
    fn index_rows_carry_a_workspace_kind_and_old_files_read_as_canvas() {
        let json = r#"{"app":"open-canvas-desktop","version":1,"groups":[],"projects":[
            {"id":"p1","title":"Old","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}]}"#;
        let index = deserialize_library(json).unwrap();
        assert_eq!(
            index.project("p1").map(|row| row.kind.as_str()),
            Some("canvas")
        );

        let mut index = LibraryIndex::new();
        index.upsert_entry(ProjectIndexEntry {
            id: "p2".into(),
            title: "Board".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            group_id: None,
            kind: "image".into(),
        });
        let json = to_json(&index).unwrap();
        assert!(json.contains("\"kind\":\"image\""));
        let back = deserialize_library(&json).unwrap();
        assert_eq!(
            back.project("p2").map(|row| row.kind.as_str()),
            Some("image")
        );

        // Canvas projects always stamp the canvas kind.
        let mut index = LibraryIndex::new();
        index.upsert_project(&sample_project());
        assert_eq!(
            index.project("proj-1").map(|row| row.kind.as_str()),
            Some("canvas")
        );
    }
}
