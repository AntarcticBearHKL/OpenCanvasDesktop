//! Inline-asset collection and reference rewriting for export archives.
//!
//! 1. [`collect_export_assets`] walks the exported projects, decodes every
//!    inline `data:` URL held by media nodes (image/video content, image-stack
//!    entries, thumbnails/posters), stores the bytes as [`Blob`]s with stable
//!    `projects/<id>/files/...` paths, and replaces each data URL with an
//!    `archive:<path>` reference.
//! 2. [`restore_export_assets`] is the inverse: it rebuilds the data URLs from
//!    the blobs and swaps the references back in.
//!
//! Only data already inside the project JSON can be collected; assets that live
//! only behind a `storageKey` need the app's media store and stay out of this
//! pure function.

use std::collections::{BTreeMap, BTreeSet};

use crate::archive::{ArchiveError, Blob, CollectedExport};
use crate::archive_base64 as base64;
use crate::archive_inline::{derived_key, parse_data_url};
use crate::archive_paths::{sanitize_project_id, unique_path};
use crate::archive_wire::member_path_is_safe;
use crate::document::{CanvasExportAsset, CanvasExportFile, CanvasProject};
use crate::node::{CanvasNodeMetadata, CanvasNodeType};

/// Prefix of a project-JSON reference to an archive member path.
pub const ARCHIVE_REF_PREFIX: &str = "archive:";

/// Wrap an archive member path as a project-JSON reference.
pub fn archive_ref(path: &str) -> String {
    format!("{ARCHIVE_REF_PREFIX}{path}")
}

/// Collect every inline data-URL asset of `export` into blobs and rewrite the
/// project JSON to reference their archive paths.
///
/// The returned export keeps `app`, `version`, `exportedAt`, `groupName`, and
/// the project list unchanged; each project's `files` is replaced with the
/// assets collected from that project. A project without inline assets keeps
/// `files: []` and serializes byte-identically to its input.
pub fn collect_export_assets(export: &CanvasExportFile) -> Result<CollectedExport, ArchiveError> {
    export.validate()?;
    let mut next = export.clone();
    let mut blobs: Vec<Blob> = Vec::new();
    for item in &mut next.projects {
        let project_id = sanitize_project_id(&item.project.id);
        let mut collector = Collector::new(&project_id, &mut blobs);
        collector.walk(&mut item.project)?;
        item.files = collector.files;
    }
    Ok(CollectedExport {
        export: next,
        blobs,
    })
}

/// Inline archive references back into data URLs using `blobs`.
///
/// Values that are already inline (a web export) and values that never were
/// asset references are left untouched. A reference with no matching blob is a
/// typed error, so a partial restore is never returned.
pub fn restore_export_assets(
    export: &CanvasExportFile,
    blobs: &[Blob],
) -> Result<CanvasExportFile, ArchiveError> {
    export.validate()?;
    let mut by_path: BTreeMap<&str, &Blob> = BTreeMap::new();
    for blob in blobs {
        if !member_path_is_safe(&blob.path) {
            return Err(ArchiveError::UnsafeMemberPath {
                path: blob.path.clone(),
            });
        }
        if by_path.insert(blob.path.as_str(), blob).is_some() {
            return Err(ArchiveError::DuplicateBlob {
                path: blob.path.clone(),
            });
        }
    }

    let mut next = export.clone();
    for item in &mut next.projects {
        restore_project(&mut item.project, &by_path)?;
    }
    Ok(next)
}

struct Collector<'a> {
    project_id: &'a str,
    blobs: &'a mut Vec<Blob>,
    files: Vec<CanvasExportAsset>,
    used: BTreeSet<String>,
}

impl<'a> Collector<'a> {
    fn new(project_id: &'a str, blobs: &'a mut Vec<Blob>) -> Self {
        Self {
            project_id,
            blobs,
            files: Vec::new(),
            used: BTreeSet::new(),
        }
    }

    fn walk(&mut self, project: &mut CanvasProject) -> Result<(), ArchiveError> {
        for node in &mut project.nodes {
            if !is_media_node(&node.node_type) {
                continue;
            }
            if let Some(metadata) = node.metadata.as_mut() {
                self.rewrite_metadata(metadata)?;
            }
        }
        Ok(())
    }

    fn rewrite_metadata(&mut self, metadata: &mut CanvasNodeMetadata) -> Result<(), ArchiveError> {
        self.rewrite_slot(&mut metadata.content, metadata.storage_key.as_deref())?;
        self.rewrite_slot(&mut metadata.thumbnail, metadata.thumbnail_key.as_deref())?;
        if let Some(images) = metadata.images.as_mut() {
            for image in images {
                self.rewrite_value(&mut image.content, image.storage_key.as_deref())?;
                self.rewrite_slot(&mut image.thumbnail, image.thumbnail_key.as_deref())?;
            }
        }
        Ok(())
    }

    fn rewrite_slot(
        &mut self,
        slot: &mut Option<String>,
        preferred_key: Option<&str>,
    ) -> Result<(), ArchiveError> {
        if let Some(value) = slot.as_mut() {
            self.rewrite_value(value, preferred_key)?;
        }
        Ok(())
    }

    fn rewrite_value(
        &mut self,
        value: &mut String,
        preferred_key: Option<&str>,
    ) -> Result<(), ArchiveError> {
        let Some(inline) = parse_data_url(value)? else {
            return Ok(());
        };
        let storage_key = preferred_key
            .filter(|key| !key.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| derived_key(&inline.mime_type, &inline.data));

        // Reuse an identical payload already recorded under the same key so the
        // same asset referenced twice produces one member.
        for (index, blob) in self.blobs.iter().enumerate() {
            if blob.mime_type == inline.mime_type
                && blob.data == inline.data
                && self.files[index].storage_key == storage_key
            {
                *value = archive_ref(&blob.path);
                return Ok(());
            }
        }

        let path = unique_path(self.project_id, &storage_key, &inline.mime_type, &self.used);
        self.used.insert(path.clone());
        self.files.push(CanvasExportAsset {
            storage_key,
            path: path.clone(),
            mime_type: inline.mime_type.clone(),
            bytes: inline.data.len() as u64,
        });
        self.blobs.push(Blob {
            path: path.clone(),
            mime_type: inline.mime_type,
            data: inline.data,
        });
        *value = archive_ref(&path);
        Ok(())
    }
}

fn is_media_node(node_type: &CanvasNodeType) -> bool {
    matches!(
        node_type,
        CanvasNodeType::Image
            | CanvasNodeType::ImageStack
            | CanvasNodeType::Video
            | CanvasNodeType::VideoGeneration
            | CanvasNodeType::ImageGeneration
    )
}

fn restore_project(
    project: &mut CanvasProject,
    by_path: &BTreeMap<&str, &Blob>,
) -> Result<(), ArchiveError> {
    for node in &mut project.nodes {
        if !is_media_node(&node.node_type) {
            continue;
        }
        if let Some(metadata) = node.metadata.as_mut() {
            restore_slot(&mut metadata.content, by_path)?;
            restore_slot(&mut metadata.thumbnail, by_path)?;
            if let Some(images) = metadata.images.as_mut() {
                for image in images {
                    restore_value(&mut image.content, by_path)?;
                    restore_slot(&mut image.thumbnail, by_path)?;
                }
            }
        }
    }
    Ok(())
}

fn restore_slot(
    slot: &mut Option<String>,
    by_path: &BTreeMap<&str, &Blob>,
) -> Result<(), ArchiveError> {
    if let Some(value) = slot.as_mut() {
        restore_value(value, by_path)?;
    }
    Ok(())
}

fn restore_value(value: &mut String, by_path: &BTreeMap<&str, &Blob>) -> Result<(), ArchiveError> {
    let Some(path) = value.strip_prefix(ARCHIVE_REF_PREFIX) else {
        return Ok(());
    };
    let Some(blob) = by_path.get(path) else {
        return Err(ArchiveError::MissingBlob {
            path: path.to_string(),
        });
    };
    *value = format!(
        "data:{};base64,{}",
        blob.mime_type,
        base64::encode(&blob.data)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{CanvasProjectExportItem, EXPORT_APP, EXPORT_VERSION};
    use crate::node::{CanvasNodeData, CanvasNodeImage, Position};

    const PNG_DATA_URL: &str = "data:image/png;base64,iVBORw0KGgo=";

    fn image_node(id: &str, content: &str, storage_key: Option<&str>) -> CanvasNodeData {
        CanvasNodeData {
            id: id.to_string(),
            node_type: CanvasNodeType::Image,
            title: id.to_string(),
            position: Position::new(0.0, 0.0),
            width: 100.0,
            height: 100.0,
            metadata: Some(CanvasNodeMetadata {
                content: Some(content.to_string()),
                storage_key: storage_key.map(str::to_string),
                ..Default::default()
            }),
        }
    }

    fn export_of(project: CanvasProject) -> CanvasExportFile {
        CanvasExportFile {
            app: EXPORT_APP.to_string(),
            version: EXPORT_VERSION,
            exported_at: "2026-01-02T00:00:00Z".to_string(),
            projects: vec![CanvasProjectExportItem {
                project,
                group_name: Some("Folder".to_string()),
                files: Vec::new(),
            }],
        }
    }

    fn project_with(nodes: Vec<CanvasNodeData>) -> CanvasProject {
        let mut project = CanvasProject::new(
            "p1",
            "Canvas",
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z",
        );
        project.nodes = nodes;
        project
    }

    #[test]
    fn collects_image_data_url_and_rewrites_content() {
        let export = export_of(project_with(vec![image_node(
            "image-1",
            PNG_DATA_URL,
            Some("image:abc"),
        )]));
        let collected = collect_export_assets(&export).unwrap();
        assert_eq!(collected.blobs.len(), 1);
        let blob = &collected.blobs[0];
        assert_eq!(blob.path, "projects/p1/files/image_abc.png");
        assert_eq!(blob.mime_type, "image/png");
        assert_eq!(blob.data, b"\x89PNG\r\n\x1a\n");

        let item = &collected.export.projects[0];
        assert_eq!(item.group_name.as_deref(), Some("Folder"));
        assert_eq!(item.files.len(), 1);
        assert_eq!(item.files[0].storage_key, "image:abc");
        assert_eq!(item.files[0].path, blob.path);
        assert_eq!(item.files[0].mime_type, "image/png");
        assert_eq!(item.files[0].bytes, 8);
        assert_eq!(
            item.project.nodes[0]
                .metadata
                .as_ref()
                .unwrap()
                .content
                .as_deref(),
            Some("archive:projects/p1/files/image_abc.png")
        );
    }

    #[test]
    fn collects_image_stack_entries_and_thumbnails() {
        let mut node = image_node("stack-1", "", None);
        node.node_type = CanvasNodeType::ImageStack;
        let metadata = node.metadata.as_mut().unwrap();
        metadata.content = None;
        metadata.thumbnail = Some(PNG_DATA_URL.to_string());
        metadata.thumbnail_key = Some("image:poster".to_string());
        metadata.images = Some(vec![
            CanvasNodeImage {
                id: "a".to_string(),
                content: PNG_DATA_URL.to_string(),
                storage_key: Some("image:a".to_string()),
                ..Default::default()
            },
            CanvasNodeImage {
                id: "b".to_string(),
                content: "data:image/webp;base64,UklGRg==".to_string(),
                storage_key: Some("image:b".to_string()),
                ..Default::default()
            },
        ]);

        let collected = collect_export_assets(&export_of(project_with(vec![node]))).unwrap();
        assert_eq!(collected.blobs.len(), 3);
        assert!(collected
            .blobs
            .iter()
            .any(|blob| blob.path.ends_with("image_b.webp")));
        let metadata = collected.export.projects[0].project.nodes[0]
            .metadata
            .as_ref()
            .unwrap();
        assert!(metadata
            .thumbnail
            .as_deref()
            .unwrap()
            .starts_with(ARCHIVE_REF_PREFIX));
        assert!(metadata.images.as_ref().unwrap()[0]
            .content
            .starts_with(ARCHIVE_REF_PREFIX));
    }

    #[test]
    fn collect_pack_unpack_restore_round_trips_project() {
        let original = export_of(project_with(vec![image_node(
            "image-1",
            PNG_DATA_URL,
            Some("image:abc"),
        )]));
        let collected = collect_export_assets(&original).unwrap();
        let zip = crate::archive::pack_export(&collected.export, &collected.blobs).unwrap();
        let (manifest, blobs) = crate::archive::unpack_export(&zip).unwrap();
        assert_eq!(manifest, collected.export);
        assert_eq!(blobs, collected.blobs);

        let restored = restore_export_assets(&manifest, &blobs).unwrap();
        assert_eq!(restored.projects[0].project, original.projects[0].project);
        assert_eq!(
            restored.projects[0].group_name,
            original.projects[0].group_name
        );
    }

    #[test]
    fn rewriting_is_idempotent_after_restore() {
        let original = export_of(project_with(vec![image_node(
            "image-1",
            PNG_DATA_URL,
            Some("image:abc"),
        )]));
        let collected = collect_export_assets(&original).unwrap();
        let restored = restore_export_assets(&collected.export, &collected.blobs).unwrap();
        let again = collect_export_assets(&restored).unwrap();
        assert_eq!(again.blobs, collected.blobs);
        assert_eq!(again.export, collected.export);
    }

    #[test]
    fn non_media_nodes_and_non_base64_data_urls_are_untouched() {
        let mut text = image_node("text-1", PNG_DATA_URL, None);
        text.node_type = CanvasNodeType::Text;
        let mut plain = image_node("image-2", "data:text/plain,hello", None);
        plain.node_type = CanvasNodeType::Image;
        let export = export_of(project_with(vec![text, plain]));

        let collected = collect_export_assets(&export).unwrap();
        assert!(collected.blobs.is_empty());
        assert_eq!(collected.export, export);
    }

    #[test]
    fn duplicate_storage_keys_get_distinct_paths() {
        let first = image_node("image-1", PNG_DATA_URL, Some("image:abc"));
        let second = image_node(
            "image-2",
            "data:image/png;base64,iVBORw0KGgs=",
            Some("image:abc"),
        );
        let collected =
            collect_export_assets(&export_of(project_with(vec![first, second]))).unwrap();
        assert_eq!(collected.blobs.len(), 2);
        assert_eq!(collected.blobs[0].path, "projects/p1/files/image_abc.png");
        assert_eq!(collected.blobs[1].path, "projects/p1/files/image_abc-2.png");
    }

    #[test]
    fn unsafe_project_and_key_values_are_sanitized() {
        let mut project = project_with(vec![image_node("image-1", PNG_DATA_URL, Some("../a:b*c"))]);
        project.id = "../evil/../p1".to_string();
        let collected = collect_export_assets(&export_of(project)).unwrap();
        let path = &collected.blobs[0].path;
        assert!(member_path_is_safe(path));
        assert!(path.starts_with("projects/"));
        assert!(path
            .split('/')
            .all(|component| component != "." && component != ".."));
        assert!(!path.contains(':'));
    }

    #[test]
    fn restore_rejects_missing_blob_without_partial_output() {
        let export = collect_export_assets(&export_of(project_with(vec![image_node(
            "image-1",
            PNG_DATA_URL,
            Some("image:abc"),
        )])))
        .unwrap()
        .export;
        assert!(matches!(
            restore_export_assets(&export, &[]),
            Err(ArchiveError::MissingBlob { .. })
        ));
    }

    #[test]
    fn restore_rejects_duplicate_blob_paths() {
        let blob = Blob {
            path: "projects/p1/files/a.png".to_string(),
            mime_type: "image/png".to_string(),
            data: vec![1],
        };
        let export = export_of(project_with(Vec::new()));
        assert!(matches!(
            restore_export_assets(&export, &[blob.clone(), blob]),
            Err(ArchiveError::DuplicateBlob { .. })
        ));
    }
}
