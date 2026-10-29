//! UI-independent application logic for the `OpenCanvas` desktop app.
//!
//! `crates/app` is one enormous codegen unit: `build.rs` runs Slint codegen and
//! the generated module is ~392 k lines that live in the same crate as all of
//! the app's Rust. Every Rust edit therefore re-runs the rustc frontend over the
//! generated module too. This crate takes the part of the app that has nothing
//! to do with Slint -- viewport math, the canvas document, gestures, hit tests,
//! persistence helpers, the cost log, the native window helpers -- so that
//! platform/domain work compiles on its own, in parallel with the codegen unit,
//! and can be tested without linking the GUI binary at all.
//!
//! Rules for this crate:
//!
//! * no `slint` dependency, no `build.rs`, no generated UI type (`AppWindow`,
//!   `NodeItem`, ...), no `Rc<RefCell<App>>`;
//! * it may depend on the domain crates (`open-canvas-core`, `open-canvas-ai`)
//!   and nothing from `crates/app`;
//! * module names and paths are identical to their old homes, so `crates/app`
//!   keeps every `crate::doc::…` / `crate::coords::…` path alive through
//!   `pub use open_canvas_app_core::{…}` -- and the moved files keep their own
//!   internal `crate::…` paths unchanged.
//!
//! Visibility note: these modules were `pub` inside a *binary* crate, where an
//! item is never "exported" and `pub(crate)` meant "visible to the app". Across
//! the crate boundary that becomes `pub` (the four `Doc` fields `crates/app`
//! still touches directly, `Doc::assemble`, `doc_edit::free_output_spot`,
//! `overlay::dashed_polyline`). This crate is an implementation detail of the app
//! binary, not a published API.
//!
//! Everything that touches a Slint type stays in `crates/app` on purpose; see
//! `docs/PROJECT_AUDIT.md` §6 for the list and the reason for each.

pub mod asset_folder;
pub mod clipboard;
pub mod connect;
pub mod coords;
pub mod cost;
pub mod doc;
pub mod doc_edit;
pub mod doc_project;
pub mod drop_bindings;
pub mod drop_files;
pub mod gesture;
pub mod glass;
pub mod inline_edit;
pub mod input;
pub mod legacy_migration;
pub mod local_models;
pub mod log_view;
pub mod nav;
pub mod overlay;
pub mod panels;
pub mod paths;
pub mod storage_scan;
pub mod system_clipboard;
pub mod wires;
