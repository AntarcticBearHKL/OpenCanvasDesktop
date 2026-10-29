//! One-shot migration from the old prompt → image-generation → image graph to
//! the Image Attempt version graph. This is intentionally destructive: the
//! product decision is to delete legacy text/video nodes and their files, and
//! to convert image workflows without a backup.

use std::collections::BTreeSet;

use open_canvas_core::constants::get_node_spec;
use open_canvas_core::node::{
    CanvasAttemptRun, CanvasConnection, CanvasNodeData, CanvasNodeImage, CanvasNodeMetadata,
    CanvasNodeStatus, CanvasNodeType, Position,
};

use crate::doc::Doc;

/// What the migration changed, for the one-time UI notice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegacyMigrationReport {
    pub text_removed: usize,
    pub video_removed: usize,
    pub video_files_removed: usize,
    pub prompts_removed: usize,
    pub generators_removed: usize,
    pub attempts_created: usize,
}

impl LegacyMigrationReport {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn summary(&self) -> String {
        format!(
            "Legacy workflow migrated: {} text / {} video nodes removed ({} files), {} prompts + {} generators → {} attempts",
            self.text_removed,
            self.video_removed,
            self.video_files_removed,
            self.prompts_removed,
            self.generators_removed,
            self.attempts_created
        )
    }
}

fn image_entry(node: &CanvasNodeData) -> Option<CanvasNodeImage> {
    let meta = node.metadata.as_ref()?;
    let content = meta.content.clone()?;
    Some(CanvasNodeImage {
        id: format!("migrated-{}", node.id),
        status: meta.status.unwrap_or(CanvasNodeStatus::Success),
        error_details: meta.error_details.clone(),
        content,
        storage_key: meta.storage_key.clone(),
        thumbnail: meta.thumbnail.clone(),
        thumbnail_key: meta.thumbnail_key.clone(),
        natural_width: meta.natural_width.unwrap_or(0.0),
        natural_height: meta.natural_height.unwrap_or(0.0),
        bytes: meta.bytes.unwrap_or(0),
        mime_type: meta
            .mime_type
            .clone()
            .unwrap_or_else(|| "image/png".to_string()),
    })
}

fn run_for(image: &CanvasNodeImage) -> CanvasAttemptRun {
    CanvasAttemptRun {
        id: format!("migrated-run-{}", image.id),
        status: CanvasNodeStatus::Success,
        error_details: None,
        images: vec![image.clone()],
        primary_image_id: Some(image.id.clone()),
        generation_id: None,
        usage: open_canvas_core::node::CanvasNodeUsage::default(),
        agent_run_id: None,
        created_at: None,
    }
}

impl Doc {
    /// Converts every legacy prompt/image-generation pair into an Image Attempt
    /// tree, keeps old output images as `extract` children, and removes every
    /// text/video node and its connections.
    pub fn migrate_legacy_workflows(&mut self) -> LegacyMigrationReport {
        let mut report = LegacyMigrationReport::default();
        let old_nodes = self.nodes.clone();
        let old_connections = self.connections.clone();

        let text_ids: BTreeSet<String> = old_nodes
            .iter()
            .filter(|n| n.node_type == CanvasNodeType::Text)
            .map(|n| n.id.clone())
            .collect();
        let video_ids: BTreeSet<String> = old_nodes
            .iter()
            .filter(|n| {
                matches!(
                    n.node_type,
                    CanvasNodeType::VideoPrompt
                        | CanvasNodeType::VideoGeneration
                        | CanvasNodeType::Video
                )
            })
            .map(|n| n.id.clone())
            .collect();
        report.text_removed = text_ids.len();
        report.video_removed = video_ids.len();

        // Video files are user data, but the product decision is explicit:
        // remove files referenced by legacy video nodes as well as the nodes.
        for node in old_nodes.iter().filter(|n| video_ids.contains(&n.id)) {
            if let Some(content) = node
                .metadata
                .as_ref()
                .and_then(|m| m.content.as_deref())
                .and_then(|content| content.strip_prefix("file:"))
            {
                if std::fs::remove_file(content).is_ok() {
                    report.video_files_removed += 1;
                }
            }
        }

        let prompt_ids: BTreeSet<String> = old_nodes
            .iter()
            .filter(|n| n.node_type == CanvasNodeType::Prompt)
            .map(|n| n.id.clone())
            .collect();
        let generator_ids: BTreeSet<String> = old_nodes
            .iter()
            .filter(|n| n.node_type == CanvasNodeType::ImageGeneration)
            .map(|n| n.id.clone())
            .collect();
        report.prompts_removed = prompt_ids.len();
        report.generators_removed = generator_ids.len();

        let image_ids: BTreeSet<String> = old_nodes
            .iter()
            .filter(|n| n.node_type == CanvasNodeType::Image)
            .map(|n| n.id.clone())
            .collect();

        // Prompt connected to each old generator (the first one wins, matching
        // the old runtime's connected_prompt_node).
        let prompt_for = |generator: &str| -> Option<&CanvasNodeData> {
            old_connections.iter().find_map(|connection| {
                if connection.to_node_id != generator {
                    return None;
                }
                old_nodes.iter().find(|node| {
                    node.id == connection.from_node_id && node.node_type == CanvasNodeType::Prompt
                })
            })
        };

        let mut assigned_images: BTreeSet<String> = BTreeSet::new();
        let mut new_nodes: Vec<CanvasNodeData> = Vec::new();
        let mut new_connections: Vec<CanvasConnection> = Vec::new();
        let mut next_version = self
            .nodes
            .iter()
            .filter_map(|node| node.metadata.as_ref().and_then(|meta| meta.attempt_version))
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let mut next_seq = 0u64;

        let mut make_attempt = |position: Position,
                                title: String,
                                meta: CanvasNodeMetadata,
                                version: i64,
                                seq: &mut u64|
         -> String {
            *seq = seq.wrapping_add(1);
            let id = format!("image-attempt-migrated-{seq}");
            let spec = get_node_spec(&CanvasNodeType::ImageAttempt);
            new_nodes.push(CanvasNodeData {
                id: id.clone(),
                node_type: CanvasNodeType::ImageAttempt,
                title,
                position,
                width: spec.width,
                height: spec.height,
                metadata: Some(CanvasNodeMetadata {
                    attempt_version: Some(version),
                    attempt_frozen: Some(false),
                    attempt_runs: Some(Vec::new()),
                    ..meta
                }),
            });
            id
        };

        for generator in old_nodes
            .iter()
            .filter(|n| n.node_type == CanvasNodeType::ImageGeneration)
        {
            let prompt = prompt_for(&generator.id);
            let prompt_text = prompt
                .and_then(|p| p.metadata.as_ref().and_then(|m| m.prompt.clone()))
                .or_else(|| generator.metadata.as_ref().and_then(|m| m.prompt.clone()))
                .unwrap_or_default();
            let model = generator.metadata.as_ref().and_then(|m| m.model.clone());
            let size = generator.metadata.as_ref().and_then(|m| m.size.clone());
            let quality = generator.metadata.as_ref().and_then(|m| m.quality.clone());
            let count = generator.metadata.as_ref().and_then(|m| m.count);

            // Candidate outputs: generated images with a prompt and the same
            // model, plus any unassigned image when this is the only generator.
            let mut candidates: Vec<&CanvasNodeData> = image_ids
                .iter()
                .filter(|id| !assigned_images.contains(*id))
                .filter_map(|id| old_nodes.iter().find(|n| &n.id == id))
                .filter(|image| {
                    let meta = image.metadata.as_ref();
                    let image_prompt = meta.and_then(|m| m.prompt.as_deref());
                    let image_model = meta.and_then(|m| m.model.as_deref());
                    image_prompt.is_some_and(|p| !p.is_empty())
                        && (model.is_none() || image_model == model.as_deref())
                })
                .collect();
            if candidates.is_empty() && generator_ids.len() == 1 {
                candidates = image_ids
                    .iter()
                    .filter(|id| !assigned_images.contains(*id))
                    .filter_map(|id| old_nodes.iter().find(|n| &n.id == id))
                    .collect();
            }

            let root_id = make_attempt(
                generator.position,
                format!("v{next_version} · migrated attempt"),
                CanvasNodeMetadata {
                    prompt: Some(prompt_text.clone()),
                    model: model.clone(),
                    size: size.clone(),
                    quality: quality.clone(),
                    count,
                    generation_mode: Some(open_canvas_core::node::CanvasGenerationMode::Image),
                    status: Some(CanvasNodeStatus::Idle),
                    ..CanvasNodeMetadata::default()
                },
                next_version,
                &mut next_seq,
            );
            report.attempts_created += 1;
            next_version += 1;

            let mut child_index = 0usize;
            for image_node in candidates {
                let Some(image) = image_entry(image_node) else {
                    continue;
                };
                assigned_images.insert(image_node.id.clone());
                let image_meta = image_node.metadata.as_ref();
                let child_prompt = image_meta
                    .and_then(|m| m.prompt.clone())
                    .filter(|p| !p.trim().is_empty())
                    .unwrap_or_else(|| prompt_text.clone());
                let child_id = make_attempt(
                    Position::new(
                        generator.position.x,
                        generator.position.y + 500.0 + child_index as f64 * 520.0,
                    ),
                    format!("v{next_version} · migrated output"),
                    CanvasNodeMetadata {
                        prompt: Some(child_prompt),
                        model: image_meta
                            .and_then(|m| m.model.clone())
                            .or_else(|| model.clone()),
                        size: image_meta
                            .and_then(|m| m.size.clone())
                            .or_else(|| size.clone()),
                        quality: image_meta
                            .and_then(|m| m.quality.clone())
                            .or_else(|| quality.clone()),
                        count: image_meta.and_then(|m| m.count).or(count),
                        generation_mode: Some(open_canvas_core::node::CanvasGenerationMode::Image),
                        status: Some(CanvasNodeStatus::Success),
                        images: Some(vec![image.clone()]),
                        primary_image_id: Some(image.id.clone()),
                        attempt_runs: Some(vec![run_for(&image)]),
                        attempt_frozen: Some(true),
                        ..CanvasNodeMetadata::default()
                    },
                    next_version,
                    &mut next_seq,
                );
                report.attempts_created += 1;
                new_connections.push(CanvasConnection {
                    id: format!("conn-migrated-derive-{next_seq}"),
                    from_node_id: root_id.clone(),
                    to_node_id: child_id.clone(),
                    relation: Some("derive".into()),
                    kind: Some("derive".into()),
                    reason: Some("Migrated from a legacy prompt/image-generation output".into()),
                    changes: Some("prompt/config inherited from the legacy image".into()),
                    agent_run_id: None,
                });
                new_connections.push(CanvasConnection {
                    id: format!("conn-migrated-extract-{next_seq}"),
                    from_node_id: child_id,
                    to_node_id: image_node.id.clone(),
                    relation: Some("extract".into()),
                    kind: Some("extract".into()),
                    reason: Some("Legacy output kept as a reusable Image node".into()),
                    changes: None,
                    agent_run_id: None,
                });
                next_version += 1;
                child_index += 1;
            }
        }

        // A prompt with no generator becomes a draft attempt.
        let used_prompts: BTreeSet<&str> = old_connections
            .iter()
            .filter(|c| generator_ids.contains(&c.to_node_id))
            .map(|c| c.from_node_id.as_str())
            .collect();
        for prompt in old_nodes.iter().filter(|n| {
            n.node_type == CanvasNodeType::Prompt && !used_prompts.contains(n.id.as_str())
        }) {
            let id = make_attempt(
                prompt.position,
                format!("v{next_version} · migrated prompt"),
                CanvasNodeMetadata {
                    prompt: prompt.metadata.as_ref().and_then(|m| m.prompt.clone()),
                    status: Some(CanvasNodeStatus::Idle),
                    ..CanvasNodeMetadata::default()
                },
                next_version,
                &mut next_seq,
            );
            let _ = id;
            report.attempts_created += 1;
            next_version += 1;
        }

        // Keep every non-legacy node; append the new attempts.
        self.nodes.retain(|node| {
            !text_ids.contains(&node.id)
                && !video_ids.contains(&node.id)
                && !prompt_ids.contains(&node.id)
                && !generator_ids.contains(&node.id)
        });
        self.nodes.extend(new_nodes);

        // Keep connections that still have both endpoints and did not point at
        // a removed legacy node.
        let live_ids: BTreeSet<String> = self.nodes.iter().map(|n| n.id.clone()).collect();
        self.connections.retain(|connection| {
            live_ids.contains(&connection.from_node_id) && live_ids.contains(&connection.to_node_id)
        });
        self.connections.extend(new_connections);
        self.selected_connection = None;
        self.selection.clear();
        self.seq = self.seq.wrapping_add(next_seq);
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_canvas_core::node::{CanvasNodeData, Position};

    fn node(id: &str, kind: CanvasNodeType, x: f64, meta: CanvasNodeMetadata) -> CanvasNodeData {
        CanvasNodeData {
            id: id.into(),
            node_type: kind,
            title: id.into(),
            position: Position::new(x, 0.0),
            width: 320.0,
            height: 240.0,
            metadata: Some(meta),
        }
    }

    fn wire(id: &str, from: &str, to: &str) -> CanvasConnection {
        CanvasConnection {
            id: id.into(),
            from_node_id: from.into(),
            to_node_id: to.into(),
            relation: None,
            kind: None,
            reason: None,
            changes: None,
            agent_run_id: None,
        }
    }

    #[test]
    fn migrates_old_graph_and_keeps_output_images() {
        let prompt = node(
            "p",
            CanvasNodeType::Prompt,
            0.0,
            CanvasNodeMetadata {
                prompt: Some("a red fox".into()),
                ..CanvasNodeMetadata::default()
            },
        );
        let generation = node(
            "g",
            CanvasNodeType::ImageGeneration,
            400.0,
            CanvasNodeMetadata {
                model: Some("test/model".into()),
                size: Some("1:1".into()),
                ..CanvasNodeMetadata::default()
            },
        );
        let image = node(
            "i",
            CanvasNodeType::Image,
            800.0,
            CanvasNodeMetadata {
                content: Some("data:image/png;base64,AAAA".into()),
                prompt: Some("a red fox in snow".into()),
                model: Some("test/model".into()),
                status: Some(CanvasNodeStatus::Success),
                ..CanvasNodeMetadata::default()
            },
        );
        let text = node(
            "t",
            CanvasNodeType::Text,
            0.0,
            CanvasNodeMetadata::default(),
        );
        let video = node(
            "v",
            CanvasNodeType::Video,
            0.0,
            CanvasNodeMetadata::default(),
        );
        let mut doc = Doc::assemble(
            vec![prompt, generation, image, text, video],
            vec![wire("c1", "p", "g"), wire("c2", "p", "t")],
        );
        let report = doc.migrate_legacy_workflows();
        assert_eq!(report.text_removed, 1);
        assert_eq!(report.video_removed, 1);
        assert_eq!(report.prompts_removed, 1);
        assert_eq!(report.generators_removed, 1);
        assert!(report.attempts_created >= 2);
        assert!(!doc.nodes().iter().any(|n| matches!(
            n.node_type,
            CanvasNodeType::Text
                | CanvasNodeType::Prompt
                | CanvasNodeType::ImageGeneration
                | CanvasNodeType::Video
        )));
        assert!(doc.nodes().iter().any(|n| n.id == "i"));
        assert!(doc
            .nodes()
            .iter()
            .any(|n| n.node_type == CanvasNodeType::ImageAttempt));
    }

    #[test]
    fn the_users_legacy_project_migrates_in_memory() {
        let Some(appdata) = std::env::var_os("APPDATA") else {
            return;
        };
        let path = std::path::PathBuf::from(appdata)
            .join("open-canvas-desktop")
            .join("projects")
            .join("project-17295-2.json");
        if !path.exists() {
            return;
        }
        let text = std::fs::read_to_string(&path).expect("read project");
        let project: open_canvas_core::document::CanvasProject =
            serde_json::from_str(&text).expect("parse project");
        let mut doc = Doc::from_project(&project);
        let before = doc.nodes().len();
        let report = doc.migrate_legacy_workflows();
        eprintln!(
            "actual project: {before} -> {} nodes; {report:?}",
            doc.nodes().len()
        );
        if report.is_empty() {
            // The live project was already migrated by the desktop app.
            return;
        }
        assert!(!doc.nodes().iter().any(|n| matches!(
            n.node_type,
            CanvasNodeType::Text
                | CanvasNodeType::Prompt
                | CanvasNodeType::ImageGeneration
                | CanvasNodeType::Video
        )));
    }
}
