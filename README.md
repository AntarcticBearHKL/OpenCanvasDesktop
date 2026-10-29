# open-canvas-desktop

Native Rust desktop port of the OpenCanvas web canvas app (`D:\Dev\open-canvas\web`),
built with **Rust + Slint 1.18**. Goal: keep the web UI design, colors, fonts and
UX motion 1:1. See `docs/PORT_SPEC.md` for the full plan and `docs/research/` for the
provenance-cited source analysis (design tokens, UI inventory, motion, domain model).

## Layout

```
Cargo.toml             # workspace (members = core, ai, geom, cms, color, raster, algo, ps, psd, plugins, app, app-core; resolver = "2")
crates/core            # open-canvas-core — canvas domain (nodes/geometry/connections/history/document)
crates/ai              # open-canvas-ai — OpenAI-compatible HTTP layer + provider/model config
crates/ps              # open-canvas-ps — image-studio domain (PS doc + compositor + adjustments/filters)
  src/node.rs          # node types + 51-field metadata + factory (mirrors web/src/types/canvas.ts)
  src/constants.rs     # NODE_SPECS default sizes + initial metadata
  src/geometry.rs      # bounds, guide/grid snapping, fit/size rules
  src/connections.rs   # relations, cubic-bezier geometry, stroke crossing, normalization
  src/history.rs       # snapshot undo/redo (180ms debounce, 50-entry cap)
  src/document.rs      # CanvasProject + "open-canvas" v3 export envelope + LibraryIndex
crates/psd              # open-canvas-psd — vendored clean-room PSD/PSB codec (see Third-party code)
crates/geom             # open-canvas-geom — rects/tiles/warp geometry for the imaging crates
crates/cms              # open-canvas-cms — pure-Rust ICC colour management
crates/color            # open-canvas-color — pixel formats, colour models, blend math
crates/raster           # open-canvas-raster — sparse copy-on-write tiled surfaces
crates/algo             # open-canvas-algo — vendored CPU filter engine (see Third-party code)
crates/app             # open-canvas-app — Slint UI (the only crate with build.rs / include_modules!)
  src/store.rs         # disk persistence: library index, per-project JSON, export/import
  src/library.rs       # library state + view models + navigation
  src/library_ops.rs   # group/project create/rename/delete actions
  src/library_export.rs# export/import modal + CanvasExportFile flows
  src/library_wire.rs  # library callback wiring
  src/glass_capture.rs # whole-desktop BitBlt + downscale + Gaussian (asset-library port)
  src/glass_runtime.rs # capture policy, worker thread, Slint image placement
  src/save.rs          # debounced autosave (400ms document / 500ms viewport)
  ui/theme.slint       # Theme global: exact canvas/node/toolbar tokens, light+dark
  ui/app.slint         # shell composition, library/editor switch, modals + toast
  ui/components/*.slint# library, top-bar, side-panel, toolbar, node-card, wires, ...
  assets/fonts/        # Baloo 2 (variable TTF, embedded at build)
crates/app-core        # open-canvas-app-core — the UI-independent half of the app
  src/doc.rs           # canvas document (nodes, connections, selection, undo)
  src/coords.rs        # WorldPos / ScreenPos / Viewport
  src/nav.rs, gesture.rs, connect.rs, doc_edit.rs, wires.rs, drop_bindings.rs   # pointer logic
  src/paths.rs, panels.rs, overlay.rs, inline_edit.rs, clipboard.rs, input.rs   # geometry + editing
  src/glass.rs         # Win32 transparent-window fallback / rounding / timer resolution
  src/asset_folder.rs  # bound-folder scan, src/drop_files.rs: drop classification
  src/cost.rs, log_view.rs, storage_scan.rs, local_models.rs, system_clipboard.rs
docs/                  # PORT_SPEC.md, research/, screenshots/
scripts/               # p4-capture.ps1 (library/persistence screenshot automation)
mcp/                   # stdio MCP server for the native app (canvas tools over the local agent bridge)
```

## Agent MCP

`mcp/` mirrors the web `canvas_*` tool surface for this native app. The app hosts
a loopback HTTP bridge (`127.0.0.1:3211` by default) on its UI thread, so MCP
edits update the live Slint document, autosave and library index together
instead of racing the app's in-memory state.

```powershell
cd D:\Dev\open-canvas-desktop\mcp
uv sync
uv run desktop-canvas-mcp
```

The desktop app must be running. See `mcp/README.md` for the tool list, the
`OC_AGENT_BRIDGE_*` environment variables and the opencode registration snippet.

## Build & run

Cargo is installed at `C:\Users\antar\.cargo\bin` but is not on `PATH`; prefix it:

```powershell
$env:PATH = "C:\Users\antar\.cargo\bin;$env:PATH"
cargo build
cargo test
cargo run -p open-canvas-app
```

`open-canvas-core` has no UI-framework, network, or persistence dependency — only
`serde`, `serde_json`, and `thiserror`.

## Persistence (P4)

The app opens on the **project library**; a project opens into the editor and the
top bar's Home button returns to the library. Everything is stored as JSON:

```
%APPDATA%\open-canvas-desktop\        # roaming app data (per-user, no admin rights)
  library.json                        # LibraryIndex: groups + one row per project
  projects\<id>.json                  # one CanvasProject per file (web-compatible JSON)
  images\<id>.json                    # one ImageFile per image project (board + base64 PNG bitmaps)
  exports\<title>.json                # CanvasExportFile envelopes written by Export
```

Data dir choice: `%APPDATA%` is the Windows-owned root for per-user application
state, so the library survives reinstalling or moving the binary and needs no
elevation. `OPEN_CANVAS_DATA_DIR` overrides the location (used by the screenshot
script and tests to keep runs hermetic).

- On-disk documents are `CanvasProject` (nodes + connections + viewport + scope)
  exactly as the web export defines them, so a file round-trips losslessly through
  `open-canvas-core`.
- **Autosave** follows the web timing: a document change marks the editor dirty and
  the 60ms app tick flushes 400ms later; viewport-only changes use 500ms
  (`use-canvas-store.ts` 400ms debounce, `project.tsx` viewport debounce). One write
  per burst — no save storms. Home and project switches flush synchronously; save
  failures are reported in a dismissible toast, never by crashing.
- **Export/Import** use the `CanvasExportFile` envelope (`app: "open-canvas"`,
  `version: 3`) with `files: []` (no binary assets exist yet). There is no native
  file dialog: the Export/Import modal is a **path prompt** starting at
  `exports\<title>.json` (import defaults to `exports\import.json`) which can be
  typed over. Import validates the envelope and adds every project, creating the
  envelope's `groupName` folder when needed and re-minting an id instead of
  overwriting an existing project. ZIP packaging with blobs is P5.
- Every write goes through a temp file + rename; all I/O errors surface in the UI.

Screenshots for the library, create/rename/delete, export/import and the
save → restart → reload check are under `docs/screenshots/p4/` and were produced by
`scripts/p4-capture.ps1`.

## Image Studio PSD (open/export)

The studio's File menu has **Open PSD…** and **Export PSD…** next to Import
Bitmap / Export PNG. Both use the same path-prompt flow (no native dialog):
import reads the typed path and opens the PSD as a new library image project;
export writes beside the project's export folder.

Mapping (`crates/app/src/image/psd_io.rs`):

- One PSD layer per project layer: name, visibility, opacity (2-decimal document
  precision) and the 16 blend modes map 1:1 to the codec's `BlendMode`.
- Pixel/image layers with no rotation, mesh transform or enabled layer style
  export their stored bitmap cropped to the document; these round-trip
  byte-identically. Layer masks export as PSD user masks and import back.
- Text, shape, adjustment, and rotated/transformed/styled layers are rendered
  through the `open-canvas-ps` compositor and stored as one flattened
  full-canvas raster layer carrying their visible appearance. An adjustment
  layer exports the changed pixels between the document above and below it, as
  an opaque normal layer: the document looks identical, the adjustment is no
  longer editable. Shape paths, text records and layer styles are not written.
- Groups export as real PSD groups; groups nested deeper than 32 levels flatten.
- The merged composite is always written (the codec never composites). The
  board background is baked into the composite only, so an import starts from a
  transparent board.
- Import accepts 8-bit RGB and Grayscale PSD/PSB only and rejects other depths
  or color modes with a clear error (no panic). Smart objects, clipping layers,
  text and adjustment layers import as raster pixel layers holding their
  appearance.

## Filter engine bridge

`crates/ps/src/algo_bridge.rs` bridges the vendored `open-canvas-algo` engine to the
Image Studio bitmap. `available_algo_filters()` returns the catalogue (stable kebab-case
id, label, group, `supported`, and a parameter schema with min/max/default) so a UI can
build a generic dialog; `apply_filter(&mut Surface, &AlgoFilter, seed)` runs it on the
whole bitmap. Ids never duplicate `crates/ps/src/filters.rs`: the port's 22 filters stay
as they are, and every other engine filter is reachable under its own id (including
`gallery-<effect>` entries for the engine's Filter Gallery effects). `displace` is listed
with `supported: false` because it needs a displacement map (a second image); it returns
`PsError::UnsupportedFilter`. Both surfaces are straight-alpha RGBA8, so pixels convert
1:1 and filters keep alpha semantically; noisy filters take the low 32 bits of `seed`.

## Third-party code

| Path | Title | Author | Source | License |
|---|---|---|---|---|
| `crates/psd` | PhotoCraft PSD codec (`photocraft-psd`, vendored here as `open-canvas-psd`) | PhotoCraft contributors / storytold | https://github.com/storytold/photocraft (`crates/psd`) | MIT OR Apache-2.0 |
| `crates/geom` | PhotoCraft geometry primitives (`photocraft-geom`, vendored here as `open-canvas-geom`) | PhotoCraft contributors / storytold | https://github.com/storytold/photocraft (`crates/geom`) | MIT OR Apache-2.0 |
| `crates/cms` | PhotoCraft ICC colour management (`photocraft-cms`, vendored here as `open-canvas-cms`) | PhotoCraft contributors / storytold | https://github.com/storytold/photocraft (`crates/cms`) | MIT OR Apache-2.0 |
| `crates/color` | PhotoCraft pixel formats and blend math (`photocraft-color`, vendored here as `open-canvas-color`) | PhotoCraft contributors / storytold | https://github.com/storytold/photocraft (`crates/color`) | MIT OR Apache-2.0 |
| `crates/raster` | PhotoCraft copy-on-write tiled surfaces (`photocraft-raster`, vendored here as `open-canvas-raster`) | PhotoCraft contributors / storytold | https://github.com/storytold/photocraft (`crates/raster`) | MIT OR Apache-2.0 |
| `crates/algo` | PhotoCraft CPU filter engine (`photocraft-algo`, vendored here as `open-canvas-algo`) | PhotoCraft contributors / storytold | https://github.com/storytold/photocraft (`crates/algo`) | MIT OR Apache-2.0 |
| `crates/algo/data/libfacedetection.bin`, `crates/algo/src/faces/cnn.rs` | libfacedetection face-detection weights and network | Shiqi Yu and contributors | https://github.com/ShiqiYu/libfacedetection | BSD-3-Clause (see `crates/algo/data/LICENSE-libfacedetection.txt`) |

The vendored ICC profile (`crates/cms/profiles/open-canvas-coated-cmyk.icc`) is generated by
`open-canvas-cms` from its documented synthetic printer model (CC0-1.0); `cargo test -p
open-canvas-cms --test regen` documents how it is rebuilt. The five imaging crates are pure Rust:
`rayon`, `rustfft` and `moxcms` (a test oracle) add no C or C++ dependency.

## Status

| Phase | Scope | State |
|---|---|---|
| P0 | workspace + canvas domain core | done |
| P1 | Slint app + theme tokens + fonts + static canvas shell | done |
| P2 | canvas interaction (pan/zoom, drag, select, connect, marquee) | done |
| P3 | panels/popovers (node panels, settings, minimap, rulers, toolbars, chip input) | done |
| P4 | persistence, project library, import/export | done |
| P5a | AI services (`crates/ai`) + provider/model config + Generate wired | done |
| P5c-1 | Image Studio domain (`crates/ps`) | done |
| P5c-2 | Image Studio UI (shell/dock/tool rail/board/panels + PNG export) | done |
| P5c-3 | adjustments/filters UI + editors + menus + remaining panels | todo |
| P5c-4 | PSD open/export (vendored codec + document mapping) | done |
| P5e | native plugin/node ABI + official plugins | todo |

Verification is code-only (unit/integration tests + headless render assertions) — no
GUI automation or screenshots for the later phases.

Known fidelity deviations (SVG icons, CJK webfont, rich text, video, library card
layout specifics, path-prompt instead of a native file dialog) are listed in
`docs/PORT_SPEC.md` §7 and in each component's comments.
