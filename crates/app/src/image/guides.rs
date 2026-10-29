//! Guide creation from the rulers, line dragging and Clear Guides — the Slint
//! half of `image-studio.tsx`'s `beginGuide`/`moveGuide`/`endGuide`/
//! `onClearGuides`, with the per-board guide list kept on [`ImageState`].
//! Owns the per-board guide list and the live guide drag (`GuideDrag`).

use crate::app::App;
use crate::AppWindow;

/// The live guide drag (`guideRef` + `guideDraft`): `index` addresses an
/// existing line when re-dragging, `None` while a ruler drag mints a new one.
#[derive(Debug, Clone, PartialEq)]
pub struct GuideDrag {
    /// true = a horizontal guide (dragged from the top ruler, web axis `"y"`).
    pub horizontal: bool,
    pub index: Option<usize>,
    pub value: f64,
}

impl App {
    /// Pointer down on a ruler strip (`beginGuide`) or on a guide line
    /// (`beginGuideDrag`).
    pub fn image_guide_drag(&mut self, ui: &AppWindow, horizontal: bool, x: f64, y: f64) {
        self.image_guide_drag_core(horizontal, x, y, None);
        self.sync_image(ui);
    }

    /// Pointer move while dragging (`moveGuide`).
    pub fn image_guide_move(&mut self, ui: &AppWindow, x: f64, y: f64) {
        let point = self.image_doc_point(x, y);
        if let Some(drag) = self.image.guide_drag.as_mut() {
            drag.value = if drag.horizontal { point.y } else { point.x };
            self.sync_image(ui);
        }
    }

    /// Pointer up (`endGuide`): commits an in-range guide, drops it otherwise.
    pub fn image_guide_drop(&mut self, ui: &AppWindow, x: f64, y: f64) {
        self.image_guide_drop_core(x, y);
        self.sync_image(ui);
    }

    /// The drag start without a window (unit tests drive this directly).
    pub fn image_guide_drag_core(
        &mut self,
        horizontal: bool,
        x: f64,
        y: f64,
        index: Option<usize>,
    ) {
        let point = self.image_doc_point(x, y);
        let value = if horizontal { point.y } else { point.x };
        self.image.guide_drag = Some(GuideDrag {
            horizontal,
            index,
            value,
        });
    }

    /// The drop without a window (unit tests drive this directly). Returns the
    /// committed guide list.
    pub fn image_guide_drop_core(&mut self, x: f64, y: f64) -> Vec<(bool, f64)> {
        let Some(drag) = self.image.guide_drag.take() else {
            return self.image.guides.clone();
        };
        let point = self.image_doc_point(x, y);
        let raw = if drag.horizontal { point.y } else { point.x };
        let limit = if drag.horizontal {
            self.image.project.height
        } else {
            self.image.project.width
        };
        let in_range = (0.0..=limit).contains(&raw) && limit > 0.0;
        match drag.index {
            Some(index) => {
                if in_range {
                    if let Some(slot) = self.image.guides.get_mut(index) {
                        slot.1 = raw.round();
                    }
                } else if index < self.image.guides.len() {
                    self.image.guides.remove(index);
                }
            }
            None => {
                if in_range {
                    self.image.guides.push((drag.horizontal, raw.round()));
                }
            }
        }
        self.image.guides.clone()
    }

    /// View > Clear Guides (`onClearGuides`).
    pub fn image_clear_guides(&mut self, ui: &AppWindow) {
        self.image_clear_guides_core();
        self.sync_image(ui);
    }

    /// The clear without a window (unit tests drive this directly).
    pub fn image_clear_guides_core(&mut self) -> bool {
        let had = !self.image.guides.is_empty();
        self.image.guides.clear();
        self.image.guide_drag = None;
        had
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn guides_board_app() -> Result<(App, tempfile::TempDir), Box<dyn std::error::Error>> {
        let mut project = crate::image::ImageState::default().project;
        project.id = "img-1".to_string();
        project.width = 64.0;
        project.height = 32.0;
        crate::test_support::app_with_image(project, HashMap::new())
    }

    #[test]
    fn ruler_drag_creates_a_rounded_guide_only_inside_the_board(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = guides_board_app()?;
        app.image.view = crate::image::ImageView {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        };

        app.image_guide_drag_core(false, 12.4, 0.0, None);
        let guides = app.image_guide_drop_core(12.4, 0.0);
        assert_eq!(guides, vec![(false, 12.0)]);

        app.image_guide_drag_core(true, 0.0, 8.6, None);
        let guides = app.image_guide_drop_core(0.0, 8.6);
        assert_eq!(guides, vec![(false, 12.0), (true, 9.0)]);

        app.image_guide_drag_core(false, 200.0, 0.0, None);
        let guides = app.image_guide_drop_core(200.0, 0.0);
        assert_eq!(guides.len(), 2, "an out-of-board drop is discarded");
        Ok(())
    }

    #[test]
    fn dragging_an_existing_guide_moves_or_deletes_it() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = guides_board_app()?;
        app.image.view = crate::image::ImageView {
            x: 0.0,
            y: 0.0,
            k: 1.0,
        };
        app.image.guides = vec![(false, 10.0), (true, 5.0)];

        app.image_guide_drag_core(false, 10.0, 0.0, Some(0));
        let guides = app.image_guide_drop_core(30.0, 0.0);
        assert_eq!(guides, vec![(false, 30.0), (true, 5.0)]);

        app.image_guide_drag_core(true, 0.0, 5.0, Some(1));
        let guides = app.image_guide_drop_core(0.0, -4.0);
        assert_eq!(guides, vec![(false, 30.0)], "an out-of-range drag deletes");
        Ok(())
    }

    #[test]
    fn clear_guides_empties_the_list() -> Result<(), Box<dyn std::error::Error>> {
        let (mut app, _dir) = guides_board_app()?;
        app.image.guides = vec![(false, 4.0), (true, 8.0)];
        assert!(app.image_clear_guides_core());
        assert_eq!(app.image.guides.len(), 0);
        assert!(!app.image_clear_guides_core(), "clearing twice is a no-op");
        Ok(())
    }
}
