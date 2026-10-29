[CONTEXT] I am porting D:\Dev\open-canvas\web (React canvas app) to Rust and must preserve the exact UI structure/design. The canvas app entry is web/src/components/canvas/open-canvas.tsx; page is web/src/pages/canvas/index.tsx and project.tsx; canvas UI components are in web/src/components/canvas/ (toolbar, top bar, side panel, dock/*, nodes/*, workspace/* including pixel-studio and image-studio, floating toolbar, minimap, rulers, connections, node hover toolbar, many dialogs); plugins in plugins/canvas/*; layout/global chrome in web/src/components/layout/*.

[GOAL] Produce a complete inventory of every visible UI surface with its structure so I can scope a faithful Rust port and lay out the shell (top bar + toolbar + side panel + dock + canvas + overlays).

[DOWNSTREAM] I will use this to (a) define the v1 port scope, and (b) build the Rust/Slint component tree with the same regions and hierarchy.

[REQUEST] Return a layered inventory:
1. Top-level app shell: what regions exist (top nav, canvas top bar, toolbars, left/right panels, bottom dock, status bar, modals/overlays) and their relative placement + sizing behavior. Cite files.
2. Canvas canvas area: how the graph is rendered (canvas 2D? reactflow? custom?), node component anatomy (header/body/handles/badges/resize handles), connections/wires, grid, minimap, rulers, selection box, hover toolbars.
3. Every dialog/popover/modal under components/canvas (list each with one-line purpose).
4. The "workspace" studios (pixel-studio/*, image-studio) — list their panels (layers, channels, history, properties, adjustments, filters, paths, text, color picker, menus) with file paths.
5. Plugin-hosted UI + plugin manager.
6. Global chrome: app-top-nav, app-config-modal, version modal, mobile drawer.
For each surface note whether it is core (needed for a usable canvas) vs secondary. Skip tests and e2e. Give file paths for everything.
<!-- OMO_INTERNAL_INITIATOR -->

---

<analysis>
**Literal Request**: Produce a complete layered inventory of every visible UI surface in the open-canvas React app (web/src), with file paths, structure, and core-vs-secondary classification.

**Actual Need**: A porting blueprint — an exhaustive, hierarchical map of UI regions and components so the user can scope a v1 Rust/Slint port and directly translate the component tree (top bar + toolbar + side panel + dock + canvas + overlays).

**Success Looks Like**: A layered inventory (app shell regions, canvas rendering + node anatomy, dialogs/popovers, workspace studios, plugin UI, global chrome), each entry with absolute file path, one-line purpose/structure, and core/secondary tag — enough to build the Slint component tree without further discovery.
</analysis>

Let me begin with broad parallel reconnaissance of the project structure.

---

Now let me explore the nested directories and read the key entry files.

---

Let me read the page entries and core shell components in parallel.

---

Let me read the side panel, dock subsystem, connections, minimap, and rulers.

---

Let me read the canvas node component and toolbars, plus the rest of project.tsx JSX.

---

Let me read the rest of project.tsx and the workspace/panel components.

---

Let me read the final JSX render tree in project.tsx and the workspace studios.

---

The image-studio is large; let me target its render structure and read pixel-studio plus the workspace panels.

---

Now let me gather the canvas dialog/popover purposes, plugin structure, and global chrome.

---

Now the plugins, plugin host, and global chrome.

---

<results>
<files>
Top-level shell
- D:\Dev\open-canvas\web\src\layouts\user-layout.tsx - app root shell: AppTopNav column + AgentRuntime + routed content
- D:\Dev\open-canvas\web\src\router.tsx - routes: /canvas (library), /canvas/:id/:workspace?, /image, /pixel, /texture, /config
- D:\Dev\open-canvas\web\src\pages\canvas\project.tsx - CanvasWorkspace; the entire canvas screen composition (top bar + side panel + canvas + toolbars + all dialogs)
- D:\Dev\open-canvas\web\src\pages\canvas\index.tsx - canvas library (group sidebar + project grid)
- D:\Dev\open-canvas\web\src\components\canvas\open-canvas.tsx - the pan/zoom canvas surface
- D:\Dev\open-canvas\web\src\components\canvas\canvas-top-bar.tsx - canvas top bar
- D:\Dev\open-canvas\web\src\components\canvas\canvas-toolbar.tsx - bottom dock toolbar + zoom/extensions/create popovers + shortcuts modal
- D:\Dev\open-canvas\web\src\components\canvas\canvas-side-panel.tsx - left resizable project panel
- D:\Dev\open-canvas\web\src\components\canvas\canvas-switcher.tsx - project list inside side panel
- D:\Dev\open-canvas\web\src\components\canvas\canvas-node.tsx - node card anatomy (title, body, handles, badges, resize)
- D:\Dev\open-canvas\web\src\components\canvas\canvas-connections.tsx - SVG wire paths
- D:\Dev\open-canvas\web\src\components\canvas\canvas-mini-map.tsx - minimap
- D:\Dev\open-canvas\web\src\components\canvas\canvas-rulers.tsx - top/left rulers
- D:\Dev\open-canvas\web\src\components\canvas\canvas-floating-toolbar.tsx - shared floating bar primitives
- D:\Dev\open-canvas\web\src\components\canvas\canvas-node-hover-toolbar.tsx - per-node hover bar + node info modal
- D:\Dev\open-canvas\web\src\components\canvas\canvas-selection-toolbar.tsx - multi-select align/layout bar
- D:\Dev\open-canvas\web\src\components\canvas\dock\* - dockable panel system (layout/tabs/panel/window menu)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\* - image + pixel studios and their panels
- D:\Dev\open-canvas\web\src\components\layout\* - global chrome (top nav, config modal, version modal, mobile drawer, user actions)
- D:\Dev\open-canvas\plugins\canvas\* - plugin SDK + official plugins
</files>

<answer>

# Open-Canvas UI Inventory (React -> Rust/Slint port blueprint)

## 0. Rendering technology (answers "canvas 2D? reactflow? custom?")
Custom DOM + SVG. NOT ReactFlow, NOT canvas-2D for the graph.
- Pan/zoom: `open-canvas.tsx` writes `transform: translate() scale()` straight to the DOM during gestures (no per-frame React render); commits state on release. Grid is a CSS `background-image` (dots / lines / blank) whose `backgroundSize/Position` is updated per gesture. `GRID_SIZE = 48`.
- Nodes: absolutely-positioned `<div>`s inside the transformed content layer (`canvas-node.tsx`, `transform: translate(x,y)`).
- Wires/selection/snap-guides: a single 10000x10000 `<svg>` layer (project.tsx ~1909-1915) using cubic-bezier paths.
- Side panel/switcher + studio docks animate with `motion/react`.

---

## 1. Top-level app shell (regions, placement, sizing)

Source of truth: `layouts/user-layout.tsx` + `router.tsx` + `pages/canvas/project.tsx`.

Root layout (`user-layout.tsx`): `flex h-dvh overflow-hidden` -> column of **[AppTopNav] + [routed page]**; `<AgentRuntime/>` is mounted page-globally but renders no UI (it is the AI-agent bridge).

AppTopNav (`layout/app-top-nav.tsx`): `sticky top-0 z-20 h-14 border-b glass-surface`.
- Left: logo + app title; mobile hamburger (opens MobileNavDrawer).
- Center (absolute, `hidden md:flex`): workspace tabs canvas / image / pixel / texture (from `constant/navigation-tools.ts`).
- Right: `UserStatusActions` (settings gear -> config modal; canvas variant also shows `CanvasAccountUsage`).
- **Auto-hidden** when path matches `^\/(canvas|image|pixel|texture)\/[^/]+` (i.e. any editor view). CORE.

### Canvas project screen (`pages/canvas/project.tsx`, `CanvasWorkspace`) - regions in z/placement order:
1. **CanvasTopBar** (`canvas-top-bar.tsx`) - `absolute left-0 right-0 top-0 z-[80]`, `h-14`, frosted, border-b. Three zones: (left) side-panel toggle, Home, inline-editable title; (center) workspace tab pills; (right) UserStatusActions. CORE.
2. **CanvasSidePanel** (`canvas-side-panel.tsx`) - left column, `motion.div` width-animated (`CANVAS_SIDE_PANEL_MIN/MAX_WIDTH`, default persisted in `localStorage canvas-side-panel-width`), `z-[60]`, `border-r glass-surface`, drag-to-resize handle on right edge; on `max-md` becomes an absolute overlay (`top-16 bottom-0 max-w-[85vw]`). Contains `CanvasSwitcherTab`. CORE (secondary on mobile).
3. **Canvas section** (`relative min-w-0 flex-1 overflow-hidden`) - holds OpenCanvas + all floating overlays:
   - **OpenCanvas** graph surface (fills section).
   - **CanvasToolbar** (`canvas-toolbar.tsx`) - `absolute bottom-5 left-0 right-0 z-50 flex justify-center`; a `h-14` rounded dock. Buttons: export, tool toggle (select/pan), undo, redo, input-group, prompt-group, generator-group, extensions, zoom, node-list, conditional delete. Opens upward popovers (`bottom-[72px]`): create-menu, extensions, zoom panel; plus the shortcuts `Modal`. CORE.
   - **CanvasRulers** (`canvas-rulers.tsx`) - `z-[75]`, left ruler (20px wide, starts below 56px top bar) + top ruler. SECONDARY.
   - **Minimap** (`canvas-mini-map.tsx`) - `absolute bottom-24 left-6 z-50`, 240x160, toggle-driven. SECONDARY.
   - **CanvasNodeListPanel** (bottom-center, `bottom-[84px] z-[60] w-[250px]`) when node-list toggled. SECONDARY.
   - **CanvasNodeHoverToolbar / CanvasSelectionToolbar** - floating bars `z-[70]`. CORE (hover toolbar).
   - Hidden `<input type=file>` for image/video import.
   - Modal stack (see section 3).
4. **Ghost drag chip** (`canvas-drag-ghost`) - `fixed z-[120]`, follows cursor during drag of content nodes. SECONDARY.

Status bar: **none exists** (no dedicated bottom status bar; status is shown by the toolbar + node info bar). Note this for the port.

---

## 2. Canvas area details

### OpenCanvas (`open-canvas.tsx`)
- Root `relative h-full w-full overflow-hidden` with `theme.canvas.background`.
- Grid layer: `absolute inset-0 opacity-40`, background-image = radial dots OR linear lines (mode from config `canvasBackgroundMode`; "blank" = none). Dot size scales with zoom.
- Content layer: `absolute origin-top-left` with `willChange: transform`.
- Interaction: wheel zoom (`factor = 1.1^(delta/100)`, clamp 0.05-5) anchored at cursor; pan on middle-mouse or pan-tool background drag or Ctrl/Space temporary tool swap; `onCanvasMouseDown` starts marquee; drop handled by `handleDrop`; wheel-scroll suppressed except on overlay targets (`isCanvasOverlayTarget`).

### Node anatomy (`canvas-node.tsx`, `CanvasNode`)
- **Title**: rendered ABOVE the card, `absolute left-3 top-[-28px]`, not for image/video content nodes; double-click to rename (inline input). Media nodes show `MediaInfoBar` instead (title · WxH · bytes · cost + info button).
- **Card body**: `rounded-3xl border-2`; frosted glass for non-media (`canvas-glass-card`); border color = selection blue when active, muted when related, dashed blue outline for drop targets.
- **Content** (`NodeContent`): dispatches by type -> `renderNodeContent` (config/image/video/prompt/assets), `ImageStackNodeContent`, Text batch content, Loading spinner, Error + retry, or a registered plugin `Content`, else missing-plugin placeholder.
- **Handles**: 4 corner `ResizeHandle`s (`-left/-top -14px`, only when unlocked) + bottom-right `ResizeGrip` (only when hovered/selected). Min size 220x160; keep-ratio for image/video/plugin spec.
- **Connection dots** (`ConnectionHandleDot`): left = target (not for image/stack), right = source (`hasSourceHandle` default true); `size-12` hit area, dot hidden until hover/select/connecting.
- **Badges**: lock badge (bottom-left), batch frame with stacked-card ghost behind text/image batches, batch toggle button (top-right).
- **Panel**: `showPanel` renders a 600px panel below the node (`absolute left-1/2 top-full w-[600px] z-[70]`) - see panel list below.
- Selection z-index: selected -> `z-50`; others `z-10`.

### Connections (`canvas-connections.tsx`)
- `ConnectionPath` (memo): invisible 16px-wide hit path + 2/3px visible bezier (`connectionGeometry`), optional relation label text (reference index). `ActiveConnectionPath`: dashed bezier drawn while dragging a connection.
- Nodes = `{id, fromNodeId, toNodeId}` in `types/canvas`.

### Selection / guides / cut
- Marquee box: inline SVG rect in project.tsx (dashed).
- Multi-selection toolbar + bounds SVG (`canvas-selection-toolbar.tsx`).
- Snap guides: SVG lines (`snapGuides`) z-45.
- Cut tool: Ctrl-drag polyline deletes crossed connections (`registerCanvasRightButtonTool("ctrl", ...)`).

### Node panels (rendered under a node, `renderNodePanel` / `renderNodeContentPanel` in project.tsx)
- `canvas-node-prompt-panel.tsx` `CanvasNodePromptPanel` - prompt composer (chip input + expanded editor Modal), ModelPicker, image/video/text settings popovers, generate/stop. CORE.
- `canvas-config-node-panel.tsx` `CanvasConfigNodePanel` - image/video generation config (ModelPicker + `ImageSettingsPanel`/`VideoSettingsPanel` + generate/stop). CORE.
- `prompt-node-panel.tsx` `PromptNodePanel` - prompt node inline tag editor. CORE.
- `nodes/video-prompt-node-content.tsx` `VideoPromptNodeContent` - first/last/reference frame slots. CORE.
- `nodes/assets-node-content.tsx` `AssetsNodeContent` - asset folder browser. SECONDARY.
- `nodes/image-stack-node-content.tsx` `ImageStackNodeContent` - stack thumbnails + view-all/split. CORE.
- `nodes/prompt-node-content.tsx` `PromptContent`, `nodes/asset-file-preview.tsx` `AssetFilePreview` (modal preview).
- Plugin panels via `renderPluginPanel` (`use-plugin-host.tsx`).

---

## 3. Dialogs / popovers / modals under components/canvas

Canvas-level modals (owned by project.tsx):
- `canvas-import-dialog.tsx` `CanvasImportDialog` - import canvas project JSON/export file. SECONDARY.
- `canvas-node-hover-toolbar.tsx` `CanvasNodeInfoModal` - node info + raw JSON viewer (Segmented info/JSON). SECONDARY.
- `canvas-plugin-manager-modal.tsx` `CanvasPluginManagerModal` - install/enable/update/uninstall plugins (tabs: official / local / third-party). CORE for plugin support.
- `canvas-node-crop-dialog.tsx` `CanvasNodeCropDialog` - crop image. CORE (image editing).
- `canvas-node-mask-edit-dialog.tsx` `CanvasNodeMaskEditDialog` - paint/erase mask + export mask. SECONDARY.
- `canvas-node-split-dialog.tsx` `CanvasNodeSplitDialog` - split by rows/cols/lines. SECONDARY.
- `canvas-node-resolution-dialog.tsx` `CanvasNodeResolutionDialog` - upscale (local algorithm vs AI tabs). SECONDARY.
- `canvas-image-analysis-dialog.tsx` `CanvasImageAnalysisDialog` - smart-crop / image analysis. SECONDARY.
- `canvas-node-segment-dialog.tsx` `CanvasNodeSegmentDialog` - segmentation (mask output). SECONDARY.
- `canvas-node-angle-dialog.tsx` `CanvasNodeAngleDialog` - generate camera angle variant. SECONDARY.
- Image preview Modal (inline in project.tsx, ~line 2102) - single image preview or `image-stack-preview.tsx` `ImageStackPreview` gallery. CORE.
- `canvas-toolbar.tsx` shortcuts `Modal`. SECONDARY.

Popovers (anchored controls):
- `canvas-image-settings-popover.tsx` `CanvasImageSettingsPopover` - image size/quality/count. CORE.
- `canvas-video-settings-popover.tsx` `CanvasVideoSettingsPopover` - video seconds/resolution/audio/watermark. CORE.
- `canvas-text-settings-popover.tsx` `CanvasTextSettingsPopover` - text reasoning effort/count. CORE.
- `canvas-text-style-popover.tsx` `CanvasTextStylePopover` - text style (bold/italic/font/size/color). SECONDARY.
- `smart-canvas-settings-popover.tsx` `SmartCanvasSettingsPopover` - board ratio/resolution/background (used by studios). CORE (studio).
- `canvas-node-layer-popover.tsx` `CanvasNodeListPanel` + `CanvasNodeLayerPopover` - node/layer list, reorder, lock/hide, bulk rename. SECONDARY.
- `canvas-account-usage.tsx` `CanvasAccountUsage` - usage/account Popover in top bars. SECONDARY.

Input helpers:
- `canvas-prompt-chip-input.tsx` `CanvasPromptChipInput`, `canvas-resource-mention-textarea.tsx` `CanvasResourceMentionTextarea` - mention/chip prompt inputs. CORE.
- `canvas-image-toolbar-tools.tsx` - quick image tool registry/config (`buildImageToolbarTools`). SECONDARY.
- `canvas-refresh-shell.tsx` `CanvasRefreshShell`, `canvas-node-generation.ts`, `use-image-editor-viewport.ts` - support utilities (no standalone UI).

---

## 4. Workspace studios

### 4a. Image Studio (Photoshop-like) - `workspace/image-studio.tsx` (default export `ImageStudio`)
Mounted from `pages/image/index.tsx` (`/image/:id`). Layout: top bar (`h-11 glass-surface`, back, board title, SmartCanvasSettingsPopover, PsMenus, DockWindowMenu, contextual option sliders, zoom %, Export) -> `DockArea` (left tool rail / center canvas / right+bottom docks) -> `PsFilterDialog` overlay.
Left tool rail: Move, Hand, Pen, Direct-select, Marquee, Ellipse-marquee, Lasso, Polygon-lasso, Wand, Quick-select, Brush, Eraser, Bucket, Gradient, Shape, Eyedropper + zoom buttons.
Center canvas: `BoardLayersView`, `PsChannelPreview`, live stroke canvas, `PsGrid`, `PsGuides`, `PsMarchingAnts`, `PsDraftOverlay`, `PsSnapLines`, `PsPathOverlay`, `PsTransformHandles`, `PsRulers`.

Dock panels (`PS_DOCK_PANELS`, image-studio.tsx:125-138):
- layers -> `ps-layers-panel.tsx` `PsLayersPanel` (embeds `ps-fx-panel.tsx` `PsFxPanel` layer effects). CORE.
- channels -> `ps-channels-panel.tsx` `PsChannelsPanel` (+ `PsChannelPreview`). SECONDARY.
- paths -> `ps-paths-panel.tsx` `PsPathsPanel`. SECONDARY.
- history -> `ps-history-panel.tsx` `PsHistoryPanel` (+ `usePsHistory`). CORE.
- actions -> `ps-actions-panel.tsx` `PsActionsPanel` (+ `usePsActionRecorder`). SECONDARY.
- properties -> `ps-properties-panel.tsx` `PsPropertiesPanel` (+ `ps-text-panel.tsx` `PsTextPanel` for text layers; `ps-adjustment-editor.tsx` `PsAdjustmentEditor`). CORE.
- adjustments -> `ps-adjustments-panel.tsx` `PsAdjustmentsPanel`. CORE.
- assets (resource pool) -> `ps-resource-pool-panel.tsx` `PsResourcePoolPanel`. CORE.
- color -> `ps-asset-panels.tsx` `PsColorPanelBody` (+ `ps-color-picker.tsx` `PsColorPicker`/`PsColorFields`/`PsForegroundBackground`). CORE.
- gradients -> `PsGradientsPanel`. SECONDARY.
- patterns -> `PsPatternsPanel`. SECONDARY.
- brushes -> `PsBrushesPanel`. SECONDARY.

Shared/support: `ps-menus.tsx` `PsMenus` (Select/Edit/Image/Filter/View menus + Image Size / Canvas Size / Rotate / Numeric Transform modals), `ps-filter-dialog.tsx` `PsFilterDialog` (all filters), `ps-overlays.tsx` (rulers/grid/guides/marching-ants/path/transform overlays), `ps-board-view.tsx` `BoardLayersView` + `layerBlendStyle`/`psLayerFrame`/`useResolvedPsLayerUrls`, `ps-param-editors.tsx` `PsCurveEditor`/`PsGradientStopsEditor`, `studio-chrome.ts` (shared class constants).

### 4b. Pixel Studio (Aseprite-like) - `workspace/pixel-studio.tsx` (default export `PixelStudio`)
Mounted from `pages/pixel/index.tsx` (`/pixel/:id`). Top bar (`h-11`): back, title, File/Edit/View dropdown menus, DockWindowMenu, zoom -, %, zoom +, fit, play/pause, Export. Center: main `<canvas>` + checker overlay + snap canvas. Dock panels (`PIXEL_DOCK_PANELS`, `pixel-studio-shared.ts:39-46`):
- tools (left), palette (left), layers (right), frames (right), properties (right), preview (bottom). CORE.
Tool set + hotkeys in `pixel-studio-shared.ts` `TOOLS` (pencil, eraser, bucket, line, rect, ellipse, eyedropper, select, move, hand); uses `ps-color-picker.tsx` `PsColorFields`. Blend modes in `BLENDS`. CORE.

### 4c. Studio output
- `components/studio/studio-output-modal.tsx` `StudioOutputModal` - download vs "output to canvas" for image/audio/text resources. CORE (studios).

---

## 5. Plugin-hosted UI + plugin manager

Host side:
- `web/src/pages/canvas/hooks/use-plugin-host.tsx` - builds `pluginHost` (canvas + AI access), `renderPluginPanel`, `buildNodeToolbarItems`; loads installed plugins on mount.
- `web/src/lib/canvas/node-registry.ts` - `getNodeDefinition`, `listNodeDefinitions`, `getNodePluginId`, `useNodeRegistryVersion` (built-in + plugin defs registered here).
- `web/src/components/canvas/nodes/builtin-nodes.tsx` `registerBuiltinNodes()` - registers the built-in node definitions.
- `web/src/lib/canvas/plugin-loader.ts` / `plugin-registry.ts` - install/load/update plugins + official registry fetch.
- `web/src/stores/canvas/use-plugin-store.ts` - installed plugin state.
- `web/src/types/canvas-plugin.ts` - host contract.
- Plugin UI appears via: node `Content` (inside `canvas-node.tsx`), node `Panel` (rendered under node), node `toolbar()` items (added to `CanvasNodeHoverToolbar via extraTools`), and `interactionToggle` (move/interact switch).
- Plugin manager UI: `canvas-plugin-manager-modal.tsx` `CanvasPluginManagerModal`.

Plugin packages (`plugins/canvas/`):
- `sdk/` - public SDK: `src/index.ts`, `src/define-plugin.ts`, `src/types.ts`, `src/runtime.ts`, `src/jsx-runtime.ts`, `src/jsx-dev-runtime.ts`.
- `registry/` - `build.mjs` builds all official plugins + `official-plugins.json` manifest.
- `template/` - starter plugin.
- `markdown/src/index.tsx` - Markdown render/edit node.
- `svg/src/index.tsx` - SVG node (transparent bg).
- `html/src/index.tsx` - sandboxed iframe HTML node with `{{input}}`.
- `panorama/src/index.tsx` - three.js 360° equirectangular viewer node.
- `sticky-note/src/index.tsx` - colored sticky note node.

---

## 6. Global chrome

- `layout/app-top-nav.tsx` `AppTopNav` - global top nav (logo, workspace tabs, right actions); hides in editors. CORE.
- `layout/user-status-actions.tsx` `UserStatusActions` - settings gear + canvas account usage. CORE.
- `layout/app-config-modal.tsx` `AppConfigModal` + `AppConfigPanel` - global settings modal; tabs: providers (channels), models, appearance, generation, local-models, local-storage, cost, agent, logs, about. CORE (config).
  - Sections: `config-providers.tsx` `ConfigProviders`, `config-models.tsx` `ConfigModels`, `config-generation-cost.tsx` `ConfigGenerationCost`, `config-local-models.tsx` `ConfigLocalModels`, `config-local-storage.tsx` `ConfigLocalStorage`, `config-runtime-logs.tsx` `ConfigRuntimeLogs`, `config-agent-audit.tsx` `ConfigAgentAudit`.
- `layout/version-release-modal.tsx` `VersionReleaseModal` - version button + release/update modal (also inlined in config "about" tab). SECONDARY.
- `layout/mobile-nav-drawer.tsx` `MobileNavDrawer` - AntD Drawer of nav tools (mobile). SECONDARY.
- `layout/app-providers.tsx` `AppProviders`, `layout/client-root-init.tsx` `ClientRootInit`, `layout/analytics-tracker.tsx` `AnalyticsTracker`, `components/agent/agent-runtime.tsx` `AgentRuntime` - non-visual infrastructure / bridges.
- Shared setting panels used by canvas panels + studio: `components/image-settings-panel.tsx` (`ImageSettingsPanel`, `ImageSettingsTheme`), `components/video-settings-panel.tsx` (`VideoSettingsPanel`), `components/text-settings-panel.tsx`, `components/model-picker.tsx` (`ModelPicker`). CORE.

---

## Core-vs-secondary summary for the v1 port
- **Core (needed for a usable canvas):** UserLayout/AppTopNav (or a canvas-only top bar), CanvasTopBar, CanvasSidePanel+Switcher, OpenCanvas (grid/pan/zoom), CanvasNode (card/title/handles/connection dots), CanvasConnections, CanvasToolbar (create/undo/redo/zoom/tool), CanvasNodeHoverToolbar, node prompt/config panels + ModelPicker + image/video/text settings popovers, image preview modal, PromptChipInput/MentionTextarea, node registry + plugin host + plugin manager modal, config modal (min: providers/models), theme tokens (`lib/canvas-theme.ts`).
- **Secondary:** rulers, minimap, node-list panel, selection toolbar, cut tool, all image-edit dialogs (crop/mask/split/segment/angle/resolution/analysis), layer popover, version modal, mobile drawer, account usage, studios (phase 2).
- **Studios (phase 2/3):** Image Studio (12 dock panels) and Pixel Studio (6 dock panels) are large independent apps; port them after the graph shell.

### Notes for the Slint tree
- Slint has no DOM compositor; replicate pan/zoom by animating a container's transform offset/scale property (equivalent to `applyViewport`).
- Draw connections/selection with Slint `Path`/`Rectangle` in an overlay instead of SVG.
- The dock system (`dock-layout.ts`) is a pure data model (groups per left/right/bottom edge, sizes, split ratio, localStorage persistence) - it ports cleanly as a Rust struct + reducer with no UI dependency.
- There is no bottom status bar to port; status is inside the toolbar and node info bars.
</answer>

<next_steps>
1. Start the Rust/Slint shell with the four regions from `project.tsx` return: CanvasTopBar (fixed 56px top) -> row[ CanvasSidePanel (240-360px resizable) | Canvas section (flex) ] -> floating bottom CanvasToolbar; then overlay rules (rulers z75, hover bar z70, minimap z50, modals top).
2. Port `lib/canvas-theme.ts` tokens first (every component reads `theme.node.*`, `theme.toolbar.*`, `theme.canvas.*`) so the Rust components can bind to one theme struct.
3. Port the dock data model (`dock-layout.ts`) as a Rust struct early; both studios depend on it.
4. If you want the exact per-panel internal layout (rows, sliders, buttons) of the Image/Pixel studio panels before porting, ask me to expand section 4 file-by-file (each `ps-*.tsx` is self-contained).
</next_steps>
</results>