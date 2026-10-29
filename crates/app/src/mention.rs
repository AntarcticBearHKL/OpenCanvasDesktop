//! Prompt composer session: the inline editor's text/caret/origin state and
//! the `metadata.prompt` write. Reference images reach the model through the
//! wired reference chips at the top of the card (prompt wires and generation
//! wires), never through tokens in the prompt text, so editing never inserts a
//! reference name and never rewrites reference metadata.

use open_canvas_core::node::{CanvasNodeMetadata, CanvasNodeType};

use crate::app::App;
use crate::AppWindow;

impl App {
    /// Double-click on a prompt body: open its editing overlay. The caret is
    /// placed at the end and the pre-edit text is remembered for Escape.
    pub fn panel_prompt_edit_begin(&mut self, ui: &AppWindow, id: &str) {
        let Some(node) = self.doc.node(id) else {
            return;
        };
        let text = node
            .metadata
            .as_ref()
            .and_then(|meta| meta.prompt.clone())
            .unwrap_or_default();
        self.panel.node = Some(id.to_string());
        self.panel.prompt.clone_from(&text);
        self.panel.prompt_origin.clone_from(&text);
        self.panel.prompt_editing = Some(id.to_string());
        self.panel.caret = text.len();
        self.panel.caret_seq = self.panel.caret_seq.wrapping_add(1);
        self.sync_rows_of(ui, &[id.to_string()]);
        self.sync_panel(ui);
    }

    /// Leaves prompt edit mode (focus loss or a click outside).
    pub fn panel_prompt_edit_end(&mut self, ui: &AppWindow) {
        let Some(id) = self.panel.prompt_editing.take() else {
            return;
        };
        self.sync_rows_of(ui, &[id]);
        self.sync_panel(ui);
    }

    /// Escape: restore the text the body had when editing began, then leave the
    /// editor. The restored value goes through the same write path so the
    /// document and the dirty flag stay consistent.
    pub fn panel_prompt_edit_cancel(&mut self, ui: &AppWindow) {
        let Some(id) = self.panel.prompt_editing.clone() else {
            return;
        };
        let origin = self.panel.prompt_origin.clone();
        self.write_prompt(&origin, origin.len());
        self.panel.prompt_editing = None;
        self.sync_rows_of(ui, &[id]);
        self.sync_panel(ui);
    }

    /// Composer lost focus: commit whatever the editor already wrote through
    /// and return the body to its read-only resting state.
    pub fn panel_prompt_blur(&mut self, ui: &AppWindow) {
        if self.panel.prompt_editing.is_some() {
            self.panel_prompt_edit_end(ui);
        }
    }

    /// User edit of the field (`edited`): the composed value writes through.
    pub fn panel_prompt_edited(&mut self, ui: &AppWindow, text: &str, caret: i32) {
        let caret = usize::try_from(caret.max(0)).unwrap_or(0);
        // The first `edited` of a session can arrive from a field whose buffer
        // was not seeded with the node's text (a row rebuilt mid-edit, or an
        // editor opened before the card knew its body). Writing that empty
        // value through would erase a non-empty prompt, so drop it; the card
        // seeds the buffer before it reports any real edit, and a genuine
        // clear after the field showed its text is still written through.
        if text.is_empty()
            && self.panel.prompt_editing.is_some()
            && !self.panel.prompt_origin.is_empty()
            && self.panel.prompt == self.panel.prompt_origin
        {
            return;
        }
        self.write_prompt(text, caret);
        self.resync_prompt(ui);
    }

    /// A reference chip dropped onto the prompt input is a no-op: the chip row
    /// at the top of the card is the reference input, and a drop must never
    /// insert its name (or any token) into the prompt text. Kept as the
    /// document-side entry point so a stray drop only selects/does nothing.
    pub fn insert_prompt_reference_at(
        &mut self,
        _ui: &AppWindow,
        prompt_id: &str,
        image_id: &str,
        x: f64,
        y: f64,
        width: f64,
    ) -> bool {
        self.insert_prompt_reference(prompt_id, image_id, x, y, width)
    }

    /// The UI-free half of [`App::insert_prompt_reference_at`]. Intentionally a
    /// no-op: dropping a wired reference chip never changes the prompt string.
    #[expect(
        clippy::unused_self,
        reason = "the history/dirty bookkeeping keeps this on App; the drop itself is a no-op"
    )]
    pub fn insert_prompt_reference(
        &mut self,
        _prompt_id: &str,
        _image_id: &str,
        _x: f64,
        _y: f64,
        _width: f64,
    ) -> bool {
        false
    }

    /// Writes a composed value through to the node metadata: the composer field
    /// (`prompt` for prompt nodes, `composerContent` for text nodes). Reference
    /// chips are not derived from the text: the wired nodes are the reference
    /// inputs, so editing the prompt only changes the prompt string.
    /// History and the dirty flag follow like any other document edit.
    pub(crate) fn write_prompt(&mut self, text: &str, caret: usize) {
        let Some(id) = self.panel.node.clone() else {
            return;
        };
        let now = self.now_ms();
        let Some(node) = self.doc.nodes_mut().iter_mut().find(|node| node.id == id) else {
            return;
        };
        let composer = matches!(
            node.node_type,
            CanvasNodeType::Prompt | CanvasNodeType::VideoPrompt
        );
        let meta = node
            .metadata
            .get_or_insert_with(CanvasNodeMetadata::default);
        if composer {
            meta.prompt = Some(text.to_string());
        } else {
            meta.composer_content = Some(text.to_string());
        }
        self.panel.prompt = text.to_string();
        self.panel.caret = caret;
        self.doc.commit(now);
        self.note_doc_change();
    }

    fn resync_prompt(&mut self, ui: &AppWindow) {
        if let Some(id) = self.panel.node.clone() {
            self.sync_rows_of(ui, &[id]);
        }
        self.sync_panel(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Doc;
    use open_canvas_core::node::{CanvasConnection, CanvasNodeData, Position};

    fn node(id: &str, node_type: CanvasNodeType, metadata: CanvasNodeMetadata) -> CanvasNodeData {
        CanvasNodeData {
            id: id.to_string(),
            node_type,
            title: id.to_string(),
            position: Position::new(0.0, 0.0),
            width: 340.0,
            height: 240.0,
            metadata: Some(metadata),
        }
    }

    fn connection(from: &str, to: &str) -> CanvasConnection {
        CanvasConnection {
            id: format!("conn-{from}-{to}"),
            from_node_id: from.to_string(),
            to_node_id: to.to_string(),
            relation: None,
            kind: None,
            reason: None,
            changes: None,
            agent_run_id: None,
        }
    }

    /// Prompt node with an upstream image and text node, panel focused on it.
    fn app_with_prompt() -> App {
        let nodes = vec![
            node(
                "img-1",
                CanvasNodeType::Image,
                CanvasNodeMetadata {
                    content: Some("data:image".into()),
                    ..Default::default()
                },
            ),
            node(
                "txt-1",
                CanvasNodeType::Text,
                CanvasNodeMetadata {
                    content: Some("a calm lake".into()),
                    ..Default::default()
                },
            ),
            node(
                "prompt-1",
                CanvasNodeType::Prompt,
                CanvasNodeMetadata {
                    prompt: Some(String::new()),
                    ..Default::default()
                },
            ),
        ];
        let connections = vec![
            connection("img-1", "prompt-1"),
            connection("txt-1", "prompt-1"),
        ];
        let mut app = App::new(crate::store::Store::at(std::env::temp_dir()));
        app.doc = Doc::assemble(nodes, connections);
        app.panel.node = Some("prompt-1".to_string());
        app
    }

    fn prompt_meta(app: &App) -> CanvasNodeMetadata {
        app.doc
            .node("prompt-1")
            .and_then(|node| node.metadata.clone())
            .unwrap_or_default()
    }

    /// A reference chip dropped onto the prompt body is a no-op: the wired chip
    /// row already carries the reference, so the drop never inserts its label.
    #[test]
    fn dropping_a_reference_never_edits_the_prompt() {
        let mut app = app_with_prompt();
        app.panel.node = Some("prompt-1".into());
        app.write_prompt("warm light", 10);
        let before = prompt_meta(&app);
        assert!(!app.insert_prompt_reference("prompt-1", "img-1", 0.0, 0.0, 308.0));
        assert!(!app.insert_prompt_reference("prompt-1", "img-1", 10_000.0, 0.0, 308.0));
        let after = prompt_meta(&app);
        assert_eq!(after.prompt.as_deref(), Some("warm light"));
        assert_eq!(after.references, before.references);
        // A node that is not a reference is refused with the same no-op.
        assert!(!app.insert_prompt_reference("prompt-1", "not-a-reference", 0.0, 0.0, 308.0));
        assert_eq!(prompt_meta(&app).prompt.as_deref(), Some("warm light"));
    }

    /// A reference label typed into the prompt is just text: editing never
    /// creates reference metadata or rewrites the card's reference list.
    #[test]
    fn editing_the_prompt_keeps_reference_metadata_out_of_the_text() {
        let mut app = app_with_prompt();
        app.write_prompt("warm light Image 1 and snow", 26);
        let meta = prompt_meta(&app);
        assert_eq!(meta.prompt.as_deref(), Some("warm light Image 1 and snow"));
        assert!(
            meta.references.is_none(),
            "editing never derives reference ids from the prompt text"
        );

        app.write_prompt("", 0);
        let meta = prompt_meta(&app);
        assert_eq!(meta.prompt.as_deref(), Some(""));
        assert!(meta.references.is_none());
    }

    /// Text nodes compose into `composerContent`; prompt nodes into `prompt`.
    #[test]
    fn text_nodes_write_composer_content() {
        let nodes = vec![node(
            "text-1",
            CanvasNodeType::Text,
            CanvasNodeMetadata {
                content: Some(String::new()),
                ..Default::default()
            },
        )];
        let mut app = App::new(crate::store::Store::at(std::env::temp_dir()));
        app.doc = Doc::assemble(nodes, Vec::new());
        app.panel.node = Some("text-1".to_string());
        app.write_prompt("plain words", 11);
        let meta = app
            .doc
            .node("text-1")
            .and_then(|node| node.metadata.clone())
            .unwrap_or_default();
        assert_eq!(meta.composer_content.as_deref(), Some("plain words"));
        assert!(meta.prompt.is_none());
    }

    /// Regression: a prompt that already has text must show that text when
    /// double-clicked. The editor starts from the stored prompt (not an empty
    /// buffer), the row keeps the same body and the pre-edit origin is the
    /// stored text, so Escape restores exactly what was there.
    #[test]
    fn a_prompt_with_text_reopens_its_editor_with_that_text(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use slint::Model as _;
        crate::config_test_support::init_backend()?;
        let (_dir, store) = crate::config_test_support::temp_store()?;
        let (ui, app) = crate::config_test_support::new_wired_app(store)?;
        ui.invoke_create_node("prompt".into());
        let id = crate::app::with_app_r(&app, |app| {
            app.doc.nodes().first().map(|node| node.id.clone())
        })
        .flatten()
        .ok_or("the dock creates a prompt node")?;
        crate::app::with_app(&app, |app| {
            app.panel_prompt_edit_begin(&ui, &id);
            app.panel_prompt_edited(&ui, "A RED FOX", 9);
            app.panel_prompt_blur(&ui);
            assert_eq!(
                app.doc
                    .node(&id)
                    .and_then(|node| node.metadata.as_ref())
                    .and_then(|meta| meta.prompt.as_deref()),
                Some("A RED FOX"),
                "typing through the editor writes the prompt"
            );
            let row = app.rows.row_data(0).expect("the prompt row exists");
            assert_eq!(row.body.as_str(), "A RED FOX");
            assert!(!row.prompt_editing);

            app.node_double_click(&ui, &id);
            assert_eq!(app.panel.prompt, "A RED FOX");
            assert_eq!(app.panel.prompt_origin, "A RED FOX");
            assert_eq!(app.panel.prompt_editing.as_deref(), Some(id.as_str()));
            let row = app.rows.row_data(0).expect("the prompt row exists");
            assert_eq!(row.body.as_str(), "A RED FOX", "the row keeps its body");
            assert!(row.prompt_editing);

            // Commit exactly what the editor displays: the card seeds its
            // buffer from the row body, so this must keep the text.
            let displayed = row.body.to_string();
            app.panel_prompt_edited(&ui, &displayed, i32::try_from(displayed.len()).unwrap_or(0));
            let stored = app
                .doc
                .node(&id)
                .and_then(|node| node.metadata.as_ref())
                .and_then(|meta| meta.prompt.clone());
            assert_eq!(stored.as_deref(), Some("A RED FOX"));
            app.panel_prompt_blur(&ui);
            assert!(app.panel.prompt_editing.is_none(), "blur closes the editor");
            let row = app.rows.row_data(0).expect("the prompt row exists");
            assert_eq!(row.body.as_str(), "A RED FOX");
            assert!(!row.prompt_editing);

            // A stray empty `edited` before the field was seeded must never
            // erase a non-empty prompt; a real edit still writes through.
            app.node_double_click(&ui, &id);
            app.panel_prompt_edited(&ui, "", 0);
            let stored = app
                .doc
                .node(&id)
                .and_then(|node| node.metadata.as_ref())
                .and_then(|meta| meta.prompt.clone());
            assert_eq!(stored.as_deref(), Some("A RED FOX"));
            app.panel_prompt_edited(&ui, "A RED FOX IN SNOW", 17);
            let stored = app
                .doc
                .node(&id)
                .and_then(|node| node.metadata.as_ref())
                .and_then(|meta| meta.prompt.clone());
            assert_eq!(stored.as_deref(), Some("A RED FOX IN SNOW"));
            // Once the field has shown a real edit, clearing it is a user edit
            // and is written through.
            app.panel_prompt_edited(&ui, "", 0);
            let stored = app
                .doc
                .node(&id)
                .and_then(|node| node.metadata.as_ref())
                .and_then(|meta| meta.prompt.clone());
            assert_eq!(stored.as_deref(), Some(""));
        });
        Ok(())
    }

    /// The card's wired drop callback is the real path a reference drag takes.
    /// It must leave both the prompt string and the wired reference list alone:
    /// references are entered only by wiring image nodes to the card.
    #[test]
    fn a_wired_reference_drop_never_edits_the_prompt() -> Result<(), Box<dyn std::error::Error>> {
        crate::config_test_support::init_backend()?;
        let (_dir, store) = crate::config_test_support::temp_store()?;
        let (ui, app) = crate::config_test_support::new_wired_app(store)?;
        ui.invoke_create_node("prompt".into());
        let id = crate::app::with_app_r(&app, |app| {
            app.doc.nodes().first().map(|node| node.id.clone())
        })
        .flatten()
        .ok_or("the dock creates a prompt node")?;

        ui.invoke_node_prompt_edited(id.clone().into(), "warm light".into(), 10);
        let before = crate::app::with_app_r(&app, |app| {
            app.doc
                .node(&id)
                .and_then(|node| node.metadata.clone())
                .unwrap_or_default()
        })
        .ok_or("the prompt node exists")?;
        assert_eq!(before.prompt.as_deref(), Some("warm light"));

        // The coordinates are irrelevant: the wired callback must be a no-op.
        ui.invoke_node_ref_insert(id.clone().into(), "img-1".into(), 10.0, 10.0, 308.0, 200.0);

        let after = crate::app::with_app_r(&app, |app| {
            app.doc
                .node(&id)
                .and_then(|node| node.metadata.clone())
                .unwrap_or_default()
        })
        .ok_or("the prompt node still exists")?;
        assert_eq!(
            after.prompt.as_deref(),
            Some("warm light"),
            "a reference drop never inserts its name into the prompt"
        );
        assert_eq!(
            after.references, before.references,
            "a reference drop never rewrites the reference list"
        );
        assert!(after.references.is_none());
        Ok(())
    }

    /// Regression through the wired UI callbacks: text written by the editor is
    /// stored, and double-clicking the prompt reopens that exact text in the
    /// editor, with the row body still showing it.
    #[test]
    fn the_wired_double_click_reopens_a_prompt_with_its_text(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use slint::Model as _;

        crate::config_test_support::init_backend()?;
        let (_dir, store) = crate::config_test_support::temp_store()?;
        let (ui, app) = crate::config_test_support::new_wired_app(store)?;
        ui.invoke_create_node("prompt".into());
        let id = crate::app::with_app_r(&app, |app| {
            app.doc.nodes().first().map(|node| node.id.clone())
        })
        .flatten()
        .ok_or("the dock creates a prompt node")?;

        ui.invoke_node_double_click(id.clone().into());
        ui.invoke_node_prompt_edited(id.clone().into(), "A RED FOX".into(), 9);
        ui.invoke_node_prompt_blur(id.clone().into());

        let stored = crate::app::with_app_r(&app, |app| {
            let stored = app
                .doc
                .node(&id)
                .and_then(|node| node.metadata.as_ref())
                .and_then(|meta| meta.prompt.clone());
            let row = app.rows.row_data(0).expect("the prompt row exists");
            (stored, row.body.to_string(), row.prompt_editing)
        })
        .ok_or("the app is borrowable")?;
        assert_eq!(stored.0.as_deref(), Some("A RED FOX"));
        assert_eq!(stored.1, "A RED FOX");
        assert!(!stored.2, "blur returns the body to its resting state");

        ui.invoke_node_double_click(id.clone().into());
        let opened = crate::app::with_app_r(&app, |app| {
            let row = app.rows.row_data(0).expect("the prompt row exists");
            (
                app.panel.prompt.clone(),
                app.panel.prompt_origin.clone(),
                app.panel.prompt_editing.clone(),
                row.body.to_string(),
                row.prompt_editing,
            )
        })
        .ok_or("the app is borrowable")?;
        assert_eq!(
            opened.0, "A RED FOX",
            "the editor buffer is the stored text"
        );
        assert_eq!(
            opened.1, "A RED FOX",
            "Escape would restore the stored text"
        );
        assert_eq!(opened.2.as_deref(), Some(id.as_str()));
        assert_eq!(opened.3, "A RED FOX", "the row body keeps the stored text");
        assert!(opened.4, "the editor branch is active");
        Ok(())
    }
}
