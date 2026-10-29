# Open Canvas — Master Gap Analysis (web → native desktop)

**Scope.** This is the merged, decision-ready gap analysis for the native desktop port of Open Canvas. The **web app is the source of truth** (`D:\Dev\open-canvas\web\src`); the **native port is `D:\Dev\open-canvas-desktop`** (Rust + Slint). Every row was produced by seven independent per-domain audits that each **verified against the actual source code, not against research notes**, then merged here without re-auditing. Status vocabulary is fixed: **IMPLEMENTED / PARTIAL / MISSING / DEVIATION / N-A**, where *IMPLEMENTED* = full fidelity, *PARTIAL* = subset, *MISSING* = absent, *DEVIATION* = present but structurally different, *N-A* = the web app does not have the feature either (no gap to close, or a net-new candidate). Rows are counted from column 3 of each source table; the Image Studio report uses the source abbreviations **IMPL/PART/MISS/DEV**, which map 1:1 onto the canonical enum.

---

## Executive summary

- **Overall verdict: bimodal parity.** The native port is an exceptionally faithful **domain/engine** port (models, reducers, math and persistence schemas round-trip 1:1) but a weak **runtime + content + tooling** port. Across 938 counted features: **469 IMPLEMENTED (50%)**, **133 PARTIAL**, **201 MISSING**, **62 DEVIATION**, **73 N-A**.
- **Canvas Board Core** — interaction parity is strong (pan, zoom-at-cursor, marquee, multi-drag, snap, connect/legality/target-highlight, resize, undo/redo, minimap, rulers, toolbars) but the **content/media and node-tooling layer is largely absent**: image nodes draw **no image at all**, and drop-bindings, copy/paste, inline editing, image tools, zoom panel, connection delete and loading/error states are missing.
- **Canvas Panels / Overlays / Library / Persistence** — the shell, library CRUD and node panels are solid, but the **app config modal is 2 of 10 tabs**, and the whole **image quick-tool + dialog family**, the **preview modal** (placeholder only), the **dock** and the **side-panel/switcher** interactions are weak.
- **AI Services / Settings / Prompt / Shortcuts / Theming / i18n** — the AI **service layer is close to parity** (image gen, edit, streaming chat/vision, retry, cancel, request shapes), but **cost/usage is entirely absent**, **i18n has no locale files or runtime switching**, the **settings modal is 2/10 tabs**, and the **image AI tool suite** (OCR, multi-angle, upscale, background-removal, segmentation) is missing.
- **Image Studio** — the **engine is remarkably complete** (all 16 tools, 16 blend modes, 15 adjustments, 22 filters, 10 layer styles, selection, paths, text rendering, channels, actions, history); the gaps are in the **UI layer**: **masks entirely missing**, **transform modes non-functional**, **no text panel**, **flat dock**, plus layer thumbnails/drag-reorder/guides.
- **Pixel Studio** — a **near 1:1 port** (10 tools, tool options, doc reducer, 5 blend modes, layers, frames, playback, undo, PNG/sheet export) with a handful of gaps: **graphical colour picker reduced to numeric fields**, **no canvas-image-node import sources**, **simplified fixed dock (no persistence)**, **no agent/headless bridge**.
- **Texture Studio** — registry (117 types / 407 params) and effects/layers/blend/params parity are strong, but **25/117 generator types silently render blank** (degrading 11/12 built-in samples), **GIF export has no UI**, three **output toggles are dead**, **keyboard undo/redo is misrouted**, and **gradient-editor interactions** are missing.
- **Plugin System** — data-shaped logic (registry, definitions, manager state machine, semver, event bus, storage traits, extras sidecar, the 5 official plugins' pure logic) is present, but **there is no plugin execution engine at all**: URL install is record-only, no fetch/eval/import, no `setup`, no custom `Panel`, no plugin `Content` — every third-party plugin is inert.
- **Single most important finding:** the port is caught between two tiers — the **data/engine tier is ~complete** while the **runtime tier (plugin execution, media/content rendering, node-level tooling, config/i18n surfaces) is missing**, so roughly **42% of counted features (396/938) are PARTIAL, MISSING or DEVIATION**, and the most visible user-facing regressions cluster in exactly those layers.
- **Regression vs net-new:** **396 rows are regressions** (web has it, native does not — PARTIAL + MISSING + DEVIATION); **73 rows are N-A** (absent in web too), of which ~35 are genuine **net-new feature candidates** (onion skin, dithering, merge/flatten, pixel-perfect mode, GIF/sheet extras, timeline, tweening, expand/contract selection, per-layer mask, etc.) and the rest are true not-applicables.

---

## Global statistics

Counting rule: a row is counted when column 3 of its source table carries a status token; group-header rows, separator rows and prose are excluded; `IMPL/PART/MISS/DEV` are normalized to the canonical enum; where a status cell is dual (e.g. `IMPL (engine) / MISS (UI)`) the leading token is counted.

| Domain | Features counted | IMPLEMENTED | PARTIAL | MISSING | DEVIATION | N-A |
|---|---:|---:|---:|---:|---:|---:|
| 1. Canvas Board Core | 95 | 42 | 12 | 31 | 8 | 2 |
| 2. Canvas Panels / Overlays / Modals / Library / Persistence | 152 | 47 | 26 | 50 | 20 | 9 |
| 3. AI Services / Settings / Prompt / Shortcuts / Theming / i18n | 122 | 44 | 21 | 40 | 5 | 12 |
| 4. Image Studio | 186 | 145 | 20 | 15 | 1 | 5 |
| 5. Pixel Studio | 110 | 65 | 4 | 9 | 4 | 28 |
| 6. Texture Studio | 113 | 60 | 9 | 17 | 19 | 8 |
| 7. Plugin System | 160 | 66 | 41 | 39 | 5 | 9 |
| **TOTAL** | **938** | **469** | **133** | **201** | **62** | **73** |

**Derived:** regressions vs web (PARTIAL + MISSING + DEVIATION) = **396** (42.2%); fully implemented = **469** (50.0%); N-A (absent in web too) = **73** (7.8%).

---

## Overlap / deduplication notes

Several features were independently audited by more than one report. Each is **kept once in the primary domain table and cross-referenced here** (not deleted):

- **Keyboard copy/paste of nodes (Ctrl+C/V + system clipboard)** — *Canvas Core* (Keyboard: copy/paste), *Panels/Overlays* (§K) and *AI/Settings* (Keyboard Shortcuts). Primary row: Canvas Core.
- **Image quick-tools + image edit dialogs (crop/mask/bg/split/resolution/analyze/segment/angle/upscale/ocr/free-resize)** — *Canvas Core* (hover toolbar) and *Panels/Overlays* (§F hover bar + §I dialogs). Toolbar entries owned by Canvas Core; dialog surfaces by Panels/Overlays.
- **App config modal reduced to providers+models / missing tabs** — *Panels/Overlays* (§A) and *AI/Settings* (Provider/Model Settings). Same gap from two angles; both kept.
- **Canvas background modes (dots/lines/blank)** — *Canvas Core* (grid modes), *Panels/Overlays* (§A appearance + §H bg controls) and *AI/Settings* (Theming). Same gap, three citations.
- **Theme not persisted / light-dark toggle** — *Panels/Overlays* (§A) and *AI/Settings* (Theming).
- **Zoom panel / reset-fit viewport / shortcuts modal** — *Canvas Core* (§H) and *Panels/Overlays* (§C/§H).
- **Node list completeness (type filter, bulk rename, layer-count header)** — *Canvas Core* (§H) and *Panels/Overlays* (§F/§H).
- **Per-node layer popover / layers popover** — *Canvas Core* (§F) and *Panels/Overlays* (§F/§H).
- **Preview modal placeholder / no image render** — *Canvas Core* (§F) and *Panels/Overlays* (§I).
- **Node panel under a node (composer/config)** — *Canvas Core* (§B) and *Panels/Overlays* (§E); both note the overlap explicitly.
- **Text style popover** — *Canvas Core* (hover text tools) and *Panels/Overlays* (§E).
- **Generation cost & usage** — *AI/Settings* (services + cost tab) and *Panels/Overlays* (§A config Cost tab).
- **Mask / inpaint UI** — *Image Studio* (Masks), *Canvas Core* (mask-edit dialog) and *AI/Settings* (inpaint mask UI).
- **Drag node → prompt / node drop bindings** — *Canvas Core* (drop bindings) and *AI/Settings* (Prompt/Chip Input).
- **Dock drag/resize/split engine + layout persistence** — appears in **four** reports: *Panels/Overlays* (§D), *Image Studio* (Dock layout), *Pixel Studio* (Dock engine), *Texture Studio* (Dock layout). Kept once per studio because the gap recurs across all four workspaces.
- **i18n / English-only string literals** — *AI/Settings* (I18n) and *Plugin System* (§J localized strings DEVIATION).
- **Colour picker fidelity (SV square / hue strip)** — *Pixel Studio* (§Drawing model), *Image Studio* (Color panel) and *Texture Studio* (colour edits hex-only).

No source report had a truncated table; all seven tables below are complete as saved.

---

## Domain reports

The seven tables below are preserved verbatim from the source reports (rows not rewritten, not dropped). Field names and path style follow each source report.

### 1. Canvas Board Core

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| **A. Node types / registry / create menu** | | | | |
| Built-in types (image, image-stack, text, prompt, video-prompt, image-generation, video-generation, video, assets) | `types/canvas.ts`, `components/canvas/nodes/builtin-nodes.tsx` | IMPLEMENTED | `crates/core/src/node.rs` (`CanvasNodeType`), `crates/plugins/src/.../builtins` | 9 variants + serde kebab ids |
| Open/plugin node types (`<pluginId>:<name>`) | `lib/canvas/node-registry.ts` | IMPLEMENTED | `node.rs` (`CanvasNodeType::Plugin`) | Lossless round-trip |
| Node registry API (register/unregister/list/get/owner/isBuiltin) | `lib/canvas/node-registry.ts` | IMPLEMENTED | `crates/plugins` `NodeRegistry` | In plugins crate |
| Node definition contract (icon, size, minimapColor, showInCreateMenu, hasSourceHandle, hidePanel, transparentBg, autoOpenPanel, useBuiltinPanel, interactionToggle, forceInteractive, keepAspectRatio, resource, toolbar, onDoubleClick) | `types/canvas-plugin.ts` | PARTIAL | `crates/plugins/src/abi.rs`, `plugin_views.rs` | `Content`/`Panel` (plugin React components) have no native equivalent — `PanelKind::Plugin`→None |
| Built-in specs (default sizes + initial metadata) | `constant/canvas.ts` | IMPLEMENTED | `crates/core/src/constants.rs` | — |
| Node factory (center-on-point, id format, metadata merge) | `lib/canvas/canvas-node-factory.ts` | IMPLEMENTED | `node.rs` `create_canvas_node`, `doc_edit.rs` | — |
| `applyNodeConfigPatch` (ratio re-size + re-center) | `lib/canvas/canvas-node-factory.ts` | IMPLEMENTED | `node.rs` `apply_node_config_patch` | — |
| Create menu (grouped prompt/generator/input popovers) | `components/canvas/canvas-toolbar.tsx` | DEVIATION | `ui/components/create-menu.slint`, `plugin_views.rs::create_rows` | Single flat list of ALL registered types + inert group buttons; no per-group popover |
| Extensions menu (plugin nodes) | `components/canvas/canvas-toolbar.tsx` | DEVIATION | `plugin_views.rs` | Merged into create menu, grouped by owner id |
| **B. Per-node content renderers / inline editing** | | | | |
| Text node content (render + inline edit) | `canvas-node.tsx` `TextContent` | PARTIAL | `ui/components/node-card.slint` | Body shown read-only; NO inline double-click edit |
| Text batch (multi texts, expand, set primary, take-out) | `canvas-node.tsx` `BatchFrame`/`ExpandedTextCard` | MISSING | — | No `metadata.texts` rendering |
| Image node content (actual bitmap) | `canvas-node.tsx` `ImageNodeContent`/`CanvasImage` | MISSING | `node-card.slint` | Card shows a placeholder box + "Image"; image never drawn |
| Image node empty state | `canvas-node.tsx` `EmptyImageContent` | DEVIATION | `node-card.slint` | Placeholder equivalent, different text |
| Image-stack node content (thumbnails, view-all, split) | `nodes/image-stack-node-content.tsx` | MISSING | — | — |
| Video node content (player) | `canvas-node.tsx` `VideoNodeContent` | MISSING | `node-card.slint` | Placeholder only; no playback |
| Prompt node inline content (chip input, ref chips, tags) | `nodes/prompt-node-content.tsx`, `prompt-node-panel.tsx`, `canvas-prompt-chip-input.tsx` | PARTIAL | `node-card.slint`, `node-panel.slint` | Panel composer exists; inline prompt editor/chip editing absent |
| Video-prompt content (first/last/reference frame slots, video-mode toggle) | `nodes/video-prompt-node-content.tsx` | PARTIAL | `node-card.slint` `FrameSlot` | Two static boxes; no slot drop/bind, no videoMode toggle |
| Image-generation content (empty hint) | `canvas-node.tsx` `ImageGenerationContent` | DEVIATION | `node-card.slint` | Body shows model string instead |
| Assets node content (folder browser) | `nodes/assets-node-content.tsx` | MISSING | — | — |
| Plugin node content (sticky/markdown/svg/html/panorama) | plugin `Content`; `canvas-node.tsx` | IMPLEMENTED | `plugin_content.rs`, `node-card.slint`, `plugin_views.rs` | html=sandbox no webview, panorama no 3D (deviations) |
| Missing-plugin placeholder | `canvas-node.tsx` `MissingPluginContent` | IMPLEMENTED | `node-card.slint` | — |
| Resource mention `@` menu in composer | `canvas-resource-mention-textarea.tsx` | IMPLEMENTED | `mention.rs`, `node-panel.slint` | — |
| Node panel under node (composer/config 600px) | `canvas-node.tsx` showPanel, `canvas-node-prompt-panel.tsx`, `canvas-config-node-panel.tsx` | IMPLEMENTED | `node-panel.slint`, `node_panel.rs`, `panel_events.rs` | May overlap panels/overlays agent |
| **C. Node anatomy** | | | | |
| Title above card + inline rename (dbl-click) | `canvas-node.tsx` | MISSING | `node-card.slint` | Title is static `Text`; no rename |
| Media info bar (WxH · bytes · cost · info) | `canvas-node.tsx` `MediaInfoBar` | MISSING | — | Info modal exists; no info bar |
| Selection border / z-index | `canvas-node.tsx` | IMPLEMENTED | `node-card.slint` | — |
| Related/focus highlight (hover chain) | `canvas-node.tsx`, `project.tsx` `relatedHighlight` | MISSING | — | No related-node/connection highlight |
| Lock badge | `canvas-node.tsx` | MISSING | — | Lock enforced, but no card badge |
| Resize corner handles (4) + grip | `canvas-node.tsx` `ResizeHandle`/`ResizeGrip` | IMPLEMENTED | `node-handles.slint`, `gesture.rs` | — |
| Resize handle visibility (corners unlocked; grip on hover/sel) | `canvas-node.tsx` | PARTIAL | `node-handles.slint` | Corners always drawn when unlocked |
| Connection dots (left target / right source; per-def) | `canvas-node.tsx` `ConnectionHandleDot` | IMPLEMENTED | `node-handles.slint`, `vm.rs` | hasSource/hasTarget from bridge |
| Keep-ratio resize + free-resize toggle | `canvas-node.tsx`, `project.tsx` `toggleNodeFreeResize` | PARTIAL | `gesture.rs` | Ratio honored; no free-resize toggle |
| **D. Canvas interactions** | | | | |
| Pan (background drag / middle button) | `open-canvas.tsx` | PARTIAL | `nav.rs`, `handlers.rs` | Middle-drag pans only in Pan tool (web pans in Select too) |
| Zoom at cursor (wheel, clamp 0.05–5) | `open-canvas.tsx` | IMPLEMENTED | `coords.rs` `zoom_at`, `handlers.rs` | — |
| Temp tool swap (Ctrl / Space) | `open-canvas.tsx` | IMPLEMENTED | `doc.rs` `effective_tool`, `shortcuts.rs` | — |
| Default tool = pan | `project.tsx` | IMPLEMENTED | `doc.rs` (`Tool::Pan`) | — |
| Marquee select (shift additive) | `project.tsx` | IMPLEMENTED | `nav.rs` | — |
| Click select (shift toggle / replace) | `project.tsx` `selectNodeByEvent` | DEVIATION | `doc.rs` `select_node` | Shift only adds; web toggles off if already selected |
| Select all (Ctrl+A) | `project.tsx` | IMPLEMENTED | `shortcuts.rs` | — |
| Background click deselect | `open-canvas.tsx` | IMPLEMENTED | `nav.rs` `end_pan` | — |
| Multi-node drag | `project.tsx` | IMPLEMENTED | `gesture.rs` | — |
| Snap to guides + grid | `canvas-node-geometry.ts` `snapDragToGuides` | IMPLEMENTED | `core/geometry.rs`, `gesture.rs` | — |
| Snap guide lines | `project.tsx` | IMPLEMENTED | `paths.rs` `guides_commands` | — |
| Connect wires (handle drag, hit test, legality, dedup) | `project.tsx` `connectNodes`, `canvas-node-geometry.ts` | IMPLEMENTED | `connect.rs`, `core/connections.rs` | — |
| Active-connection preview (dashed bezier + target snap) | `canvas-connections.tsx` `ActiveConnectionPath` | IMPLEMENTED | `overlay.rs` `active_wire_commands` | — |
| Connection target highlight | `canvas-node.tsx` | IMPLEMENTED | `node-card.slint` connect-target | — |
| Wire rendering (cubic bezier) | `canvas-connections.tsx` `ConnectionPath` | IMPLEMENTED | `paths.rs` `wires_commands` | — |
| Wire relation label (reference index text) | `canvas-connections.tsx` | MISSING | — | — |
| Wire click-select + delete | `canvas-connections.tsx`, `use-canvas-document.ts` `deleteConnection` | MISSING | — | Connections can't be selected/deleted |
| Cut tool (Ctrl+right-drag deletes crossed wires) | `canvas-pointer-tools.ts`, `project.tsx` | MISSING | — | Host not ported |
| Right-click context menu | (web suppresses native menu; only cut tool) | N-A | — | Neither has a node context menu |
| Node resize gesture (min 220×160, ratio lock) | `canvas-node.tsx` | IMPLEMENTED | `gesture.rs` | — |
| Drag ghost chip for drop sources | `project.tsx` `canvas-drag-ghost` | MISSING | — | — |
| Drop bindings: image→assets (collect) | `canvas-drop-bindings.ts`, `project.tsx` | MISSING | — | — |
| Drop bindings: image→image-stack (append) | `canvas-drop-bindings.ts`, `project.tsx` | MISSING | — | — |
| Drop bindings: image/video→video-prompt slot | `canvas-video-slots.ts`, `canvas-drop-bindings.ts` | MISSING | — | `videoSlots` metadata modelled in core, never bound |
| Double-click node (image→preview, text→edit, plugin→action) | `canvas-node.tsx` | PARTIAL | `bar_events.rs` `node_double_click` | Preview + plugin toggle; text inline edit missing |
| Double-click background → create node | (none) | DEVIATION | `handlers.rs` `canvas_double_click` | Native creates a Text node (P2 stand-in) |
| Keyboard: undo/redo, select-all, delete, escape, space | `project.tsx` | IMPLEMENTED | `shortcuts.rs`, `doc.rs` | — |
| Keyboard: copy / paste nodes (Ctrl+C/V + system paste) | `project.tsx`, `use-canvas-document.ts` | MISSING | — | Not in `shortcuts.rs` |
| Focus/edit mode + `interactionToggle` | `canvas-node.tsx` `contentInteractive` | PARTIAL | `plugin_views.rs`, `plugin_content.rs` | Plugin interact flag only |
| **E. Node operations** | | | | |
| Duplicate node | `use-canvas-document.ts` `duplicateNode` | IMPLEMENTED | `doc_edit.rs` `duplicate_node` | — |
| Delete nodes (locked excluded, cascades connections) | `use-canvas-document.ts` `deleteNodes` | IMPLEMENTED | `doc_edit.rs` `delete_selection` | — |
| Copy/paste nodes (clipboard, connection remap) | `use-canvas-document.ts` `copySelectedNodes`/`pasteCopiedNodes` | MISSING | — | — |
| Z-order up/down | `project.tsx` `moveNodeLayer` | IMPLEMENTED | `doc_edit.rs` `move_node` | — |
| Grouping (node groups) | (web has none; folders are library-level) | N-A | — | — |
| Lock / hide flags | `project.tsx` `toggleNodeFlag` | IMPLEMENTED | `doc_edit.rs` `toggle_node_flag` | — |
| Bulk rename | `canvas-node-geometry.ts` `bulkRenameTitles` | MISSING | — | — |
| Node-list type filter | `canvas-node-layer-popover.tsx` | MISSING | — | — |
| **F. Floating toolbars** | | | | |
| Hover toolbar base actions (info, duplicate, delete, font) | `canvas-node-hover-toolbar.tsx` | IMPLEMENTED | `bars.rs`, `floating.slint` | Subset |
| Hover toolbar image tools (crop, mask, bg-removal, OCR, segment, angle, split, upscale, analyze, upload, copy, download, free-resize) | `canvas-image-toolbar-tools.tsx`, `project.tsx` | MISSING | — | All image tooling absent (sibling dialogs scope, but the toolbar entries are core) |
| Hover toolbar video tools (capture frames, download, replace) | `canvas-node-hover-toolbar.tsx`, `project.tsx` `captureVideoNodeFrame` | MISSING | — | — |
| Hover toolbar text tools (generate image, text style popover) | `canvas-node-hover-toolbar.tsx`, `canvas-text-style-popover.tsx` | PARTIAL | `bars.rs` | Font ±2 only; no generate-image, no style popover |
| Hover toolbar layers popover (reorder/lock/hide/bulk rename) | `canvas-node-layer-popover.tsx` | DEVIATION | `bar_events.rs` | Button toggles the node-list panel instead |
| Hover toolbar "show labels" toggle | `canvas-node-hover-toolbar.tsx` | MISSING | — | — |
| Plugin toolbar extras | `use-plugin-host.tsx` `buildNodeToolbarItems` | IMPLEMENTED | `plugin_views.rs` `toolbar_extras` | — |
| Selection toolbar (align/distribute/delete + bounds) | `canvas-selection-toolbar.tsx` | IMPLEMENTED | `bars.rs`, `floating.slint` | — |
| Node info modal (info + JSON) | `canvas-node-hover-toolbar.tsx` `CanvasNodeInfoModal` | IMPLEMENTED | `modals.slint`, `bars.rs` | — |
| Image preview modal | `project.tsx` Modal, `image-stack-preview.tsx` | MISSING | `modals.slint` | Shows "Preview unavailable" placeholder only |
| **G. Undo/redo** | | | | |
| Snapshot {nodes,connections}, 180ms debounce, 50 cap | `hooks/use-canvas-history.ts` | IMPLEMENTED | `core/history.rs` | — |
| Pause history during drags | `project.tsx` `historyPausedRef` | IMPLEMENTED | `handlers.rs` (commit on drag end) | Equivalent behavior |
| **H. Canvas chrome** | | | | |
| Grid modes dots/lines/blank (config-driven) | `open-canvas.tsx`, config `canvasBackgroundMode` | PARTIAL | `paths.rs` `grid_patch` | Dots only; no lines/blank, no setting |
| Grid zoom-adaptive density | `open-canvas.tsx` | IMPLEMENTED | `paths.rs`, `app.rs` | — |
| Minimap (blobs, viewport rect, click/drag to center) | `canvas-mini-map.tsx` | IMPLEMENTED | `minimap.slint`, `panels.rs`, `handlers.rs` | — |
| Rulers (top/left, zoom-adaptive ticks) | `canvas-rulers.tsx` | IMPLEMENTED | `rulers.slint`, `panels.rs` | — |
| Zoom panel (5–500% slider, reset view, minimap toggle, shortcuts) | `canvas-toolbar.tsx` | MISSING | `toolbar.slint` | Zoom label is inert; no slider/reset/shortcuts modal |
| Reset / fit viewport | `use-canvas-document.ts` `resetViewport` | MISSING | — | — |
| Shortcuts help modal | `canvas-toolbar.tsx` | MISSING | — | — |
| Export canvas button | `canvas-toolbar.tsx`, `canvas-export.ts` | DEVIATION | `library_export.rs`, `toolbar.slint` | Path-prompt JSON export vs ZIP+blobs/file-saver |
| Node list/outline panel | `canvas-node-layer-popover.tsx` | PARTIAL | `node-list.slint`, `app.rs` `sync_node_list` | Has rows + reorder + lock/hide; missing filter, bulk rename, layer-count header |
| Left side panel (project list) | `canvas-side-panel.tsx` | IMPLEMENTED | `side-panel.slint` | — |
| Canvas top bar title inline rename | `canvas-top-bar.tsx` | MISSING | `top-bar.slint` | Static title |
| **I. Empty / loading states** | | | | |
| Project load shell (spinner) | `canvas-refresh-shell.tsx` | MISSING | — | Native opens instantly; no refresh shell |
| Node loading state (generating spinner) | `canvas-node.tsx` `LoadingContent` | MISSING | — | — |
| Node error state + retry | `canvas-node.tsx` `ErrorContent` | MISSING | — | — |

### 2. Canvas Panels / Overlays / Modals / Library / Persistence

Status legend (source): **IMPLEMENTED** = full fidelity · **PARTIAL** = subset · **MISSING** = absent · **DEVIATION** = present but structurally different · **N-A** = not present in web either.

#### A. Shell / Top bar

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Global app top nav (logo, workspace tabs, hamburger) | `web\src\components\layout\app-top-nav.tsx`, `layouts\user-layout.tsx` | DEVIATION | `crates\app\ui\components\top-bar.slint` | No global nav; a single TopBar shows on both library + editors; no logo/hamburger. |
| Canvas top bar: side-panel toggle | `canvas-top-bar.tsx` | IMPLEMENTED | `top-bar.slint` | panel-left HoverButton wired to toggle-panel. |
| Canvas top bar: Home/projects button | `canvas-top-bar.tsx` | IMPLEMENTED | `top-bar.slint`, `library.rs` (go_home) | Home flushes save then returns to library. |
| Canvas top bar: inline title edit (dbl-click) | `canvas-top-bar.tsx:79-101` | MISSING | `top-bar.slint` (static Text) | Editor title is read-only; rename only in library. |
| Canvas top bar: workspace tab pills | `canvas-top-bar.tsx:105-141` | IMPLEMENTED | `top-bar.slint`, `library.rs` (select_workspace) | Tabs fall back to library when no project open. |
| Canvas top bar: settings gear | `user-status-actions.tsx` | IMPLEMENTED | `top-bar.slint`, `config_ui.rs` | Opens config modal (providers tab). |
| Canvas top bar: account usage popover | `canvas-account-usage.tsx` | MISSING | — | OpenRouter usage popover absent. |
| Canvas top bar: theme toggle | `app-config-modal.tsx` appearance + `use-theme-store` | DEVIATION | `top-bar.slint` (moon icon) | Native toggles `Theme.dark` directly; web sets theme via config/appearance. |
| App config modal | `layout\app-config-modal.tsx` | PARTIAL | `ui\components\config-modal.slint`, `config_ui.rs` | Native 520 vs web 980 width; only Providers+Models. |
| Config tab: Providers/channels | `config-providers.tsx` | PARTIAL | `config-modal.slint` | Name/baseUrl/key/presets(OpenRouter/DeepSeek/Matilda)/add/remove/models; no per-model edit dialog, no channel ordering. |
| Config tab: Models | `config-models.tsx` | PARTIAL | `config-modal.slint` | Capability cycle + default pick; no model list search. |
| Config tab: Appearance (language, theme, canvas bg) | `app-config-modal.tsx:50-90` | MISSING | — | No canvasBackgroundMode/locale config surface. |
| Config tab: Generation (image count) | `app-config-modal.tsx:91-109` | MISSING | — | `canvasImageCount` not configurable. |
| Config tab: Local models | `config-local-models.tsx` | MISSING | — | No bg-removal/MobileSAM model manager. |
| Config tab: Local storage | `config-local-storage.tsx` | MISSING | — | No storage usage/inspector. |
| Config tab: Cost | `config-generation-cost.tsx` | MISSING | — | No cost records view. |
| Config tab: Agent audit | `config-agent-audit.tsx` | MISSING | — | No agent audit log. |
| Config tab: Runtime logs | `config-runtime-logs.tsx` | MISSING | — | No runtime log viewer. |
| Config tab: About + shortcuts list | `app-config-modal.tsx:140-172` | MISSING | — | Version/shortcut reference absent. |
| Version/release modal | `layout\version-release-modal.tsx` | MISSING | — | No update check UI. |
| Mobile nav drawer | `layout\mobile-nav-drawer.tsx` | N-A | — | Desktop-only port (intentional). |
| Toasts / notices | `layout\app-providers.tsx` (AntD App) | IMPLEMENTED | `app.slint` (notice), `app.rs` sync_notice | Single dismissible toast, success/error variant. |

#### B. Left side panel & switcher

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Side panel show/hide | `canvas-side-panel.tsx` | IMPLEMENTED | `side-panel.slint`, `app.slint` | 500ms ease animation. |
| Side panel drag-to-resize (220–480) | `canvas-side-panel.tsx:26-44`, `use-canvas-side-panel-store.ts` | MISSING | `side-panel.slint` | Fixed 240px, no resize handle. |
| Side panel width/open persistence | `use-canvas-side-panel-store.ts` (localStorage) | MISSING | — | No persisted panel geometry. |
| Project switcher list (open project) | `canvas-switcher.tsx` | PARTIAL | `side-panel.slint`, `library.rs` side_rows | Native lists all projects; open works. |
| Switcher "New" project button | `canvas-switcher.tsx:75-85` | MISSING | — | No create from panel. |
| Switcher "Load"/import button | `canvas-switcher.tsx:86-96` | MISSING | — | Import only from library header. |
| Switcher drag-reorder projects | `canvas-switcher.tsx:98-181` | MISSING | — | No reorder in panel. |
| Switcher per-row arm/confirm delete | `canvas-switcher.tsx:162-172` | MISSING | — | Delete only in library. |
| Switcher node-count per row | `canvas-switcher.tsx:158-160` | MISSING | — | Native row shows title only. |

#### C. Bottom dock toolbar

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Dock toolbar shell (h-14, rounded, centered) | `canvas-toolbar.tsx` | IMPLEMENTED | `toolbar.slint` (DockBar) | Floating bottom-center dock. |
| Export button (+ spinner) | `canvas-toolbar.tsx:116-135` | PARTIAL | `toolbar.slint`, `library_export.rs` | Wired to export modal; no in-button spinner. |
| Tool toggle (select/pan) | `canvas-toolbar.tsx:137-139` | IMPLEMENTED | `toolbar.slint`, `shortcuts.rs` toggle_tool | — |
| Undo / Redo | `canvas-toolbar.tsx:140-145` | IMPLEMENTED | `toolbar.slint`, `shortcuts.rs` | Disabled state from can-undo/redo. |
| Input group popover (Assets/ImageStack) | `canvas-toolbar.tsx:147-165` | DEVIATION | `create-menu.slint`, `plugin_views.rs` | Replaced by one flat create menu of all types. |
| Prompt group popover (Prompt/VideoPrompt) | `canvas-toolbar.tsx:166-184` | DEVIATION | `create-menu.slint` | Same merged create menu. |
| Generator group popover (Image/Video gen) | `canvas-toolbar.tsx:185-203` | DEVIATION | `create-menu.slint` | Same merged create menu. |
| Extensions popover (plugin node types) | `canvas-toolbar.tsx:204-325` | PARTIAL | `create-menu.slint`, `plugin_views.rs` create_rows | Plugin types listed in create menu, no separate grouped popover. |
| Toolbar Tooltip labels | `canvas-toolbar.tsx` DockTip | MISSING | `toolbar.slint` | No hover tooltips. |
| Zoom popover (slider 5–500%, reset, minimap, shortcuts) | `canvas-toolbar.tsx:327-367` | MISSING | `toolbar.slint` | Zoom label is inert; no slider/reset/shortcuts. |
| Node-list toggle | `canvas-toolbar.tsx:243-256` | IMPLEMENTED | `toolbar.slint`, `shortcuts.rs` | — |
| Minimap toggle | `canvas-toolbar.tsx` zoom panel action | DEVIATION | `toolbar.slint` | Native has a compass button (web minimap toggles from zoom panel). |
| Delete-selected (conditional) | `canvas-toolbar.tsx:257-264` | IMPLEMENTED | `toolbar.slint`, `shortcuts.rs` | Visible when has-selection. |
| Plugin-manager button (puzzle) | web: extensions only | DEVIATION | `toolbar.slint` (puzzle) | Native adds a direct plugin-manager button. |
| Shortcuts/help modal | `canvas-toolbar.tsx:369-378` | MISSING | — | No shortcuts modal in canvas. |
| Toolbar popovers close on outside/Escape | `canvas-toolbar.tsx:91-110` | IMPLEMENTED | `bars.rs` close_overlays | Escape precedence handled. |

#### D. Dock / panel show-hide menus (studios)

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Dock layout model (edges/groups/split) | `dock\dock-layout.ts` | DEVIATION | `ui\components\pixel.slint` (StudioMenu dock) | Native dock menus are simpler; no groups/tabs/split/sizes. |
| DockWindowMenu (toggle + reset) | `dock\dock-window-menu.tsx` | PARTIAL | `pixel.slint`, `image.slint`, `texture.slint` dock menus | Toggle panels; no reset/move. |
| Dock panel drag-reorder / move to edge | `dock\dock-panel.tsx` (DockArea) | MISSING | — | No panel drag. |
| Dock tabs (stacked panels) | `dock\dock-tabs.tsx` | MISSING | — | No dock tabs. |
| Dock size/split persistence | `dock-layout.ts:84-122` | MISSING | — | No dock geometry persistence. |

#### E. Node panels (per node kind) & settings popovers

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Composer panel (prompt/video-prompt/text) | `canvas-node-prompt-panel.tsx` | PARTIAL | `node-panel.slint`, `node_panel.rs` | Renders composer+chips+mentions+model+settings+generate; missing inline contentEditable chips, text-style popover, "expand editor" modal. |
| Config panel (image gen / video gen) | `canvas-config-node-panel.tsx` | PARTIAL | `node-panel.slint`, `node_panel.rs` | Input summary + model + settings + generate/stop; no per-input badges detail. |
| Prompt node inline tag editor | `prompt-node-panel.tsx` | DEVIATION | `node-panel.slint` | Prompt node uses generic composer instead of tag editor. |
| Video-prompt frame slots (first/last/ref) | `nodes\video-prompt-node-content.tsx` | PARTIAL | `node-card.slint` (video-prompt row) | Slot UI reduced to a row, no per-slot frame handling. |
| Assets node browser | `nodes\assets-node-content.tsx` | MISSING | — | No asset folder browser content. |
| Image stack node content (thumbnails) | `nodes\image-stack-node-content.tsx` | MISSING | — | No stack thumbnail content/view-all/split. |
| Prompt node content (rendered text) | `nodes\prompt-node-content.tsx` | PARTIAL | `node-card.slint` (prompt body) | Text body only. |
| Image settings popover (quality/size/res/count) | `canvas-image-settings-popover.tsx`, `components\image-settings-panel.tsx` | IMPLEMENTED | `panel-popovers.slint` (ImageSettingsPopover) | Quality/size(10 ratios)/resolution/count. |
| Video settings popover (res/seconds/audio/watermark) | `canvas-video-settings-popover.tsx`, `video-settings-panel.tsx` | IMPLEMENTED | `panel-popovers.slint` (VideoSettingsPopover) | — |
| Text settings popover (reasoning/count) | `canvas-text-settings-popover.tsx`, `text-settings-panel.tsx` | IMPLEMENTED | `panel-popovers.slint` (TextSettingsPopover) | — |
| Text style popover (bold/italic/font/size/color) | `canvas-text-style-popover.tsx` | MISSING | — | No text style controls. |
| Model picker (Select w/ search) | `components\model-picker.tsx` | PARTIAL | `panel-popovers.slint` (ModelListCard) | Static list from presets; no config-driven models/search. |
| Smart-canvas board settings popover (ratio/res/bg) | `smart-canvas-settings-popover.tsx` | PARTIAL | `image.slint` board controls | Board ratio/res/bg present in image studio; no unified popover. |
| Prompt chip input (inline contentEditable chips) | `canvas-prompt-chip-input.tsx` | PARTIAL | `floating.slint` (RefChip), `node-panel.slint` | Chips rendered as a strip above the field, not inline. |
| Resource mention textarea (`@` menu) | `canvas-resource-mention-textarea.tsx` | IMPLEMENTED | `mention.rs`, `floating.slint` (MentionMenu) | `@` menu rows + insert. |
| Generate/Stop state | `canvas-node-prompt-panel.tsx` | IMPLEMENTED | `node-panel.slint`, `ai_service.rs` | Running→stop. |

#### F. Node hover toolbar, info modal, layer popover

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Hover bar (keep-alive across gap) | `canvas-node-hover-toolbar.tsx`, `canvas-floating-toolbar.tsx` | IMPLEMENTED | `floating.slint`, `bar_events.rs` | Hover-intent grace matches web. |
| Hover actions: info, duplicate | `canvas-node-hover-toolbar.tsx:140-142` | IMPLEMENTED | `bars.rs` | — |
| Hover actions: font -/+ (text) | `canvas-node-hover-toolbar.tsx:159-160` | IMPLEMENTED | `bars.rs` font_step | ±2 clamp 10..32. |
| Hover action: layers popover | `canvas-node-hover-toolbar.tsx:184-185` | PARTIAL | `bar_events.rs` | Toggles the global node list instead of a per-node layer popover. |
| Hover action: delete | `canvas-node-hover-toolbar.tsx:143,187` | IMPLEMENTED | `bars.rs` | — |
| Image quick tools (upload/download/copy/crop/mask/bg/split/res/analyze/ocr/segment/angle/free-resize/retry) | `canvas-node-hover-toolbar.tsx:144-165`, `canvas-image-toolbar-tools.tsx` | MISSING | — | None of the image quick tools exist. |
| Image quick-tool config + showLabels persistence | `canvas-image-toolbar-tools.tsx:40` (localStorage) | MISSING | — | No quick-tool customization. |
| Video frame capture (first/last/current) | `canvas-node-hover-toolbar.tsx:147-152` | MISSING | — | No video frame capture. |
| Video task query/retry | `canvas-node-hover-toolbar.tsx:129,145` | MISSING | — | No video task re-query. |
| Plugin-contributed toolbar items | `use-plugin-host.tsx:128` (`buildNodeToolbarItems`) | PARTIAL | `plugin_views.rs` toolbar_extras | Interact/generate/reset/palette only. |
| Node info modal (Info/JSON segmented) | `canvas-node-hover-toolbar.tsx:192-261` | IMPLEMENTED | `modals.slint` (NodeInfoModal), `bars.rs` | Rows + redacted JSON. |
| Info modal: image-size + error block | `canvas-node-hover-toolbar.tsx:245-250` | PARTIAL | `bars.rs` info_rows | No formatted image-size row. |

#### G. Selection toolbar, align/distribute

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Selection bounds box (dashed rounded) | `canvas-selection-toolbar.tsx:77-92` | IMPLEMENTED | `floating.slint`, `bars.rs`, `overlay.rs` | Dash 7,5 rx16. |
| Selection bar: align menu (6) | `canvas-selection-toolbar.tsx:13-19` | IMPLEMENTED | `floating.slint`, `bars.rs` align_action | — |
| Selection bar: layout/distribute menu (2) | `canvas-selection-toolbar.tsx:20-21` | IMPLEMENTED | `floating.slint` | Disabled <3 nodes. |
| Selection bar: delete | `canvas-selection-toolbar.tsx:115` | IMPLEMENTED | `floating.slint` | — |
| Align/distribute math | `lib\canvas\alignment.ts` | IMPLEMENTED | `core\src\alignment.rs` | — |

#### H. Minimap, rulers, node list/outline

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Minimap (240×160, blobs, viewport rect) | `canvas-mini-map.tsx` | IMPLEMENTED | `minimap.slint`, `panels.rs` | Click/drag recenters viewport. |
| Minimap node colors by type | `canvas-mini-map.tsx:101` | IMPLEMENTED | `theme.slint` minimap-color | — |
| Rulers (top/left, world ticks) | `canvas-rulers.tsx` | IMPLEMENTED | `rulers.slint`, `panels.rs` | Left labels rotated 90° (documented deviation). |
| Node list panel (bottom-center) | `canvas-node-layer-popover.tsx:29-168` | PARTIAL | `node-list.slint` | Has reorder/lock/hide; missing type filter, bulk rename, layer-count header, multi-select checkboxes. |
| Per-node layer popover | `canvas-node-layer-popover.tsx:19-27` | MISSING | — | No per-node layer popover with stack count + bring-forward/send-backward. |
| Background controls (dots/lines/blank) | `app-config-modal.tsx` appearance (`canvasBackgroundMode`) | MISSING | `paths.rs` grid_patch | Canvas grid is fixed dots; no mode switch. |
| Zoom controls (slider 5–500%) | `canvas-toolbar.tsx:327-367` | MISSING | `toolbar.slint` | Wheel zoom works; no slider/reset UI. |
| Reset viewport | `canvas-toolbar.tsx:348` | MISSING | — | No reset-view action. |

#### I. Canvas-level modals / dialogs

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Image preview modal (single) | `project.tsx:2102-2114` | DEVIATION | `modals.slint` (ImagePreviewModal) | Native shows "Preview unavailable" placeholder only. |
| Image stack preview gallery (cover/take-out/remove/retry/download/copy/split-all) | `image-stack-preview.tsx`, `project.tsx:2115` | MISSING | — | No gallery/actions. |
| Crop dialog | `canvas-node-crop-dialog.tsx` | MISSING | — | Not present. |
| Mask-edit dialog (paint/erase/export mask) | `canvas-node-mask-edit-dialog.tsx` | MISSING | — | Not present. |
| Split dialog (rows/cols/lines) | `canvas-node-split-dialog.tsx` | MISSING | — | Not present. |
| Resolution/upscale dialog (local vs AI) | `canvas-node-resolution-dialog.tsx` | MISSING | — | Not present. |
| Image analysis dialog (smart-crop) | `canvas-image-analysis-dialog.tsx` | MISSING | — | Not present. |
| Segment dialog (MobileSAM mask) | `canvas-node-segment-dialog.tsx` | MISSING | — | Not present. |
| Camera-angle dialog | `canvas-node-angle-dialog.tsx` | MISSING | — | Not present. |
| Asset file preview modal | `nodes\asset-file-preview.tsx` | MISSING | — | No file preview. |
| Import dialog (ZIP drag-drop) | `canvas-import-dialog.tsx` | DEVIATION | `library.slint` (ExportImportModal), `library_export.rs` | Native = JSON path prompt; no drag-drop, no ZIP, no blob restore. |
| Plugin manager modal (tabs, enable/disable/uninstall/install) | `canvas-plugin-manager-modal.tsx` | PARTIAL | `plugin-manager.slint`, `plugin_manager.rs` | Records only (no fetch/import/execute); official/local/third-party tabs present. |
| Node double-click opens preview / panel | `project.tsx` node handlers | PARTIAL | `bar_events.rs` node_double_click | Double-click preview for image/stack only; no dialog toggle model. |

#### J. Library (project browser)

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Group sidebar (list, count) | `pages\canvas\index.tsx:80-160` | IMPLEMENTED | `library.slint`, `library.rs` | — |
| Create group | `index.tsx:83` | IMPLEMENTED | `library_ops.rs` library_create_group | — |
| Inline group rename (check/x) | `index.tsx:88-103` | IMPLEMENTED | `library.slint` NameEdit, `library_ops.rs` | — |
| Group delete (arm/confirm, cascades projects) | `index.tsx:52-61,140-153` | IMPLEMENTED | `library.slint`, `library_ops.rs` delete_group | Cascades to project files. |
| Card grid | `index.tsx:188-193` | IMPLEMENTED | `library.slint`, `library.rs` place_cards | Fixed 300×170 cards, max 6 cols. |
| Create project (canvas/pixel/image/texture) | `index.tsx:169`, stores | IMPLEMENTED (canvas) / DEVIATION | `library_ops.rs` | Native adds Pixel/Image/Texture create buttons (web creates these from their own pages). |
| Card: create + open | `canvas-project-row.tsx:37-39` | IMPLEMENTED | `library_ops.rs` library_open_project | Kind-routed open. |
| Card: rename inline | `canvas-project-row.tsx:103-115` | IMPLEMENTED | `library.slint`, `library_ops.rs` | — |
| Card: delete (arm/confirm) | `canvas-project-row.tsx:158-170` | IMPLEMENTED | `library.slint`, `library_ops.rs` | — |
| Card: export | `canvas-project-row.tsx:136-146` | IMPLEMENTED | `library.slint`, `library_export.rs` | JSON envelope, not ZIP. |
| Card: move-to-group dropdown | `canvas-project-row.tsx:118-135` | MISSING | — | No cross-group move. |
| Card: node/connection stats line | `canvas-project-row.tsx:108` | DEVIATION | `library.slint` | Native shows updated time + kind badge instead of node/conn counts. |
| Card: kind badge (CANVAS/PIXEL/IMAGE/TEXTURE) | — | DEVIATION | `library.slint` | Native-only addition (web has no kind badge). |
| Card: select (multi) + selected border | `use-canvas-ui-store.ts` selectedProjectIds | MISSING | — | No multi-select. |
| Card drag reorder / reorderProjects | `canvas-project-row.tsx:44-96` | MISSING | — | No reorder. |
| Empty states (no group / empty group) | `index.tsx:180-201` | IMPLEMENTED | `library.slint` | "No folders yet" / "This folder is empty". |
| Hydration loading state (Spin) | `index.tsx:176-179` | N-A | — | Native loads synchronously from disk. |
| Sort/search/filter projects | — | N-A | — | Not in web canvas library. |
| Thumbnails | — | N-A | — | Web cards have no thumbnail. |
| Duplicate project | — | N-A | — | Not in web. |
| Import JSON (library) | `canvas-import-dialog.tsx` | DEVIATION | `library.slint`, `library_export.rs` | JSON path prompt. |

#### K. Persistence / export / import / clipboard

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Project library schema | `stores\canvas\use-canvas-store.ts` | DEVIATION | `crates\core\src\document.rs`, `store.rs` | Web localforage/IndexedDB; native `library.json` + per-project JSON. |
| Per-project document schema | `types\canvas.ts` CanvasProject | IMPLEMENTED | `core\src\document.rs` | Lossless `CanvasProject` JSON round-trip. |
| Autosave (400ms doc / 500ms viewport debounce) | `use-canvas-store.ts:57-68`, `project.tsx:394-404` | IMPLEMENTED | `save.rs` | Same debounce windows. |
| localStorage config keys (side panel, bg mode, etc.) | `use-canvas-side-panel-store.ts:8-9` et al. | MISSING | — | No panel width/open, no quick-tool config persistence. |
| Cross-tab sync (BroadcastChannel) | `use-canvas-store.ts:59-66` | N-A | — | No multi-window sync. |
| Blob storage (image_files/media_files localforage) | `services\image-storage.ts`, `file-storage.ts` | DEVIATION | embedded base64 / `pixels\*.json` cels | Canvas has no media blobs; image/pixel store bitmaps in-file. |
| Export to disk (JSON envelope) | `lib\canvas\canvas-export.ts`, `types\canvas-export.ts` | PARTIAL | `library_export.rs`, `store.rs` | Native writes single-project envelope with `files: []`; no ZIP/multi-project/blob packaging. |
| Export file-saver / ZIP | `canvas-export.ts` | DEVIATION | `store.rs` write_export | Native "export" = write JSON to typed path. |
| Import (ZIP) validation + blob restore | `canvas-import-dialog.tsx` | PARTIAL | `library_export.rs` | JSON only; validates app/version, re-mints ids, creates group. |
| Import navigates to first project | `canvas-import-dialog.tsx:89` | DEVIATION | `library_export.rs` | Native stays in library with a toast. |
| Storage cleanup on group delete | `pages\canvas\index.tsx:60` cleanupUnusedCanvasImages | N-A | — | No blobs to clean. |
| Clipboard copy/paste nodes (Ctrl+C/V) | `project.tsx:1223-1233` | MISSING | `shortcuts.rs` | No C/V handling; only undo/redo/select-all/delete. |
| System clipboard paste of images | `project.tsx` paste/mount | MISSING | — | No paste-import. |
| Share links | — | N-A | — | Not in web. |
| Agent-mode deep links (`?mode=new/recent`) | `pages\canvas\index.tsx:35-74` | N-A | — | No agent auto-open. |
| Studio PNG export (pixel frame/sheet, image board, texture png/sheet/channel) | `pixel-studio.tsx:757-780`, `image-studio.tsx:1210,1638`, `texture\sheet-export-dialog.tsx` | PARTIAL | `pixel_export.rs`, `image_export.rs`, `texture_export.rs`, `texture-dialogs.slint` | Present via typed path; export modes partly covered. |
| Studio "output to canvas" (image/audio/text) | `studio\studio-output-modal.tsx` | PARTIAL | `texture-dialogs.slint` (TextureOutputModal) | Texture has output modal; image/pixel lack "save to canvas". |
| Export/import modal chrome | `AntD Modal` | DEVIATION | `library.slint` ExportImportModal | Shared path-prompt shell (hint + path + message). |

#### L. Notifications / errors / loading

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Success/error toasts | AntD `App.useApp().message` across canvas/store | IMPLEMENTED | `app.slint` (notice), `app.rs` | Single toast, dismissible. |
| Save-failure surface | web toast in projects | IMPLEMENTED | `save.rs` `set_notice` | "Autosave failed". |
| I/O error surface (export/import/library) | web message | IMPLEMENTED | `library_export.rs`, `store.rs` | Errors stay in modal / toast. |
| Node error state + retry | `canvas-node.tsx`, hover toolbar retry | PARTIAL | `node-card.slint`, `bars.rs` | Node card shows state; retry/query-video not wired. |
| Error details in info modal | `canvas-node-hover-toolbar.tsx:246-250` | IMPLEMENTED | `bars.rs` info_error | — |
| Plugin metadata load error | — | DEVIATION | `library.rs:297-302` | Native adds extras-sidecar failure notice. |
| Library empty/loading states | `index.tsx` Empty/Spin | PARTIAL | `library.slint` | Empty states present; no spinner (sync load). |

### 3. AI Services / Settings / Prompt Input / Shortcuts / Theming / i18n

#### AI SERVICES

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| text-to-image generation | D:\Dev\open-canvas\web\src\services\api\image.ts (`requestGeneration`, L465) | IMPLEMENTED | D:\Dev\open-canvas-desktop\crates\ai\src\image.rs (`request_generation` L351); ai_service.rs `image_outcome` | POST /images, IMAGE_MODEL forced, n clamp 1–10, retry×3; wired to node Generate |
| image edit / reference img2img | image.ts (`requestEdit` L516), canvas-node-generation.ts | IMPLEMENTED | crates\ai\src\image.rs (`request_edit` L364) | POST /images `input_references`, n=1 |
| local edit / inpaint mask UI | D:\Dev\open-canvas\web\src\components\canvas\canvas-node-mask-edit-dialog.tsx | MISSING | - | mask paint → reference image not ported |
| video gen (OpenAI multipart) | D:\Dev\open-canvas\web\src\services\api\video.ts (`createOpenAIVideoTask` L225) | PARTIAL | crates\ai\src\video.rs (`create_openai_task` L262) | native sends images only, mode hardcoded `reference`; no frames/first-last, no video[]/audio[] |
| video gen (OpenRouter) | video.ts (`createOpenRouterVideoTask` L284) | PARTIAL | crates\ai\src\video.rs (`create_openrouter_task` L221) | no `frame_images`/frame slots; no video/audio `referenceKinds` validation |
| video gen (plugin/user-script) | video.ts (`createPluginVideoTask` L169) | MISSING | - | JS script runtime intentionally not ported |
| video task poll + timeout (120×2500ms) | video.ts (`waitForVideoGenerationTask` L97) | IMPLEMENTED | crates\ai\src\video.rs (`wait_for_video_task` L499) | abortable each step |
| video resume / query-by-task-id / download | D:\Dev\open-canvas\web\src\components\canvas\canvas-node-hover-toolbar.tsx (queryVideoTask/downloadVideo) | MISSING | - | no native video task UI |
| text / vision streaming chat | image.ts (`requestImageQuestion` L571, `requestStreamingResponse` L431) | IMPLEMENTED | crates\ai\src\response.rs (`stream_response` L242); ai_service.rs `text_outcome` | SSE parse + on_delta; text nodes wired |
| image analysis / vision Q&A UI | D:\Dev\open-canvas\web\src\components\canvas\canvas-image-analysis-dialog.tsx | PARTIAL | crates\ai\src\response.rs `responses_body` | request path ported; no dedicated vision dialog |
| OCR (extract image text) | D:\Dev\open-canvas\web\src\lib\canvas\canvas-ocr.ts | MISSING | - | no native OCR wiring |
| multi-angle image regenerate | D:\Dev\open-canvas\web\src\components\canvas\canvas-node-angle-dialog.tsx; canvas-generation-helpers.ts `buildAnglePrompt` L149 | MISSING | - | |
| AI resolution enhance (upscale) | D:\Dev\open-canvas\web\src\components\canvas\canvas-node-resolution-dialog.tsx (`kind:"ai"`); project.tsx `aiUpscaleImageNode` L1620 | MISSING | - | |
| background removal (local onnx worker) | D:\Dev\open-canvas\web\src\services\background-removal.ts + .worker.ts | MISSING | - | |
| image segmentation (MobileSAM) | D:\Dev\open-canvas\web\src\lib\image\mobile-sam.ts | MISSING | - | |
| prompt enhance / optimize | (none found) | N-A | - | no web implementation |
| audio / speech generation | (config fields only, no service) | N-A | - | web has no audio service file |
| model plugin scripts (img/text/video) | D:\Dev\open-canvas\web\src\services\api\model-plugin.ts | MISSING | - | `runModelPlugin` sandbox not ported |
| generation cost lookup `/generation` | image.ts (`fetchGenerationCost` L277) | MISSING | - | crates\ai\src\image.rs comment: deferred to studios |
| OpenRouter account/key usage | D:\Dev\open-canvas\web\src\services\api\openrouter-account.ts; components\canvas\canvas-account-usage.tsx | MISSING | - | |
| canvas AI agent (SDK runtime) | D:\Dev\open-canvas\web\src\services\api\canvas-agent.ts; lib\agent\*, components\agent\agent-runtime.tsx | MISSING | - | P5e not started |

**Request shape / streaming / retry / errors**

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| request bodies match web | image.ts / video.ts | IMPLEMENTED | crates\ai\src\image.rs / video.rs | field-for-field (unit tests assert) |
| quality/size/background → request | image.ts `resolveImageRequestOptions`/`resolveSize` | IMPLEMENTED | crates\ai\src\image.rs | |
| system prompt prepend (image) | image.ts `withSystemPrompt` L305 | IMPLEMENTED | crates\ai\src\image.rs `with_system_prompt` | |
| streaming vs polling | image.ts SSE; video.ts poll | IMPLEMENTED | response.rs; video.rs | |
| retry (3 attempts / 2s) | lib\canvas\canvas-generation-helpers.ts `GENERATION_*` | IMPLEMENTED | crates\ai\src\retry.rs `run_with_retry` | |
| concurrency queue (2) | canvas-generation-helpers.ts `createGenerationQueue` | PARTIAL | crates\ai\src\retry.rs `GenerationQueue` | queue exists but ai_service spawns directly, not through it |
| abort / Stop | AbortController | IMPLEMENTED | crates\ai\src\client.rs `Cancel`; ai_service.rs `ai_cancel` | |
| timeouts | web: none (AbortSignal only) | DEVIATION | crates\ai\src\client.rs (30s connect / 300s request) | documented in client.rs |
| error envelope unwrap / status map | D:\Dev\open-canvas\web\src\services\api\api-error.ts | IMPLEMENTED | crates\ai\src\error.rs | |
| localized error strings | i18n `apiErrors.*` | DEVIATION | crates\ai\src\error.rs | literal English (en-US only) |
| raw-response logging / runtime log | D:\Dev\open-canvas\web\src\lib\runtime-log.ts | MISSING | - | no native runtime-log store |
| cost/usage display | D:\Dev\open-canvas\web\src\lib\canvas\generation-cost.ts; stores\use-generation-cost-store.ts; config-generation-cost.tsx | MISSING | - | |
| node status UI (loading/success/error) | types\canvas.ts `NODE_STATUS_*`; canvas-node.tsx | IMPLEMENTED | crates\app\src\ai_service.rs `set_node_state`; core node status | |
| node error detail + retry UI | metadata.errorDetails; canvas-node-hover-toolbar.tsx retry | PARTIAL | ai_service.rs `set_notice` toast | error stored; limited retry/error surface |
| interrupted-generation notice | i18n `canvas.generation.interrupted` | MISSING | - | |

#### PROVIDER / MODEL SETTINGS

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| settings modal tabs (10) | D:\Dev\open-canvas\web\src\components\layout\app-config-modal.tsx | PARTIAL | crates\app\src\config_ui.rs `ConfigTab`; ui\components\config-modal.slint | native ships only Providers + Models |
| appearance tab (language/theme/canvas background) | app-config-modal.tsx L51-89 | MISSING | - | |
| generation tab (canvasImageCount) | app-config-modal.tsx L91-109 | MISSING | - | |
| local-models tab | config-local-models.tsx | MISSING | - | |
| local-storage tab | config-local-storage.tsx | MISSING | - | |
| cost tab | config-generation-cost.tsx | MISSING | - | |
| agent audit tab | config-agent-audit.tsx | MISSING | - | |
| runtime logs tab | config-runtime-logs.tsx | MISSING | - | |
| about / shortcuts / version | app-config-modal.tsx L140-172 | MISSING | - | |
| channel name / baseUrl / apiKey | D:\Dev\open-canvas\web\src\components\layout\config-providers.tsx | IMPLEMENTED | config_ui.rs `config_channel_field`; config-modal.slint | |
| API key masked (Input.Password) | config-providers.tsx L121 | DEVIATION | config-modal.slint `FieldInput` | key shown in plain text |
| channel preset picker | config-providers.tsx L103-113 | IMPLEMENTED | config_ui.rs `config_channel_preset` | openrouter/deepseek/matilda |
| channel add / remove (min 1) | config-providers.tsx | IMPLEMENTED | config_ui.rs | |
| per-channel model add/remove | config-providers.tsx `AddModelRow` | IMPLEMENTED | config_ui.rs `config_model_add/remove` | |
| model capability assign | config-providers.tsx Select | PARTIAL | config_ui.rs `config_model_capability` | native cycles via button, no dropdown |
| default model per capability (5) | D:\Dev\open-canvas\web\src\components\layout\config-models.tsx | IMPLEMENTED | config_ui.rs `capability_rows` | |
| API key storage | localStorage persisted `open-canvas:ai_config_store` | DEVIATION | crates\app\src\store.rs `config_path`/`save_ai_config` | plaintext JSON in %APPDATA%, sent direct to endpoint (same posture, new location) |
| base URL `/v1` normalization | use-config-store.ts `buildApiUrl` L406 | IMPLEMENTED | crates\ai\src\config.rs `build_api_url` | |
| `channelId::model` encoding | use-config-store.ts `encodeChannelModel` | IMPLEMENTED | crates\ai\src\config.rs `encode_channel_model` | |
| capability guess keywords | use-config-store.ts `guessCapability` L135 | IMPLEMENTED | crates\ai\src\config.rs `guess_capability` | |
| text-only base URL forces text capability | provider-presets.ts `isTextOnlyBaseUrl` | IMPLEMENTED | crates\ai\src\config.rs `is_text_only_base_url` | |
| provider/model catalog | D:\Dev\open-canvas\web\src\lib\provider-presets.ts | IMPLEMENTED | crates\ai\src\config.rs `PROVIDER_PRESETS` | identical 3 presets/models |
| preset model defaults auto-select | use-config-store.ts `applyPresetModelDefaults` | IMPLEMENTED | crates\ai\src\config.rs `normalize` | partial (native simpler) |
| per-node model picker | D:\Dev\open-canvas\web\src\components\model-picker.tsx; canvas-node-prompt-panel.tsx | PARTIAL | crates\app\src\node_panel.rs `models_for` L124 | native list = hardcoded preset per capability, NOT configured channels' namespaced options |
| per-node overrides (quality/size/bg/count/seconds/vquality/audio/watermark/mode/model) | canvas-node-prompt-panel.tsx `buildNodeConfig`; image/video/text-settings-panel/popover | PARTIAL | crates\app\src\ai_service.rs `request_config` L172 | reasoningEffort + textCount NOT applied to request |
| node image settings popover (quality/size/ratio/transparent/count) | canvas-image-settings-popover.tsx + image-settings-panel.tsx | PARTIAL | node_panel.slint | summary label only; full editors reduced |
| node video settings popover (resolution/ratio/seconds/audio/mode) | canvas-video-settings-popover.tsx + video-settings-panel.tsx | PARTIAL | node_panel.slint | |
| node text settings popover (reasoning/count) | canvas-text-settings-popover.tsx + text-settings-panel.tsx | PARTIAL | node_panel.slint | |
| provider connection test | (none) | N-A | - | web has no test button |
| local / ollama / OpenAI-compatible base URL | config-providers.tsx baseUrl field | IMPLEMENTED | config_ui.rs base-url + config.rs `build_api_url` | any OpenAI-compatible endpoint |
| channelMode remote/local | use-config-store.ts (forced `local`) | N-A | crates\ai\src\config.rs `normalize` forces local | both hardcode local |
| audio/speech model fields (voice/format/speed/instructions) | use-config-store.ts L37-42 | MISSING | crates\ai\src\config.rs (fields exist, no UI/service) | config persisted, unusable |
| model icon resolution | model-picker.tsx `resolveModelIcon` | MISSING | - | no provider icons native |

#### PROMPT / CHIP INPUT

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| `@`-mention reference menu | D:\Dev\open-canvas\web\src\components\canvas\canvas-prompt-chip-input.tsx; canvas-resource-mention-textarea.tsx | IMPLEMENTED | crates\app\src\mention.rs | |
| upstream-node reference discovery | canvas-resource-references.ts | IMPLEMENTED | crates\core\src\references.rs `mention_references` | |
| chip rendering (thumbnails) | canvas-prompt-chip-input.tsx `createReferenceChip` | PARTIAL | mention.rs + panel-chip strip | native = label text in field + pill strip; no thumbnail/in-text chip |
| menu keyboard nav (↑↓ / Enter / Esc, scroll window) | canvas-prompt-chip-input.tsx L215-235 | IMPLEMENTED | mention.rs `panel_prompt_key` / `mention_moved` | |
| atomic chip delete (Backspace/Delete) | canvas-prompt-chip-input.tsx `deleteAdjacentReference` | IMPLEMENTED | mention.rs `chip_removed` | |
| candidate search/filter | canvas-prompt-chip-input.tsx `candidates` | IMPLEMENTED | core references `filter_candidates` | |
| drag node → prompt (droppable) | canvas-prompt-chip-input.tsx `onDrop`/`CANVAS_REFERENCE_DRAG_TYPE` | MISSING | - | no Slint drag-drop |
| inline image chip click-to-preview | canvas-prompt-chip-input.tsx `imagePreview` | MISSING | - | |
| text highlight overlay for labels | canvas-resource-mention-textarea.tsx `MentionHighlightText` | MISSING | - | native plain TextInput |
| expanded prompt editor modal | canvas-node-prompt-panel.tsx L145 | MISSING | - | |
| multi-line composer | contentEditable `whitespace-pre-wrap` | PARTIAL | node-panel.slint TextInput | |
| placeholder per mode | i18n `canvas.promptPanel.*` | IMPLEMENTED | node_panel.rs placeholder | |
| references → `metadata.references` write | canvas generation helpers | IMPLEMENTED | mention.rs `write_prompt` | |
| resource labeling / prompt assembly (`【Text N】`, ref prefix) | D:\Dev\open-canvas\web\src\components\canvas\canvas-node-generation.ts | PARTIAL | crates\app\src\ai_service.rs `collect_inputs` | native simple concatenation; no 【Text N】 labels, no upstream video/audio |
| editing-text wrapper (`editTextPrompt`) | use-canvas-generation.ts L442; i18n `canvas.projectPage.editTextPrompt` | PARTIAL | - | native lacks edit-existing-text framing |
| prompt enhancer | (none) | N-A | - | |
| slash commands | (none) | N-A | - | |
| text variables / `{{input}}` substitution | (none) | N-A | - | only i18n interpolation uses braces |

#### GLOBAL KEYBOARD SHORTCUTS

Source of truth: `D:\Dev\open-canvas\web\src\components\layout\app-config-modal.tsx` (About tab list) + `pages\canvas\project.tsx` `handleKeyDown` (L1193) + `components\canvas\open-canvas.tsx` (L74) + `pixel-studio.tsx` (L710).

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Ctrl / Space temporary Select↔Move | open-canvas.tsx | IMPLEMENTED | crates\app\src\handlers.rs L48 `effective_tool(ctrl, space_held)` | hold-based parity |
| mouse-wheel zoom | open-canvas.tsx `handleWheel` | IMPLEMENTED | handlers.rs `canvas_scroll` | not a key |
| zoom slider precise zoom | canvas-toolbar.tsx | PARTIAL | ui\components\toolbar.slint | |
| Ctrl+Z undo | project.tsx | IMPLEMENTED | crates\app\src\shortcuts.rs | |
| Ctrl+Shift+Z / Ctrl+Y redo | project.tsx | IMPLEMENTED | shortcuts.rs | |
| Ctrl+A select all | project.tsx | IMPLEMENTED | shortcuts.rs | |
| Ctrl+C / Ctrl+V copy-paste nodes + clipboard media/text | project.tsx `copySelectedNodes`/`pasteCopiedNodes`/`pasteSystemClipboard` | MISSING | crates\app\src\doc_edit.rs (only `duplicate_node`) | no clipboard copy/paste |
| Delete / Backspace delete selection | project.tsx | IMPLEMENTED | shortcuts.rs | |
| Esc clear selection / close overlays | project.tsx | IMPLEMENTED | shortcuts.rs `close_overlays` | |
| Shift / Ctrl+click add to selection | project.tsx | IMPLEMENTED | handlers.rs `select_node(shift)`; doc.rs `begin_marquee(additive)` | |
| drag box-select | project.tsx | IMPLEMENTED | handlers.rs `begin_marquee` | |
| drop media → upload | project.tsx upload handler | MISSING | - | |
| Pixel Studio tool hotkeys (B/E/G/…) | pixel-studio.tsx L710 | IMPLEMENTED | crates\app\src\pixel_ops.rs `PixelTool::from_hotkey` | |
| Pixel space-pan | pixel-studio.tsx | IMPLEMENTED | pixel_ops.rs / crates\app\src\pixel.rs | |
| minimap / node-list toggles | canvas-toolbar.tsx | IMPLEMENTED | shortcuts.rs `toggle_minimap`/`toggle_node_list` | button, not key |
| shortcut help list | app-config-modal.tsx About | MISSING | - | |

#### THEMING

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| light/dark toggle | D:\Dev\open-canvas\web\src\stores\use-theme-store.ts; app-config appearance | PARTIAL | ui\theme.slint `dark`; ui\components\top-bar.slint `toggle-theme` (app.slint L772) | native toggles only from top-bar; works instantly |
| theme persistence | use-theme-store.ts `persist` | MISSING | - | native dark not saved across runs |
| system theme following (`prefers-color-scheme`) | (none) | N-A | - | neither follows system |
| custom accent | fixed brand token only | N-A | - | |
| canvas background presets (dots/lines/blank) | config `canvasBackgroundMode`; app-config-modal.tsx L76-86 | MISSING | - | no native mode switch |
| canvas/node/toolbar theme tokens | D:\Dev\open-canvas\web\src\lib\canvas-theme.ts | IMPLEMENTED | ui\theme.slint | literal 1:1 transcribe |
| global tokens + glass | D:\Dev\open-canvas\web\src\styles\globals.css | PARTIAL | ui\theme.slint `glass`/`glass-strong` | no blur (documented deviation) |
| AntD alias/component tokens | D:\Dev\open-canvas\web\src\lib\app-theme.ts | N-A | - | no AntD native |

#### I18N

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| locale resources | D:\Dev\open-canvas\web\src\i18n\locales\en-US.ts (62.7KB), zh-CN.ts (60.6KB) | MISSING | - | native strings inline English |
| runtime language switching | i18n\index.ts `changeAppLocale` | MISSING | - | |
| language selector | app-config-modal.tsx L57-64 (en-US only) | MISSING | - | |
| forced en-US at boot | i18n\index.ts L12 | DEVIATION | crates\ai\src\error.rs / crates\core\src\constants.rs comments | both effectively en-US only |
| RTL | (none) | N-A | - | |

#### ONBOARDING / HELP / TOOLTIPS / EMPTY STATES

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| shortcut help panel | app-config-modal.tsx About | MISSING | - | |
| version / changelog modal | D:\Dev\open-canvas\web\src\components\layout\version-release-modal.tsx | MISSING | - | |
| about tab | app-config-modal.tsx | MISSING | - | |
| tooltips | numerous web components | PARTIAL | ui\components\toolbar.slint DockTip; hover-button.slint | |
| empty states | config-providers `noModels`; node-list `empty` | PARTIAL | config-modal.slint "No models on any provider" | limited |
| onboarding tour | (none) | N-A | - | |

### 4. Image Studio

Source status abbreviations (mapped 1:1 to the canonical enum): IMPL = IMPLEMENTED · PART = PARTIAL · MISS = MISSING · DEV = DEVIATION · N-A = not in web.

#### Document model / board
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| ImageProject board model (w/h/ratio/resolution/bg/bgOpacity/layers/paths/channels/channelVis) | lib/canvas/smart-canvas.ts, stores/use-image-store.ts, types/canvas.ts | IMPL | crates/ps/src/doc.rs | serde 1:1, camelCase, lossless round-trip; `ImageProject`+`SmartCanvasBoard` |
| Layer kinds (image/text/group/pixel/shape/adjustment) | types/canvas.ts:75 | IMPL | crates/ps/src/doc.rs:34 | enum + factories |
| Shape kinds (rectangle/rounded-rectangle/ellipse/polygon/line) | lib/canvas/smart-canvas.ts:38 | IMPL | crates/ps/src/board.rs:16 | rasterizer + SVG path data |
| Board ratio table (9 ratios × 1k/2k/4k) | lib/media-size.ts:95 | IMPL | crates/ps/src/board.rs:71-118 | export-size table |
| Board size auto-grow to fit overflow layers | image-studio.tsx:270-281 | MISS | — | native `image_board_apply` is manual only |
| Board arrange layouts (grid/row/column/feature) | lib/canvas/smart-canvas.ts:265 | IMPL (engine only) | crates/ps/src/board.rs:557 | `arrange_ps_layers` not wired to any UI/menu |
| Agent op reducer (`image-agent-ops.ts` union) | lib/canvas/image-agent-ops.ts | MISS | — | no native command/op surface |
| Persistence (localforage blobs → JSON+base64 PNG) | services/image-storage.ts | IMPL (DEV) | crates/app/src/image.rs:787-828 | native embeds base64 PNG in `images\<id>.json` |

#### Tools (rail) — Web `TOOLS` (16) + zoom buttons
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| move (V) | image-studio.tsx:100 | PART | crates/app/src/image_ops.rs:470 | move + 4-corner uniform scale; **no rotate handle** (web has ROTATE_SCREEN_OFFSET) |
| hand (H) | image-studio.tsx:101 | IMPL | image_ops.rs:160 | + space-hold pan |
| pen (P) | image-studio.tsx:102 | IMPL | image_ops.rs:811 | anchors + mirrored handles |
| direct-select (Shift+P) | image-studio.tsx:103 | IMPL | image_ops.rs:863 | anchor/handle hit+drag |
| marquee (M) | image-studio.tsx:104 | IMPL | image_ops.rs:700 | rect, feather/AA, shift/alt modes |
| ellipse-marquee (Shift+M) | image-studio.tsx:105 | IMPL | image_ops.rs:700 | |
| lasso (L) | image-studio.tsx:106 | IMPL | image_ops.rs:717 | freehand polygon |
| polygon-lasso (Shift+L) | image-studio.tsx:107 | IMPL | image_ops.rs:193/727 | Enter/double-click close |
| wand (W) | image-studio.tsx:108 | IMPL | image_ops.rs:746 | tolerance/contiguous/sampleAll |
| quick-select (A) | image-studio.tsx:109 | IMPL | image_ops.rs:769 | round-brush grow, alt=erase |
| brush (B) | image-studio.tsx:110 | PART | image_ops.rs:538,1040 | dab stamps size/hardness/opacity only; **spacing/scatter/angle/roundness/dynamics/texture ignored in stroke** (preview-only) |
| eraser (E) | image-studio.tsx:111 | IMPL | image_ops.rs:319/1040 | destination-out |
| bucket (G) | image-studio.tsx:112 | PART | image_ops.rs:572 | flood fill w/ tolerance/sampleAll; **no gradient-fill mode**; pattern fill only via Patterns panel |
| gradient (Shift+G) | image-studio.tsx:113 | PART | image_ops.rs:607 | linear/radial/reverse/stop-list; **no gradient blend-mode + gradient opacity** from options bar |
| shape (U) | image-studio.tsx:114 | PART | image_ops.rs:661 | drag-create 5 kinds; fill OR stroke (not both w/ separate colors); no post-create edit |
| eyedropper (I) | image-studio.tsx:115 | IMPL | image_ops.rs:173/519 | samples composite |
| zoom in / out rail buttons | image-studio.tsx:1664-1669 | IMPL | image_actions.rs:72 | |
| crop tool / text tool / clone / smudge / blur tool / dodge-burn / liquify brush / zoom tool | — | N-A | — | absent in web; liquify is a filter, text via layers panel |
| tool hotkeys (`TOOL_HOTKEYS`, `[`/`]` size, Space, Enter, Esc, Ctrl+A/D/Shift+I/Z/Y) | image-studio.tsx:408-497 | IMPL | image_ops.rs:962-1023 | Ctrl+T/Ctrl+Shift+T/Ctrl+F **not bound** (menus only) |

#### Tool options bars (contextual)
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| brush size / hardness / opacity | image-studio.tsx:1555-1566 | IMPL | image_actions.rs:147 / image_render.rs:847 | |
| selection feather + antialias | image-studio.tsx:1567-1568 | IMPL | image_actions.rs:137 | |
| wand tolerance / contiguous / sampleAll | image-studio.tsx:1569-1575 | IMPL | image_actions.rs:135-136 | |
| bucket tolerance | image-studio.tsx:1576 | IMPL | image_actions.rs:152 | |
| gradient type / mode / opacity / stop / reverse | image-studio.tsx:1577-1585 | PART | image_actions.rs:138-139 | type(linear/radial)+reverse only; **mode+opacity+stop missing** |
| shape type / fill / stroke / strokeWidth | image-studio.tsx:1586-1593 | PART | image_actions.rs:154/165 | type+strokeWidth; **single fill/stroke toggle + fg color**, no separate stroke color |
| color picker in options bar | image-studio.tsx:1594/1614 | IMPL | image_actions.rs:658 | |
| transform mode segmented (free/skew/distort/perspective/warp) | image-studio.tsx:1604-1611 | MISS | image_menu.rs:305 | menu arms `transform_mode` but **no UI segment and no interactive mesh handles** |
| active-path / pattern name + "clear pattern" | image-studio.tsx:1595-1603 | PART | image_render.rs:610 | pattern-clear exists; active-path label not surfaced |

#### Layer system
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Create image / text / pixel / shape / adjustment layers | ps-layers-panel.tsx:64-83 | IMPL | image_actions.rs:301-321 | image via Assets panel; text via layers panel |
| Duplicate | ps-layers-panel.tsx:140-152 | IMPL | image_actions.rs:323 | deep bitmap copy |
| Delete | ps-layers-panel.tsx:153 | IMPL | image_actions.rs:352 | |
| Group / Ungroup | ps-layers-panel.tsx:124-139 | IMPL | image_actions.rs:368-383 | one level only (web too) |
| Reorder up / down | ps-layers-panel.tsx:156-161 | IMPL | image_actions.rs:358-367 | |
| Reorder by drag-drop (before/after/into) | ps-layers-panel.tsx:97-111/244-254 | MISS | — | `move_ps_layer_to` exists in ps, **no Slint drag/drop** |
| Visibility toggle | ps-layers-panel.tsx:275-286 | IMPL | image_actions.rs:384 | |
| Lock toggle | ps-layers-panel.tsx:339-351 | IMPL | image_actions.rs:395 | |
| Rename (inline double-click) | ps-layers-panel.tsx:317-338 | IMPL | image_actions.rs:406-427 | |
| Group collapse/expand | ps-layers-panel.tsx:256-274 | MISS | — | no collapse state |
| Layer opacity | ps-layers-panel.tsx:179-191 | IMPL | image_actions.rs:260 | |
| Layer blend mode select | ps-layers-panel.tsx:168-178 | IMPL | image_actions.rs:271 | 16 ids |
| Layer thumbnail / mask thumb / fx badge | ps-layers-panel.tsx:384-400 | PART | image_render.rs:909 | row has kind icon only; **no bitmap thumbnail, no mask thumb** |
| Merge down / Flatten / Merge visible | — | N-A | — | absent in web |
| Clipping mask / vector mask | — | N-A | — | absent in web |
| Smart objects / embedded images | — | N-A | — | absent in web |

#### Blend modes (blend-modes.ts:7, 16 ids — all IMPL in crates/ps/src/blend.rs with W3C formulas)
`normal, multiply, screen, overlay, darken, lighten, color-dodge, color-burn, hard-light, soft-light, difference, exclusion, hue, saturation, color, luminosity` — **IMPL** (blend.rs:7-24, non-separable HSL handled). Native: `crates/ps/src/blend.rs`. No gap.

#### Layer styles / fx (ps-fx-panel.tsx, 10 kinds — all IMPL in crates/ps/src/layer_styles.rs + UI)
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Stroke (size/position/color/opacity/blendMode) | ps-fx-panel.tsx:42-48 | IMPL | image_editor.rs:347, layer_styles.rs | blendMode param editable via editor |
| Drop Shadow (color/opacity/angle/distance/spread/size/blendMode) | ps-fx-panel.tsx:49-57 | IMPL | image_editor.rs:353 | |
| Inner Shadow | ps-fx-panel.tsx:58-66 | IMPL | image_editor.rs:353 | |
| Outer Glow | ps-fx-panel.tsx:67-73 | IMPL | image_editor.rs:361 | |
| Inner Glow | ps-fx-panel.tsx:74-80 | IMPL | image_editor.rs:361 | |
| Bevel (style/direction/depth/size/soften/angle/highlight/shadow) | ps-fx-panel.tsx:81-92 | PART | image_editor.rs:367 | **`style`(inner/emboss/outer) + `direction`(up/down) selects missing**; highlight/shadow colors not exposed |
| Satin | ps-fx-panel.tsx:93-100 | IMPL | image_editor.rs:375 | |
| Color Overlay | ps-fx-panel.tsx:101-105 | IMPL | image_editor.rs:382 | |
| Gradient Overlay (stops/angle/scale/reverse/opacity/blendMode) | ps-fx-panel.tsx:106-113 | IMPL | image_editor.rs:386 | stop editor present |
| Pattern Overlay (pattern/patternKey/scale/angle/opacity/blendMode) | ps-fx-panel.tsx:114-121 | PART | image_editor.rs:403 | **no saved-pattern (`patternKey`) picker**; procedural kinds only |
| fx add/enable/remove/edit wiring | ps-fx-panel.tsx | IMPL | image_actions.rs:428-507, image_editor.rs:878 | |
| Pattern kinds (checker/dots/stripes/crosshatch) | ps-layer-styles.ts:23 | IMPL | layer_styles.rs:30 | |

#### Masks
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Add layer mask | ps-properties-panel.tsx:140-142 (onAddMask) | MISS | — | no Slint callback/action |
| Apply layer mask (bake) | ps-properties-panel.tsx:37-46/132 | MISS | — | `psBakeMask` not ported |
| Delete layer mask | ps-properties-panel.tsx:135-137 | MISS | — | |
| Paint on mask target / mask-render view | image-studio.tsx:239-252, ps-layers-panel.tsx:300-315 | MISS | — | compositor honours `mask_storage_key` (compositor.rs:224) but **no UI to create/select/view a mask** |

#### Adjustments (ps-adjustments.ts / ps-adjustment-editor.tsx — 15 types)
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Adjustments panel grid (8-col chips) | ps-adjustments-panel.tsx | IMPL | image_editor.rs:20, image_render.rs:650 | |
| brightness-contrast (brightness/contrast) | ps-adjustment-editor.tsx | IMPL | adjustments.rs:56, image_editor.rs:170 | |
| levels (channel/inBlack/inWhite/gamma/outBlack/outWhite) | ps-adjustment-editor.tsx | IMPL | image_editor.rs:174 | |
| curves (channel + control points) | ps-adjustment-editor.tsx | IMPL | image_editor.rs:182 | |
| exposure (exposure/offset/gamma) | ps-adjustment-editor.tsx | IMPL | image_editor.rs:196 | |
| vibrance (vibrance/saturation) | ps-adjustment-editor.tsx | IMPL | image_editor.rs:201 | |
| hue-saturation (channel/hue/sat/light) | ps-adjustment-editor.tsx | IMPL | image_editor.rs:205 | |
| color-balance (tone + 3 ch + preserve) | ps-adjustment-editor.tsx | IMPL | image_editor.rs:216 | |
| black-white (6 ranges) | ps-adjustment-editor.tsx | IMPL | image_editor.rs:231 | |
| photo-filter (color/density/preserve) | ps-adjustment-editor.tsx | IMPL | image_editor.rs:240 | |
| channel-mixer (output/monochrome/12 values) | ps-adjustment-editor.tsx | IMPL | image_editor.rs:245 | |
| gradient-map (stops/reverse) | ps-adjustment-editor.tsx | IMPL | image_editor.rs:259 | |
| invert (channel) | ps-adjustment-editor.tsx | IMPL | image_editor.rs:273 | |
| posterize (levels) | ps-adjustment-editor.tsx | IMPL | image_editor.rs:276 | |
| threshold (level) | ps-adjustment-editor.tsx | IMPL | image_editor.rs:279 | |
| selective-color (range/method/36 values) | ps-adjustment-editor.tsx | IMPL | image_editor.rs:282 | |
| Curve editor (add/drag/dbl-click-delete, min 2) | ps-param-editors.tsx:20-80 | IMPL | image_editor.rs:645-708, image_render.rs:690 | SVG polyline path |
| Gradient-stops editor (add/drag/delete/color/position/midpoint) | ps-param-editors.tsx:82-141 | IMPL | image_editor.rs:711-831 | |

#### Filters (ps/filters.ts — 22 filters)
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Filter dialog (320px preview, params, reset, liquify drag, rasterize warning) | ps-filter-dialog.tsx | IMPL | image_filter.rs | apply routes through recorder |
| gaussian-blur | filters.ts:41 | IMPL | filters.rs:457 | |
| box-blur | filters.ts:42 | IMPL | filters.rs:458 | |
| motion-blur (angle/distance) | filters.ts:43 | IMPL | filters.rs:506 | |
| radial-blur (amount/method zoom-spin) | filters.ts:44 | IMPL | filters.rs:507 | |
| tilt-shift (blur/angle/position/width) | filters.ts:45 | IMPL | filters.rs:508 | |
| iris-blur (blur/x/y/radius/feather) | filters.ts:46 | IMPL | filters.rs:509 | |
| sharpen (amount) | filters.ts:47 | IMPL | filters.rs:459 | |
| unsharp-mask (amount/radius/threshold) | filters.ts:48 | IMPL | filters.rs:463 | |
| add-noise (amount/distribution/monochrome) | filters.ts:49 | IMPL (DEV) | filters.rs:769 | deterministic seed vs web `Math.random` (intentional) |
| despeckle | filters.ts:50 | IMPL | filters.rs:479 | |
| dust-scratches (radius/threshold) | filters.ts:51 | IMPL | filters.rs:480 | |
| pixelate (size) | filters.ts:52 | IMPL | filters.rs:498 | |
| crystallize (size) | filters.ts:53 | IMPL (DEV) | filters.rs:502 | deterministic seed |
| emboss (angle/height/amount) | filters.ts:54 | IMPL | filters.rs:511 | |
| find-edges | filters.ts:55 | IMPL | filters.rs:512 | |
| solarize (threshold) | filters.ts:56 | IMPL | filters.rs:483 | |
| clouds (size/seed) | filters.ts:57 | IMPL | filters.rs:513 | |
| lens-flare (brightness/x/y) | filters.ts:58 | IMPL | filters.rs:530 | |
| ripple (amount/size) | filters.ts:59 | IMPL | filters.rs:531 | |
| pinch (amount) | filters.ts:60 | IMPL | filters.rs:542 | |
| spherize (amount/mode) | filters.ts:61 | IMPL | filters.rs:557 | |
| liquify (size/pressure, interactive) | filters.ts:62 | IMPL | filters.rs:407, image_filter.rs:366 | |
| Filter groups (blur/sharpen/noise/pixelate/stylize/distort) | filters.ts:39 | IMPL | filters.rs:41 | |
| Repeat Last Filter (Ctrl+F / menu) | ps-menus.tsx:129 | IMPL | image_filter.rs:475-487 | |

#### Selection
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Rect/ellipse/lasso/polygon/wand/quick-select ops | image-studio.tsx | IMPL | image_ops.rs, crates/ps/src/selection.rs | |
| Combine modes (replace/add/subtract/intersect) | image-studio.tsx:148 | IMPL | image_ops.rs:150 | |
| Feather | ps-menus.tsx:106 | IMPL | select → `selection_feather` | |
| Invert | ps-menus.tsx:104 | IMPL | image_actions.rs:730 | |
| Select All / Deselect | ps-menus.tsx:102-103 | IMPL | image_actions.rs:709-727 | |
| Expand / Contract | — | N-A | — | absent in web |
| Marching-ants overlay | ps-overlays.tsx:28 | IMPL | image_render.rs:201 | static dashes (no animated phase) — DEV |
| Save/load selection via channels | ps-channels-panel.tsx:116-123 | IMPL | image_channels.rs:187-223 | |

#### Channels
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| RGB/R/G/B rows + visibility + active view | ps-channels-panel.tsx:147-203 | IMPL | image_channels.rs:148-166, image_render.rs:552 | |
| Alpha channel list (thumb/rename/load/delete) | ps-channels-panel.tsx:213-241 | IMPL | image_channels.rs:126-236 | |
| Save selection as channel | ps-channels-panel.tsx:116-123 | IMPL | image_channels.rs:187 | |
| Channel preview isolation | ps-channels-panel.tsx:22-60 | IMPL | image_channels.rs:555 | |

#### Paths / pen
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| New / duplicate / delete path | ps-paths-panel.tsx | IMPL | image_channels.rs:246-330 | |
| Path visibility / rename / active | ps-paths-panel.tsx | IMPL | image_channels.rs | |
| Make selection / Fill / Stroke | ps-paths-panel.tsx | IMPL | image_channels.rs:322-390, path_ops.rs | |
| Path SVG thumbnail | ps-paths-panel.tsx | MISS | — | rows show name only (`ImagePathRow` has no thumb) |
| Path paint color | ps-paths-panel.tsx | IMPL | image_channels.rs:376 | uses foreground |

#### Text
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Text layer type + content | ps-properties-panel.tsx:192-199 | IMPL | image_actions.rs:555 | |
| Font size / color | ps-properties-panel.tsx:200-211 | IMPL | image_actions.rs:562-582 | |
| Font family select (11 families) | ps-text-panel.tsx:68 | PART | image_text.rs:25 | model has 11 families; **only one embedded face (Baloo 2) used for all** |
| Glyph rasterization into composite | lib/canvas/ps-text.ts | IMPL | image_text.rs (GlyphRasterizer) | ab_glyph |
| faux bold/italic, underline, strikethrough | ps-text-panel.tsx:77-80 | IMPL (engine) / MISS (UI) | image_text.rs:188-203 | render supported; **no UI controls** |
| tracking / kerning / leading / baseline-shift | ps-text-panel.tsx:83-86 | IMPL (engine) / MISS (UI) | text.rs:203, image_text.rs:328 | |
| text scale X/Y | ps-text-panel.tsx:87-93 | IMPL (engine) / MISS (UI) | text.rs:87, image_text.rs:331 | |
| text case (none/upper/lower/small-caps) | ps-text-panel.tsx:94-99 | IMPL (engine) / MISS (UI) | text.rs:149 | |
| Paragraph (align/indents/space before-after/hyphenate) | ps-text-panel.tsx:101-134 | IMPL (engine) / MISS (UI) | text.rs:168, image_text.rs:101-155 | |
| Text-on-path (`textPathId` select) | ps-text-panel.tsx:136-151 | IMPL (engine) / MISS (UI) | text.rs:222, image_text.rs:206 | |
| Text warp (10 styles + bend/horizontal/vertical) | ps-text-panel.tsx:152-185 | IMPL (engine) / MISS (UI) | text.rs:268-316, image_text.rs:81 | |
| Character/Paragraph tab panel itself | ps-text-panel.tsx | MISS | — | no Slint text panel at all |
| In-canvas live text edit (textarea overlay) | image-studio.tsx (editingTextId) | MISS | — | edit only via properties field |

#### Menus
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Select menu (All/Deselect/Inverse/Feather) | ps-menus.tsx:101-107 | IMPL | image_actions.rs:49 | |
| Edit menu (Free/Skew/Distort/Perspective/Warp/Numeric/Repeat/Clear) | ps-menus.tsx:163-171 | PART | image_actions.rs:60, image_menu.rs:305 | numeric/repeat/clear IMPL; **mesh transform modes arm a no-op** |
| Image menu (Size/Canvas Size/rotate ±90/180/arbitrary/Crop/Trim) | ps-menus.tsx:108-119 | IMPL | image_actions.rs:62, image_channels.rs:536 | |
| Filter menu (Repeat + all filters by group) | ps-menus.tsx:128-135 | IMPL | image_actions.rs:38, image_filter.rs:476 | |
| View menu (Rulers/Grid/Guides/Snap/Clear Guides) | ps-menus.tsx:120-127 | IMPL | image_actions.rs:64-69 | |
| File menu / Export | top bar `onOutput` | IMPL (DEV) | image_actions.rs:51, image_export.rs | native = PNG path-prompt (no native dialog) |
| Dock window menu (toggle/reset panels) | ps-menus, dock-window-menu.tsx | IMPL | image_actions.rs:82-108 | |

#### Modals: Image Size / Canvas Size / Rotate / Numeric Transform
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Image Size (W/H + constrain proportions + resample bitmaps) | ps-menus.tsx:197-208 | IMPL | image_menu.rs:238 | |
| Canvas Size (W/H + 3×3 anchor) | ps-menus.tsx:209-227 | IMPL | image_menu.rs:284 | |
| Rotate arbitrary (−180..180) | ps-menus.tsx:231-250 | IMPL | image_menu.rs:208 | |
| Numeric Transform (dx/dy/scaleX/scaleY/rotation/skewX/skewY) | ps-menus.tsx:252-287 | IMPL | image_menu.rs:38-49,187 | |
| Repeat / Clear transform | ps-menus.tsx:169-170 | IMPL | image_menu.rs:327, image_actions.rs:756 | |

#### Panels
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Layers panel | ps-layers-panel.tsx | IMPL | ui/components/image-layers.slint (ImageLayersPanel) | minus drag-reorder + mask UI (above) |
| Channels panel | ps-channels-panel.tsx | IMPL | image-panels-extra.slint (ImageChannelsPanel) | |
| Paths panel | ps-paths-panel.tsx | IMPL | image-panels-extra.slint (ImagePathsPanel) | no thumb |
| History panel (snapshots, limit 50, jump) | ps-history-panel.tsx | IMPL | image-layers.slint (ImageHistoryPanel), image.rs:318-388 | native snapshots also carry bitmap deltas |
| Actions panel (record/stop/save/play/rename/delete) | ps-actions-panel.tsx | IMPL | image-panels-extra.slint (ImageActionsPanel), image_channels.rs:419 | |
| Properties panel | ps-properties-panel.tsx | PART | image-layers.slint (ImagePropertiesPanel) | name/xyz/wh/rot/opacity/blend/text basic IMPL; **mask ops + shape props (type/fill/stroke/width/radius/sides) missing** |
| Adjustments panel | ps-adjustments-panel.tsx | IMPL | image-panels-extra.slint (ImageAdjustmentsPanel) | |
| Assets / resource pool (search+add) | ps-resource-pool-panel.tsx | PART | image-panels-extra.slint (ImageAssetsPanel), image_assets.rs:1000-1112 | search+add IMPL; **no drag-drop**; sources = other image projects (web = canvas image nodes) |
| Color panel | ps-asset-panels.tsx:324, ps-color-picker.tsx | PART | image-panels.slint (ImageColorPanel), image_actions.rs:610 | fg/bg/swap/reset/swatches + numeric H/S/V + R/G/B + hex; **no SV square / hue strip** |
| Gradients panel (presets + editor + save) | ps-asset-panels.tsx:32 | IMPL | ImageGradientsPanel, image_assets.rs:583-743 | |
| Patterns panel (define from selection/layer, pick, fill, delete) | ps-asset-panels.tsx:166 | IMPL | ImagePatternsPanel, image_assets.rs:753-877 | |
| Brushes panel (preset list, preview, sliders, save) | ps-asset-panels.tsx:262 | IMPL | ImageBrushesPanel, image_assets.rs:886-998 | |
| Dock layout (edges/splitters/tabs/persist) | dock/dock-layout.ts, dock-panel.tsx | DEV | image.rs ImagePanels | native = flat boolean visibility flags + reset, **no resizable/split/tabbed dock** |
| fx panel (layer effects) | ps-fx-panel.tsx | IMPL | ImageFxPanel | |
| Shared per-type editor | ps-adjustment-editor.tsx, ps-param-editors.tsx | IMPL | ImageEditorPanel, image_editor.rs | |

#### Canvas overlays & view
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Checkerboard under transparency | image-studio.tsx:266 | IMPL | image_render.rs:106 | |
| Grid (50px) | ps-overlays.tsx:129 | IMPL | image_render.rs:142 | |
| Rulers (adaptive ticks) | ps-overlays.tsx:70 | IMPL | image_render.rs:430 | |
| Guides (per-board, drag from rulers) | ps-overlays.tsx:144, image-studio.tsx guideRef | PART | image_render.rs:173 | render + clear IMPL; **no drag-to-create guide** |
| Guides drag-to-move | ps-overlays.tsx | MISS | — | |
| Snap lines | ps-overlays.tsx:197 | IMPL | image_ops.rs:432 | |
| Snap to board targets | image-studio.tsx:518-591 | IMPL | image_ops.rs:432 | |
| Draft overlays (rect/ellipse/path/line) | ps-overlays.tsx:173 | IMPL | image_render.rs:255 | |
| Path overlay (anchors + handles) | ps-overlays.tsx:211 | IMPL | image_render.rs:313 | |
| Transform handles (box + rotate handle) | ps-overlays.tsx:266 | PART | image_render.rs:371 | 4 corner handles only; **no rotate handle, no warp mesh (16 pts)** |
| Warp/skew/distort/perspective mesh interaction | ps-transform.ts + image-studio.tsx | MISS | — | ps has `move_transform_handle`/`transform_handles_doc` but unwired |
| Zoom (wheel, fit, actual, min/max) | image-studio.tsx:92-94/355-365 | IMPL | image_ops.rs:35-110 | native ZOOM_MAX 64 vs web 8 |
| Pan | image-studio.tsx | IMPL | image_ops.rs:160 | |

#### Rasterize / import / export
| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Rasterize layer | ps-filter-dialog.tsx + layer-ops rasterizePsLayer | IMPL | image_actions.rs:854, crates/ps/src/layer_ops.rs:189 | text/shape/group → pixel |
| Image import/placement | ps-layers-panel.tsx add-image picker; ps-resource-pool | PART | image_assets.rs:1068-1112 | add from other image projects; **no file-open/import of a new bitmap** |
| PNG export (compose → file) | pages/image/index.tsx (composeSmartCanvas) | IMPL (DEV) | image_export.rs | path-prompt instead of browser download |
| Save-to-canvas / Output modal | components/studio/studio-output-modal.tsx | MISS | — | no output-to-canvas-node flow |
| Panel drag-drop / cross-canvas node assets | ps-resource-pool-panel.tsx | PART | image_assets.rs | library scan cached; no live drag |

### 5. Pixel Studio

**Native verified against:** `N/pixel/src/{doc,raster,shapes,ops,color,history,png,project,lib}.rs`, `N/app/src/{pixel,pixel_ops,pixel_actions,pixel_export,pixel_render,pixel_wire}.rs`, `N/app/ui/components/{pixel,pixel-panels,pixel-layers}.slint`, `N/app/src/{store,library,library_ops}.rs`.

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| **TOOLS** | | | | |
| Pencil (B) | W/components/canvas/workspace/pixel-studio.tsx:612,508 | IMPLEMENTED | N/app/src/pixel_ops.rs:208,377 | brush block + Bresenham drag |
| Eraser (E) | pixel-studio.tsx:509,208 | IMPLEMENTED | pixel_ops.rs:208,379 | same path, alpha 0 |
| Bucket / Fill (G) | pixel-studio.tsx:604,605 | IMPLEMENTED | pixel_ops.rs:198 | floodFill + tolerance, commits immediately |
| Line (L) | pixel-studio.tsx:683,525 | IMPLEMENTED | pixel_ops.rs:342,551 | live draft then commit |
| Rectangle (U) | pixel-studio.tsx:525,527 | IMPLEMENTED | pixel_ops.rs:568 | filled flag, +1 inclusive |
| Ellipse (O) | pixel-studio.tsx:528 | IMPLEMENTED | pixel_ops.rs:577 | midpoint ellipse, radii=|b−a| |
| Eyedropper (I) | pixel-studio.tsx:582,496 | IMPLEMENTED | pixel_ops.rs:158,357 | top-most visible alpha |
| Select marquee (M) | pixel-studio.tsx:587,644 | IMPLEMENTED | pixel_ops.rs:164,279 | no bitmap effect |
| Move (V) | pixel-studio.tsx:593,651 | IMPLEMENTED | pixel_ops.rs:174,290 | requires selection |
| Hand / pan (H) | pixel-studio.tsx:571 | IMPLEMENTED | pixel_ops.rs:144 | |
| Tool hotkey table (B/E/G/L/U/O/I/M/V/H) | W/components/canvas/workspace/pixel-studio-shared.ts:26 | IMPLEMENTED | N/app/src/pixel.rs:25 | |
| Dither / pattern tool | — | N-A | — | not in web |
| Spray tool | — | N-A | — | not in web |
| Shading tool | — | N-A | — | not in web |
| **TOOL OPTIONS** | | | | |
| Brush size 1..32 | pixel-studio.tsx:981 | IMPLEMENTED | N/app/ui/components/pixel-panels.slint:350 | |
| Tolerance 0..255 | pixel-studio.tsx:986 | IMPLEMENTED | pixel-panels.slint:379 | used by bucket |
| Shape fill (Filled) | pixel-studio.tsx:989 | IMPLEMENTED | pixel-panels.slint:398 | rect/ellipse only |
| Symmetry Mirror X / Mirror Y | pixel-studio.tsx:997,519-538 | IMPLEMENTED | pixel-panels.slint:416; pixel_ops.rs:387,598 | stamp+shape mirrored |
| Pixel grid toggle | pixel-studio.tsx:1004 | IMPLEMENTED | pixel-panels.slint:432 | |
| Per-tool opacity | — | N-A | — | only layer opacity exists |
| Pixel-perfect mode | — | N-A | — | not in web |
| Symmetry axes count/position | — | N-A | — | only center mirror |
| **DRAWING MODEL / COLOR / PALETTE** | | | | |
| Pixel doc model + pure reducer | W/lib/canvas/pixel/document.ts | IMPLEMENTED | N/pixel/src/doc.rs | 1:1 (layers[0]=bottom, min-1 guards) |
| Default 16-color palette | W/types/canvas.ts:117 | IMPLEMENTED | N/pixel/src/doc.rs:20 | |
| Palette add current colour (+) | pixel-studio.tsx:1019 | IMPLEMENTED | N/app/src/pixel_actions.rs:300 | dedup by normalized hex |
| Palette remove swatch (right-click) | pixel-studio.tsx:1032 | IMPLEMENTED | pixel_actions.rs:319 | keeps ≥1 |
| Palette import / export (.gpl/.pal) | — | N-A | — | not in web |
| Palette reorder | — | N-A | — | not in web (both) |
| Colour picker numeric HSB/RGB/hex | W/components/canvas/workspace/ps-color-picker.tsx:104-127 | IMPLEMENTED | N/app/src/pixel_actions.rs:330; pixel-panels.slint:515-587 | |
| Colour picker SV square + hue slider | ps-color-picker.tsx:73-94 | MISSING | — | native has only numeric fields + swatch |
| Foreground/background pair + swap/reset | ps-color-picker.tsx:138 (unused by pixel) | N-A | — | pixel passes no bg; single colour |
| Dithering | — | N-A | — | not in web |
| Doc background (transparent/white/black/custom) | pixel-studio.tsx:1115 | IMPLEMENTED | pixel_actions.rs:370; pixel-layers.slint:374 | native custom = hex field only |
| **LAYERS** | | | | |
| Add layer | pixel-studio.tsx:1049 | IMPLEMENTED | pixel_actions.rs:154 | |
| Delete layer | pixel-studio.tsx:1050 | IMPLEMENTED | pixel_actions.rs:161 | min-1 guard |
| Visibility toggle | pixel-studio.tsx:1055 | IMPLEMENTED | pixel-layers.slint:77 | |
| Reorder up / down | pixel-studio.tsx:1059 | IMPLEMENTED | pixel-layers.slint:91 | |
| Active-layer opacity 0..100 | pixel-studio.tsx:1068 | IMPLEMENTED | pixel-layers.slint:128 | |
| Blend mode (normal/multiply/screen/overlay/add) | pixel-studio.tsx:1072 | IMPLEMENTED | pixel-layers.slint:157 | |
| Reversed list (top layer first) | pixel-studio.tsx:1053 | IMPLEMENTED | N/app/src/pixel_render.rs:388 | |
| Duplicate layer | — | N-A | — | not in web |
| Merge layers | — | N-A | — | not in web |
| Rename layer | — | N-A | reducer `LayerPatch.name` only | no UI in web or native |
| Lock layer | — | N-A | — | not in web |
| **FRAMES & ANIMATION** | | | | |
| Add / Delete / Duplicate frame | pixel-studio.tsx:1086 | IMPLEMENTED | N/app/src/pixel_actions.rs:238 | |
| Frame chips (select) | pixel-studio.tsx:1092 | IMPLEMENTED | pixel-layers.slint:264 | |
| FPS input 1..60 | pixel-studio.tsx:1084 | IMPLEMENTED | pixel-layers.slint:222 | stored, display-only (not playback) |
| Frame duration 20..5000 ms | pixel-studio.tsx:1100 | IMPLEMENTED | pixel-layers.slint:295 | |
| Play / Pause loop (per-frame ms, max 20) | pixel-studio.tsx:248,1231 | IMPLEMENTED | N/app/src/pixel_ops.rs:489,516 | |
| Frame reorder | reducer only | N-A | reducer `move_frame` only | no UI in web or native |
| Onion skin | — | N-A | — | not in web |
| Timeline (per-layer rows + playhead) | — | N-A | — | chip strip only in both |
| Tweening | — | N-A | — | not in web |
| **CANVAS VIEW** | | | | |
| Auto-fit once per project/size | pixel-studio.tsx:275 | IMPLEMENTED | N/app/src/pixel_ops.rs:70 | |
| Zoom in/out/fit/actual (0.05..64, nextZoom) | pixel-studio.tsx:237; shared.ts:48 | IMPLEMENTED | pixel_ops.rs:92,105 | |
| Wheel zoom at cursor | pixel-studio.tsx:470 | IMPLEMENTED | pixel_ops.rs:117 | no snap-canvas trick in native |
| Pan (middle / hand / space) | pixel-studio.tsx:571 | IMPLEMENTED | pixel_ops.rs:144 | |
| Checkerboard background | pixel-studio.tsx:212,288 | IMPLEMENTED | N/app/src/pixel_render.rs:44 | native path quads, tile-capped |
| Pixel grid overlay at k≥4 | pixel-studio.tsx:320 | IMPLEMENTED | pixel_render.rs:106 | |
| Marching-ants selection | pixel-studio.tsx:336 | IMPLEMENTED | pixel_render.rs:142 | |
| Symmetry guide lines | pixel-studio.tsx:346 | IMPLEMENTED | pixel_render.rs:178 | |
| Gesture snap-canvas optimisation | pixel-studio.tsx:393-468 | DEVIATION | — | native recomposites each frame (perf only) |
| Canvas resize (nearest, all cels) | pixel-studio.tsx:811 | IMPLEMENTED | N/app/src/pixel_actions.rs:401 | |
| Crop | — | N-A | — | not in web |
| Flip / rotate canvas | — | N-A | — | not in web |
| Tile mode | — | N-A | — | not in web |
| Draggable guides / rulers | — | N-A | — | not in web |
| **UNDO / HISTORY** | | | | |
| Undo/redo, 100 cap | W/lib/canvas/pixel/history.ts | IMPLEMENTED | N/pixel/src/history.rs | |
| Undo reverts bitmaps (cel deltas) | pixel-studio.tsx:144 | IMPLEMENTED | N/app/src/pixel.rs:352 | native before/after delta store |
| Clear layer | pixel-studio.tsx:174 | IMPLEMENTED | N/app/src/pixel_ops.rs:450 | |
| Deselect | pixel-studio.tsx:1206 | IMPLEMENTED | pixel_actions.rs:74 | |
| **PANELS / DOCK** | | | | |
| 6 dock panels (tools/palette/layers/frames/properties/preview) | shared.ts:39 | IMPLEMENTED | N/app/src/pixel.rs:39 | |
| Dock drag / resize / split engine | W/components/canvas/dock/dock-panel.tsx | DEVIATION | N/app/ui/components/pixel.slint:586 | native fixed 288/288 cols, show/hide only |
| Dock layout persistence (v4 localStorage) | W/components/canvas/dock/dock-layout.ts | MISSING | — | not persisted in native |
| Dock show/hide + reset menu | pixel-studio.tsx:1217 | IMPLEMENTED | pixel.slint:528 | |
| Top-bar File/Edit/View menus | pixel-studio.tsx:1197-1216 | IMPLEMENTED | pixel.slint:338-582 | |
| Top-bar zoom % / Play / Export | pixel-studio.tsx:1220-1237 | IMPLEMENTED | pixel.slint:368-411 | |
| **PREVIEW / PROPERTIES PANELS** | | | | |
| Preview panel (true-size, pixelated) | pixel-studio.tsx:1154 | IMPLEMENTED | pixel-panels.slint:594 | current frame; animates during playback |
| Preview max-100% fit | pixel-studio.tsx:1157 | IMPLEMENTED | pixel-panels.slint:599 | |
| Properties: size W/H + Apply | pixel-studio.tsx:1108 | IMPLEMENTED | pixel-layers.slint:335 | |
| Properties: background select + picker | pixel-studio.tsx:1115 | PARTIAL | pixel-layers.slint:374 | native custom = hex field, no SV picker |
| Properties: import fit (Fit/Stretch/Original) | pixel-studio.tsx:1133 | IMPLEMENTED | pixel-layers.slint:443 | |
| Properties: import File button | pixel-studio.tsx:1134 | IMPLEMENTED | N/app/src/pixel_export.rs:139 | native typed-path prompt, no native dialog |
| Properties: canvas image-node import list | pixel-studio.tsx:1136 | MISSING | — | native has no image-node sources |
| Properties: Export PNG / Sheet buttons | pixel-studio.tsx:1146 | IMPLEMENTED | pixel-layers.slint:483 | |
| **EXPORT / IMPORT / PERSISTENCE** | | | | |
| Export PNG (active-frame composite) | pixel-studio.tsx:757 | IMPLEMENTED | N/app/src/pixel_export.rs:19 | native writes typed path |
| Export spritesheet (horizontal) | pixel-studio.tsx:764 | IMPLEMENTED | pixel_export.rs:31 | |
| Spritesheet layout options (cols/padding/grid) | — | N-A | — | web is horizontal-only |
| Spritesheet metadata JSON | — | N-A | — | not in web |
| Animated GIF export | — | N-A | — | not in web |
| Per-frame export | via PNG active frame | IMPLEMENTED | pixel_export.rs | |
| Export scale / upscale | dead `doExport(scale)` param | N-A | — | unwired in both |
| Import image → pixels (fit modes) | pixel-studio.tsx:786; W/lib/canvas/pixel/io.ts | IMPLEMENTED | N/pixel/src/png.rs:96 | native decodes PNG/JPEG |
| Import from canvas image node | pixel-studio.tsx:806 | MISSING | — | |
| Project persistence | W/stores/use-pixel-store.ts (localforage blob store) | DEVIATION | N/app/src/pixel.rs:479; store.rs:252 | native `pixels\<id>.json` + base64 PNG |
| Project import/export envelope | web library export (canvas only) | PARTIAL | N/app/src/library_export.rs | pixel projects not in envelope (both) |
| Cel storage indirection (`storageKey`) | W/services/image-storage.ts | DEVIATION | N/pixel/src/project.rs | native in-memory `BTreeMap` + embedded base64 |
| **KEYBOARD SHORTCUTS** | | | | |
| Tool hotkeys B/E/G/L/U/O/I/M/V/H | pixel-studio.tsx:728 | IMPLEMENTED | N/app/src/pixel_ops.rs:458 | |
| Space = temporary pan | pixel-studio.tsx:712 | IMPLEMENTED | pixel_ops.rs:459 | |
| Ctrl/Cmd+Z, Ctrl/Cmd+Shift+Z, Ctrl/Cmd+Y | pixel-studio.tsx:717 | IMPLEMENTED | pixel_ops.rs:464 | |
| Ignore shortcuts while typing | pixel-studio.tsx:709 | PARTIAL | pixel.slint:904 (FocusScope) | native relies on focus, no explicit input guard |
| Escape → deselect / cancel | — | N-A | — | not in web |
| Zoom keyboard shortcuts | — (menu-only) | N-A | — | not in web |
| **AGENT / HEADLESS BRIDGE** | | | | |
| Agent namespace (pixel op schema + applyOps) | W/lib/canvas/pixel-agent-ops.ts; pixel-studio.tsx:930 | MISSING | — | native has no agent bridge for pixel |
| Headless op runner (no UI) | W/lib/canvas/pixel/headless.ts | MISSING | — | |
| Screenshot provider | pixel-studio.tsx:945 | MISSING | — | |
| **LIBRARY / PROJECT LIFECYCLE** *(outside workspace scope)* | | | | |
| Pixel project create | W/pages/pixel/index.tsx:107 | PARTIAL | N/app/src/library_ops.rs:184 | native 64×64 only; no size presets |
| Size presets (16…4K, custom) | pages/pixel/index.tsx:11 | MISSING | — | |
| Pixel groups (create/rename/delete/move) | pages/pixel/index.tsx:87-205 | MISSING | — | native reuses shared canvas groups |
| Project rename / delete in library | pages/pixel/index.tsx:281 | IMPLEMENTED | N/app/src/library_ops.rs / library.rs | shared library path works |

### 6. Texture Studio

**Corrections vs the brief / research note (verified by the source report):** the web has **117 types but only 3 UI categories** (`Basic` id≤199, `Pattern` 200–260, `FX` ≥261; the "8 categories" are the native renderer's internal modules, not a web taxonomy); the 25 unported types are **24 category-FX + 1 category-Basic (`AnalogGlitch`, id 110)**; native has **14** post effects (not 17); web has **no per-output PBR channel config** (only a generic channel-pack), **no per-layer mask** (mask is only a blend mode), and **every `ParamSpec` is a numeric slider** (no color/bool/select/vector2 params).

Status: IMPLEMENTED / PARTIAL / MISSING / DEVIATION / N-A.

#### GENERATOR TYPE REGISTRY

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Type list: 117 names/ids/order | `D:\Dev\open-canvas\web\src\lib\texture\registry.ts` (TYPE_ORDER/TYPE_ID) | IMPLEMENTED | `D:\Dev\open-canvas-desktop\crates\texture\src\registry.rs`, `registry_data.rs` | 117 mirrored; test asserts 117 unique ids (domain.rs) |
| Param specs: 407 numeric specs | `registry.ts` (PARAM_SPECS) | IMPLEMENTED | `registry_data.rs`; `registry.rs::param_specs/default_params` | test asserts 407 |
| **Type render coverage** | `shaders\texture.ts` (`typeXxx` + `getVal0-3`) | **PARTIAL (92/117)** | `renderer\types\mod.rs` + 8 modules | 25 ids → `None` → layer value 0 (blank/transparent); no UI warning |
| Missing types (25) | — | MISSING | — | Crescent, EnergyRing, AuraRing, Pulse, Glare, StarFlare, Burst, ImpactLines, VoronoiFluid, PolarDots, DistortionWave, ToxicCloud, GeoRelief, Crystal, AbsNoise, PlasmaV2, GrungeV2, Twirl, CosmicPortal, Wormhole, AnalogGlitch, CyberBlock, LaserBeam, GlitchBlock, SparkBurst |
| Category mapping (3-way) | `type-browser.tsx:31-37` | IMPLEMENTED | `registry.rs::category` | same id ranges |
| 8-module taxonomy | N-A (web has none) | N-A | `renderer\types\{basic,shapes,noise,fluids,pattern,grid,detail,fx}.rs` | native-only grouping |

#### LAYERS

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Add layer | `layer-list.tsx` | IMPLEMENTED | `texture.rs::texture_layer_action("add")`, `texture-panels.slint` | inserts default `Circle` |
| Delete layer | `layer-list.tsx` | IMPLEMENTED | same | refuses below 1 |
| Reorder up/down | `layer-list.tsx` | IMPLEMENTED | same ("up"/"down") | reversed display parity |
| Duplicate layer | none | N-A (not in web or native) | — | absent both sides |
| Rename layer | `params-panel.tsx` | IMPLEMENTED | `texture.rs::texture_layer_name`, params TextInput | rename lives in params on both |
| Blend modes (normal/add/multiply/screen/mask) | `types.ts:6-9` | IMPLEMENTED | `model.rs` BlendMode; `renderer\layer.rs::blend` | all 5 shader ints |
| Opacity | `params-panel.tsx` | IMPLEMENTED | `texture.rs::texture_opacity`, `layer.rs` premult | 0–100% |
| Visibility | `layer-list.tsx` | IMPLEMENTED | `texture.rs` visibility; `renderer\mod.rs` filters | invisible skipped |
| Per-layer transform (offset/scale/rotation) | `params-panel.tsx` | IMPLEMENTED | `texture.rs::texture_transform`, `layer.rs::transform_uv` | math present |
| Transform slider ranges | `params-panel.tsx` | DEVIATION | `texture_render.rs::transform_rows` | offset ±1 vs ±5; scale .05–4 vs .01–10; rotation 0–360 vs −180–180; scroll ±2 vs ±5 |
| Scroll (uv)*time | `types.ts` + `texture.ts:2586` | IMPLEMENTED | `layer.rs::transform_uv` | time-scroll |
| Polar conversion | `params-panel.tsx` + shader | IMPLEMENTED | `layer.rs::transform_uv` | |
| Invert | `params-panel.tsx` + shader | IMPLEMENTED | `layer.rs::foreground` | |
| Gradient tint (LUT) | `gradient-editor.tsx`, `renderer.ts:88-103` | IMPLEMENTED | `gradient.rs::build_gradient_lut`, `layer.rs::sample_grad` | 256 LUT, linear |
| Solid-color tint | `params-panel.tsx` | IMPLEMENTED | `layer.rs::foreground` | multiplies gradient |
| Per-layer mask | N-A | N-A | — | mask only as blend mode |

#### PARAMS PANEL

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Panel sections (Layer/Type/Transform/Parameters) | `params-panel.tsx` | IMPLEMENTED | `texture-panels.slint`, `texture_render.rs` | |
| Param kinds | `registry.ts` ParamSpec | N-A (numeric only) | `registry_data.rs` | no color/bool/select/vector2 in web |
| Param sliders editable (all specs) | `params-panel.tsx` | IMPLEMENTED | `texture_render.rs::param_rows`, `texture.rs::texture_param` | all 407 dynamic |
| Param **step** snapping | AntD `Slider step` | DEVIATION | `pixel-panels.slint` PixelSlider | `step` carried in row structs but never applied → continuous values |
| Inline type selector (searchable Select) | `params-panel.tsx` | PARTIAL | browser only (`texture-dialogs.slint`) | type change only via View→Browse Types |
| Reset Parameters | `params-panel.tsx` + Edit menu | MISSING | — | not wired |
| Reset Transform | `params-panel.tsx` + Edit menu | MISSING | — | not wired |
| Whole-document Reset | N-A | DEVIATION | `texture.rs::texture_action("edit.reset")` | native-only destructive reset; replaces web's two resets |
| Color picker (solid fill) | AntD ColorPicker | DEVIATION | solid hex TextInput | hex only, no picker/alpha |

#### OUTPUT PANEL

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Resolution 64–2048 | `output-panel.tsx` (RESOLUTIONS) | IMPLEMENTED | output chips; `texture.rs::texture_resolution` | export at state.resolution |
| Channel config / named PBR outputs | N-A | N-A | — | only channel-pack export exists |
| Per-output settings | N-A | N-A | — | resolution only |
| Animate toggle (output panel) | `output-panel.tsx` | DEVIATION (dead) | output checkbox → `texture.rs::texture_toggle` | key "animate" unhandled → no-op; top-bar/View still works |
| Black Background toggle (output panel) | View menu only in web | DEVIATION (dead) | `texture_toggle` | key "bg" unhandled → no-op; View menu works |
| Checkerboard toggle (output panel) | View menu only in web | DEVIATION (dead) | `texture_toggle` | key "checker" unhandled → no-op; View menu works |
| Time control | Slider 0–100 | PARTIAL | text input 0–3600 | editable, different widget/range |
| Anim Speed slider | Slider 0–5 | MISSING | state.anim_speed unreachable | affects preview/sheet/GIF |
| GIF FPS | Slider 1–60 | IMPLEMENTED | text input | |
| GIF Duration | Slider 0.5–10 | IMPLEMENTED | text input 0.1–30 | |
| Seamless Loop | CheckRow | MISSING | state.gif_seamless unreachable | GIF export also absent |

#### POST EFFECTS (14)

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| POST EFFECTS (14) | `effects-panel.tsx` / `shaders\post.ts` | — | `post.rs`, `post_data.rs`, `renderer\post.rs`, `texture-panels.slint` | all 14 CPU-ported in same order incl. UV passes before sampling |
| Blur | post.ts:125 | IMPLEMENTED | renderer\post.rs | |
| Bloom | post.ts:136 | IMPLEMENTED | same | |
| Sharpen | post.ts:151 | IMPLEMENTED | same | |
| Pixelation | post.ts:110 | IMPLEMENTED | same | |
| Chromatic Aberration | post.ts:118 | IMPLEMENTED | same | |
| Vignette | post.ts:201 | IMPLEMENTED | same | |
| Scanline | post.ts:172 | IMPLEMENTED | same | |
| Kaleidoscope | post.ts:106 | IMPLEMENTED | same | |
| Mirror Tile | post.ts:99 | IMPLEMENTED | same | |
| Swirl | post.ts:102 | IMPLEMENTED | same | |
| Edge Detection | post.ts:161 | IMPLEMENTED | same | |
| Toon | post.ts:178 | IMPLEMENTED | same | |
| Radial Mask (vignetteMask) | post.ts:195 | IMPLEMENTED | same | no params (parity) |
| Color Correction | post.ts:184 | IMPLEMENTED | same | |
| Effect param kinds (slider/check/color) | `effects-panel.tsx` | IMPLEMENTED | `post_data.rs` | |
| Effect color editing | AntD ColorPicker | DEVIATION | hex TextInput | no picker |
| Effect slider step | AntD step | DEVIATION | PixelSlider | no step snapping |
| Effect params shown while disabled | web always renders children | DEVIATION | `texture-panels.slint` `visible: effect.enabled` | params hidden until enabled |

#### GRADIENT EDITOR

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Gradient editor | `gradient-editor.tsx` | — | `gradient.rs`, `ramps_data.rs`, `texture-panels.slint` | |
| Grad enable | panel checkbox | IMPLEMENTED (relocated) | `texture.rs::texture_toggle("grad")` in params panel | same flag |
| Stop list | GradientStop[] | IMPLEMENTED | `gradient.rs`, `texture_render.rs::stop_rows` | |
| Add Stop (widest gap) | `gapPosition()` | DEVIATION | `texture_gradient("add")` | appends at last+0.5, always white |
| Click strip to add stop | `handleStripClick` | MISSING | — | strip is non-interactive Image |
| Drag stop handle | 220 ms press-drag | MISSING | position slider only | |
| Double-click stop delete | `handleStopDoubleClick` | MISSING | remove button only | |
| Delete selected stop | Delete button | IMPLEMENTED | `texture_gradient("remove")` | keeps ≥2 |
| Reset to B/W | Reset button | MISSING | — | B/W ramp exists as workaround |
| Sample color on new stop | `sampleGradientColor` | MISSING | — | |
| Stop color picker | AntD ColorPicker | DEVIATION | hex TextInput | |
| 16 ramp presets | `PRESETS` | IMPLEMENTED | `ramps_data.rs` / `ramp_names/ramp_stops` | names+stops verified |
| Interpolation options | N-A (linear only) | N-A | `gradient.rs` linear | none in web |
| Ramp preview swatches | mini-strips | MISSING (styling) | name chips only | |

#### TYPE BROWSER

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Type browser | `type-browser.tsx` | — | `texture-dialogs.slint`, `texture_render.rs` | |
| Search | Input | IMPLEMENTED | `filtered_types` | |
| Category filter (All/Basic/Pattern/FX) | Segmented | IMPLEMENTED | `category()` | |
| Favorites toggle + filter | localStorage list | IMPLEMENTED (session) | `toggle_favorite`, `browser_favorites_only` | |
| Favorites persistence | `oc_texture_type_favorites` | MISSING | `texture.rs` favorites in-memory only | lost on restart (despite comment) |
| 64px thumbnails | `use-texture-thumbnails.ts` | PARTIAL | `type_thumb` (64px) | eager render; 25 unsupported types → blank tiles, no badge |
| Drag-to-add | N-A (click only) | N-A | — | |

#### SAMPLES MODAL

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Samples modal | `samples-modal.tsx`, `presets.ts` | — | `samples.rs`, `data\samples.json`, `texture-dialogs.slint` | |
| 12 built-in samples + descriptions | BUILTIN_SAMPLES | IMPLEMENTED | `samples.rs` | ids verified (magic-energy-ring…aurora-field) |
| 128px thumbnails | samples-modal | PARTIAL | `sample_thumb` (128px) | 11/12 samples reference unported types → degraded |
| Apply full state | `onApply` | IMPLEMENTED | `texture_apply_sample` | |

#### PRESET LIBRARY

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Preset library | `preset-library-modal.tsx` | — | `project.rs`, `texture.rs`, `texture-dialogs.slint` | |
| Save current | name required; upsert by name | PARTIAL | `texture_preset_action("save")` | auto-names blank input; always appends (no upsert/rename) |
| Load | | IMPLEMENTED | "load" | |
| Delete | | IMPLEMENTED | "delete" | |
| Import JSON | parses & **applies state**, closes | DEVIATION | "import" adds to list, does not apply | format `{version:1,...state}` both |
| Export JSON | current state | IMPLEMENTED | `texture_preset_io("export")` | |
| Cap 40 | MAX_PRESETS | IMPLEMENTED | trims to 40 | |
| Persistence | localStorage | IMPLEMENTED (file) | `store.rs` `textures\presets.json` | equivalent |

#### PREVIEW CANVAS

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Preview canvas | `texture-studio.tsx` | — | `texture_render.rs`, `texture.slint` | |
| Preview resolution fixed 512 | PREVIEW_RESOLUTION | IMPLEMENTED | `PREVIEW_SIZE` | same |
| Zoom/pan | wheel + middle/alt pan | IMPLEMENTED | `TextureView::zoom_at/pan_to`, slint | native pans on any mouse-down (minor deviation) |
| Background / checkerboard | CSS + View menu | IMPLEMENTED | `checker_path`, black bg | |
| Animate + time + play/pause | rAF loop, animSpeed | PARTIAL | `tick_texture`/`advance_time`, top-bar play | no animSpeed UI; CPU re-render per 60 ms tick (not real-time) |
| Rendering backend | WebGL2 ping-pong | DEVIATION | CPU `renderer\*` | same math, CPU-only (perf) |
| Undo/redo (menu) | `history.ts` 50 | IMPLEMENTED | `history.rs`, Edit menu | |
| Keyboard Ctrl+Z / Ctrl+Y | keydown handler | **MISSING (bug)** | `shortcuts.rs::key_down` | Texture view falls through to **canvas** undo/redo; no texture branch |
| Share link `#tc=` ingest/generate | `decodeTextureShare` + apply | MISSING (UI) | `project.rs::encode/decode_share` | codecs exist, no UI |
| Dock layout move/resize/split | dock-panel | DEVIATION | fixed 288px columns + toggles | show/hide/reset implemented |

#### EXPORT

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Export | `export.ts`, `texture-studio.tsx` | — | `texture_export.rs` | |
| PNG at resolution | `renderStateToCanvas`/`downloadPng` | IMPLEMENTED | `texture_png_bytes` + path prompt | typed path vs auto filename |
| Normal map (Sobel) | `generateNormalMap` (default strength 2) | DEVIATION | `cpu.rs::generate_normal_map`; `texture_run_normal` passes **1.0** | flatter normals than web |
| Channel pack | `packChannels`, 10 sources, 4 presets | DEVIATION | `channels.rs`, `cpu.rs`, dialog | sources/presets parity; normal strength 1 vs 2; dialog retains last config (web resets to base) |
| Sprite sheet (animation/hue, grid) | `exportSheet` | PARTIAL | `sheet.rs`, dialog | layout + hue rotation parity |
| Sheet metadata JSON | toggle `saveMetadata`, embeds image.file | DEVIATION | always writes `.json`; no `image.file` | no toggle; padding default 0 vs web 1; free numerics vs discrete lists |
| GIF export (+ progress, seamless) | `exportGif`, progress modal | MISSING (UI) | `gif_export.rs` domain only | encoder exists; no menu/modal; seamless blend not implemented |
| Output resolution scaling | `state.resolution` | IMPLEMENTED | `render_at` | resolution-driven both |
| Save to canvas | `exportToCanvasNode` | PARTIAL | `texture_save_to_canvas` | requires an already-open canvas project; no target/file/title picker |

#### PERSISTENCE / LIBRARY

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Project save/load | `use-texture-store.ts` | IMPLEMENTED | `store.rs` (`textures\<id>.json`), `project.rs` | lossless serde |
| Autosave | debounced persist | IMPLEMENTED | `save.rs` 400 ms + tick | |
| Groups create/rename/delete | `index.tsx` | IMPLEMENTED | `library_ops.rs` | |
| Texture create (name+group picker) | create modal | DEVIATION | `library_create_texture_project` | auto title `Texture N`, uses selected group |
| Move project between groups | `setProjectGroup` + Move menu | MISSING | — | no move action in native library |
| Preset persistence | localStorage | IMPLEMENTED | `textures\presets.json` | |
| Favorites persistence | localStorage | MISSING | — | (see Type browser) |

#### Type registry counts

Web-official categories (authoritative):

| Category (web, by TYPE_ID) | Web count | Native count | Missing type names |
|---|---|---|---|
| Basic (≤199) | 10 | 9 | AnalogGlitch |
| Pattern (200–260) | 49 | 49 | — |
| FX (≥261) | 58 | 34 | Crescent, EnergyRing, AuraRing, Pulse, Glare, StarFlare, Burst, ImpactLines, VoronoiFluid, PolarDots, DistortionWave, ToxicCloud, GeoRelief, Crystal, AbsNoise, PlasmaV2, GrungeV2, Twirl, CosmicPortal, Wormhole, CyberBlock, LaserBeam, GlitchBlock, SparkBurst |
| **Total** | **117** | **92** | **25** |

Native renderer modules (the 8 names in the brief; these are internal, not a web category):

| Native module | Ported fns | Module file |
|---|---|---|
| basic | 22 | `renderer\types\basic.rs` |
| shapes | 5 | `renderer\types\shapes.rs` |
| noise | 12 | `renderer\types\noise.rs` |
| fluids | 14 | `renderer\types\fluids.rs` |
| pattern | 8 | `renderer\types\pattern.rs` |
| grid | 13 | `renderer\types\grid.rs` |
| detail | 14 | `renderer\types\detail.rs` |
| fx | 4 | `renderer\types\fx.rs` |
| **Total** | **92** | `renderer\types\mod.rs` dispatch |

#### Sample-fidelity corollary (verified)

11 of 12 built-in samples reference ≥1 unported type, so their native thumbnails/renders are degraded: magic-energy-ring (EnergyRing, AuraRing, SparkBurst), fire-burst (Burst, StarFlare, SparkBurst), water-vortex (Wormhole, CosmicPortal, AuraRing), impact-shockwave (Burst, StarFlare, SparkBurst), cyber-portal (EnergyRing, CyberBlock, GlitchBlock), frost-nova (Crystal, EnergyRing, SparkBurst), toxic-pulse (ToxicCloud, Pulse), holy-flare (Burst, Glare, StarFlare), dark-singularity (Wormhole, CosmicPortal, EnergyRing), electric-storm (SparkBurst, Burst), hologram-shield (EnergyRing, Pulse, GlitchBlock). Only aurora-field is fully renderable.

### 7. Plugin System

#### A. Plugin package / API surface

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| `CanvasPlugin` object (id/name/version/description/css/nodes) | W `types/canvas-plugin.ts:138` | PARTIAL | N `plugins/src/plugin.rs:20` | Plain data struct; nothing executes it |
| `minAppVersion` field | W `types:143` | N-A | N `plugins/src/plugin.rs:25` | Declared but never enforced in **either** codebase |
| `setup?(app)` hook + cleanup | W `lib/canvas/plugin-loader.ts:37` | MISSING | N `plugins/src/plugin.rs:4` | Not modelled (doc comment) |
| Default-export object vs factory `(runtime)=>plugin` | W `plugin-loader.ts:17` | MISSING | — | No bundle eval at all (no eval/import) |
| `app.injectCSS(css,key)` + removal | W `plugin-runtime.ts:17` | MISSING | — | CSS only stored as `CanvasPlugin.css`, never applied |
| Host runtime bridge `window.OpenCanvasRuntime` (React/jsx/Fragment/version/emit/on) | W `plugin-runtime.ts:28` | MISSING | — | Browser-bound; not portable |
| Plugin bundle cache (`source`) + pinned offline | W `use-plugin-store.ts:12` | PARTIAL | N `plugins/src/manager.rs:21` | `source` field kept; always `"builtin"` for officials |
| Plugin-string i18n / localized names | W `i18n` + plugin Chinese literals | DEVIATION | N `ui/components/plugin-manager.slint` | Manager UI hardcoded English; no i18n layer |

#### B. Node-definition ABI (`CanvasNodeDefinition`)

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| `type` / `title` / `description` | W `types/canvas-plugin.ts:101` | IMPLEMENTED | N `plugins/src/abi.rs:147` | |
| `icon` (`ReactNode`) | W `types:104` | PARTIAL | N `abi.rs:152` | Native `String`; lucide-key or emoji only |
| `defaultSize` / `defaultMetadata` | W `types:106` | IMPLEMENTED | N `abi.rs:154` | |
| `minimapColor` | W `types:108` | PARTIAL | N `plugins/src/view.rs:64` | Carried in view, but Slint colors are hardcoded per type (`ui/theme.slint:119`); custom colors → `node-muted` |
| `showInCreateMenu` | W `types:109` | IMPLEMENTED | N `abi.rs:159` | |
| `hasSourceHandle` | W `types:110` | IMPLEMENTED | N `abi.rs:160` | |
| `hidePanel` | W `types:111` | IMPLEMENTED | N `view.rs:149` | |
| `transparentBackground` | W `types:112` | IMPLEMENTED | N `view.rs:126` | |
| `autoOpenPanel` | W `types:113` | MISSING | N `abi.rs:167` (field exists) | Field declared, **no app consumer** |
| `useBuiltinPanel{mode,promptPrefix,writeBackToSelf}` | W `types:114` | PARTIAL | N `abi.rs:108`, `plugin_views.rs:183`, `ai_service.rs:378` | Panel+prefix work; `write_back_to_self` never read |
| `interactionToggle` | W `types:116` | PARTIAL | N `abi.rs:169` | Toggle action flips `metadata.interactive`, but no content is actually interactive |
| `forceInteractive(node)` | W `types:118` | PARTIAL | N `abi.rs:233` | Predicate resolved; nothing consumes the result for pointer events |
| `keepAspectRatio(node)` | W `types:119` | MISSING | N `abi.rs:240` | Resolved into `view.keep_aspect_ratio`; card resize never reads it |
| `resource(node)` resolver | W `types:120` | PARTIAL | N `abi.rs:247` | Resolved, but `references.rs` has no plugin lookup → not consumed |
| `Content` React component | W `types:122` | MISSING | — | Native app maps type-ids to fixed renderers instead |
| `Panel` React component | W `types:123` | MISSING | N `view.rs:155` → `plugin_views.rs:188` | `PanelKind::Plugin` explicitly resolves to `None` |
| `toolbar(ctx)` arbitrary items | W `types:124` | PARTIAL | N `plugin_views.rs:126` | Host synthesizes 4 fixed ids only; no plugin callback |
| `onDoubleClick(ctx)` callback | W `types:125` | PARTIAL | N `plugin_actions.rs:34` | Host hardcodes edit-toggle for 4 types; no definition hook |
| Plugin-private default metadata (`defaultMetadata` extras) | W `types:107` | IMPLEMENTED | N `abi.rs:157`, `sticky_note.rs:101` | `MetadataExtras` + sidecar |

#### C. Node context / host capabilities (`CanvasNodeContext`)

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| `node`, `scale`, `isSelected`, plugin extras | W `types:42` | IMPLEMENTED | N `plugins/src/context.rs:43` | Trait; **no host `impl` exists** |
| `updateMetadata(patch)` | W `types:48` | IMPLEMENTED | N `context.rs:52` | Trait method; app patches via its own path |
| `updateNode(partial)` | W `types:49` | IMPLEMENTED | N `context.rs:54` | |
| graph accessors `getNode/…/getDownstream` | W `types:51-55` | IMPLEMENTED | N `context.rs:56-60` | App helper `upstream_nodes` (only upstream used) |
| `applyOps(CanvasAgentOp[])` | W `types:57` | MISSING | N `context.rs:4` | Explicitly deferred |
| `emit` / `on` (inter-plugin bus) | W `types:59` | PARTIAL | N `plugins/src/event_bus.rs` | Bus built & tested; never wired to a live node context |
| `ai` (generateImage/Video/Text, listModels, defaultModel) | W `types:22,62`; `use-plugin-host.tsx:42` | MISSING | N `context.rs:5` | Deferred; plugins get no AI surface |
| `openPanel` / `closePanel` | W `types:64` | PARTIAL | N `context.rs:65` | Trait only; app opens panels by selection, not plugin call |
| `storage` (get/set/remove) | W `types:67` | PARTIAL | N `plugins/src/storage.rs:24` | Trait + memory + namespace; no disk backend, no host wiring |
| `theme` tokens | W `types:4434` | MISSING | N `context.rs:5` | No native `CanvasTheme` type |

#### D. Registry (`node-registry.ts`)

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| register / replace-in-place / version bump | W `lib/canvas/node-registry.ts:17` | IMPLEMENTED | N `plugins/src/registry.rs:68` | |
| `unregisterPluginNodes(pluginId)` | W `node-registry.ts:25` | IMPLEMENTED | N `registry.rs:98` | |
| `getNodeDefinition` / `listNodeDefinitions` | W `node-registry.ts:34,42` | IMPLEMENTED | N `registry.rs:107,115` | |
| `getNodePluginId` (owner, default builtin) | W `node-registry.ts:38` | IMPLEMENTED | N `registry.rs:166` | |
| `getNodeSpec` fallback 340×240 | W `node-registry.ts:50` | IMPLEMENTED | N `plugins/src/registry.rs:125` | |
| Fallback title localized (`canvas.node.node`) | W `node-registry.ts:50` | DEVIATION | N `registry.rs:136` | Hardcoded `"Node"` |
| `isBuiltinNodeType` | W `node-registry.ts:59` | IMPLEMENTED | N `registry.rs:197` | |
| Zustand version signal for UI | W `node-registry.ts:12` | IMPLEMENTED | N `registry.rs:182` | `version()` counter (no reactive binding) |
| 9 built-in definitions | W `components/canvas/nodes/builtin-nodes.tsx` | IMPLEMENTED | N `plugins/src/builtins.rs:21` | Full resource/aspect mapping |

#### E. Lifecycle (register / enable / disable / uninstall / upgrade)

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Activate: register nodes + inject CSS + setup | W `plugin-loader.ts:31` | PARTIAL | N `app/src/plugin_host.rs:84` | Nodes only; no CSS/setup |
| Deactivate (unregister + dispose) | W `plugin-loader.ts:42` | IMPLEMENTED | N `plugin_host.rs:93` | |
| `setPluginEnabled` | W `plugin-loader.ts:75` | IMPLEMENTED | N `plugin_host.rs:99` | |
| `uninstallPlugin` | W `plugin-loader.ts:87` | IMPLEMENTED | N `plugin_host.rs:108` | |
| `installPluginFromUrl` (fetch+eval+activate) | W `plugin-loader.ts:61` | PARTIAL | N `plugin_manager_model.rs:100` | **Record-only**; no fetch/eval (stated in UI) |
| `updatePlugin` (cache-bust refetch) | W `plugin-loader.ts:70` | MISSING | — | Update button merely re-installs the record |
| `ensurePluginsLoaded()` startup hydrate | W `plugin-loader.ts:95` | PARTIAL | N `plugin_host.rs:49` | Officials auto-installed; no store rehydrate; third-party records not persisted |
| `setEnabled` re-fetch for local plugins | W `plugin-loader.ts:82` | MISSING | — | No local/network reload |
| Persist installed-plugin list | W `use-plugin-store.ts` (localforage) | MISSING | N `store.rs` (only `plugins_dir`) | Native manager state is **in-memory**; lost on restart |
| InstalledPlugin record (enabled/local/official/installedAt) | W `use-plugin-store.ts:6` | IMPLEMENTED | N `plugins/src/manager.rs:13` | `installed_at` host-supplied |
| upsert prepend/replace / setEnabled / remove | W `use-plugin-store.ts:31-38` | IMPLEMENTED | N `manager.rs:128-201` | |

#### F. Install sources

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Install from URL | W `plugin-loader.ts:61` | PARTIAL | N `plugin_manager_model.rs:100` | URL validation + id derivation; no download |
| URL id derivation (`plugin_id_from_url`) | W (implicit via plugin.id) | IMPLEMENTED (native extra) | N `plugin_manager_model.rs:146` | Native-only helper |
| Local plugins (`/plugins/index.json`) | W `plugin-loader.ts:117` | MISSING | — | Local tab never populated |
| Dev plugins (`VITE_DEV_PLUGINS`) | W `plugin-loader.ts:153` | MISSING | — | |
| Official registry fetch (`official-plugins.json`) | W `lib/canvas/plugin-registry.ts:17` | PARTIAL | N `plugin_manager_model.rs:81` | Static built-in entries only; no network manifest |
| Marketplace / third-party indexed sources | W third-party tab via URL | MISSING | — | Only free-typed URL records |
| Cache-busting query | W `plugin-loader.ts:55` | MISSING | — | |
| Official URL resolution relative→absolute | W `plugin-registry.ts:30` | MISSING | — | Native uses `builtin://official/<id>.js` |

#### G. Versioning / upgrade

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| `compareSemver` / `hasUpgrade` | W `plugin-registry.ts:36,47` | IMPLEMENTED | N `plugins/src/manager.rs:208,222` | Numeric major.minor.patch only |
| Installed vs remote version compare | W `canvas-plugin-manager-modal.tsx:172` | IMPLEMENTED | N `plugins/src/manager.rs:82`, `plugin_views.rs:74` | `upgradable` flag |
| Upgrade dot + `old → new` title | W `canvas-plugin-manager-modal.tsx:109,179` | MISSING | N `ui/components/plugin-manager.slint:234` | Only a plain "Update" button |
| `minAppVersion` gating | W `types:143` (unused) | N-A | N `plugin.rs:25` | Unused in both |

#### H. Sandboxing / permissions

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Runtime sandbox for plugin code | none — warning banner `canvas-plugin-manager-modal.tsx:218` | N-A | — | Web runs plugin JS unsandboxed; native runs no code |
| Permission model | none | N-A | — | Neither implements |

#### I. Event bus & storage

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| In-memory event bus `emitCanvasEvent`/`onCanvasEvent` | W `lib/canvas/canvas-event-bus.ts:9,19` | IMPLEMENTED | N `plugins/src/event_bus.rs:47,63` | Native adds Drop-unsubscribe |
| Per-plugin namespaced storage | W `canvas-event-bus.ts:32` (localforage) | PARTIAL | N `plugins/src/storage.rs:33` | Namespace wrapper + memory backend; no persistent backend |
| Plugin storage persistence | W localforage (IndexedDB) | MISSING | — | Extras are persisted, plugin storage is not |

#### J. Styles / i18n / metadata schema

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Plugin `css` injection/removal | W `plugin-runtime.ts:17`, `plugin-loader.ts:36` | MISSING | N `plugins/src/plugin.rs:26` | Field exists, never applied |
| Plugin metadata = flat open bag | W `types/canvas-plugin.ts`/`types/canvas.ts` | PARTIAL | N `plugins/src/abi.rs:24` | Core schema fixed + `MetadataExtras` sidecar |
| Extras persisted with project | W part of node JSON | IMPLEMENTED | N `app/src/store.rs:190`, `save.rs:99` | `projects/<id>.extras.json` sidecar |
| Extras copied on duplicate / pruned on delete | W via node copy | IMPLEMENTED | N `plugin_host.rs:168,175` | |
| Localized plugin/manager strings | W i18n | DEVIATION | N `plugin-manager.slint` | English literals |

#### K. Canvas node integration

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Create-menu entry for plugin node types | W `components/canvas/canvas-toolbar.tsx:69` | DEVIATION | N `ui/components/create-menu.slint`, `plugin_views.rs:52` | Web = separate "extensions" dropdown (non-builtin only); native merges built-ins + plugins, grouped by owner |
| Node content rendering (per plugin kind) | W `canvas-node.tsx:499-508` | PARTIAL | N `app/src/plugin_content.rs:98`, `ui/components/node-card.slint:277` | Native fixed native renderers, no plugin Content |
| Missing-plugin placeholder | W `canvas-node.tsx:551` | IMPLEMENTED | N `plugins/src/view.rs:99`, `node-card.slint:279` | |
| Hover-toolbar host interaction toggle | W `use-plugin-host.tsx:136` | PARTIAL | N `plugin_views.rs:138`, `plugin_actions.rs:77` | Flag flips; content not interactive |
| Hover-toolbar plugin items (interact/generate/reset/palette) | W `use-plugin-host.tsx:128` | PARTIAL | N `plugin_views.rs:126` | Synthesized, not from `definition.toolbar` |
| Toolbar edit toggle (✎/👁) | W each plugin `toolbar` | DEVIATION | N `plugin_actions.rs:34` | Editing instead toggled by double-click |
| Built-in generation panel (`useBuiltinPanel`) | W `project.tsx:1823` | IMPLEMENTED | N `node_panel.rs:162`, `plugin_views.rs:183` | Maps to `PanelKind::Config` |
| Custom plugin `Panel` | W `project.tsx:1809` | MISSING | N `plugin_views.rs:188` | `PanelKind::Plugin` → `None` |
| `autoOpenPanel` on click | W `project.tsx` panel logic | MISSING | — | No consumer |
| Resource as generation/mention input | W `lib/canvas/canvas-resource-references.ts:54,93` | MISSING | N `core/src/references.rs:83` | Native resource detection is builtin-only |
| `promptPrefix` applied to generation | W `use-canvas-generation.ts:376,411` | IMPLEMENTED | N `ai_service.rs:378` | Applied to every plugin node with a prefix |
| `writeBackToSelf` write-back | W `use-canvas-generation.ts:368,403` | PARTIAL | N `ai_service.rs:505` | Native writes result to node content regardless; no text-append/`texts[]` semantics |
| `{{input}}` / upstream content resolution | W `html/index.tsx:65` | IMPLEMENTED | N `plugins/src/official/html.rs:26`, `plugin_content.rs:98` | |
| Plugin-node persistence in project JSON | W node type string | IMPLEMENTED | N `core/src/node.rs:63,92,108` | `CanvasNodeType::Plugin(String)` round-trips |
| Minimap color per plugin node | W `canvas-mini-map.tsx:101` | PARTIAL | N `ui/theme.slint:119` | Hardcoded mapping for the 5 official types only |
| Plugin manager modal open | W `project.tsx:2068` | IMPLEMENTED | N `ui/components/toolbar.slint:120`, `plugin_wire.rs:58` | |
| Node actions from manager (refresh rows/create menu) | W registry version hook | IMPLEMENTED | N `plugin_manager.rs:139` | |

#### L. Plugin manager UI

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| 3 tabs (Official/Local/Third-party) | W `canvas-plugin-manager-modal.tsx:209-213` | IMPLEMENTED | N `ui/components/plugin-manager.slint:92`, `plugin_manager_model.rs:17` | Web hides Local when empty; native always shows |
| Official list + install | W modal `officialTab:149` | PARTIAL | N `ui/components/plugin-manager.slint:143`, `plugin_views.rs:69` | Static 5 built-ins; no icons/loading/error/refresh |
| Registry load/refresh/error states | W modal `loadOfficial:28`, `155-166` | MISSING | — | |
| Install from URL field | W modal `197-207` | PARTIAL | N `plugin-manager.slint:245`, `plugin_manager_model.rs:100` | Record-only, hint shown |
| Enable/disable | W `Switch` `modal:88` | IMPLEMENTED | N `plugin-manager.slint:215` | Native button vs web switch |
| Uninstall (+ confirm) | W `Popconfirm` `modal:99` | PARTIAL | N `plugin-manager.slint:221` | No confirmation dialog |
| Update action / upgrade highlight | W modal `91-98` | PARTIAL | N `plugin-manager.slint:234` | Re-installs record; no fetch |
| Upgrade dot / version transition text | W modal `109,179` | MISSING | — | |
| Local-tab rows | W modal `localTab:195` | MISSING | N `plugin_views.rs:88` | Never populated |
| Third-party rows | W modal `197` | IMPLEMENTED | N `plugin_views.rs:95` | |
| Warning banner (untrusted code) | W modal `218` | MISSING | — | |
| Install/feedback messaging | W AntD `message` | PARTIAL | N `plugin_manager.rs:89`, `plugin-manager.slint:283` | Inline message line |
| Empty states | W modal `122` | IMPLEMENTED | N `plugin-manager.slint:130` | |

#### M. Official plugin — html (`html:render`)

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Node type/title/icon/size/minimap/hidePanel/interactionToggle | O `html/src/index.tsx:108-137` | IMPLEMENTED | N `plugins/src/official/html.rs:93` | |
| defaultMetadata `content:""` | O `html/index.tsx:115` | IMPLEMENTED | N `html.rs:97` | |
| `{{input}}` substitution (regex incl. spaces) | O `html/index.tsx:74` | IMPLEMENTED | N `official/html.rs:38` | Byte-accurate incl. unicode |
| Upstream text join (newline, non-empty) | O `html/index.tsx:65` | IMPLEMENTED | N `official/html.rs:26` | |
| Sandboxed `iframe srcDoc` render | O `html/index.tsx:93` | MISSING | N `plugin_content.rs:158`, `node-card.slint:54` | Native shows substituted text in a framed "sandbox · no webview" box |
| Editor: line-number gutter | O `html/index.tsx:20-45` | MISSING | — | |
| Editor: monospace font 12 / line 20 metrics | O `html/index.tsx:9-10` | PARTIAL | N `official/html.rs:22` | Constants exist; **unused** |
| Editing toggle via metadata | O `html/index.tsx:123` | IMPLEMENTED | N `plugin_actions.rs:34` | Double-click, not toolbar |
| forceInteractive while editing | O `html/index.tsx:120` | PARTIAL | N `html.rs:103` | Predicate exists; no pointer-events effect |
| Empty-state hint UI | O `html/index.tsx:80-87` | PARTIAL | N `node-card.slint:367` | Different copy/visuals |
| Plugin CSS / setup / AI | none | N-A | — | |

#### N. Official plugin — markdown (`markdown:doc`)

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Node type/size/minimap/hidePanel/interactionToggle | O `markdown/src/index.tsx:88-124` | IMPLEMENTED | N `official/markdown/mod.rs:80` | |
| defaultMetadata `content:""` | O `markdown/index.tsx:101` | IMPLEMENTED | N `markdown/mod.rs:84` | |
| `resource` kind=text | O `markdown/index.tsx:107` | IMPLEMENTED | N `markdown/mod.rs:106` | |
| Full CommonMark via `marked@14` (tables, nested lists, ref links, autolinks) | O `markdown/index.tsx:18` | PARTIAL | N `official/markdown/mod.rs` (+`inline.rs`) | Documented subset; no tables/nested/ref links |
| Render markdown → HTML in card | O `markdown/index.tsx:58-63` | PARTIAL | N `plugin_content.rs:136` | `to_html` exists in crate but app uses `to_text` → **plain text**, images/links unstyled |
| Editor textarea + editing toggle | O `markdown/index.tsx:68` | IMPLEMENTED | N `plugin_actions.rs:34`, `node-card.slint:349` | |
| Empty-doc placeholder italic line | O `markdown/index.tsx:22` | IMPLEMENTED | N `markdown/mod.rs:21` | |
| Injected `styles.css` (`.cnv-md`) | O `markdown/src/styles.css` | MISSING | — | No CSS pipeline |
| Parse cache / flicker-free DOM writes | O `markdown/index.tsx:26,60` | N-A | — | DOM-specific |

#### O. Official plugin — svg (`svg:vector`)

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Node type/size/minimap | O `svg/src/index.tsx:78-96` | IMPLEMENTED | N `official/svg.rs:169` | |
| `transparentBackground` | O `svg/index.tsx:93` | IMPLEMENTED | N `svg.rs:173` | |
| Upstream SVG adoption | O `svg/index.tsx:10-20` | IMPLEMENTED | N `svg.rs:104,114` | Plus `should_adopt` semantics |
| Source validation | none (browser tolerates) | IMPLEMENTED (native extra) | N `svg.rs:38` | Native adds strict well-formedness checks |
| Render SVG (`dangerouslySetInnerHTML`) | O `svg/index.tsx:70` | PARTIAL | N `app/src/plugin_content.rs:193` | Headless `resvg` raster, 720px cap, `Image` fit-contain; no CSS animation/interactivity |
| Edit toggle / textarea / Esc exit | O `svg/index.tsx:51-73` | PARTIAL | N `plugin_actions.rs:34`, `node-card.slint:382` | Toggle via double-click; outside-click/Esc handling not present |
| Click-outside-to-exit-edit | O `svg/index.tsx:24-31` | MISSING | — | |
| Resource | none declared | N-A | — | |

#### P. Official plugin — sticky-note (`sticky-note:note`)

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Node type/size/minimap/hidePanel | O `sticky-note/src/index.tsx:142-159` | IMPLEMENTED | N `official/sticky_note.rs:110` | |
| defaultMetadata `content:"", pluginColor` | O `sticky-note/index.tsx:154` | IMPLEMENTED | N `sticky_note.rs:101,114` | Extras sidecar |
| 8 preset colors | O `sticky-note/index.tsx:7` | IMPLEMENTED | N `sticky_note.rs:23` | |
| Default/blank color fallback | O `sticky-note/index.tsx:19` | IMPLEMENTED | N `sticky_note.rs:35` | Native trims + falls back |
| Palette popover (color dot button) | O `sticky-note/index.tsx:87-113` | PARTIAL | N `node-card.slint:312`, `plugin_views.rs:166` | Inline swatch row instead of popover+dialog |
| Custom color picker (`<input type=color>`) | O `sticky-note/index.tsx:109-111` | MISSING | — | No custom color path |
| Throttled color preview (150ms) | O `sticky-note/index.tsx:44-51` | MISSING | — | |
| Double-click edit textarea | O `sticky-note/index.tsx:117` | IMPLEMENTED | N `plugin_actions.rs:34`, `node-card.slint:332` | |
| Blur/Esc exit editing | O `sticky-note/index.tsx:123-129` | PARTIAL | N `node-card.slint` | No explicit blur/Esc handling seen |
| Empty placeholder text | O `sticky-note/index.tsx:136` | IMPLEMENTED | N `sticky_note.rs:28` | |
| Resource | none declared | N-A | — | |

#### Q. Official plugin — panorama (`panorama:viewer`)

| Feature | Web (file) | Native status | Native (file) | Note |
|---|---|---|---|---|
| Node type/size/minimap/interactionToggle | O `panorama/src/index.tsx:260-308` | IMPLEMENTED | N `official/panorama.rs:68` | |
| `useBuiltinPanel` image + `promptPrefix` + `writeBackToSelf` | O `panorama/index.tsx:278` | PARTIAL | N `panorama.rs:74`, `ai_service.rs:378` | Config + prefix applied; write-back flag ignored |
| Equirectangular system prompt | O `panorama/index.tsx:16` | IMPLEMENTED | N `panorama.rs:27` | |
| 3D three.js viewer (sphere, drag pan, wheel FOV, auto-rotate, resize) | O `panorama/index.tsx:25-197` | MISSING | N `node-card.slint:102` | 2:1 placeholder only ("No panorama yet") |
| GPU-max texture downsample | O `panorama/index.tsx:138-154` | MISSING | — | |
| Local upload (`FileReader` → dataURL) | O `panorama/index.tsx:207-215` | MISSING | N `node-card.slint:135` | Empty state mentions generate/upload but no picker |
| Upstream image fallback | O `panorama/index.tsx:250-254` | IMPLEMENTED | N `panorama.rs:45` | |
| `resource` kind=image | O `panorama/index.tsx:280` | IMPLEMENTED | N `panorama.rs:59` | Not consumed by native references |
| Toolbar: AI generate + 换图 reset | O `panorama/index.tsx:286-308` | IMPLEMENTED | N `plugin_views.rs:152`, `plugin_actions.rs:95` | `plugin-generate` / `plugin-reset` |
| Loading / error overlays | O `panorama/index.tsx:190-194` | MISSING | — | |
| `is_equirectangular` 2:1 check | O `panorama/index.tsx` (implicit) | PARTIAL | N `panorama.rs:35` | Implemented but only used by tests, not the app |
| CDN `import("three")` | O `panorama/index.tsx:10` | N-A | — | No JS runtime |

---

## Top 25 prioritized gaps

Ranked by **user-visible impact first, then effort**. Every item below is a **[REGRESSION]** — the web app has the feature and the native port does not (or only partly). Net-new candidates (neither codebase has them) rank below these and are listed separately.

1. **App config modal reduced to 2 of 10 tabs** — Domain 2 (+3) · PARTIAL/MISSING · *[REGRESSION]* — users cannot set theme, language, canvas background, image count, cost, logs or see About/shortcuts anywhere; the settings surface is the most-used non-canvas screen.
2. **No plugin execution engine** — Domain 7 · MISSING/PARTIAL · *[REGRESSION]* — URL install is record-only (no fetch/eval/import), no `setup`, no CSS, no custom `Panel`/`Content`; every third-party plugin is inert, so the plugin system cannot run code.
3. **Media/content rendering absent** — Domain 1 (+2) · MISSING · *[REGRESSION]* — image nodes draw **no bitmap**, video has no player, and image-stack/assets content is absent; the canvas cannot show the generated media it exists to produce.
4. **Masks entirely missing (add/apply/delete/paint/view)** — Domain 4 · MISSING · *[REGRESSION]* — the compositor honours `mask_storage_key` but no UI exists; a whole Photoshop-pillar editing workflow is impossible.
5. **Image quick-tool family + edit dialogs absent** — Domain 2 (+1) · MISSING · *[REGRESSION]* — crop, mask-edit, split, resolution/upscale, analysis, segment, camera-angle, remove-bg, OCR, copy/download and free-resize do not exist; no image post-processing from the canvas.
6. **No copy/paste of nodes (Ctrl+C/V + system clipboard)** — Domain 1 (+2,+3) · MISSING · *[REGRESSION]* — a core editing shortcut is absent; duplication is hover-button only.
7. **Node drop bindings + drag ghost absent** — Domain 1 (+3) · MISSING · *[REGRESSION]* — image→assets (collect), image→image-stack (append) and image/video→video-prompt slots do not work; image-flow composition is broken.
8. **Zoom panel + reset/fit viewport + shortcuts modal missing** — Domain 1 (+2) · MISSING · *[REGRESSION]* — the zoom control is an inert label; there is no reset-view action and no in-app shortcut reference.
9. **Preview modal is a placeholder** — Domain 2 (+1) · DEVIATION/MISSING · *[REGRESSION]* — clicking an image shows "Preview unavailable"; no stack gallery (cover/take-out/split/download).
10. **25/117 texture generator types unported** — Domain 6 · PARTIAL/MISSING · *[REGRESSION]* — affected layers silently render blank (value 0) with no UI warning and 11/12 built-in samples render degraded; EnergyRing/SparkBurst/Burst/AuraRing should be first.
11. **Connection select/delete + relation labels + cut tool missing** — Domain 1 · MISSING · *[REGRESSION]* — wires cannot be selected or deleted, no reference-index label is drawn, and the Ctrl+right-drag cut tool is not ported.
12. **Inline node editing missing** — Domain 1 (+3) · PARTIAL/MISSING · *[REGRESSION]* — text body, node title and on-card prompt chips can only be edited via panels, not in place.
13. **Image Studio text panel absent** — Domain 4 · MISSING · *[REGRESSION]* — tracking/kerning/leading/baseline/scale/case/paragraph/text-on-path/warp are implemented in the engine but have no UI; only content/size/color/family are editable.
14. **Dock engine flat/fixed (no drag/resize/split/tabs, no persistence)** — Domain 2 (+4,+5,+6) · MISSING/DEVIATION · *[REGRESSION]* — recurs in all four studios; users cannot arrange the workspace or keep a layout across restarts.
15. **Transform modes non-functional (Free/Skew/Distort/Perspective/Warp)** — Domain 4 · MISSING · *[REGRESSION]* — the Edit menu arms a mode nothing consumes; no warp mesh and no rotate handle on the move tool.
16. **Cost & usage entirely absent** — Domain 3 (+2) · MISSING · *[REGRESSION]* — `/generation` cost lookup, OpenRouter usage and the cost tab do not exist, so every generation is unpriced/unaccounted.
17. **Texture GIF export has no UI** — Domain 6 · MISSING · *[REGRESSION]* — the encoder exists but is never called; no Export menu entry/modal/progress and no seamless-loop blending.
18. **Pixel colour picker reduced to numeric fields** — Domain 5 · MISSING · *[REGRESSION]* — no SV square / hue strip; only H/S/B + R/G/B + hex, a direct UX regression for the primary colour workflow.
19. **i18n: no locale files / runtime switching / zh-CN; hardcoded English errors** — Domain 3 · MISSING/DEVIATION · *[REGRESSION]* — blocks any non-en-US ship despite the web shipping en-US + zh-CN resources.
20. **Video generation fidelity + video frame/task UI** — Domain 3 (+1,+2) · PARTIAL/MISSING · *[REGRESSION]* — no first/last-frame or video/audio references, no plugin video provider, no frame capture (first/last/current) and no task query/download.
21. **Node loading/error/retry states + media info bar missing** — Domain 1 (+2) · MISSING/PARTIAL · *[REGRESSION]* — no generating spinner, no error + retry surface, and no WxH · bytes · cost info bar on nodes.
22. **Text batch + image-stack batch controls + per-node layer popover missing** — Domain 1 (+2) · MISSING · *[REGRESSION]* — multi-text expand/set-primary/take-out, stack thumbnails/view-all/split, and the per-node layer popover are absent.
23. **Side panel fixed-width & non-persistent + switcher actions missing** — Domain 2 · MISSING · *[REGRESSION]* — no resize handle, no width/open persistence, and the switcher lacks new/import/reorder/delete/node-count.
24. **Library card & group actions truncated** — Domain 2 · MISSING · *[REGRESSION]* — no move-to-group, no drag-reorder, no multi-select, and no node/connection stats line on cards.
25. **Plugin manager state not persisted + no plugin CSS/setup/i18n** — Domain 7 · MISSING/PARTIAL · *[REGRESSION]* — third-party installs and enable flags vanish on restart and plugin styles/localized strings are dropped.

**Quick wins / honorable mentions (not in the 25):** Texture output-panel **Animate/Black-Background/Checkerboard toggles are dead** and **Ctrl+Z/Y is misrouted to the canvas** in the Texture view (one-line routing fixes); **Normal-map / channel-pack Sobel strength is 1.0 vs web 2.0** (flatter normals); **gradient-editor interactions** (strip click-add, handle drag, double-click delete, reset-to-B/W, colour sampling); **per-node model picker uses a hardcoded preset list** instead of configured channels; **quick-tool/showLabels and side-panel config persistence**; **markdown renders as plain text** though `to_html` exists; **panorama has no 3D viewer** and **html has no sandboxed webview**.

---

## Regressions vs web (native behind)

The subset of gaps where the **web HAS the feature and the native port does not** = rows whose status is **PARTIAL, MISSING or DEVIATION** = **396 features** (133 PARTIAL + 201 MISSING + 62 DEVIATION). This is the highest-priority list. Grouped highlights (full per-row detail is in the domain tables above):

- **Canvas Board Core (51 regressions: 12 PARTIAL + 31 MISSING + 8 DEVIATION)** — media/content renderers (image/video/stack/assets), drop bindings + drag ghost, image/video quick tools, copy/paste, inline editing (text/title/prompt chips), wire select/delete + labels + cut tool, zoom panel/reset/shortcuts, text batch, loading/error states, media info bar, per-node layer popover, related-highlight, lock badge, grid line/blank modes, node-list filter/bulk-rename, side-panel width persistence.
- **Canvas Panels / Overlays / Library / Persistence (96 regressions: 26 PARTIAL + 50 MISSING + 20 DEVIATION)** — 8 missing config tabs + version modal + account usage; the whole image quick-tool/dialog family (crop/mask-edit/split/resolution/analysis/segment/angle/asset preview); preview gallery; dock drag/resize/split/tabs/persistence; side-panel resize/persistence + switcher new/import/reorder/delete/count; library move-to-group/drag-reorder/multi-select/stats; ZIP/blob export & import; Clipboard Ctrl+C/V.
- **AI / Settings / Prompt / Shortcuts / Theming / i18n (66 regressions: 21 PARTIAL + 40 MISSING + 5 DEVIATION)** — cost/usage + `/generation` lookup + runtime logs; i18n locale files/switching; settings tabs; per-node reasoningEffort/textCount + model picker; OCR/multi-angle/upscale/background-removal/segmentation/AI-agent/model-plugin; drag-node-into-prompt, inline chip thumbnails/preview, highlight overlay, expand editor; video fidelity + frame/task UI; theme persistence + canvas background presets.
- **Image Studio (36 regressions: 20 PARTIAL + 15 MISSING + 1 DEVIATION)** — masks (add/apply/delete/paint/view); transform modes + rotate handle; text panel; dock; layer drag-reorder + group collapse + thumbnails; guide creation; colour SV square; gradient/bucket/pattern option depth; brush dynamics; import a new bitmap; save-to-canvas/Output modal.
- **Pixel Studio (17 regressions: 4 PARTIAL + 9 MISSING + 4 DEVIATION)** — colour SV square + hue strip; canvas-image-node import sources; dock engine + layout persistence; agent/headless/screenshot bridge; pixel project size presets; pixel-specific groups; import File prompt is typed-path.
- **Texture Studio (45 regressions: 9 PARTIAL + 17 MISSING + 19 DEVIATION)** — 25 unported types; GIF export UI + seamless loop; dead output toggles; misrouted Ctrl+Z/Y; Reset Transform/Parameters (and destructive whole-doc reset); Sobel strength 1.0 vs 2.0; gradient-editor interactions; preset import semantics; favorites persistence; Anim Speed; sheet metadata toggle/padding; type selector; slider step; colour pickers (hex-only); share link UI; move-to-group.
- **Plugin System (85 regressions: 41 PARTIAL + 39 MISSING + 5 DEVIATION)** — no execution engine; `Content`/`Panel`/`toolbar`/`onDoubleClick` callbacks; `ai`/`applyOps`/`theme` context; wired event bus & persistent storage; plugin `resource` consumption; persisted manager state + local/dev install sources; CSS/setup/i18n; `autoOpenPanel`/`keepAspectRatio`/`forceInteractive`/`writeBackToSelf` inert; html sandbox, panorama 3D, markdown HTML, svg full fidelity, sticky custom colour.

---

## Net-new candidates (absent in web too)

Rows marked **N-A** in the source tables = **73 features**. Of these, roughly **35 are genuine net-new feature candidates** in scope (the rest are true not-applicables such as mobile-only, RTL, AntD tokens, cross-tab sync, DOM-specific caches). Neither codebase implements them today:

- **Pixel Studio (Aseprite-style scope):** onion skin; per-layer timeline + playhead; tweening; dither/pattern tool; spray tool; shading tool; pixel-perfect mode; per-tool opacity; symmetry axes count/position; palette import/export (.gpl/.pal); palette reorder; layer duplicate / merge / rename / lock; frame reorder UI; canvas crop; flip/rotate canvas; tile mode; draggable guides/rulers; spritesheet layout options (cols/padding/grid); spritesheet metadata JSON; **animated GIF export (Pixel)**; export scale/upscale; Escape-to-deselect; zoom keys.
- **Image Studio (Photoshop-scope not in web):** merge-down / flatten / merge-visible; clipping mask / vector mask; smart objects / embedded images; selection expand/contract; additional tools (crop tool, text tool, clone, smudge, blur, dodge-burn, liquify brush, zoom tool).
- **Texture Studio:** duplicate layer; named PBR channel config / per-output settings; per-layer mask; non-linear gradient interpolation; drag-to-add types.
- **Plugin System:** runtime sandbox for plugin code; permission model; `minAppVersion` enforcement.
- **Canvas / Panels:** node grouping (groups are library-level only); right-click node context menu; sort/search/filter and thumbnails in the canvas library; duplicate project; share links; library hydration spinner; BroadcastChannel cross-tab sync; storage-usage cleanup.
- **AI / Settings:** prompt enhancer; slash commands; `{{input}}` text variables; system-theme following; custom accent; provider connection test; onboarding tour.

> Note on GIF export: it is a **net-new candidate in Pixel Studio** (web has no pixel GIF export) but a **regression in Texture Studio** (web ships `exportGif`, native has only an unwired encoder).

---

## Deviations by design

Intentional, structural differences (present but different), not defects — keep as-is unless a contract requires otherwise:

- **File paths vs browser download** — native writes exports to a **typed path prompt** (JSON envelope, PNG, sprite sheet) instead of `file-saver`/browser download or a native file dialog (`store.rs`, `library_export.rs`, `image_export.rs`, `pixel_export.rs`, `texture_export.rs`).
- **JSON + base64 vs localforage blobs** — project persistence is `library.json` + per-project JSON and `images\<id>.json` / `pixels\<id>.json` with embedded base64 PNG, instead of web localforage/IndexedDB blob stores (`document.rs`, `store.rs`, `pixel.rs`, `image.rs`).
- **No ZIP / blob packaging** — export is a single-project JSON envelope with `files: []`; import is JSON-only (validates app/version, re-mints ids) rather than ZIP with blob restore.
- **No wgpu / no WebGL** — the Texture Studio renders on the CPU (`renderer\*`, `cpu.rs`) with the same math instead of web WebGL2 ping-pong; Image/Pixel compositing is likewise CPU.
- **No webview** — the html plugin shows substituted text in a "sandbox · no webview" box instead of a sandboxed `iframe srcDoc`; the panorama plugin shows a 2:1 placeholder instead of a three.js viewer.
- **en-US only** — strings are inline English and error messages are hard-coded English; the web itself forces en-US at boot, so both are effectively single-locale (but the native port has no locale files at all).
- **Zero-provider / presets parity** — both hardcode `channelMode = local`; native config is stored as plaintext JSON in `%APPDATA%` (same posture as web localStorage, different location); API key is shown in plain text (web masks it).
- **Deterministic RNG** — noise/crystallize filters use a fixed seed where the web uses `Math.random` (reproducible output).
- **AI timeouts** — native adds 30s connect / 300s request timeouts (web relies on `AbortSignal` only).
- **Create menu grouping** — the prompt/generator/input popovers and the extensions dropdown are merged into one flat create menu; the minimap toggle is a compass button; the native toolbar adds a direct plugin-manager button.
- **Plugin add-ons** — native adds a kind badge on library cards, an extras-sidecar failure notice, a `builtin://official/<id>.js` scheme, and URL-id derivation helpers.
- **Dock model** — native uses flat boolean panel-visibility flags with show/hide + reset instead of the web's resizable/split/tabbed `DockArea` with persisted layout.
- **Silent-load library** — native loads synchronously from disk (no Spin), and filters/renders media in-file rather than streaming deltas.
- **Texture/VG deviations** — non-separable overlap handled in native blend; left ruler labels rotated 90°; native tags on nodes when media is missing; VG canvas pans on any mouse-down.
- **Canvas pan-middle** — native middle-drag pans only in the Pan tool (web pans in Select too); double-click on empty canvas creates a Text node as a stand-in.
- **Image Style popovers** — native reduces image/video/text settings popovers to summary labels rather than the full web editors; text style popover and text batch are absent.
  
