[CONTEXT] I am porting D:\Dev\open-canvas\web (React canvas app) to Rust. Stores are in web/src/stores/ (use-canvas-store, canvas/use-canvas-ui-store, canvas/use-plugin-store, use-image-store, use-pixel-store, use-theme-store, use-config-store, etc.), types in web/src/types/canvas.ts, canvas lib in web/src/lib/canvas/**, persistence via localforage (web/src/lib/localforage-storage.ts).

[GOAL] Produce a precise model of the canvas domain so a Rust core can implement the same data model, node system, persistence and interactions.

[DOWNSTREAM] I will define Rust structs/enums for nodes, edges, viewport, selection, history, and design the persistence (serde JSON to disk).

[REQUEST] Return with file:line provenance:
1. Canvas data model: CanvasNode / node types (builtin node kinds + fields), edges/connections shape, viewport/transform, selection, groups, z-order, node geometry/size rules.
2. Store shape (use-canvas-store): state fields, actions, undo/redo history (what is snapshotted), dirty tracking.
3. UI store (use-canvas-ui-store): panels, dock tabs, tool state, theme, zoom, active tools.
4. Node registry / factory (node-registry.ts, canvas-node-factory.ts): how node kinds are declared and what each renders.
5. Persistence: what is saved where (localforage keys), project/document schema, import/export formats (canvas-export.ts).
6. Interaction model: pointer tools (canvas-pointer-tools.ts), drag, connect, marquee, pan/zoom, keyboard shortcuts (list them).
7. Which features require external services (AI image/video generation, OCR, background removal) vs pure local.
Return structured lists per section. Skip tests/e2e.
<!-- OMO_INTERNAL_INITIATOR -->

---

I'll start by exploring the repository structure and launching parallel searches across all the areas you mentioned.

---

Let me explore the stores/canvas directory and key lib files in parallel.

---

Now let me read the core canvas files in parallel.

---

The editing state (nodes/selection/history) isn't in these project-library stores. Let me find the canvas editing layer and node definitions.

---

Now the editing layer: `use-canvas-document.ts`, `use-canvas-history.ts`, and node definitions. Reading key files in parallel.

---

Now the remaining stores and the big canvas page. Let me grep `project.tsx` for state/tools/shortcuts and read the other stores.

---

Let me read the key sections of `project.tsx` and the config/services/persistence layers.

---

Now the interaction handlers and persistence/service layers.

---

Now zoom/pan, dock/UI panels, background-removal, plugin system, and persistence keys.

---

Zoom/pan search and the canvas UI shell. Let me locate wheel handling and read the toolbar plus render section.

---

Let me read the canvas render section and remaining handlers in `project.tsx`.

---

Now external services, import/export, and plugin runtime.

---

Now generation/external services and remaining helpers.

---

Let me grab the remaining external-service handlers and supporting types.

---

Final verification of where the image-tool functions live (OCR, bg removal, segment, AI upscale).

---

<analysis>
**Literal Request**: A file:line-provenanced model of the open-canvas web client's canvas domain across 7 areas (data model, store, UI store, node registry, persistence, interactions, external-vs-local services) to drive a Rust port.
**Actual Need**: A complete, faithful spec of the in-memory/wire data structures and behavior so Rust structs/enums, serde schemas, and interaction reducers can be written without re-reading the TS codebase.
**Success Looks Like**: Every node type, field, default, persistence key, history rule, interaction, and network boundary identified with exact provenance, including the important discovery that editing state lives in `project.tsx` (not in the Zustand stores named).
</analysis>

I read the stores, types, `lib/canvas/**`, `pages/canvas/**`, and services. Key structural finding: **the Zustand `stores/canvas/*` are not the per-canvas editing state** — `use-canvas-store` is the project *library* (folders of `CanvasProject`s), `use-canvas-ui-store` is only project-list editing, and the actual editing state (nodes/connections/viewport/selection/history) is `useState` in `pages/canvas/project.tsx` plus the two hooks `use-canvas-document.ts` and `use-canvas-history.ts`.

---

# 1. Canvas Data Model

All core types are in `D:\Dev\open-canvas\web\src\types\canvas.ts`.

## Node kinds
- `CanvasNodeType` enum (canvas.ts:16-26): `image`, `image-stack`, `text`, `prompt`, `video-prompt`, `image-generation`, `video-generation`, `video`, `assets`.
- `CanvasNodeTypeId = CanvasNodeType | (string & {})` — open strings; plugins use `"<pluginId>:<name>"` (canvas.ts:28-29).
- `CanvasWorkspace = "canvas" | "image" | "pixel"` (canvas.ts:6-8); `CANVAS_WORKSPACES` (canvas.ts:8).

## Node core
- `CanvasNodeData` (canvas.ts:263-271): `{ id, type: CanvasNodeTypeId, title, position: {x,y}, width, height, metadata? }`.
- `CanvasNodeMetadata` (canvas.ts:209-261) — the union of all per-kind fields: `content, composerContent, prompt, status, errorDetails, fontSize, lineHeight, fontFamily, fontWeight("normal"|"bold"), italic, textAlign("left"|"center"|"right"), textColor, generationMode("text"|"image"|"video"), generationType("generation"|"edit"), model, reasoningEffort("auto"|"low"|"medium"|"high"|"xhigh"), size, quality, background, count, textCount, texts: CanvasNodeText[], primaryTextId, seconds, vquality, generateAudio, watermark, videoMode("frames"|"reference"), videoSlots, references: string[], naturalWidth, naturalHeight, freeResize, images: CanvasNodeImage[], primaryImageId, storageKey, thumbnail, thumbnailKey, mimeType, bytes, durationMs, videoTaskId, videoTaskProvider("openai"|"plugin"|"openrouter"), blendMode, opacity, interactive, locked, hidden, assetSource("folder"|"cache"|"studio"), assetStudioProjects, canvasTitle`.
- `CanvasNodeImage` (canvas.ts:35-47): `{ id, status, errorDetails?, content, storageKey?, thumbnail?, thumbnailKey?, naturalWidth, naturalHeight, bytes, mimeType }`.
- `CanvasNodeText` (canvas.ts:49-54): `{ id, status, errorDetails?, content }`.
- `CanvasNodeStatus = "idle"|"success"|"loading"|"error"` (canvas.ts:31); runtime constants in `canvas-node-constants.ts:4-7`.
- `CanvasVideoMode = "frames"|"reference"` (canvas.ts:56); `CanvasVideoSlot = "firstFrame"|"lastFrame"|"reference"` (canvas.ts:58); `CanvasVideoSlots = { firstFrame?, lastFrame?, references?: string[] }` (canvas.ts:61-65) — stores **node ids**, never payloads.
- `CanvasAssetSource = "folder"|"cache"|"studio"` (canvas.ts:67); `CanvasAssetStudioProjects = { image?: string[]; pixel?: string[] }` (canvas.ts:70-73).

## Defaults / sizes / factory
- `NODE_DEFAULT_SIZE` and `NODE_SPECS` in `D:\Dev\open-canvas\web\src\constant\canvas.ts`: sizes (canvas.ts:14-24): image 340×240, image-stack 420×320, text 340×240, prompt 340×240, video-prompt 340×300, image-generation 412×576, video-generation 412×576, video 420×236, assets 360×320. Initial metadata per kind (canvas.ts:33-43), e.g. text `{content:"",status:"idle",fontSize:14}`, video-prompt `{prompt:"",status:"idle",videoMode:"frames"}`.
- `getNodeSpec` resolves built-ins directly, plugins via registry (canvas.ts:46-50).
- Factory `createCanvasNode` (`lib\canvas\canvas-node-factory.ts:9-25`): id = `${type}-${Date.now()}-${rand}`; **position is centered** by subtracting `width/2, height/2` from the requested point; metadata merged `{...spec.metadata, ...metadata}`. Helpers: `imageMetadata`, `videoMetadata`, `referenceUrl`, `buildImageGenerationMetadata`, `applyNodeConfigPatch` (canvas-node-factory.ts:27-57; the patch re-derives size from a ratio string for Image/Video and re-centers).
- Size rules: `fitNodeSize(w,h,maxW=640,maxH=640)` scales down preserving ratio (canvas-node-size.ts:1-6); `nodeSizeFromRatio("WxH", baseW, baseH)` rejects ratios <0.25 or >4 (canvas-node-size.ts:8-16).

## Geometry / z-order / selection
- `nodeBounds`, `CANVAS_GRID_SIZE = 16`, `snapDragToGuides` (snaps to other nodes' left/center/right edges, else grid), `nodeCenterInside`, `isNodeLocked`/`isNodeHidden`, `normalizeConnection` (`lib\canvas\canvas-node-geometry.ts:5-134`).
- **Z-order = index order in the `nodes` array**; `moveNodeLayer(id,"up"|"down")` splices (`pages\canvas\project.tsx:1019-1029`). Rendering iterates `visibleNodes` in order (`project.tsx:1917`).
- `SelectionBox` (canvas.ts:299-306): `{ startWorldX, startWorldY, currentWorldX, currentWorldY, additive, initialSelectedNodeIds }`. Selection is a `Set<string>` of node ids plus optional `selectedConnectionId` (`project.tsx:233-234`).
- **Groups**: there is no node-group concept. `CanvasGroup` (use-canvas-store.ts:21-25) is a project **folder**. (PS layer grouping `children: string[]` exists only in the image studio, canvas.ts:206.)
- `CanvasScope` (canvas.ts:281-285): `{ writeProjectId, role:"character"|"location"|"plot"|"graph", entityId? }` marks a project owned by a writing project rather than the library.

## Edges / connections
- `CanvasConnection` (canvas.ts:273-278): `{ id, fromNodeId, toNodeId, relation? }`.
- Relation inference: `SOURCE_RELATIONS` maps source type → label key (text/prompt/video-prompt→`prompt`, video→`video-reference`, image→`reference`) (`lib\canvas\canvas-connections.ts:7-13`); `inferConnectionRelation` returns undefined (canvas-connections.ts:21-23), so `relation` is effectively always derived.
- Geometry: cubic Bezier from right-mid of `from` to left-mid of `to`, curvature `max(|dx|*0.5, 50)` (canvas-connections.ts:25-32); `connectionCrossesStroke` samples 24 segments for the cut tool (canvas-connections.ts:34-80).
- Connection legality `normalizeConnection` (canvas-node-geometry.ts:121-134): rejects self, generation→generation, anything→Image; orients generation nodes as sinks; enforces `isPromptConnectionAllowed` (canvas-node-geometry.ts:103-119).
- Video slot binding rules: `bindVideoSlot` (canvas-video-slots.ts:19-39) + drop rules `CANVAS_DROP_RULES` (canvas-drop-bindings.ts:8-13): image→assets `collect`, image→image-stack `collect`, image→video-prompt `slot`, video→video-prompt `slot`.

---

# 2. Store Shape — `use-canvas-store` (project library)

`D:\Dev\open-canvas\web\src\stores\canvas\use-canvas-store.ts`. This is the **library/registry of projects**, not the live canvas.

**State** (use-canvas-store.ts:34-53):
- `hydrated: boolean`, `projects: CanvasProject[]`, `groups: CanvasGroup[]`, `deletedProjects: CanvasDeletedProject[]`, `remoteSync: {projectId, nonce} | null`.
- `CanvasProject` (use-canvas-store.ts:9-19): `{ id, title, createdAt, updatedAt, groupId?, nodes: CanvasNodeData[], connections: CanvasConnection[], viewport: ViewportTransform, scope? }`.
- `CanvasGroup` (21-25): `{id,name,createdAt}`; `CanvasDeletedProject` (27-30): `{id,deletedAt}`.

**Actions** (use-canvas-store.ts:40-52): `createProject(title?,groupId?)`, `importProject(partial)`, `openProject(id)`, `renameProject`, `deleteProjects(ids)`, `replaceProjects`, `reorderProjects`, `updateProject(id, patch)` (patch limited to nodes/connections/viewport/groupId/scope and bumps `updatedAt`), `createGroup`, `renameGroup`, `deleteGroup`, `setProjectGroup`, `applyRemote`.

**Persistence** (use-canvas-store.ts:57-68, 201-227): key `open-canvas:canvas_store`, debounced localforage (`createDebouncedPersist(["projects","groups","deletedProjects"])`, 400ms), `partialize` persists only those 3 arrays; on rehydrate repairs missing/invalid `groupId` (213-225). Cross-tab sync via `BroadcastChannel("open-canvas:canvas-store")` with a per-instance `SENDER_ID` (59-66, 230-231).

**Undo/redo** — NOT in this store. It is `D:\Dev\open-canvas\web\src\pages\canvas\hooks\use-canvas-history.ts`:
- Snapshots are `{ nodes, connections }` only (`CanvasHistoryEntry`, use-canvas-history.ts:5-8).
- A debounced effect (180ms) commits the *previous* entry to `past` (capped at 50 via `past.slice(-49)`), clears `future` (use-canvas-history.ts:53-77).
- `undoCanvas`/`redoCanvas` move entries and `applyHistory` sets nodes+connections and **clears selection** (use-canvas-history.ts:79-110).
- `historyPausedRef` suppresses history during drags (`project.tsx:870,918`); `resetHistory` on project load (project.tsx:344).

**Dirty tracking**: `updateProject` stamps `updatedAt` (use-canvas-store.ts:151); the page persists nodes+connections on change (`project.tsx:363-370`) and viewport via a 500ms debounce (`project.tsx:394-404`); `skipPersistRef` avoids echo loops on remote sync (`project.tsx:365-368`).

---

# 3. UI Store — `use-canvas-ui-store` (and where the real UI state lives)

`D:\Dev\open-canvas\web\src\stores\canvas\use-canvas-ui-store.ts` is **only project-list editing/selection** (use-canvas-ui-store.ts:3-27):
- `editingProjectId`, `editingProjectTitle`, `selectedProjectIds: string[]`, `selectedGroupId` + `startEditingProject/setEditingProjectTitle/stopEditingProject/toggleSelectedProjectId/removeSelectedProjectIds/setSelectedGroupId`.

The **canvas workspace editing state is component-local `useState`** in `D:\Dev\open-canvas\web\src\pages\canvas\project.tsx`:
- Core (project.tsx:228-243): `nodes, connections, viewport, canvasTool("select"|"pan", default `"pan"`), size, selectedNodeIds:Set, selectedConnectionId, hoveredNodeId, connectingParams:ConnectionHandle|null, connectionTargetNodeId, mouseWorld, selectionBox, cutStroke, runningNodeId, isMiniMapOpen, projectLoaded`.
- Dialogs/UI (project.tsx:247-273): `toolbarNodeId, nodeImageSettingsOpen, dialogNodeId, infoNodeId, pluginManagerOpen, cropNodeId, maskEditNodeId, splitNodeId, resolutionNodeId, analyzeNodeId, segmentNodeId, angleNodeId, previewNodeId, previewImageId, titleEditing, titleDraft, expandedBatchNodeIds, isNodeDragging, isNodeResizing, dropTargetAssetsNodeId, dropTargetStackNodeId, dropSlotState, snapGuides, ghostMeta, isNodeListOpen`.

Related UI stores/constants:
- **Theme**: `use-theme-store.ts:1-19` — `theme:"light"|"dark"`, `setTheme`; persist key `open-canvas:theme_store`. Resolved via `canvasThemes[theme]` (`lib\canvas-theme.ts`).
- **Zoom** = `viewport.k`; clamp **0.05–5** (`use-canvas-document.ts:193-203`); toolbar slider 5–500% (`canvas-toolbar.tsx:336-346`).
- **Left side panel**: `use-canvas-side-panel-store.ts:1-53` — `width` (220–480), `panelOpen`, `panelMounted`, `panelClosing`; localStorage keys `canvas-side-panel-width`, `canvas-side-panel-open`.
- **Dock layout** (used by image/pixel studios): `components\canvas\dock\dock-layout.ts` — `DockLayout { groups: {left,right,bottom: DockGroup[]}, sizes, split }`, edges/min sizes (dock-layout.ts:30-64), persisted to `localStorage` prefix `open-canvas:studio-dock:v4:<studio>` (dock-layout.ts:41,116-122).
- **Node registry version**: `useNodeRegistryVersion` (node-registry.ts:12).

---

# 4. Node Registry / Factory

## Registry — `D:\Dev\open-canvas\web\src\lib\canvas\node-registry.ts`
- Internal `definitions: Map<string, CanvasNodeDefinition>` + `ownerByType: Map<type, pluginId>` (node-registry.ts:8-9).
- `registerNodeDefinitions(defs, pluginId="builtin")` (17-23), `unregisterPluginNodes(pluginId)` (25-32), `getNodeDefinition(type)` (34-36), `getNodePluginId` (38-40), `listNodeDefinitions` (42-44), `isRegisteredNodeType` (46-48), `getNodeSpec` with fallback `{340,240}` (50-57), `isBuiltinNodeType` (59-61). `bump()` increments the version store on any change (11-15).

## Definition contract — `D:\Dev\open-canvas\web\src\types\canvas-plugin.ts`
`CanvasNodeDefinition` (canvas-plugin.ts:101-126): `{ type, title, icon, description?, defaultSize:{width,height}, defaultMetadata?, minimapColor?, showInCreateMenu?(default true), hasSourceHandle?(default true), hidePanel?, transparentBackground?, autoOpenPanel?, useBuiltinPanel?:{mode:"image"|"video"|"text", promptPrefix?, writeBackToSelf?}, interactionToggle?, forceInteractive?, keepAspectRatio?, resource?, Content?, Panel?, toolbar?, onDoubleClick? }`.
- Plugin host context `CanvasNodeContext` (canvas-plugin.ts:42-68): `node, theme, scale, isSelected, updateMetadata, updateNode, getNode/getNodes/getConnections/getUpstream/getDownstream, applyOps, emit/on, ai (generateImage/Video/Text, listModels, defaultModel), openPanel/closePanel, storage`.
- `CanvasPlugin` (canvas-plugin.ts:138-147): `{ id, name, version, description?, minAppVersion?, css?, nodes, setup? }`.

## Built-ins — `D:\Dev\open-canvas\web\src\components\canvas\nodes\builtin-nodes.tsx`
`BUILTIN_DEFINITIONS` (builtin-nodes.tsx:20-33): one entry per `CanvasNodeType` with `icon`, `minimapColor`, and flags:
- text `#` none; prompt `#eab308`; video-prompt `#fb923c`; image `#10b981` + `keepAspectRatio: (n)=>!n.metadata?.freeResize`; image-stack `#10b981` + `hasSourceHandle:false` + `transparentBackground:true`; video `#f97316` + `keepAspectRatio`; image-generation `#f472b6` + `hasSourceHandle:false` + `useBuiltinPanel:{mode:"image"}` + keepAspectRatio; video-generation `#fb7185` + `useBuiltinPanel:{mode:"video"}`; assets `#64748b` + `hasSourceHandle:false` + `hidePanel:true`.
- `resource()` returns `{kind:"image"|"video"|"text", url?/text?}` for reference chaining (builtin-nodes.tsx:10-16). Built-ins omit `Content` and render via `canvas-node.tsx`'s internal renderer (builtin-nodes.tsx:8-9). `registerBuiltinNodes()` is idempotent (35-40).

---

# 5. Persistence

## Backends
- **localforage** instance `name:"open-canvas", storeName:"app_state"` holds all zustand-persisted JSON (`lib\localforage-storage.ts:4-7`); `localForageStorage` is the `StateStorage` adapter with `localStorage` fallback (9-34); `createDebouncedPersist` (400ms, skips reference-equal watched keys) (37-61).
- **Blob stores** (localforage createInstance): `image_files`, `image_generation_logs`, `video_generation_logs` in `services\image-storage.ts:18-20`; `media_files` in `services\file-storage.ts:6`; plugin-private stores in `name:"open-canvas-plugins"`, `storeName:<pluginId>` (`lib\canvas\canvas-event-bus.ts:35`).

## Zustand persist keys (all inside `app_state`)
`open-canvas:canvas_store` (use-canvas-store.ts:58), `open-canvas:image_projects` (use-image-store.ts:157), `open-canvas:pixel_projects` (use-pixel-store.ts:125), `open-canvas:plugin_store` (use-plugin-store.ts:41), `open-canvas:ai_config_store` (use-config-store.ts:69), `open-canvas:theme_store` (use-theme-store.ts:17), `open-canvas:texture_projects` (use-texture-store.ts:119), `open-canvas:ps_asset_store` (use-ps-asset-store.ts:40), `open-canvas:generation_cost_store` (use-generation-cost-store.ts:18), `open-canvas:runtime_log_store` (use-runtime-log-store.ts:21), `open-canvas:agent_audit_store` (use-agent-audit-store.ts:27).

## localStorage keys (small config)
`canvas-side-panel-width`, `canvas-side-panel-open` (use-canvas-side-panel-store.ts:8-9), `canvas-image-quick-tools-v16` (canvas-image-toolbar-tools.tsx:40), `open-canvas:studio-dock:v4:<studio>` (dock-layout.ts:41), `canvas-bg-removal-ready` / `canvas-mobile-sam-ready` (use-local-model-store.ts:27-30), locale key (i18n), texture favorites/presets (texture pages).

## storageKey conventions
- Images: `image:<nanoid>` (`image-storage.ts:48`), thumbnails `thumb:<id>` (`image-storage.ts:66-67`).
- Media: `<prefix>:<nanoid>` (`file-storage.ts:11`).
- A `storageKey` containing `":"` marks a blob reference (export/cleanup scan: canvas-export.ts:33-38; file-storage.ts:77).

## Document schemas
- **CanvasProject** = the drawable document (nodes+connections+viewport+scope), persisted in `open-canvas:canvas_store` (use-canvas-store.ts:9-19).
- **ImageProject** (image studio doc) (use-image-store.ts:8-24): `{ id, title, timestamps, groupId, width, height, boardRatio, boardResolution:"1k"|"2k"|"4k", boardBackground, boardBackgroundOpacity, boardLayers: CanvasPsLayer[], boardPaths?, boardAlphaChannels?, boardChannelVisibility? }`. Full PS document types in canvas.ts:75-207 (`CanvasPsLayer`, `CanvasPsTransform`, `CanvasPsLayerStyle`, paths, alpha channels, text warp, etc.). `IMAGE_PRESETS` (use-image-store.ts:40-47).
- **PixelProject** (use-pixel-store.ts:9-16): `{ id,title,timestamps,groupId, doc: CanvasPixelDoc }`; `CanvasPixelDoc` (canvas.ts:106-114): `{ width, height, palette: string[], layers: CanvasPixelLayer[], frames: CanvasPixelFrame[], fps, background }`; `CanvasPixelLayer` (canvas.ts:88-94), `CanvasPixelCel = {storageKey?}`, `CanvasPixelFrame = {id,durationMs,cels: Record<layerId,cel>}` (canvas.ts:96-103). Creation/defaults `createPixelDoc` (pixel\document.ts:25-30, default fps 10, one layer/frame, `DEFAULT_PIXEL_PALETTE` canvas.ts:117).

## Import / Export — `lib\canvas\canvas-export.ts`
- Export `exportCanvasProjects` (canvas-export.ts:10-31): collects every `storageKey` (deep recursive scan), pulls blobs from `getImageBlob`/`getMediaBlob`, writes `projects.json` + `projects/<id>/files/<safeName>.<ext>` into a ZIP saved via `file-saver`.
- JSON envelope `CanvasExportFile` (`types\canvas-export.ts:3-21`): `{ app:"open-canvas", version:3, exportedAt, projects:[{ project, groupName?, files:[{ storageKey, path, mimeType, bytes }] }] }`.
- Import `components\canvas\canvas-import-dialog.tsx`: validates `.zip`, `app==="open-canvas"`, `version===3` (SUPPORTED_VERSION, line 14), every referenced asset present, restores blobs via `setImageBlob`/`setMediaBlob`, then `importProject` per project (canvas-import-dialog.tsx:36-97).

---

# 6. Interaction Model

## Tool model & pointer tools
- `canvasTool: "select" | "pan"`, default `"pan"` (project.tsx:231). Ctrl or Space temporarily swaps select↔pan (`open-canvas.tsx:161-163,254-255`).
- **Right-button modifier tool host**: `registerCanvasRightButtonTool(combo, tool)` + `installCanvasPointerToolHost()` (`lib\canvas\canvas-pointer-tools.ts:14-62`); the only registered combo is `"ctrl"` → **cut/freehand-lasso edge cut** (`project.tsx:771-808`, register at 808), which deletes connections crossed by the stroke.

## Pan / Zoom — `components\canvas\open-canvas.tsx`
- Wheel zoom (open-canvas.tsx:111-154): accumulates `-deltaY`, `factor = 1.1^(delta/100)`, new scale clamped **0.05–5**, zoom anchored at cursor; writes CSS transform/grid directly (no React rerender per frame), commits `onViewportChange` after 160ms idle.
- Pan (open-canvas.tsx:156-239): middle button, or left-button on background with tool=pan (or ctrl/space); `setPointerCapture`; moves `x/y`; a non-moved press on background triggers `onCanvasDeselect` (217-219); commits viewport on release.
- Wheel on page scroll prevented except over overlay targets (241-252); overlay selector in `canvas-overlays.ts:1-4`.
- `screenToCanvas` (screen→world) and `getCanvasCenter` (`project.tsx:446-461`).

## Node drag
- Capture-phase selection `handleNodeSelectCapture` (project.tsx:831-840) + `handleNodeMouseDown` starts drag with `dragRef` (842-874); locked/hidden nodes excluded.
- rAF-throttled `handleGlobalMouseMove` computes `dx,dy` (÷k), calls `snapDragToGuides(..., 6/k, CANVAS_GRID_SIZE=16)` (project.tsx:1046-1106).
- `finishNodeDrag` (project.tsx:900-1017): resolves drop targets — assets (collect), image-stack (append images), video-slot (bind) — else applies snapped move; a no-move click toggles the node dialog.

## Connect (drag handles)
- `handleConnectStart` (project.tsx:1262-1272) → `connectingParams: ConnectionHandle`. `ConnectionHandle = {nodeId, handleType:"source"|"target"}` (canvas.ts:294-297).
- `getConnectionDropTarget` hit-tests handle/inside/expanded at scale (project.tsx:505-540); `handleGlobalMouseUp` calls `connectNodes`→`normalizeConnection` and dedups existing from/to (project.tsx:487-503, 1141-1153).

## Marquee selection
- `handleCanvasMouseDown` on background left-button creates `SelectionBox` (shift = additive) (project.tsx:743-769).
- `handleGlobalPointerMove` computes rect intersection, selects unlocked/unhidden nodes (project.tsx:1108-1139); cleared on mouseup (1141-1146); rect rendered at 1974-1986.

## Resize / align / z-order
- `handleNodeResize` (project.tsx:1274-1276), `toggleNodeFreeResize` preserves image ratio (1287-1298); alignment via `alignNodes` (`lib\canvas\alignment.ts:29-44`); `moveNodeLayer` for z-order (1019-1029).

## Keyboard shortcuts (project.tsx:1192-1260)
- `Ctrl/Cmd+Z` → undo (1202-1207); `Ctrl/Cmd+Shift+Z` → redo (same block); `Ctrl/Cmd+Y` → redo (1209-1213); `Ctrl/Cmd+A` → select all (1215-1221); `Ctrl/Cmd+C` → copy selected nodes (1223-1227); `Ctrl/Cmd+V` → paste (fallback to system clipboard paste) (1229-1233); `Delete`/`Backspace` → delete selected nodes else selected connection (1235-1241); `Escape` → clear all selection/dialogs (1243-1255).
- open-canvas.tsx: `Space`/`Control` keydown/up toggle temporary tool and set `isSpacePressed`/`isControlPressed` (open-canvas.tsx:74-109).
- Documented in the toolbar shortcuts modal (`canvas-toolbar.tsx:369-378`): Ctrl/Space+drag = toggle tool, wheel = zoom, drag = box select, Shift/Cmd+click = add selection, Ctrl/Cmd+C/V = copy/paste, Delete/Backspace = delete.

---

# 7. External Services vs Pure Local

## Requires external services (browser → OpenAI-compatible API; API key stored locally and sent from the front end — see `AGENTS.md`)
- **Image generation / edit**: `requestGeneration`, `requestEdit` (`services\api\image.ts:465,516`).
- **Vision / question / text generation**: `requestImageQuestion` (`services\api\image.ts:571`) — used for OCR (`lib\canvas\canvas-ocr.ts:21-36`), image analysis, and text-mode node generation (`use-canvas-generation.ts:413,719,883`).
- **Video generation**: `createVideoGenerationTask`, `waitForVideoGenerationTask`, polling (`services\api\video.ts:93-136`).
- **Camera-angle re-render**: `requestEdit` with a generated prompt (`use-canvas-generation.ts:302-352`).
- **AI upscale** (kind `"ai"`): `requestEdit` (`project.tsx:661` dispatch → handler in use-canvas-generation).
- **User model-call scripts**: `runModelPlugin` executes arbitrary `new Function(...)` with http/request/poll injected (`services\api\model-plugin.ts:127-204`).
- **Provider config**: channels with `baseUrl`/`apiKey`/models (`use-config-store.ts:14-57,74-111`); OpenRouter default base URL (use-config-store.ts:71).
- **Plugin code/registry fetch**: plugin JS loaded by URL/`import()` (`lib\canvas\plugin-loader.ts:11-68`) and official registry fetch (`lib\canvas\plugin-registry.ts:17-32`).

## Pure local compute (but some *download* model weights once and cache them)
- **Background removal**: `@imgly/background-removal` model `isnet_quint8` running in a Web Worker (`services\background-removal.worker.ts:1-16`; host `services\background-removal.ts:42-48`); model bytes fetched/cached by the library.
- **Interactive segmentation (click-to-mask)**: MobileSAM via `onnxruntime-web` WASM; encoder/decoder ONNX downloaded from HuggingFace and cached in Cache Storage, then run locally (`lib\image\mobile-sam.ts:9-16,67-83,139-176`).
- **Local model lifecycle**: `use-local-model-store.ts` (`background-removal`, `mobile-sam`) with localStorage readiness flags (27-96).
- **Image ops (no network)**: crop, grid split, nearest/bilinear/step upscale, resize — pure Canvas 2D (`lib\canvas\canvas-image-data.ts:33-124`).
- **Pixel-art docs/frame compositing/PNG export**: `lib\canvas\pixel\*.ts` (`document.ts`, `raster.ts`, `io.ts`, `export.ts`).
- **PS/image-studio raster compositor, adjustments, FX, paths, text, selections**: `lib\canvas\ps\*.ts`, `smart-canvas.ts` (all local).
- **Blob storage/upload**: localforage (`services\image-storage.ts`, `services\file-storage.ts`).

---

<results>
<files>
- D:\Dev\open-canvas\web\src\types\canvas.ts - master domain types: node types, metadata, connections, viewport, selection, pixel/PS docs
- D:\Dev\open-canvas\web\src\types\canvas-plugin.ts - CanvasNodeDefinition contract + plugin host context
- D:\Dev\open-canvas\web\src\constant\canvas.ts - NODE_DEFAULT_SIZE / NODE_SPECS / getNodeSpec
- D:\Dev\open-canvas\web\src\stores\canvas\use-canvas-store.ts - project library store, CanvasProject schema, BroadcastChannel sync, persistence
- D:\Dev\open-canvas\web\src\stores\canvas\use-canvas-ui-store.ts - project-list editing/selection UI state only
- D:\Dev\open-canvas\web\src\stores\canvas\use-plugin-store.ts - installed plugin records
- D:\Dev\open-canvas\web\src\pages\canvas\project.tsx - the actual canvas editing state, all pointer/keyboard handlers, render tree
- D:\Dev\open-canvas\web\src\pages\canvas\hooks\use-canvas-document.ts - create/delete/duplicate/copy-paste/viewport-zoom operations
- D:\Dev\open-canvas\web\src\pages\canvas\hooks\use-canvas-history.ts - undo/redo snapshot rules
- D:\Dev\open-canvas\web\src\pages\canvas\hooks\use-canvas-generation.ts - generation/OCR/angle/AI-upscale orchestration
- D:\Dev\open-canvas\web\src\pages\canvas\index.tsx - project library page (folders/groups)
- D:\Dev\open-canvas\web\src\components\canvas\open-canvas.tsx - pan/zoom/tool-swap engine
- D:\Dev\open-canvas\web\src\components\canvas\canvas-toolbar.tsx - toolbar, create menus, zoom panel, shortcuts
- D:\Dev\open-canvas\web\src\components\canvas\nodes\builtin-nodes.tsx - built-in node definitions registration
- D:\Dev\open-canvas\web\src\components\canvas\canvas-image-toolbar-tools.tsx - per-image tool list (crop/mask/bg/ocr/segment/angle...)
- D:\Dev\open-canvas\web\src\components\canvas\dock\dock-layout.ts - dock/panel/tab layout model + localStorage persistence
- D:\Dev\open-canvas\web\src\components\canvas\canvas-import-dialog.tsx - zip import flow + validation
- D:\Dev\open-canvas\web\src\lib\canvas\node-registry.ts - node registry API
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-node-factory.ts - node creation + metadata builders
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-node-geometry.ts - bounds/snap/lock/connection normalization
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-node-size.ts - fitNodeSize / nodeSizeFromRatio
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-connections.ts - edge relations + Bezier geometry + crossing test
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-drop-bindings.ts - drag-drop binding rules
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-video-slots.ts - video keyframe/reference slot binding
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-resource-references.ts - upstream resource reference model
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-generation-helpers.ts - buildGenerationConfig, image hydration, retry queue
- D:\Dev\open-canvas\web\src\lib\canvas\text-style.ts - text style resolution/clamping
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-export.ts - export zip format
- D:\Dev\open-canvas\web\src\types\canvas-export.ts - export JSON schema
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-pointer-tools.ts - right-button modifier tool host
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-ocr.ts - OCR via requestImageQuestion
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-image-data.ts - local crop/split/upscale
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-event-bus.ts - plugin event bus + namespaced storage
- D:\Dev\open-canvas\web\src\lib\canvas\plugin-loader.ts - plugin evaluation/install/enable lifecycle
- D:\Dev\open-canvas\web\src\lib\canvas\plugin-runtime.ts - plugin runtime (React, CSS, events)
- D:\Dev\open-canvas\web\src\lib\canvas\viewport-signal.ts - viewport signal for imperative consumers
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-node-constants.ts - status constants, video max size
- D:\Dev\open-canvas\web\src\lib\canvas\pixel\document.ts - CanvasPixelDoc operations/defaults
- D:\Dev\open-canvas\web\src\lib\canvas\pixel\export.ts - pixel frame PNG compositing
- D:\Dev\open-canvas\web\src\lib\canvas\pixel\io.ts - pixel buffer/PNG IO
- D:\Dev\open-canvas\web\src\stores\use-image-store.ts - ImageProject doc + IMAGE_PRESETS
- D:\Dev\open-canvas\web\src\stores\use-pixel-store.ts - PixelProject doc store
- D:\Dev\open-canvas\web\src\stores\use-theme-store.ts - light/dark theme
- D:\Dev\open-canvas\web\src\stores\use-config-store.ts - AiConfig, channels, model resolution
- D:\Dev\open-canvas\web\src\stores\use-canvas-side-panel-store.ts - side panel width/open state
- D:\Dev\open-canvas\web\src\stores\use-local-model-store.ts - local model (bg-removal, MobileSAM) lifecycle
- D:\Dev\open-canvas\web\src\stores\use-browser-cache-store.ts - browser cache status/items
- D:\Dev\open-canvas\web\src\lib\localforage-storage.ts - localforage adapter + debounced persist
- D:\Dev\open-canvas\web\src\services\image-storage.ts - image blob store + upload + generation logs
- D:\Dev\open-canvas\web\src\services\file-storage.ts - media blob store
- D:\Dev\open-canvas\web\src\services\background-removal.ts - bg-removal host
- D:\Dev\open-canvas\web\src\services\background-removal.worker.ts - bg-removal local worker (imgly isnet)
- D:\Dev\open-canvas\web\src\lib\image\mobile-sam.ts - local MobileSAM segmentation
- D:\Dev\open-canvas\web\src\services\api\image.ts - external image generation/edit/vision
- D:\Dev\open-canvas\web\src\services\api\video.ts - external video generation
- D:\Dev\open-canvas\web\src\services\api\model-plugin.ts - user model-script executor
- D:\Dev\open-canvas\web\src\lib\canvas\plugin-registry.ts - official plugin registry fetch
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-overlays.ts - overlay hit-test selector
- D:\Dev\open-canvas\web\src\lib\canvas\alignment.ts - align/distribute math
</files>

<answer>
The model to port is: a `CanvasProject` document = `nodes: Vec<CanvasNode>` + `connections: Vec<Connection>` + `Viewport{x,y,k}` + optional `scope`. A node is `{ id, type, title, position{x,y}, width, height, metadata: Option<NodeMetadata> }`; `type` is an open string keyed to a registry (`builtin` kinds plus `"<pluginId>:<name>"`), and `metadata` is a single wide struct holding every per-kind field (status, prompt, images, videoSlots, generation params, flags like locked/hidden/freeResize/interactive). Connections are directed `{ id, fromNodeId, toNodeId, relation? }` with orientation rules (generation nodes are sinks, Image can't be a target). Z-order is array order. Persistence is localforage/IndexedDB: project documents live in the zustand key `open-canvas:canvas_store` inside localforage DB `open-canvas`/store `app_state`; binary blobs live in `image_files`/`media_files` keyed by `image:<id>` / `<prefix>:<id>`; export is a ZIP with `projects.json` (app/version=3) + referenced blobs. Editing state (selection, connecting, marquee, history) is intentionally ephemeral and component-local; history snapshots only `{nodes, connections}` with a 50-entry cap and 180ms debounce. Interaction: pan/zoom on a compositor-driven transform (clamp 0.05–5), left-drag on node moves with guide/grid snapping, background left-drag marquee-selects, handle-drag connects, right-button+Ctrl cuts edges crossed by the stroke, plus the documented keyboard shortcuts. Services split cleanly into external AI (image/video/vision/OCR/AI-upscale/model-scripts, all direct browser→OpenAI-compatible HTTP with a local key) and local compute (imgly background removal and MobileSAM segmentation run in-browser WASM/workers after one-time weight downloads; crop/split/upscale/pixel/PS compositing are pure canvas).
</answer>

<next_steps>
For the Rust core, define: (1) `CanvasNodeType` as an enum for the 9 built-ins + a newtype fallback for plugin ids; (2) `NodeMetadata` as a struct with `Option` fields (mirror canvas.ts:209-261 verbatim to keep serde round-trips lossless); (3) `CanvasProject`/`Connection`/`Transform` (rename ViewportTransform), and `Selection`/`SelectionBox`; (4) a `NodeRegistry` trait with `(type, plugin_id) -> NodeDefinition`; (5) a `History { past: VecDeque<Snapshot>, future, last, paused }` with snapshot = (nodes, connections) and 180ms debounce/50-cap semantics; (6) persistence as JSON files per project with an `open-canvas`/`version:3` export ZIP matching canvas-export.ts (include the `storageKey`→relative-path file map). Decide blob storage parity: the TS side keys blobs by `image:<id>`; a Rust port can store files under `<project>/files/` and rewrite storage keys on load/export. If you want, I can produce an exact Rust struct/enum draft mapped field-by-field to canvas.ts lines.
</next_steps>
</results>