//! Node-row content tests: empty vs populated bodies, media decode + info bar,
//! status surfaces and the related highlight flags.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use open_canvas_core::document::LibraryIndex;
use open_canvas_core::node::{
    CanvasConnection, CanvasNodeData, CanvasNodeImage, CanvasNodeMetadata, CanvasNodeStatus,
    CanvasNodeType, Position,
};

use crate::inline_edit::EditState;
use crate::media::MediaCache;
use crate::plugin_host::PluginHost;
use crate::vm::{self, RowContext};
use crate::NodeItem;
use slint::Model as _;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn red_png_data_url() -> Result<String, Box<dyn std::error::Error>> {
    let surface = {
        let mut surface = open_canvas_ps::Surface::new(4, 4);
        for x in 0..4 {
            for y in 0..4 {
                surface.set(x, y, [255, 0, 0, 255]);
            }
        }
        surface
    };
    let png = open_canvas_ps::encode_png(&surface)?;
    Ok(format!("data:image/png;base64,{}", BASE64.encode(&png)))
}

fn node(kind: CanvasNodeType, id: &str, metadata: CanvasNodeMetadata) -> CanvasNodeData {
    CanvasNodeData {
        id: id.into(),
        node_type: kind,
        title: format!("{id} title"),
        position: Position::new(40.0, 40.0),
        width: 340.0,
        height: 240.0,
        metadata: Some(metadata),
    }
}

/// Builds one row the way `App::rows_for_nodes` does.
fn build_row(
    node: &CanvasNodeData,
    media: &mut MediaCache,
    edit: &EditState,
    focus: Option<&str>,
    connections: &[CanvasConnection],
) -> NodeItem {
    let nodes = std::slice::from_ref(node);
    let library = LibraryIndex::new();
    let related = crate::canvas_content::related_ids(focus, connections);
    let mut plugin = PluginHost::new(std::env::temp_dir());
    let panel = crate::node_ui::Panel::default();
    let mut folders = crate::asset_folder::FolderCache::default();
    let expanded = std::collections::HashMap::new();
    let reference_slots = std::collections::HashMap::new();
    let smart_previews = std::collections::HashMap::new();
    let index = vm::NodeIndex::new(nodes);
    let mut context = RowContext {
        nodes,
        connections,
        index: &index,
        library: &library,
        media,
        edit,
        related: &related,
        reference_slots: &reference_slots,
        drop_target: None,
        drop_slot: None,
        panel: &panel,
        folders: &mut folders,
        asset_expanded: &expanded,
        smart_previews: &smart_previews,
    };
    vm::node_row(node, false, 1.0, &mut plugin, &mut context)
}

#[test]
fn empty_image_row_shows_the_empty_state() {
    let node = node(
        CanvasNodeType::Image,
        "image",
        CanvasNodeMetadata::default(),
    );
    let mut media = MediaCache::default();
    let edit = EditState::default();
    let row = build_row(&node, &mut media, &edit, None, &[]);
    assert!(row.content_empty, "no content decodes to no bitmap");
    assert!(!row.media_info, "the empty card keeps its title");
    assert_eq!(row.info, "");
    assert_eq!(row.status, "");
}

/// An image card renders its output dot but no input dot: it can feed a prompt
/// reference or a consumer, and is never wired into. An Image Stack renders
/// neither, while every other kind keeps both.
#[test]
fn image_rows_render_a_source_handle_but_no_target_handle() {
    let mut media = MediaCache::default();
    let edit = EditState::default();
    let image = node(
        CanvasNodeType::Image,
        "image",
        CanvasNodeMetadata::default(),
    );
    let row = build_row(&image, &mut media, &edit, None, &[]);
    assert!(row.has_source, "an image sources a wire");
    assert!(!row.has_target, "an image cannot receive a wire");

    let stack = node(
        CanvasNodeType::ImageStack,
        "stack",
        CanvasNodeMetadata::default(),
    );
    let row = build_row(&stack, &mut media, &edit, None, &[]);
    assert!(!row.has_source);
    assert!(!row.has_target);

    let text = node(CanvasNodeType::Text, "text", CanvasNodeMetadata::default());
    let row = build_row(&text, &mut media, &edit, None, &[]);
    assert!(row.has_source, "other kinds keep their source handle");
    assert!(row.has_target, "other kinds keep their target handle");
}

#[test]
fn populated_image_row_decodes_and_fills_the_info_bar() -> TestResult {
    let content = red_png_data_url()?;
    let node = node(
        CanvasNodeType::Image,
        "image",
        CanvasNodeMetadata {
            content: Some(content),
            natural_width: Some(800.0),
            natural_height: Some(600.0),
            bytes: Some(2048),
            ..Default::default()
        },
    );
    let mut media = MediaCache::default();
    let edit = EditState::default();
    let row = build_row(&node, &mut media, &edit, None, &[]);
    assert!(!row.content_empty, "the decoded bitmap is present");
    assert_eq!(row.content_image.size().width, 4);
    assert!(
        row.media_info,
        "image content swaps the title for the info bar"
    );
    assert_eq!(row.info, "image title · 800 x 600 · 2.0 KB");
    assert_eq!(row.content_fit, "contain");
    assert_eq!(media.decodes(), 1);

    // A second row build for the same node reuses the cache.
    let again = build_row(&node, &mut media, &edit, None, &[]);
    assert!(!again.content_empty);
    assert_eq!(media.decodes(), 1, "one decode per distinct bitmap");
    Ok(())
}

#[test]
fn image_nodes_ignore_legacy_free_resize_and_keep_contain() -> TestResult {
    let node = node(
        CanvasNodeType::Image,
        "image",
        CanvasNodeMetadata {
            content: Some(red_png_data_url()?),
            free_resize: Some(true),
            ..Default::default()
        },
    );
    let mut media = MediaCache::default();
    let edit = EditState::default();
    let row = build_row(&node, &mut media, &edit, None, &[]);
    assert_eq!(
        row.content_fit, "contain",
        "Image nodes never stretch the bitmap"
    );
    Ok(())
}

#[test]
fn text_row_switches_between_content_and_the_edit_hint() {
    let mut media = MediaCache::default();
    let edit = EditState::default();
    let populated = node(
        CanvasNodeType::Text,
        "text",
        CanvasNodeMetadata {
            content: Some("hello world".into()),
            ..Default::default()
        },
    );
    let row = build_row(&populated, &mut media, &edit, None, &[]);
    assert_eq!(row.body, "hello world");
    assert_eq!(row.batch_count, 0);

    let empty = node(CanvasNodeType::Text, "text", CanvasNodeMetadata::default());
    let row = build_row(&empty, &mut media, &edit, None, &[]);
    assert_eq!(
        row.body, "",
        "an empty text node renders the edit hint instead"
    );
}

#[test]
fn text_row_carries_the_batch_count_label() -> TestResult {
    use open_canvas_core::node::CanvasNodeText;
    let text = |id: &str, content: &str| CanvasNodeText {
        id: id.into(),
        status: CanvasNodeStatus::Success,
        error_details: None,
        content: content.into(),
    };
    let node = node(
        CanvasNodeType::Text,
        "text",
        CanvasNodeMetadata {
            texts: Some(vec![text("a", "first"), text("b", "second")]),
            primary_text_id: Some("b".into()),
            ..Default::default()
        },
    );
    let mut media = MediaCache::default();
    let edit = EditState::default();
    let row = build_row(&node, &mut media, &edit, None, &[]);
    assert_eq!(row.body, "second", "the primary alternative leads");
    assert_eq!(row.batch_count, 2);
    assert_eq!(row.batch_label, "2 options");
    assert_eq!(row.batch.row_count(), 2, "every alternative is rendered");
    let first = row.batch.row_data(0).ok_or("option 0")?;
    assert_eq!(first.id.as_str(), "a");
    assert!(!first.primary, "the secondary alternative is expandable");
    let second = row.batch.row_data(1).ok_or("option 1")?;
    assert!(second.primary, "the primary is marked");
    assert!(first.can_take_out, "two alternatives can lose one");
    Ok(())
}

#[test]
fn status_surfaces_follow_the_metadata() {
    let mut media = MediaCache::default();
    let edit = EditState::default();

    let loading = node(
        CanvasNodeType::ImageGeneration,
        "gen",
        CanvasNodeMetadata {
            status: Some(CanvasNodeStatus::Loading),
            ..Default::default()
        },
    );
    let row = build_row(&loading, &mut media, &edit, None, &[]);
    assert_eq!(row.status, "loading", "the spinner owns the body");
    assert_eq!(row.status_label, "Generating");

    let failed = node(
        CanvasNodeType::ImageGeneration,
        "gen",
        CanvasNodeMetadata {
            status: Some(CanvasNodeStatus::Error),
            error_details: Some("The API returned no image".into()),
            ..Default::default()
        },
    );
    let row = build_row(&failed, &mut media, &edit, None, &[]);
    assert_eq!(row.status, "error");
    assert_eq!(row.error_text, "The API returned no image");
    assert_eq!(row.status_label, "Failed");

    let done = node(
        CanvasNodeType::ImageGeneration,
        "gen",
        CanvasNodeMetadata {
            status: Some(CanvasNodeStatus::Success),
            ..Default::default()
        },
    );
    let row = build_row(&done, &mut media, &edit, None, &[]);
    assert_eq!(row.status, "", "success shows the content");
    assert_eq!(row.status_label, "Ready");
}

#[test]
fn video_row_carries_the_poster_state_and_duration() -> TestResult {
    let poster = node(
        CanvasNodeType::Video,
        "video",
        CanvasNodeMetadata {
            content: Some("blob:https://example.test/video".into()),
            thumbnail: Some(red_png_data_url()?),
            duration_ms: Some(6000.0),
            status: Some(CanvasNodeStatus::Success),
            ..Default::default()
        },
    );
    let mut media = MediaCache::default();
    let edit = EditState::default();
    let row = build_row(&poster, &mut media, &edit, None, &[]);
    assert!(
        !row.content_empty,
        "the poster decodes from metadata.thumbnail"
    );
    assert_eq!(row.duration, "0:06");
    assert_eq!(row.status_label, "Ready");
    assert!(!row.media_info, "video nodes keep their title");

    let bare = node(
        CanvasNodeType::Video,
        "video",
        CanvasNodeMetadata {
            duration_ms: Some(92_000.0),
            ..Default::default()
        },
    );
    let row = build_row(&bare, &mut media, &edit, None, &[]);
    assert!(
        row.content_empty,
        "no poster decodes to the video placeholder"
    );
    assert_eq!(row.duration, "1:32");
    Ok(())
}

#[test]
fn stack_row_builds_the_contact_sheet() -> TestResult {
    let image = |id: &str, content: String| CanvasNodeImage {
        id: id.into(),
        status: CanvasNodeStatus::Success,
        error_details: None,
        content,
        storage_key: None,
        thumbnail: None,
        thumbnail_key: None,
        natural_width: 4.0,
        natural_height: 4.0,
        bytes: 0,
        mime_type: "image/png".into(),
    };
    let node = node(
        CanvasNodeType::ImageStack,
        "stack",
        CanvasNodeMetadata {
            images: Some(vec![
                image("a", red_png_data_url()?),
                image("b", red_png_data_url()?),
            ]),
            primary_image_id: Some("b".into()),
            ..Default::default()
        },
    );
    let mut media = MediaCache::default();
    let edit = EditState::default();
    let row = build_row(&node, &mut media, &edit, None, &[]);
    assert_eq!(row.stack_count, 2);
    assert_eq!(row.stack_count_label, "2 images");
    assert_eq!(row.stack_cover.size().width, 4, "the cover bitmap decodes");
    assert_eq!(
        slint::Model::row_count(&row.stack),
        1,
        "one backing card behind the cover"
    );
    assert_eq!(row.stack_cover_error, "");
    assert!(
        row.content_empty,
        "the stack body is the sheet, not a single image"
    );
    Ok(())
}

#[test]
fn related_follow_the_focus() {
    let connections = vec![CanvasConnection {
        id: "c1".into(),
        from_node_id: "prompt".into(),
        to_node_id: "image".into(),
        relation: None,
        kind: None,
        reason: None,
        changes: None,
        agent_run_id: None,
    }];
    let prompt = node(
        CanvasNodeType::Prompt,
        "prompt",
        CanvasNodeMetadata::default(),
    );
    let image = node(
        CanvasNodeType::Image,
        "image",
        CanvasNodeMetadata::default(),
    );
    let mut media = MediaCache::default();
    let edit = EditState::default();

    let focus = Some("image");
    let focused = build_row(&image, &mut media, &edit, focus, &connections);
    assert!(focused.related);

    let neighbour = build_row(&prompt, &mut media, &edit, focus, &connections);
    assert!(
        neighbour.related,
        "a direct neighbour joins the related set"
    );

    let stranger = build_row(
        &node(CanvasNodeType::Text, "text", CanvasNodeMetadata::default()),
        &mut media,
        &edit,
        focus,
        &connections,
    );
    assert!(
        !stranger.related,
        "an unrelated card is not part of the related set"
    );

    let unfocused = build_row(&image, &mut media, &edit, None, &connections);
    assert!(!unfocused.related);
}

/// The video-prompt slot set follows the mode tab, and the reference count is
/// the editable N clamped to the provider's limit.
#[test]
fn video_prompt_slots_follow_the_mode_tab() {
    use crate::vm::{
        clamp_reference_slots, video_slot_labels, REFERENCE_SLOT_DEFAULT, REFERENCE_SLOT_MAX,
        REFERENCE_SLOT_MIN,
    };

    // Frames: the two keyframe slots only - never a reference slot.
    assert_eq!(
        video_slot_labels("frames", 3),
        vec!["First frame", "Last frame"]
    );
    assert_eq!(
        video_slot_labels("frames", 9),
        vec!["First frame", "Last frame"]
    );

    // Reference: exactly N reference slots, N whatever the stepper holds.
    assert_eq!(
        video_slot_labels("reference", REFERENCE_SLOT_DEFAULT),
        vec!["Reference"; REFERENCE_SLOT_DEFAULT]
    );
    assert_eq!(video_slot_labels("reference", 1).len(), 1);
    assert_eq!(video_slot_labels("reference", REFERENCE_SLOT_MAX).len(), 9);
    // The stepper cannot leave its range.
    assert_eq!(clamp_reference_slots(0), REFERENCE_SLOT_MIN);
    assert_eq!(clamp_reference_slots(-4), REFERENCE_SLOT_MIN);
    assert_eq!(clamp_reference_slots(12), REFERENCE_SLOT_MAX);
    assert_eq!(clamp_reference_slots(5), 5);

    // The row carries the same count and one title per slot.
    let metadata = CanvasNodeMetadata {
        video_mode: Some(open_canvas_core::node::CanvasVideoMode::Reference),
        ..CanvasNodeMetadata::default()
    };
    let prompt = node(CanvasNodeType::VideoPrompt, "prompt", metadata);
    let mut media = MediaCache::default();
    let edit = EditState::default();
    let row = build_row(&prompt, &mut media, &edit, None, &[]);
    assert_eq!(
        row.ref_slot_count,
        i32::try_from(REFERENCE_SLOT_DEFAULT).unwrap_or(0)
    );
    assert_eq!(
        slint::Model::row_count(&row.ref_slot_titles),
        REFERENCE_SLOT_DEFAULT
    );
    assert_eq!(row.video_mode, "reference");
}

/// A status surface owns the card: loading/error rows carry no decoded bitmap
/// (even if stale content is still in metadata), success rows do.
#[test]
fn status_rows_clear_the_media_bitmap() -> TestResult {
    let content = red_png_data_url()?;
    let mut media = MediaCache::default();
    let edit = EditState::default();
    let loading = node(
        CanvasNodeType::Image,
        "loading",
        CanvasNodeMetadata {
            content: Some(content.clone()),
            status: Some(CanvasNodeStatus::Loading),
            ..CanvasNodeMetadata::default()
        },
    );
    let row = build_row(&loading, &mut media, &edit, None, &[]);
    assert_eq!(row.status, "loading");
    assert_eq!(row.content_image.size().width, 0);
    assert!(row.content_empty);

    let error = node(
        CanvasNodeType::Image,
        "error",
        CanvasNodeMetadata {
            content: Some(content.clone()),
            status: Some(CanvasNodeStatus::Error),
            ..CanvasNodeMetadata::default()
        },
    );
    let row = build_row(&error, &mut media, &edit, None, &[]);
    assert_eq!(row.status, "error");
    assert_eq!(row.content_image.size().width, 0);

    let success = node(
        CanvasNodeType::Image,
        "success",
        CanvasNodeMetadata {
            content: Some(content),
            status: Some(CanvasNodeStatus::Success),
            ..CanvasNodeMetadata::default()
        },
    );
    let row = build_row(&success, &mut media, &edit, None, &[]);
    assert_eq!(row.status, "");
    assert!(row.content_image.size().width > 0);
    assert!(!row.content_empty);
    Ok(())
}

/// A `metadata.mentions` list from an older document is accepted and then
/// ignored by the prompt renderer: the resting body is always the plain prompt
/// text, never a reconstructed token/chip sequence.
#[test]
fn legacy_mentions_do_not_change_the_prompt_body() {
    let legacy: CanvasNodeMetadata = serde_json::from_str(
        r#"{
            "prompt": "a Image 1 in snow",
            "mentions": [{ "start": 2, "end": 9, "nodeId": "img" }]
        }"#,
    )
    .expect("old metadata still loads");
    let prompt = node(CanvasNodeType::Prompt, "prompt", legacy);
    let mut media = MediaCache::default();
    let edit = EditState::default();
    let row = build_row(&prompt, &mut media, &edit, None, &[]);
    assert_eq!(row.body.as_str(), "a Image 1 in snow");
    assert_eq!(row.ref_images.iter().count(), 0);
}

#[test]
fn attempt_row_carries_prompt_image_and_version() -> TestResult {
    let content = red_png_data_url()?;
    let image = CanvasNodeImage {
        id: "image-0".into(),
        content: content.clone(),
        natural_width: 4.0,
        natural_height: 4.0,
        mime_type: "image/png".into(),
        ..CanvasNodeImage::default()
    };
    let attempt = node(
        CanvasNodeType::ImageAttempt,
        "attempt",
        CanvasNodeMetadata {
            prompt: Some("a red fox".into()),
            status: Some(CanvasNodeStatus::Success),
            attempt_version: Some(7),
            attempt_title: Some("fox study".into()),
            attempt_frozen: Some(true),
            images: Some(vec![image.clone()]),
            primary_image_id: Some(image.id.clone()),
            attempt_runs: Some(vec![open_canvas_core::node::CanvasAttemptRun {
                id: "run-0".into(),
                status: CanvasNodeStatus::Success,
                images: vec![image],
                primary_image_id: Some("image-0".into()),
                ..Default::default()
            }]),
            ..CanvasNodeMetadata::default()
        },
    );
    let mut media = MediaCache::default();
    let edit = EditState::default();
    let row = build_row(&attempt, &mut media, &edit, None, &[]);
    assert_eq!(row.body.as_str(), "a red fox");
    assert_eq!(row.attempt_version, 7);
    assert_eq!(row.attempt_title.as_str(), "fox study");
    assert!(row.attempt_frozen);
    assert_eq!(row.attempt_run_count, 1);
    assert_eq!(row.attempt_image_count, 1);
    assert!(row.content_image.size().width > 0);
    Ok(())
}
