//! Headless tests for the dock engine: the resize clamps, the split ratio, the
//! move-to-edge / tab-stacking model, the empty shipped default, the panel
//! close path, the dock menu, the persistence round-trip and the tooltip state
//! machine (project rule: code-only verification).

use std::error::Error;

use crate::image::dock_engine::{
    clamp_size, clamp_split, default_layout, drop_target, label, load, panels_for,
    parse_min_animate, save, studio, DockEdge, Studio, TooltipState, DOCK_MAX_BOTTOM,
    DOCK_MAX_SIDE, DOCK_MIN_BOTTOM, DOCK_MIN_SIDE, DOCK_SPLIT_MAX, DOCK_SPLIT_MIN, STUDIO_IMAGE,
};
use crate::image::ImagePanels;

type TestResult = Result<(), Box<dyn Error>>;

fn image() -> Result<&'static Studio, Box<dyn Error>> {
    studio(STUDIO_IMAGE).ok_or_else(|| "the image studio is registered".into())
}

#[test]
fn resize_clamps_each_edge_to_the_web_bounds() {
    // Sides: 168..640 (`DOCK_MIN_SIZE` / `DOCK_MAX_SIZE`).
    assert_eq!(clamp_size(DockEdge::Left, 10.0, 270.0), DOCK_MIN_SIDE);
    assert_eq!(clamp_size(DockEdge::Left, 9000.0, 270.0), DOCK_MAX_SIDE);
    assert_eq!(clamp_size(DockEdge::Right, 10.0, 288.0), DOCK_MIN_SIDE);
    assert_eq!(clamp_size(DockEdge::Right, 9000.0, 288.0), DOCK_MAX_SIDE);
    // Bottom: 120..420 (`dockClampSize` bottom branch).
    assert_eq!(clamp_size(DockEdge::Bottom, 1.0, 178.0), DOCK_MIN_BOTTOM);
    assert_eq!(clamp_size(DockEdge::Bottom, 9000.0, 178.0), DOCK_MAX_BOTTOM);
    // Non-finite input keeps the studio default instead of a bound.
    assert_eq!(clamp_size(DockEdge::Right, f64::NAN, 288.0), 288.0);
    // In-range sizes round and pass through.
    assert_eq!(clamp_size(DockEdge::Right, 311.4, 288.0), 311.0);
}

#[test]
fn set_size_stores_the_clamped_value_and_reports_changes() {
    let mut layout = default_layout(STUDIO_IMAGE);
    assert!(layout.set_size(DockEdge::Right, 400.0));
    assert_eq!(layout.size(DockEdge::Right), 400.0);
    // The same value again is not a change (no persistence churn).
    assert!(!layout.set_size(DockEdge::Right, 400.0));
    // Out-of-range values land on the bound, and report the change.
    assert!(layout.set_size(DockEdge::Right, 9000.0));
    assert_eq!(layout.size(DockEdge::Right), DOCK_MAX_SIDE);
    assert!(layout.set_size(DockEdge::Bottom, 0.0));
    assert_eq!(layout.size(DockEdge::Bottom), DOCK_MIN_BOTTOM);
}

#[test]
fn split_ratio_clamps_between_point_two_and_point_eight() {
    assert_eq!(clamp_split(0.0), DOCK_SPLIT_MIN);
    assert_eq!(clamp_split(1.0), DOCK_SPLIT_MAX);
    assert_eq!(clamp_split(f64::INFINITY), 0.5);
    let mut layout = default_layout(STUDIO_IMAGE);
    assert!(layout.set_split(DockEdge::Left, 0.65));
    assert_eq!(layout.split(DockEdge::Left), 0.65);
    // The right side keeps its own ratio.
    assert_eq!(layout.split(DockEdge::Right), 0.5);
    assert!(layout.set_split(DockEdge::Right, 5.0));
    assert_eq!(layout.split(DockEdge::Right), DOCK_SPLIT_MAX);
    // The bottom edge has no split of its own; it edits the left side's slot.
    assert!(!layout.set_split(DockEdge::Bottom, 0.65));
    assert_eq!(layout.split(DockEdge::Bottom), 0.65);
}

#[test]
fn dropping_a_panel_on_an_empty_edge_restores_that_edges_size() -> TestResult {
    crate::config_test_support::init_backend()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    let studio = image()?;
    crate::app::with_app(&app, |app| {
        app.dock_request(&ui, STUDIO_IMAGE, "drag-start", "layers", "", 300.0, 100.0);
        // Drop in the left quarter: the left edge was empty in the shipped
        // default, so the move must also restore its natural width.
        app.dock_request(&ui, STUDIO_IMAGE, "drag-end", "", "", 10.0, 100.0);
        assert_eq!(app.image.dock.locate("layers"), Some((DockEdge::Left, 0)));
        assert_eq!(
            app.image.dock.size(DockEdge::Left),
            studio.sizes.left,
            "an empty edge gets its natural size back"
        );
    });
    Ok(())
}

#[test]
fn move_panel_carries_it_to_another_edge() -> TestResult {
    let studio = image()?;
    let mut layout = default_layout(STUDIO_IMAGE);
    for id in ["color", "gradients", "patterns", "brushes"] {
        layout.reveal(studio, id);
    }
    assert_eq!(layout.locate("color"), Some((DockEdge::Bottom, 0)));
    layout.move_panel("color", DockEdge::Right, 0);
    assert_eq!(layout.locate("color"), Some((DockEdge::Right, 0)));
    // The vacated bottom group collapses, and the other bottom panels shift up.
    assert!(!layout.bottom.iter().any(|group| group.panels.is_empty()));
    assert_eq!(layout.bottom.len(), 3);
    assert_eq!(layout.locate("gradients"), Some((DockEdge::Bottom, 0)));
    // The moved panel becomes the active tab of its new group.
    assert_eq!(layout.right[0].active, "color");
    Ok(())
}

#[test]
fn stacking_two_panels_shares_one_tab_group() -> TestResult {
    let studio = image()?;
    let mut layout = default_layout(STUDIO_IMAGE);
    layout.reveal(studio, "layers");
    layout.reveal(studio, "history");
    // Each reveal starts as its own group on the home edge.
    assert_eq!(layout.right.len(), 2);
    assert!(layout.right.iter().all(|group| group.panels.len() == 1));
    // Dropping "history" onto the first right group makes it a tab.
    layout.move_panel("history", DockEdge::Right, 0);
    assert_eq!(layout.right[0].panels, vec!["layers", "history"]);
    assert_eq!(layout.right[0].active, "history");
    assert_eq!(layout.right.len(), 1);
    assert!(layout.is_visible("history"));
    // Activating a sibling tab switches the group without moving panels.
    assert!(layout.set_active(DockEdge::Right, 0, "layers"));
    assert_eq!(layout.right[0].active, "layers");
    assert!(!layout.set_active(DockEdge::Right, 0, "color"));
    assert!(!layout.set_active(DockEdge::Right, 9, "layers"));
    Ok(())
}

#[test]
fn toggle_hides_and_reveals_a_panel_on_its_home_edge() -> TestResult {
    let studio = image()?;
    let mut layout = default_layout(STUDIO_IMAGE);
    // The shipped layout starts empty, so the first toggle reveals.
    layout.toggle(studio, "layers");
    assert!(layout.is_visible("layers"));
    assert_eq!(layout.locate("layers"), Some((DockEdge::Right, 0)));
    layout.toggle(studio, "layers");
    assert!(!layout.is_visible("layers"));
    assert!(layout.right.is_empty(), "the emptied edge collapses");
    layout.toggle(studio, "layers");
    assert!(layout.is_visible("layers"));
    // A revealed panel comes back as its own group at the end of its edge.
    layout.toggle(studio, "history");
    assert_eq!(
        layout.right.last().map(|group| group.active.clone()),
        Some("history".to_string())
    );
    assert_eq!(layout.right.len(), 2);
    Ok(())
}

#[test]
fn the_shipped_default_has_three_empty_edges_and_no_visible_panel() -> TestResult {
    let studio = image()?;
    let layout = default_layout(STUDIO_IMAGE);
    // The studio opens as tool rail + board only.
    assert!(layout.left.is_empty(), "the left edge ships empty");
    assert!(layout.right.is_empty(), "the right edge ships empty");
    assert!(layout.bottom.is_empty(), "the bottom edge ships empty");
    // Every panel is available (defined) but none is laid out.
    assert_eq!(studio.defs.len(), 13, "all thirteen panels stay defined");
    for def in studio.defs {
        assert!(!layout.is_visible(def.id), "`{}` ships hidden", def.id);
    }
    // The derived `panels` flags are all false, so every Dock menu row starts
    // unchecked.
    let flags = panels_for(&layout);
    assert_eq!(flags, ImagePanels::default());
    assert!(!flags.layers);
    assert!(!flags.fx);
    assert!(!flags.history);
    assert!(!flags.properties);
    assert!(!flags.color);
    assert!(!flags.channels);
    assert!(!flags.paths);
    assert!(!flags.actions);
    assert!(!flags.assets);
    assert!(!flags.adjustments);
    assert!(!flags.gradients);
    assert!(!flags.patterns);
    assert!(!flags.brushes);
    Ok(())
}

#[test]
fn revealing_a_panel_uses_its_home_edge_at_the_natural_size() -> TestResult {
    let studio = image()?;
    let mut layout = default_layout(STUDIO_IMAGE);
    // A stale size from a previous layout must not decide the first reveal.
    assert!(layout.set_size(DockEdge::Bottom, 400.0));
    assert_eq!(layout.size(DockEdge::Bottom), 400.0);

    layout.reveal(studio, "color");
    assert_eq!(layout.locate("color"), Some((DockEdge::Bottom, 0)));
    assert_eq!(
        layout.size(DockEdge::Bottom),
        studio.sizes.bottom,
        "the first reveal restores the edge's natural size"
    );

    let before = layout.size(DockEdge::Left);
    layout.reveal(studio, "properties");
    assert_eq!(layout.locate("properties"), Some((DockEdge::Left, 0)));
    assert_eq!(layout.size(DockEdge::Left), before);

    layout.reveal(studio, "layers");
    assert_eq!(layout.locate("layers"), Some((DockEdge::Right, 0)));
    assert_eq!(layout.size(DockEdge::Right), studio.sizes.right);
    assert!(panels_for(&layout).layers);
    assert!(panels_for(&layout).color);
    assert!(panels_for(&layout).properties);
    Ok(())
}

#[test]
fn closing_a_panel_removes_it_and_the_dock_menu_still_offers_it() -> TestResult {
    let studio = image()?;
    let mut layout = default_layout(STUDIO_IMAGE);
    layout.reveal(studio, "layers");
    layout.reveal(studio, "history");
    layout.move_panel("history", DockEdge::Right, 0);
    assert!(layout.is_visible("history"));

    // Close the active tab: only that panel leaves the group.
    assert!(layout.close("history"));
    assert!(!layout.is_visible("history"));
    assert_eq!(layout.locate("layers"), Some((DockEdge::Right, 0)));
    assert_eq!(layout.right[0].panels, vec!["layers"]);
    assert_eq!(layout.right[0].active, "layers");

    // Closing the last panel of the edge drops the group and the column.
    assert!(layout.close("layers"));
    assert_eq!(layout.right.len(), 0, "the emptied edge is gone");
    assert!(!layout.close("layers"), "closing twice is a no-op");

    // "history" is still a defined panel, so the Dock menu can bring it back.
    assert!(studio.defs.iter().any(|def| def.id == "history"));
    layout.toggle(studio, "history");
    assert!(layout.is_visible("history"));
    assert_eq!(layout.locate("history"), Some((DockEdge::Right, 0)));
    Ok(())
}

#[test]
fn default_layout_matches_the_studio_and_its_labels() -> TestResult {
    let image_layout = default_layout(STUDIO_IMAGE);
    assert_eq!(image_layout.left.len(), 0);
    assert_eq!(image_layout.right.len(), 0);
    assert_eq!(image_layout.bottom.len(), 0);
    // The natural sizes still describe the first reveal of every edge.
    assert_eq!(image_layout.sizes.left, 270.0);
    assert_eq!(image_layout.sizes.right, 288.0);
    assert_eq!(image_layout.sizes.bottom, 178.0);

    assert_eq!(label(image()?, "fx"), "Layer Effects");
    assert_eq!(label(image()?, "nope"), "nope");
    Ok(())
}

#[test]
fn drop_target_maps_the_studio_quarters() {
    // 1000x800: the bottom quarter is the bottom edge.
    assert_eq!(
        drop_target(500.0, 700.0, 1000.0, 800.0),
        Some((DockEdge::Bottom, 0))
    );
    // The outer quarters of the two halves are the two group slots of a side.
    assert_eq!(
        drop_target(100.0, 100.0, 1000.0, 800.0),
        Some((DockEdge::Left, 0))
    );
    assert_eq!(
        drop_target(100.0, 600.0, 1000.0, 800.0),
        Some((DockEdge::Left, 1))
    );
    assert_eq!(
        drop_target(950.0, 100.0, 1000.0, 800.0),
        Some((DockEdge::Right, 0))
    );
    assert_eq!(
        drop_target(950.0, 600.0, 1000.0, 800.0),
        Some((DockEdge::Right, 1))
    );
    // The middle is not a drop zone.
    assert_eq!(drop_target(500.0, 300.0, 1000.0, 800.0), None);
    // A degenerate area has no zones.
    assert_eq!(drop_target(0.0, 0.0, 0.0, 0.0), None);
}

#[test]
fn sanitize_drops_unknown_and_duplicate_panels() -> TestResult {
    let studio = image()?;
    let mut layout = default_layout(STUDIO_IMAGE);
    layout.reveal(studio, "properties");
    layout.reveal(studio, "layers");
    layout.left[0].panels = vec![
        "properties".to_string(),
        "not-a-panel".to_string(),
        "layers".to_string(),
        "layers".to_string(),
    ];
    layout.left[0].active = "not-a-panel".to_string();
    layout.right[0].panels.clear();
    layout.sizes.left = 10_000.0;
    layout.split.right = -1.0;
    let clean = layout.sanitize(studio);
    // Unknown ids and duplicates are gone, "layers" is now a left tab.
    assert_eq!(clean.left[0].panels, vec!["properties", "layers"]);
    assert_eq!(clean.left[0].active, "properties");
    assert!(clean.right.iter().all(|group| !group.panels.is_empty()));
    assert_eq!(clean.sizes.left, DOCK_MAX_SIDE);
    assert_eq!(clean.split.right, DOCK_SPLIT_MIN);
    Ok(())
}

#[test]
fn layout_round_trips_through_the_store() -> TestResult {
    let studio = image()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let mut layout = default_layout(STUDIO_IMAGE);
    layout.reveal(studio, "layers");
    layout.reveal(studio, "history");
    layout.move_panel("history", DockEdge::Right, 0);
    layout.set_size(DockEdge::Left, 331.0);
    layout.set_split(DockEdge::Right, 0.35);
    save(&store, STUDIO_IMAGE, &layout);

    let loaded = load(&store, STUDIO_IMAGE);
    assert_eq!(loaded, layout, "the saved layout loads back unchanged");
    Ok(())
}

#[test]
fn a_missing_or_foreign_dock_file_falls_back_to_the_default() -> TestResult {
    let (_dir, store) = crate::config_test_support::temp_store()?;
    assert_eq!(load(&store, STUDIO_IMAGE), default_layout(STUDIO_IMAGE));

    // A file from another app / version is ignored.
    std::fs::write(
        store.dock_path(),
        r#"{"app":"something-else","version":9,"layouts":{}}"#,
    )?;
    assert_eq!(load(&store, STUDIO_IMAGE), default_layout(STUDIO_IMAGE));

    // Malformed JSON is ignored too.
    std::fs::write(store.dock_path(), "{not json")?;
    assert_eq!(load(&store, STUDIO_IMAGE), default_layout(STUDIO_IMAGE));

    // A layout with an unknown panel id sanitizes instead of failing.
    std::fs::write(
        store.dock_path(),
        r#"{"app":"open-canvas-desktop-dock","version":1,"layouts":{"image":{"left":[{"panels":["ghost"],"active":"ghost"}],"right":[],"bottom":[],"sizes":{"left":1,"right":1,"bottom":1},"split":{"left":0,"right":2}}}}"#,
    )?;
    let loaded = load(&store, STUDIO_IMAGE);
    assert!(loaded.left.is_empty(), "the unknown panel is dropped");
    assert_eq!(loaded.sizes.left, DOCK_MIN_SIDE);
    assert_eq!(loaded.split.right, DOCK_SPLIT_MAX);
    Ok(())
}

#[test]
fn a_persisted_layout_keeps_working() -> TestResult {
    let studio = image()?;
    let (_dir, store) = crate::config_test_support::temp_store()?;
    let mut layout = default_layout(STUDIO_IMAGE);
    layout.reveal(studio, "layers");
    layout.set_size(DockEdge::Right, 360.0);
    save(&store, STUDIO_IMAGE, &layout);

    let loaded = load(&store, STUDIO_IMAGE);
    assert!(loaded.is_visible("layers"), "a saved panel stays visible");
    assert_eq!(loaded.size(DockEdge::Right), 360.0);
    assert!(panels_for(&loaded).layers);
    Ok(())
}

#[test]
fn tooltip_waits_shows_and_hides() {
    let mut tooltip = TooltipState::default();
    assert_eq!(tooltip.visible_text(), "");
    // Hovering does not show it yet: the web waits `mouseEnterDelay` (200ms).
    tooltip.hover("Layers", "Browse the layer stack", 10.0, 20.0, true, 1_000);
    assert!(!tooltip.is_visible());
    assert!(!tooltip.tick(1_100), "still inside the delay");
    assert!(tooltip.tick(1_200), "the delay elapsed: show the name");
    assert!(tooltip.is_visible());
    assert_eq!(tooltip.visible_text(), "Layers");
    assert_eq!(tooltip.visible_description(), "", "the second stage waits");
    assert_eq!(tooltip.anchor(), (10.0, 20.0));
    assert!(!tooltip.tick(1_300), "no further change while hovered");
    // Leaving hides it immediately.
    tooltip.leave();
    assert!(!tooltip.is_visible());
    assert_eq!(tooltip.visible_text(), "");
    assert_eq!(tooltip.visible_description(), "");
    assert!(!tooltip.tick(1_400), "nothing to tick once left");
}

#[test]
fn tooltip_reveals_its_description_after_the_second_delay() {
    let mut tooltip = TooltipState::default();
    tooltip.hover("Zoom", "Set the zoom level", 0.0, 0.0, true, 0);
    assert!(tooltip.tick(200), "1st stage: the name");
    assert_eq!(tooltip.visible_text(), "Zoom");
    assert_eq!(tooltip.visible_description(), "", "no description yet");
    // ~800ms more of hovering: the same surface gains the one-line description.
    assert!(!tooltip.tick(600), "still inside the second delay");
    assert!(!tooltip.is_visible() || tooltip.visible_description() == "");
    assert!(tooltip.tick(1_000), "2nd stage: the description");
    assert_eq!(tooltip.visible_text(), "Zoom");
    assert_eq!(tooltip.visible_description(), "Set the zoom level");
    assert!(
        !tooltip.tick(2_000),
        "no further change once fully revealed"
    );
    // A target with no description never gets a second stage.
    tooltip.leave();
    tooltip.hover("Undo", "", 0.0, 0.0, true, 3_000);
    assert!(tooltip.tick(3_200));
    assert!(!tooltip.tick(4_200));
    assert_eq!(tooltip.visible_description(), "");
    // A new target resets both stages.
    tooltip.hover("Redo", "Replay the edit", 0.0, 0.0, true, 5_000);
    assert!(!tooltip.is_visible());
    assert!(tooltip.tick(5_200));
    assert_eq!(tooltip.visible_description(), "");
    assert!(tooltip.tick(6_000));
    assert_eq!(tooltip.visible_description(), "Replay the edit");
}

#[test]
fn tooltip_never_shows_for_a_disabled_or_empty_target() {
    let mut tooltip = TooltipState::default();
    tooltip.hover("Save", "Save the project", 0.0, 0.0, false, 0);
    assert!(!tooltip.tick(10_000));
    assert!(!tooltip.is_visible());
    // A disabled hover also clears a tooltip that was already up.
    tooltip.hover("Save", "Save the project", 0.0, 0.0, true, 0);
    assert!(tooltip.tick(1_000));
    tooltip.hover("Save", "Save the project", 0.0, 0.0, false, 1_000);
    assert!(!tooltip.is_visible());
    tooltip.hover("", "Save the project", 0.0, 0.0, true, 1_000);
    assert!(!tooltip.tick(2_000));
}

#[test]
fn tooltip_restarts_the_delay_for_a_new_target() {
    let mut tooltip = TooltipState::default();
    tooltip.hover("Layers", "", 0.0, 0.0, true, 0);
    assert!(tooltip.tick(200));
    // Moving onto another target restarts the delay.
    tooltip.hover("History", "", 5.0, 5.0, true, 1_000);
    assert!(!tooltip.is_visible());
    assert!(!tooltip.tick(1_100));
    assert!(tooltip.tick(1_200));
    assert_eq!(tooltip.visible_text(), "History");
}

#[test]
fn reduced_motion_comes_from_the_os_animation_flag() {
    // `reg query ... /v MinAnimate` prints a value column; 0 = animations off.
    assert_eq!(
        parse_min_animate("    MinAnimate    REG_SZ    0"),
        Some(true)
    );
    assert_eq!(
        parse_min_animate("    MinAnimate    REG_SZ    1"),
        Some(false)
    );
    assert_eq!(
        parse_min_animate("    MinAnimate    REG_DWORD    0x0"),
        Some(true)
    );
    assert_eq!(
        parse_min_animate("    MinAnimate    REG_DWORD    0x1"),
        Some(false)
    );
    assert_eq!(parse_min_animate(""), None);
    assert_eq!(parse_min_animate("no value here"), None);
}

/// The `close` dock op is the × path: it removes the panel from the layout and
/// persists the change, while the panel stays in the Dock menu.
#[test]
fn the_close_op_removes_the_panel_and_persists() -> TestResult {
    crate::config_test_support::init_backend()?;
    let (dir, store) = crate::config_test_support::temp_store()?;
    let (ui, app) = crate::config_test_support::new_wired_app(store)?;
    crate::app::with_app(&app, |app| {
        if let Some(studio) = studio(STUDIO_IMAGE) {
            app.image.dock.reveal(studio, "layers");
            app.image.dock.reveal(studio, "history");
            app.image.dock.move_panel("history", DockEdge::Right, 0);
            app.sync_dock(&ui, STUDIO_IMAGE);
        }
        app.dock_request(&ui, STUDIO_IMAGE, "close", "history", "", 0.0, 0.0);
        assert!(
            !app.image.dock.is_visible("history"),
            "the × removes the panel from the layout"
        );
        assert!(
            app.image.dock.is_visible("layers"),
            "the other tab of the group stays"
        );
        assert!(!app.image.panels.history, "the derived flag follows");
    });
    // The commit wrote the layout back to `dock.json`.
    let probe = crate::store::Store::at(dir.path().to_path_buf());
    let loaded = load(&probe, STUDIO_IMAGE);
    assert!(!loaded.is_visible("history"), "the close persisted");
    assert!(loaded.is_visible("layers"));
    // And the Dock menu still lists the closed panel.
    assert!(image()?
        .labels
        .iter()
        .any(|(id, label)| *id == "history" && *label == "History"));
    Ok(())
}

/// The Dock menu is the one place every panel stays reachable from, whatever
/// the layout does. It lists all thirteen panels with their action ids.
#[test]
fn the_dock_menu_lists_every_panel_with_its_action_id() -> TestResult {
    let studio = image()?;
    let src = include_str!("../../ui/components/image.slint").replace("\r\n", "\n");
    for def in studio.defs {
        let action = format!(r#"root.action("dock.{}")"#, def.id);
        assert!(
            src.contains(&action),
            "the Dock menu must offer `{}`",
            def.id
        );
    }
    assert!(
        src.contains(r#"root.action("dock.reset")"#),
        "the Dock menu keeps Reset Layout"
    );
    Ok(())
}

/// The studio bar is a menu bar only: no back arrow, no title, the menus in the
/// conventional order (the dock menu is Photoshop's Window menu), and no tool or
/// zoom controls — those live in the floating tool-options card and the status
/// bar.
#[test]
fn the_studio_top_bar_is_a_left_aligned_menu_bar() {
    let src = include_str!("../../ui/components/image.slint").replace("\r\n", "\n");
    let start = src.find("// ---- menu bar").unwrap_or_default();
    let end = src[start..]
        .find("// ---- dock area")
        .map_or(src.len(), |offset| start + offset);
    assert!(start > 0 && end > start, "the studio menu bar is findable");
    let bar = &src[start..end];
    assert!(
        !bar.contains("Icons.arrow-left"),
        "the studio bar has no back arrow"
    );
    assert!(
        !bar.contains("root.title"),
        "the studio bar has no project title"
    );
    assert!(
        !src.contains("Icons.arrow-left"),
        "the back arrow is gone from the studio entirely"
    );

    let labels: Vec<&str> = bar
        .split("label: \"")
        .skip(1)
        .filter_map(|chunk| chunk.split('"').next())
        .collect();
    assert_eq!(
        labels.iter().take(8).copied().collect::<Vec<_>>(),
        vec!["File", "Edit", "Image", "Layer", "Select", "Filter", "View", "Window"],
        "the menu bar reads File, Edit, Image, Layer, Select, Filter, View, Window"
    );
    // No invented menus: Help is absent because the studio has no About action.
    assert!(!labels.contains(&"Help"), "no dead Help menu ships");
    // The menu bar carries menus only: export lives in File (Export as
    // PNG/JPEG/BMP/PSD), not as a button on the right.
    assert!(
        !bar.contains("label: \"Export\""),
        "the menu bar has no Export button"
    );
    // Zoom moved to the status bar (it is a document read-out, not a menu).
    assert!(
        !bar.contains("Icons.zoom-out") && !bar.contains("Icons.zoom-in"),
        "the menu bar carries menus only"
    );
}

/// The status bar is the studio's document read-out: zoom controls, the size,
/// the colour mode, the active tool and the panel control.
#[test]
fn the_status_bar_carries_the_document_read_out() {
    let src = include_str!("../../ui/components/image.slint").replace("\r\n", "\n");
    let start = src.find("// ---- status bar").unwrap_or_default();
    assert!(start > 0, "the status bar is findable");
    let bar = &src[start..];
    assert!(
        bar.contains(r#"root.action("view.zoom-in")"#)
            && bar.contains(r#"root.action("view.zoom-out")"#),
        "the zoom controls sit in the status bar"
    );
    assert!(
        bar.contains(r#"root.action("view.fit")"#),
        "fit-on-screen survives"
    );
    assert!(
        bar.contains("round(root.view-k * 100)"),
        "the zoom % is shown"
    );
    assert!(
        bar.contains("round(root.doc-w)") && bar.contains("round(root.doc-h)"),
        "the document dimensions are shown"
    );
    assert!(
        bar.contains(r#"root.action("dock")"#),
        "the panel control opens the panel list"
    );
}

/// Every File / Layer menu entry routes an id the studio actually implements:
/// no dead entries ship.
#[test]
fn the_file_and_layer_menus_only_reference_implemented_actions() -> TestResult {
    let src = include_str!("../../ui/components/image.slint").replace("\r\n", "\n");
    let actions = include_str!("actions.rs");
    for menu in ["file", "layer"] {
        let opener = format!("if root.menu == \"{menu}\" : StudioMenu");
        let start = src
            .find(&opener)
            .ok_or_else(|| format!("the {menu} menu exists"))?;
        let end = src[start + opener.len()..]
            .find("if root.menu ==")
            .map_or(src.len(), |offset| start + opener.len() + offset);
        let block = &src[start..end];
        let mut ids = 0;
        for chunk in block.split("root.action(\"").skip(1) {
            let id = chunk.split('"').next().unwrap_or_default();
            assert!(!id.is_empty(), "the {menu} menu has an empty action id");
            assert!(
                actions.contains(&format!("\"{id}\"")),
                "the {menu} menu entry `{id}` has no implementation in src/image/actions.rs"
            );
            ids += 1;
        }
        assert!(ids >= 4, "the {menu} menu carries its entries ({ids})");
    }
    Ok(())
}

/// The panel front carries a close × on the active tab and the group's collapse
/// chevron; both ride the one dock channel or the studio's own tab state.
#[test]
fn the_panel_front_carries_a_close_and_a_collapse() {
    let src = include_str!("../../ui/components/dock.slint").replace("\r\n", "\n");
    assert_eq!(
        src.matches(r#"Theme.dock-request(root.studio, "close""#)
            .count(),
        1,
        "one close, on the active tab of the group's front"
    );
    assert!(
        src.contains("close-ta := TouchArea"),
        "the group's active tab has a close target"
    );
    assert!(
        src.contains("chevron-ta := TouchArea") && src.contains("root.collapse()"),
        "the group's chevron folds it to the header"
    );
    assert!(
        src.contains("StudioTheme.panel-tab") && src.contains("StudioTheme.panel-header"),
        "the tabs paint the PS palette"
    );
}
