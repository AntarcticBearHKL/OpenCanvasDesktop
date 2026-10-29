I have a complete picture of the Image Studio. Here is the exhaustive map.

# Image Studio — Rust+Slint Port Map

Entry/mount: `D:\Dev\open-canvas\web\src\pages\image\index.tsx` builds `resourcePoolNodes` (all image nodes across canvases, tagged with `canvasTitle`) and mounts `<ImageStudio board={currentProject} nodes={resourcePoolNodes} onBoardChange onOutput onBack/>`. `currentProject` is an `ImageProject`, and `onBoardChange` patches it via `updateProject`. Export path uses `composeSmartCanvas` → PNG (download) or upload + new canvas image node.

---

## 1. Document model

### 1a. Project / board
`ImageProject` — `web/src/stores/use-image-store.ts:8-24`
| field | type | notes |
|---|---|---|
| id, title, createdAt, updatedAt, groupId | string | library metadata |
| width, height | number | document coordinate system (layers/crop/rotate/raster all use it) |
| boardRatio | string | e.g. "16:9" |
| boardResolution | "1k"\|"2k"\|"4k" | export target size multiplier |
| boardBackground | string | CSS color or "transparent" |
| boardBackgroundOpacity | number | 0..1 |
| boardLayers | CanvasPsLayer[] | index 0 = bottom |
| boardPaths? | CanvasPsPath[] | |
| boardAlphaChannels? | CanvasPsAlphaChannel[] | |
| boardChannelVisibility? | { r, g, b } | |

`SmartCanvasBoard` — `web/src/lib/canvas/smart-canvas.ts:20-33` — the exact same shape minus id-independent fields (id/title/width/height/boardRatio/boardResolution/boardBackground/boardBackgroundOpacity/boardLayers/boardPaths/boardAlphaChannels/boardChannelVisibility). `ImageProject` structurally satisfies it, so the studio treats the project **as** the board. Defaults: ratio "16:9", resolution "2k", background "transparent", base size 640×360 (`smart-canvas.ts:40-47`).

Supporting store types: `ImageGroup` (`use-image-store.ts:26-30`), `ImagePreset` + `IMAGE_PRESETS` (`:32-47`), `ImageStore` API (`:49-65`).

### 1b. PS types — all in `web/src/types/canvas.ts`
- `CanvasPsLayerKind` = image|text|group|pixel|shape|adjustment (`:75`)
- `CanvasPsShapeKind` = rectangle|rounded-rectangle|ellipse|polygon|line (`:77`)
- `CanvasPsAdjustmentType` — 15 values (`:80`)
- `CanvasPsLayerStyleType` — 10 values (`:82`)
- `CanvasPsParamValue` = number|string|number[]|string[] (`:84`)
- `CanvasPsTransform` (`:120-123`): `quad: {x,y}[4]` (TL,TR,BR,BL); `warp?: {x,y}[16]` (4×4 row-major, takes precedence)
- `CanvasPsTextCase` (`:125`), `CanvasPsTextAlign` (`:127`), `CanvasPsTextWarpStyle` (10) (`:129`)
- `CanvasPsTextWarp` (`:132`): `{ style, bend, horizontal, vertical }` (-100..100)
- `CanvasPsTextParagraph` (`:134-142`): align, indentLeft, indentRight, indentFirst, spaceBefore, spaceAfter, hyphenate
- `CanvasPsPathAnchor` (`:145`): `{ x, y, handleIn:{x,y}, handleOut:{x,y} }` (handles are offsets from anchor)
- `CanvasPsPath` (`:148`): `{ id, name, anchors, closed, visible }`
- `CanvasPsAlphaChannel` (`:151`): `{ id, name, storageKey }`
- `CanvasPsLayerStyle` (`:154-159`): `{ id, type, enabled, params: Record<string,CanvasPsParamValue> }`

`CanvasPsLayer` — `types/canvas.ts:162-207` (complete field list):
```
id, name, kind: CanvasPsLayerKind,
content?: string            // image layers: fallback content URL
storageKey?: string         // image: source bitmap; pixel: own bitmap (sized to layer box)
maskStorageKey?: string     // optional per-layer mask (white shows)
adjustment?: CanvasPsAdjustmentType
adjustmentParams?: Record<string,CanvasPsParamValue>
styles?: CanvasPsLayerStyle[]
text?: string
shape?: CanvasPsShapeKind
shapeRadius?: number        // rounded-rectangle
shapeSides?: number         // polygon 3..24
shapeFill?: string
shapeStroke?: string
shapeStrokeWidth?: number
x, y, width, height: number
rotation: number
opacity: number             // 0..1
blendMode: string
hidden: boolean
locked: boolean
fontSize?, color?, fontFamily?      // text
tracking?   // 1/1000 em
leading?    // px; 0 => 1.2*fontSize
kerning?    // extra px between glyph pairs
baselineShift?
textScaleX?, textScaleY?    // percent
fauxBold?, fauxItalic?, underline?, strikethrough?: boolean
textCase?: CanvasPsTextCase
paragraph?: CanvasPsTextParagraph
textPathId?: string
textWarp?: CanvasPsTextWarp
transform?: CanvasPsTransform
children?: string[]         // group: one level only
```

Blend modes = 16 entries (`web/src/lib/canvas/blend-modes.ts:7-24`), each with canvas `GlobalCompositeOperation` + CSS string; default "normal"; `clampLayerOpacity` (`:32-35`), `resolveBlendMode` (`:37-39`).

Bitmap keys: `image:<nanoid>` in localforage store `open-canvas/image_files` (`web/src/services/image-storage.ts:18,48`). Masks & alpha channels reuse the same `image:` convention so the cleanup sweep keeps them (`:279-288`).

---

## 2. Panel inventory + dock layout

Dock panel defs: `PS_DOCK_PANELS` — `web/src/components/canvas/workspace/image-studio.tsx:125-138`. 12 panels across 3 edges:

| id | default dock | component | file |
|---|---|---|---|
| layers | right | `PsLayersPanel` | `ps-layers-panel.tsx` |
| channels | right | `PsChannelsPanel` | `ps-channels-panel.tsx` |
| paths | right | `PsPathsPanel` | `ps-paths-panel.tsx` |
| history | right | `PsHistoryPanel` | `ps-history-panel.tsx` |
| actions | right | `PsActionsPanel` | `ps-actions-panel.tsx` |
| properties | left | `PsPropertiesPanel` | `ps-properties-panel.tsx` |
| adjustments | left | `PsAdjustmentsPanel` | `ps-adjustments-panel.tsx` |
| assets | left | `PsResourcePoolPanel` | `ps-resource-pool-panel.tsx` |
| color | bottom | `PsColorPanelBody` | `ps-asset-panels.tsx:324` |
| gradients | bottom | `PsGradientsPanel` | `ps-asset-panels.tsx:32` |
| patterns | bottom | `PsPatternsPanel` | `ps-asset-panels.tsx:166` |
| brushes | bottom | `PsBrushesPanel` | `ps-asset-panels.tsx:262` |

Panel dispatch: `renderPsPanel(panelId)` — `image-studio.tsx:1422-1498`.

What each shows/does:
- **Layers** (`ps-layers-panel.tsx`): header actions add-image-picker / add-text / group / ungroup / duplicate / delete / move-up / move-down (`:115-162`); blend-mode Select + opacity slider + % (`:164-199`); scrollable rows reversed from top (`:56-62`) with group chevron, visibility eye, thumbnail (`LayerThumb` `:384-400`), mask thumb, double-click rename, lock toggle, drag-drop reorder (before/after/into) (`dropPositionFor`/`dropRow` `:97-111`); footer add-adjustment dropdown + "layer style" (opens `PsFxPanel`); double-click row opens fx.
- **Channels** (`ps-channels-panel.tsx`): composite rows RGB/R/G/B with visibility toggles + active view (`:147-203`); alpha channels list with thumbnail, rename, load-into-selection, delete (`:213-241`); save-selection-as-channel (`:116-123`). `PsChannelPreview` (`:22-60`) renders a channel-isolated composite.
- **Paths** (`ps-paths-panel.tsx`): new / duplicate / delete; list with SVG thumb, visibility, rename; make-selection / fill / stroke; paint color.
- **History** (`ps-history-panel.tsx`): in-session layer snapshots only (never persisted), limit 50 (`PS_HISTORY_LIMIT` `:16`), back/forward/snapshot, restore. `usePsHistory` hook `:24-75`.
- **Actions** (`ps-actions-panel.tsx`): record/stop document commands, save named `PsAction` (persisted in asset store), play/rename/delete. `usePsActionRecorder` `:22-48`. Commands: `PS_MENU_COMMANDS` `:10`, `PS_LAYER_COMMANDS` `:11`. Strokes are intentionally not recordable (`:18-21`).
- **Properties** (`ps-properties-panel.tsx`): name; adjustment layers embed `PsAdjustmentEditor`; else position X/Y, size W/H, rotation; blend + opacity; mask add/apply/delete; shape props (type/fill/stroke/strokeWidth/radius/sides); text (content/fontSize/color). Text layer also shows `PsTextPanel`.
- **Adjustments** (`ps-adjustments-panel.tsx`): 8-col grid of the 15 adjustment icons (`ICONS` `:17-33`); clicking creates/replaces an adjustment layer.
- **Assets / resource pool** (`ps-resource-pool-panel.tsx`): search all canvas image nodes; drag (`IMAGE_NODE_DRAG_MIME = "application/x-open-canvas-image-node"` `:9`) or "+" to add as image layer.
- **Color** (`ps-asset-panels.tsx:324` + `ps-color-picker.tsx`): `PsColorFields` — SV square, hue strip, H/S/B, R/G/B, hex, foreground/background with swap/reset; swatches list (default 12, `use-ps-asset-store.ts:42`) add/remove.
- **Gradients** (`ps-asset-panels.tsx:32`): two dynamic presets (fg→transparent, fg→bg), saved gradients with gradient bar; editable stop bar (add/drag/double-click delete), stop color/position, midpoint, save. Stop model `PsGradientStopPreset { color, position, midpoint }` (`use-ps-asset-store.ts:8`); midpoints expanded to real stops by `psExpandGradientStops` (`:82-92`).
- **Patterns** (`ps-asset-panels.tsx:166`): define from selection (raster crop) or from selected layer; tile grid; pick (sets bucket tool) / fill-layer; delete.
- **Brushes** (`ps-asset-panels.tsx:262`): preset list with canvas preview (`psBrushPreview`), sliders size/hardness/opacity/spacing/scatter/angle/roundness/dynamics/texture, save. Defaults `use-ps-asset-store.ts:43-48`.

Dock layout model — `web/src/components/canvas/dock/dock-layout.ts`:
- `DockEdge` = left|right|bottom; `DockSide` = left|right (`:3-4`)
- `DockPanelDef { id, labelKey, icon, dock, hidden? }` (`:6-14`)
- `DockGroup { panels[], active }` (`:16-20`)
- `DockLayout { groups: Record<DockEdge,DockGroup[]>, sizes, split }` (`:22-28`)
- Constraints: min 168 / max 640 (bottom 120/420), `DOCK_MAX_GROUPS` left/right=2, bottom=1, min group 96, split 0.2–0.8 (`:30-47`)
- Default layout from defs (`:55-64`); persistence `open-canvas:studio-dock:v4:ps-image` in localStorage (`:41`, `:84-122`); ops `dockTogglePanel`/`dockRevealPanel`/`dockMovePanel`/`dockSetActive`/`dockSetSize`/`dockSetSplit` (`:134-168`).
- Renderer: `DockArea` + `useDockLayout` (`web/src/components/canvas/dock/dock-panel.tsx`) — columns per edge, resizable splitters, stacked groups, tab drag-and-drop drop-zones, per-panel error boundary; `DockTabs` (`dock-tabs.tsx`), window menu `DockWindowMenu` (`dock-window-menu.tsx`). Instance: `useDockLayout("ps-image", PS_DOCK_PANELS)` (`image-studio.tsx:210`).

---

## 3. Tool rail

`TOOLS` — `image-studio.tsx:99-116`; render `:1646-1669`; hotkeys handler `:408-497`; `TOOL_HOTKEYS` `:124`.

| tool | hotkey | behavior |
|---|---|---|
| move (V) | V | select layer, drag move, corner resize, rotate handle; snapping |
| hand (H) | H | pan (also space-hold) |
| pen (P) | P | add path anchors, drag handles, click-first to close |
| direct-select (Shift+P) | Shift+P | edit anchors/handles of active path |
| marquee (M) | M | rectangular selection |
| ellipse-marquee (Shift+M) | Shift+M | elliptical selection |
| lasso (L) | L | freehand polygon selection |
| polygon-lasso (Shift+L) | Shift+L | click polygon points, Enter/close-radius to finish |
| wand (W) | W | flood selection on composite/active layer; tolerance, contiguous, sampleAll |
| quick-select (A) | A | round-brush grow/shrink selection |
| brush (B) | B | paint dabs on pixel layer/mask |
| eraser (E) | E | erase (destination-out) |
| bucket (G) | G | flood fill or pattern fill w/ tolerance |
| gradient (Shift+G) | Shift+G | linear/radial gradient fill |
| shape (U) | U | rectangle/rounded/ellipse/polygon/line drag-create |
| eyedropper (I) | I | sample composited board color |

Rail also has zoom in/out buttons (`:1663-1669`). Tool sets: `SELECTION_TOOLS`, `PAINT_TOOLS`, `RING_TOOLS`, `COLOR_TOOLS` (`:117-120`). Contextual option bar per tool (`:1550-1618`): brush size/hardness/opacity, feather/antialias/tolerance/contiguous/sampleAll, gradient type/mode/opacity/stop/reverse, shape type/fill/stroke/width, transform mode segmented, colors. Brush size `[`/`]` step logic `:480-486`. Zoom wheel `:355-365`; zoom min/max/step `:92-94`. Keyboard: Ctrl+A/D/Shift+I, Ctrl+T, Ctrl+Shift+T, Esc, Enter, Shift+P/M/L/G (`:408-497`).

---

## 4. Canvas / board rendering

Studio tree `image-studio.tsx:1500-1896`: top bar (title, `SmartCanvasSettingsPopover`, menus, dock window menu, tool options, zoom %, fit, Export) then `DockArea` containing the tool rail + center viewport + dock columns; filter dialog mounted at bottom (`:1882-1894`).

Center viewport (`:1672-1879`):
1. Outer pan/hit container (cursor by mode).
2. Board transform div: `left/top = view.x/y`, `width/height = board`, `transform: scale(k)`, `transformOrigin 0 0`, `isolation isolate`, `backgroundColor = backgroundFill` (`:1715-1717`).
3. `viewBoard` (`:253-265`) substitutes draft layers, hides the stroke layer's storageKey, and swaps a layer to its mask bitmap in mask-render mode.
4. Depth order inside: `PsGrid` → `BoardLayersView` (or raster board) → `PsChannelPreview` → live stroke `<canvas>` → `PsGuides` → `PsMarchingAnts` → `PsDraftOverlay` → `PsSnapLines` → `PsPathOverlay` → `PsTransformHandles` → layer hit-boxes / selection handles / text edit textarea (`:1775-1876`); `PsRulers` sits out-of-board (`:1878`).

Layer preview — `web/src/components/canvas/workspace/ps-board-view.tsx`:
- `BoardLayersView` (`:16-70`): pure DOM when possible — `<img>` for image/pixel (mask via CSS `mask-image`), `psTextRenderStyle` div for text, `<svg>` + `psShapePathData` for shapes, groups as isolated containers with computed box; blend via `mixBlendMode` (`layerBlendStyle` `:8-10`).
- `psDocumentNeedsRaster` (`smart-canvas.ts:113-117`) → `PsRasterBoard` (`:72-89`) debounced 60 ms render through the shared compositor.
- `useResolvedPsLayerUrls` (`:91-107`) resolves storageKey/content + masks to blob URLs.

Overlays — `web/src/components/canvas/workspace/ps-overlays.tsx`: `PsMarchingAnts` (`:28-68`, edge pixels from `psSelectionEdges`, animated dash phase, 120 ms tick, limit 12000), `PsRulers` (`:70-127`, adaptive ticks, drag to create guides), `PsGrid` (`:129-142`, 50 px), `PsGuides` (`:144-171`), `PsDraftOverlay` (`:173-195`, rect/ellipse/path/line), `PsSnapLines` (`:197-209`), `PsPathOverlay` (`:211-264`, anchors + in/out handle dots), `PsTransformHandles` (`:266-295`). Snap targets & logic in `image-studio.tsx:518-591`; guides state per board `:206-208`, `:1308-1352`.

Shared raster compositor — `web/src/lib/canvas/smart-canvas.ts`:
- `renderPsDocument` (`:309-345`) → size from `readMediaDimensions("", resolution, ratio)` (`:87-89`), clip, background, iterate `psTopLayers`, apply channel visibility (`:348-360`).
- `drawPsLayer` (`:375-413`): translate to centre, rotate, opacity, blend; if mask/styles/mesh → padded scratch, `psComposeLayerStyles`, mask `destination-in`, `drawPsWarpedContent`.
- `drawPsAdjustment` (`:422-447`): reads back everything below, `applyPsAdjustment`, draws on top (optionally masked).
- `drawPsLayerContent` (`:449-463`): text / shape / bitmap; `drawPsTextLayer` (`:560-575`): text-on-path, text-warp (mesh), or plain lines.
- `drawPsGroup` (`:489-525`): isolates children then applies group opacity/blend/styles/mask.
- `renderPsLayerBitmap` (`:528-550`): rasterize a single layer (used by rasterize command, filters, patterns).
- Composite cache (2 entries) keyed by `psCompositeSignature` (`:279-304`, `:362-373`).
- Geometry helpers: `psRotatePoint`, `psBoxUnion`, `psLayerBox` (groups derive box from children), `psTextRenderStyle`, `psShapePathData`, `arrangePsLayers`, layer factory functions `createPsPixelLayer`/`TextLayer`/`ShapeLayer`/`AdjustmentLayer`/`ImageLayer` (`:148-205`).
- `PsChannelPreview` composites via `renderPsDocument` then isolates one channel (`ps-channels-panel.tsx:22-60`).

Board sizing sync: studio grows the board to fit overflowing layers up to the next ratio size (`image-studio.tsx:270-281`).

---

## 5. Menus + modals

`web/src/components/canvas/workspace/ps-menus.tsx` (props type `:15-42`; live handlers wired at `image-studio.tsx:1520-1548`).

- **Select** (`:101-107`, `:136-141`): Select All (Ctrl+A), Deselect (Ctrl+D), Inverse (Ctrl+Shift+I), Feather.
- **Edit** (`:163-177`): Free Transform (Ctrl+T), then Skew / Distort / Perspective / Warp, Numeric Transform, Repeat (Ctrl+Shift+T), Clear Transform.
- **Image** (`:108-119`, `:142-151`): Image Size…, Canvas Size…, Rotate 90 CW / 90 CCW / 180 / Arbitrary…, Crop to Selection (disabled w/o selection), Trim.
- **Filter** (`:128-135`, `:159-162`): Repeat Last Filter + all filters grouped by `PS_FILTER_GROUPS`.
- **View** (`:120-127`, `:152-158`): Rulers, Grid, Guides, Snap toggles; Clear Guides.

Modals inside `ps-menus.tsx`: **Image Size / Canvas Size** (`:197-229`) — W/H number inputs with constrain-proportions (size) or 3×3 anchor grid (canvas); **Rotate** (`:231-250`) — −180..180; **Numeric Transform** (`:252-287`) — dx, dy, scaleX, scaleY, rotation, skewX, skewY. Handlers: `imageSize` (`image-studio.tsx:967-973`, resamples bitmaps via `psResampleLayerBitmaps`), `canvasSize` (`:975-981`), `rotateDocument` (`:983-989`), `cropSelection` (`:991-998`), `trimDocument` (`:1000-1007`), `numericTransform` (`:1164-1169`), `repeatTransform` (`:1171-1174`), `clearTransform` (`:1176-1180`). A separate modal in `smart-canvas-settings-popover.tsx` handles ratio/resolution/background.

---

## 6. Filters, adjustments, curve & gradient-stop editors

### Filters — `web/src/lib/canvas/ps/filters.ts`
`PsFilterDef { type, labelKey, group, controls[], defaults }` (`:32-33`); groups `blur|sharpen|noise|pixelate|stylize|distort` (`:39`). 22 filters (`PS_FILTERS` `:41-70`):

| group | type | controls (range) / defaults |
|---|---|---|
| blur | gaussian-blur | radius 0-100 (4) |
| blur | box-blur | radius 0-100 (4) |
| blur | motion-blur | angle −180..180 (0), distance 1-100 (12) |
| blur | radial-blur | amount 1-100 (10), method zoom/spin |
| blur | tilt-shift | blur 0-100 (12), angle (0), position 0-100 (50), width 0-100 (30) |
| blur | iris-blur | blur (12), x (50), y (50), radius (35), feather (40) |
| sharpen | sharpen | amount 1-500 (100) |
| sharpen | unsharp-mask | amount (100), radius 1-100 (2), threshold 0-255 (0) |
| noise | add-noise | amount 1-100 (12), distribution uniform/gaussian, monochrome |
| noise | despeckle | none |
| noise | dust-scratches | radius 1-3 (1), threshold 0-255 (16) |
| pixelate | pixelate | size 2-200 (12) |
| pixelate | crystallize | size 3-200 (16) |
| stylize | emboss | angle (135), height 1-20 (3), amount 1-500 (100) |
| stylize | find-edges | none |
| stylize | solarize | threshold 0-255 (128) |
| stylize | clouds | size 2-400 (64), seed (7) |
| stylize | lens-flare | brightness 10-300 (100), x (30), y (30) |
| distort | ripple | amount 1-100 (10), size 2-400 (60) |
| distort | pinch | amount −100..100 (50) |
| distort | spherize | amount (−100..100, 50), mode normal/horizontal/vertical |
| distort | liquify | size 10-300 (80), pressure 1-100 (60) — interactive brush |

Implementations: `applyPsFilter` (`:261-633`) with helpers `psBoxBlur` (`:88`), `psGaussianBlur` (3× box, `:116`), `psConvolve` (`:121`), `psMedian` (`:149`), `psSampleBilinear` (`:181`), `psRemap` (`:198`), `psValueNoise` (`:215`), `psApplyBlurMix` (`:247`). `psLiquifyPush` (`:636-659`). Pipeline: `psLoadFilterSource` (`:662-678`), `psFilterMask` (`:681-706`), `psMixFiltered` (`:708-719`), `psFilterLayerBitmap` (`:721-731`). Filter dialog `ps-filter-dialog.tsx` (320 px preview, liquify drag, reset, rasterize warning; applies via `rasterizePsLayer`).

### Adjustments — `web/src/lib/canvas/ps-adjustments.ts`
15 types `PS_ADJUSTMENT_TYPES` (`:3`); defaults `PS_ADJUSTMENT_DEFAULTS` (`:27-43`); applier `applyPsAdjustment` (`:352-433`).

| type | params (defaults) |
|---|---|
| brightness-contrast | brightness (−150..150, 0), contrast (−100..100, 0) |
| levels | channel rgb/red/green/blue, inBlack 0-254, inWhite 1-255, gamma 0.1-9.99, outBlack 0-255, outWhite 0-255 |
| curves | channel, points[] (identity [0,0,255,255]) |
| exposure | exposure −5..5 EV, offset −0.5..0.5, gamma 0.1-9.99 |
| vibrance | vibrance −100..100, saturation −100..100 |
| hue-saturation | channel master/reds/yellows/greens/cyans/blues/magentas, hue −180..180, saturation −100..100, lightness −100..100 |
| color-balance | tone shadows/midtones/highlights, shadows[3]/midtones[3]/highlights[3] (−100..100), preserve |
| black-white | reds/yellows/greens/cyans/blues/magentas −200..300 |
| photo-filter | color #ec8a00, density 0-100, preserve |
| channel-mixer | output red/green/blue, monochrome, values[12] (−200..200) |
| gradient-map | stops[] ("#hex@pos"), reverse |
| invert | channel |
| posterize | levels 2-255 |
| threshold | level 1-255 |
| selective-color | range 0-8, method relative/absolute, values[36] (−100..100) |

Editors: `ps-adjustment-editor.tsx` per-type controls; appliers + helpers in `ps-adjustments.ts` (`psLut`/`psApplyLut` `:58-72`, `psCurveLut` `:74-93`, HSL conversions `:95-127`, per-effect functions `:129-349`). Gradient stop string helpers `psParseGradientStops`/`psFormatGradientStop` (`:263-274`).

### Curve & gradient-stop editors — `web/src/components/canvas/workspace/ps-param-editors.tsx`
- `PsCurveEditor` (`:20-80`): SVG 255×255, click adds control point, drag moves, double-click deletes (min 2), `psSortPairs` keeps x order; points stored as flat `[x,y,...]`.
- `PsGradientStopsEditor` (`:82-141`): gradient bar, click adds stop, drag/select, double-click deletes (min 2), color picker + position slider; stops are `"#hex@pos"` strings.

---

## 7. Pure local raster math vs external

Everything is **pure local Canvas2D + typed-array math**; there is no backend.
- Pure/computational (no I/O): `ps-adjustments.ts`, `ps-layer-styles.ts` (composite math; pattern overlays load bitmaps only when referencing a saved pattern), `ps-text.ts` (layout/warp math; needs a Canvas2D `measureText`), `ps-transform.ts` (mesh/matrix), `ps-path.ts`, `ps-color.ts`, `ps/selection.ts` (mask math), `ps/filters.ts` pixel kernels, `blend-modes.ts`, `image-ops.ts`/`layer-ops.ts` (pure list/geometry transforms), `image-agent-ops.ts` (pure reducer), `canvas-2d.ts` (just `createElement("canvas")`).
- Canvas2D-bound (still 100% browser-local): `ps/paint.ts` (strokes/fill/gradient; loads bitmaps), `ps-board-view.tsx`, `ps-overlays.tsx` (marching ants), `smart-canvas.ts` compositor, `ps-layer-styles.ts` pattern tiles, `ps-params-editors`/dialogs (only view state).
- Storage/external I/O: `web/src/services/image-storage.ts` — bitmap blobs in **localforage** (local IndexedDB), object URLs; `fetchImageBlob` only hits the network when handed a remote `http(s)` URL (`:104-126`). `uploadImage`/`resolveImageUrl` are the sole bitmap gateway. Agent namespace registration (`image-studio.tsx:1219-1229`) and screenshot provider (`:1234-1241`) are optional integrations, not core. No server calls anywhere in the editor logic.

Rust port implication: the entire filter/adjustment/compositor/path/text/mesh math translates directly (use `image` crate + own kernels, or a small canvas-equivalent raster buffer). Only the storage layer and the agent hooks are environment-specific.

---

## 8. Size, core vs secondary

Line counts (true totals from the file reads):

**Core (needed for a usable editor)**
| file | LOC |
|---|---|
| `...\workspace\image-studio.tsx` | 1923 |
| `...\lib\canvas\smart-canvas.ts` (compositor + model) | 598 |
| `...\lib\canvas\ps\filters.ts` | 731 |
| `...\lib\canvas\ps\paint.ts` | 485 |
| `...\lib\canvas\ps-adjustments.ts` | 433 |
| `...\lib\canvas\ps-layer-styles.ts` | 344 |
| `...\lib\canvas\ps-text.ts` | 302 |
| `...\lib\canvas\ps-transform.ts` | 244 |
| `...\lib\canvas\ps\selection.ts` | 243 |
| `...\lib\canvas\ps\layer-ops.ts` | 217 |
| `...\lib\canvas\ps\image-ops.ts` | 81 |
| `...\lib\canvas\ps-path.ts` | 104 |
| `...\lib\canvas\ps\path-ops.ts` | 61 |
| `...\lib\canvas\ps-color.ts` | 62 |
| `...\lib\canvas\blend-modes.ts` | 39 |
| `...\types\canvas.ts` (PS types) | 306 |
| `...\stores\use-image-store.ts` (doc persistence) | 181 |
| `...\workspace\ps-board-view.tsx` | 108 |
| `...\workspace\ps-overlays.tsx` | 295 |
| `...\workspace\ps-menus.tsx` | 290 |
| `...\workspace\ps-layers-panel.tsx` | 409 |
| `...\workspace\ps-properties-panel.tsx` | 219 |
| `...\workspace\ps-channels-panel.tsx` | 250 |
| `...\workspace\ps-color-picker.tsx` | 185 |
| `...\workspace\ps-text-panel.tsx` | 190 |
| `...\workspace\ps-fx-panel.tsx` | 254 |
| `...\workspace\ps-filter-dialog.tsx` | 229 |
| `...\workspace\ps-adjustment-editor.tsx` | 238 |
| `...\workspace\ps-param-editors.tsx` | 141 |
| `...\workspace\ps-asset-panels.tsx` (color/gradient/pattern/brush) | 370 |
| `...\workspace\ps-adjustments-panel.tsx` | 79 |
| `...\services\image-storage.ts` | 297 |
| `...\components\canvas\dock\{dock-panel,dock-layout,dock-tabs,dock-window-menu}` | 287/168/86/36 |
| `...\lib\canvas\canvas-2d.ts` | 6 |

**Secondary / optional**
| file | LOC | why |
|---|---|---|
| `...\workspace\ps-history-panel.tsx` | 122 | in-session snapshots |
| `...\workspace\ps-paths-panel.tsx` | 149 | vector paths |
| `...\workspace\ps-actions-panel.tsx` | 144 | macros |
| `...\workspace\ps-resource-pool-panel.tsx` | 121 | cross-canvas assets |
| `...\stores\use-ps-asset-store.ts` | 191 | brush/gradient/pattern/action presets |
| `...\lib\canvas\image-agent-ops.ts` | 248 | agent op API |
| `...\components\canvas\workspace\studio-chrome.ts` | 17 | shared class constants |
| `...\pages\image\index.tsx` | 663 | image library + mount |
| `...\components\image-settings-panel.tsx` | 258 | theme wrapper (`ImageSettingsTheme`) |
| `...\lib\media-size.ts` | 139 | `readMediaDimensions`/`inferMediaRatio` |

Rough split: **~9.7k LOC core** (studio + panels + compositor + all raster math), ~2.3k LOC secondary.

---

<results>
<files>
- D:\Dev\open-canvas\web\src\components\canvas\workspace\image-studio.tsx - Main orchestrator (1923 LOC): tool rail, gestures, state, menus wiring, dock, panel dispatch, board viewport
- D:\Dev\open-canvas\web\src\lib\canvas\smart-canvas.ts - SmartCanvasBoard model + shared raster compositor (598 LOC)
- D:\Dev\open-canvas\web\src\types\canvas.ts - All PS types: CanvasPsLayer, transform, layer styles, paths, alpha channels, text warp (306 LOC)
- D:\Dev\open-canvas\web\src\stores\use-image-store.ts - ImageProject/ImageGroup/ImagePreset doc model + persistence (181 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\ps\filters.ts - 22 filters + pixel kernels + filter pipeline (731 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\ps-adjustments.ts - 15 adjustment types + appliers + curve/gradient helpers (433 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\ps-layer-styles.ts - 10 layer styles raster compositor (344 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\ps-text.ts - text layout, decorations, text-on-path, warp mesh (302 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\ps-transform.ts - quad/mesh transform, numeric transform, warped draw (244 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\ps\selection.ts - selection mask algebra (243 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\ps\layer-ops.ts - layer list/geometry mutations (217 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\ps\image-ops.ts - doc resize/rotate/trim/anchor + resample (81 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\ps\paint.ts - strokes, bucket, gradient, pattern, samplers (485 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\ps-path.ts - path anchors, flattening, arc-length sampler (104 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\ps\path-ops.ts - path→selection, path fill/stroke (61 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\ps-color.ts - HSB/RGB/hex conversions (62 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\blend-modes.ts - 16 blend modes + opacity clamp (39 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-menus.tsx - 5 menus + Image/Canvas/Rotate/Numeric modals (290 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-layers-panel.tsx - layers dock panel (409 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-channels-panel.tsx - channels + channel preview (250 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-paths-panel.tsx - paths dock panel (149 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-history-panel.tsx - history snapshots (122 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-actions-panel.tsx - action recorder/macros (144 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-properties-panel.tsx - properties dock panel (219 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-adjustments-panel.tsx - adjustment grid (79 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-adjustment-editor.tsx - per-adjustment controls (238 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-resource-pool-panel.tsx - assets/resource pool (121 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-color-picker.tsx - color fields + popover (185 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-asset-panels.tsx - color/gradients/patterns/brushes panels (370 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-text-panel.tsx - character/paragraph/text-path/warp (190 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-fx-panel.tsx - layer style (fx) editor (254 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-filter-dialog.tsx - filter dialog + liquify (229 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-param-editors.tsx - curve + gradient-stops editors (141 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-board-view.tsx - DOM layer preview + raster fallback (108 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-overlays.tsx - rulers/grid/guides/ants/path/transform overlays (295 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\dock\dock-panel.tsx - DockArea + useDockLayout (287 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\dock\dock-layout.ts - DockLayout model + ops + persistence (168 LOC)
- D:\Dev\open-canvas\web\src\stores\use-ps-asset-store.ts - brush/gradient/pattern/action presets + swatches (191 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\image-agent-ops.ts - agent op union + pure reducer (248 LOC)
- D:\Dev\open-canvas\web\src\services\image-storage.ts - localforage bitmap store + upload/resolve (297 LOC)
- D:\Dev\open-canvas\web\src\pages\image\index.tsx - library page + ImageStudio mount + export (663 LOC)
</files>

<answer>
The Image Studio is a self-contained, fully client-side raster+vector editor. Its domain model is minimal and clean: a "board" document (ImageProject == SmartCanvasBoard) holding width/height/ratio/resolution/background + `boardLayers: CanvasPsLayer[]`, plus optional `boardPaths`, `boardAlphaChannels`, and `boardChannelVisibility`. Every pixel effect funnels through one shared raster compositor (`renderPsDocument`), and every document mutation funnels through `commitBoardLayers`. Bitmaps live as blobs under `image:` keys in localforage; masks/alpha channels reuse the same convention. There is no server.

For the Rust port, the natural module split mirrors the source exactly: (1) doc model in the type file + store, (2) pure math crates for filters/adjustments/layer-styles/text/transform/path/selection/ops, (3) a compositor equivalent to smart-canvas, (4) the studio shell with tool rail + gestures, (5) 12 dock panels behind a dock-layout model, (6) menus/modals. The `image-agent-ops.ts` reducer is a bonus: it is already a pure fold over the document and can be reused verbatim as a command/undo surface in Rust.
</answer>

<next_steps>
Suggested port phasing using this map:
1. Domain model first (Section 1) — mirror `CanvasPsLayer` + board + paths/channels verbatim as Rust structs/enums; they are the contract for everything else.
2. Pure math layer (Section 6/7): port `ps-adjustments`, `ps-layer-styles`, `ps-text`, `ps-transform`, `ps-path`, `ps-color`, `ps/selection`, `ps/filters`, `ps/layer-ops`, `ps/image-ops`, `image-agent-ops` with unit tests — these are deterministic and testable without any UI.
3. Compositor (Section 4): port `smart-canvas.renderPsDocument` + the `drawPs*` functions to a raster buffer; this unblocks board preview and export.
4. Panel layout + dock model (Section 2): `dock-layout.ts` is already a clean pure reducer — port it directly; build the 12 panels as Slint components.
5. Studio shell + tools + gestures (Sections 3/5): tool state machine, overlays, menus/modals.
6. Persistence (Section 7): swap localforage for the Rust equivalent; keep the `image:` key convention.
Ready to proceed — no follow-up needed from this search.
</next_steps>
</results>