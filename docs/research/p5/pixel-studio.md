I now have a complete picture. Here is the exhaustive map.

---

# Pixel Studio — Complete Port Map

## 0. File inventory & LOC

| File | LOC | Role | Tier |
|---|---|---|---|
| `web/src/components/canvas/workspace/pixel-studio.tsx` | **1262** | The entire editor: state machine, canvas render, panels, pointer/keyboard handling, export/import, agent bridge | **Core** |
| `web/src/lib/canvas/pixel/raster.ts` | **260** | All pixel algorithms + layer compositor | **Core** |
| `web/src/lib/canvas/pixel/document.ts` | **111** | Pure doc reducer (CRUD on doc/layers/frames/cels) | **Core** |
| `web/src/lib/canvas/pixel/headless.ts` | **172** | Headless op runner (agent/no-UI path) | Core-ish |
| `web/src/lib/canvas/pixel/io.ts` | **87** | Buffer↔Canvas↔PNG↔dataURL; image import fit | **Core** |
| `web/src/lib/canvas/pixel/history.ts` | **38** | Snapshot undo/redo stack | **Core** |
| `web/src/lib/canvas/pixel/export.ts` | **26** | Doc/frame→PNG File (resolution path) | Secondary |
| `web/src/components/canvas/workspace/pixel-studio-shared.ts` | **50** | `TOOLS`, `BLENDS`, dock defs, zoom helpers, types | **Core** |
| `web/src/lib/canvas/pixel-agent-ops.ts` | **126** | Flat agent op schema + pure fold | Core (agent) |
| `web/src/stores/use-pixel-store.ts` | **149** | Project/group persistence (localforage, zustand persist) | **Core** |
| `web/src/types/canvas.ts` (L86-117) | ~32 | `CanvasPixelDoc/Layer/Cel/Frame/Blend` types | **Core** |
| `web/src/components/canvas/workspace/ps-color-picker.tsx` | **185** | HSB/SV/RGB/hex picker + fg/bg swatch | Secondary |
| `web/src/lib/canvas/ps-color.ts` | **62** | Hex↔RGB↔HSB conversions | Secondary |
| `web/src/pages/pixel/index.tsx` | **358** | Library page + create modal + mounts editor | Secondary |
| `web/src/components/canvas/dock/dock-panel.tsx` | **287** | Dock engine: `useDockLayout`, `DockArea`, resize/split/drag | Reusable infra |
| `web/src/components/canvas/dock/dock-layout.ts` | **168** | Dock layout model + persistence | Reusable infra |
| `web/src/components/canvas/dock/dock-tabs.tsx` | 94 | Tab strip | Reusable infra |
| `web/src/components/canvas/dock/dock-window-menu.tsx` | 39 | Show/hide/reset panel menu | Reusable infra |
| `web/src/components/canvas/workspace/studio-chrome.ts` | 17 | Shared className constants | Reusable infra |
| `web/src/services/image-storage.ts` | **297** | `uploadImage`/`resolveImageUrl`/blob blob-store (cel backing) | **Core dependency** |

---

## 1. Document model

**Types** — `web/src/types/canvas.ts`:

```
L86  CanvasPixelBlend = "normal" | "multiply" | "screen" | "overlay" | "add"
L88  CanvasPixelLayer = { id: string; name: string; visible: boolean; opacity: number /*0..1*/; blend: CanvasPixelBlend }
L97  CanvasPixelCel   = { storageKey?: string }        // one layer's bitmap on one frame
L99  CanvasPixelFrame = { id: string; durationMs: number; cels: Record<layerId, CanvasPixelCel> }
L106 CanvasPixelDoc   = {
        width: number;
        height: number;
        palette: string[];              // hex colors
        layers: CanvasPixelLayer[];     // index 0 = BOTTOM
        frames: CanvasPixelFrame[];
        fps: number;
        background: string;             // CSS color or "transparent"
     }
L117 DEFAULT_PIXEL_PALETTE = 16 colors (below)
```

**Defaults** — `web/src/lib/canvas/pixel/document.ts`:

```
L5   DEFAULT_FRAME_MS = 100
L6   MIN_SIZE = 1
L25-30 createPixelDoc(w,h,palette?):
   layer = { id:"layer-1", name:"layer-1", visible:true, opacity:1, blend:"normal" }
   frame = { id:"frame-1", durationMs:100, cels:{ "layer-1": {} } }
   doc   = { width:max(1,round(w)), height:max(1,round(h)),
             palette:[...DEFAULT_PIXEL_PALETTE], layers:[layer], frames:[frame],
             fps:10, background:"transparent" }
```

16-color palette (L117): `#000000 #ffffff #7f7f7f #c0c0c0 #880015 #ed1c24 #ff7f27 #fff200 #22b14c #00a2e8 #3f48cc #a349a4 #b97a57 #ffaec9 #ffc90e #efe4b0`

**Pure doc reducer functions** (`document.ts`, all immutable / return same ref if no-op):

| Fn | Line | Behavior / guard |
|---|---|---|
| `findPixelLayer` | 32 | lookup by id |
| `addPixelLayer(doc,id?,name?)` | 36 | id default `nanoid()`; name default id; adds empty cel `{}` to every frame |
| `removePixelLayer(doc,id)` | 41 | **min 1 layer**; removes layer + its cel key from all frames |
| `movePixelLayer(doc,id,"forward"\|"backward")` | 53 | swaps adjacent; index 0 = bottom, forward = up |
| `patchPixelLayer(doc,id,patch)` | 58 | partial layer merge |
| `addPixelFrame(doc,id?,durationMs?)` | 63 | id `nanoid()`, dur 100; cels `{}` for every current layer |
| `removePixelFrame(doc,id)` | 68 | **min 1 frame** |
| `duplicatePixelFrame(doc,id,newId?)` | 73 | shallow-copies each cel (`{...cel}`); inserts after source |
| `movePixelFrame`,`patchPixelFrame` | 81,86 | swap; patch `{durationMs}` |
| `setFrameCel(doc,frameId,layerId,storageKey?)` | 91 | sets `{storageKey}` or `{}` |
| `setPalette`/`setDocSize`/`setDocFps`/`setDocBackground` | 97-110 | size max(1,round), fps max(1,round) |

**Project/group store** — `web/src/stores/use-pixel-store.ts`:
```
L9  PixelProject = { id, title, createdAt, updatedAt, groupId, doc: CanvasPixelDoc }
L18 PixelGroup   = { id, name, createdAt }
L49 createProject(title?, {width,height}, groupId?): default size 64×64 (L70), auto-creates "Default Group"
L125 persist key "open-canvas:pixel_projects" via createDebouncedPersist(["projects","groups"]) (L41/L6 lib/localforage-storage)
L127-145 rehydrate: repairs missing groupId / creates default group
```
New doc size default is **64×64** (store L70); headless default is also 64×64 (`headless.ts:43`). Library page size presets (16…4K) at `pages/pixel/index.tsx:11-25`.

---

## 2. Tools

**Tool list + hotkeys** — `pixel-studio-shared.ts:26-38`:

| id | Icon | Label | Hotkey |
|---|---|---|---|
| `pencil` | Pencil | Pencil | **B** |
| `eraser` | Eraser | Eraser | **E** |
| `bucket` | PaintBucket | Bucket | **G** |
| `line` | Minus | Line | **L** |
| `rect` | Square | Rect | **U** |
| `ellipse` | Circle | Ellipse | **O** |
| `eyedropper` | Pipette | Eyedropper | **I** |
| `select` | SquareDashed | Select | **M** |
| `move` | Move | Move | **V** |
| `hand` | Hand | Hand | **H** |

`TOOL_HOTKEYS` inverts to `{hotkeyLower → id}` (L38). `PixelTool` union at L16.

**Per-tool behavior** (`pixel-studio.tsx`):

| Tool | Trigger | Behavior | Ref |
|---|---|---|---|
| pencil | down/drag | `paintAt`: stamps `brushSize`×`brushSize` block; drag draws Bresenham line between last→current per brush offset; `rgba=hexToRgba(color)` | down 599-614; move 629-643; `paintAt` 508-523 |
| eraser | down/drag | identical but `rgba=[0,0,0,0]` | 509 |
| bucket | down | `floodFill(x,y,color,tolerance)`; commits immediately (flush) | 604-609 |
| line | down→up | live `updatePreview` draft (semi-composed), commit `drawShape` on up | 541-550, 683-689 |
| rect | down→up | same; `filled` flag; rect dims `+1` inclusive | 525-529 |
| ellipse | down→up | same; center=a, radii=|b-a| | 528 |
| eyedropper | down | `sampleColor`: scans top-most→bottom visible layer, first `a>0` → hex; sets color | 496-506, 582-585 |
| select | down/drag | marquee `Rect` (normalized min/max, +1), marching-ants overlay; no bitmap effect | 587-591, 644-650 |
| move | down/drag/up | requires selection; snapshots `orig`, extracts region, on move restores region→clears→blits offset; commits on up | 593-598, 651-667, 698-705 |
| hand | down | pan view | 571-577 |

**Symmetry (Mirror X/Y)**: applied in `paintAt` (519-521) for pencil/eraser and `drawShapeWithSymmetry` (531-539) for shapes; mirror lines drawn 346-363. Toggle buttons in Tools panel 997-1002.

**Keyboard** (708-740):
- `Space` down → temporary hand (`spaceHeld`); up → release (712-715, 731-733)
- `Ctrl/Cmd+Z` undo, `Ctrl/Cmd+Shift+Z` redo, `Ctrl/Cmd+Y` redo (717-727)
- any tool hotkey → `setTool` (728-729)
- ignored when typing in INPUT/TEXTAREA/contentEditable (709)
- Middle mouse button also pans (571)

**Tool options** (Tools panel, 978-1008): Brush size `Slider 1..32` default `1` (59,981); Tolerance `Slider 0..255` default `0` (60,986); `Filled` checkbox for rect/ellipse default `true` (61,992); Mirror X/Y; Grid toggle default `false` (64).

**Blend modes** — `pixel-studio-shared.ts:24-25`:
`["normal","multiply","screen","overlay","add"]` with labels Normal/Multiply/Screen/Overlay/Add. Implemented in `raster.ts:223-231` (`blendChannel`: multiply=cb·cs, screen=1-(1-cb)(1-cs), overlay=cb<.5?2cbcs:1-2(1-cb)(1-cs), add=min(255,cb+cs), normal=source).

---

## 3. Panels

**Dock definitions** — `pixel-studio-shared.ts:39-46`:

| id | Label | Icon | Default dock |
|---|---|---|---|
| tools | Tools | Wrench | **left** |
| palette | Palette | Palette | **left** |
| layers | Layers | Layers | **right** |
| frames | Frames | Film | **right** |
| properties | Properties | Settings2 | **right** |
| preview | Preview | MonitorPlay | **bottom** |

`renderPanel` implementations (`pixel-studio.tsx`):

- **tools** (974-1011): 5-col grid of the 10 tool buttons (`toolButton` 965-971); Brush slider; Tolerance slider; Filled checkbox (rect/ellipse only); Mirror X/Y buttons; Grid checkbox.
- **palette** (1012-1044): "Colors" + `+` add-current-color (dedup by normalized hex, 1019); 8-col swatch grid (1026-1038) — click=set color, right-click=remove (keeps ≥1, 1032); `PsColorFields` HSB/RGB/hex editor (1040).
- **layers** (1045-1078): header Add/Delete (1049-1050); **reversed** list = top layer first (1053); per row: visibility eye (1055), select name (1058), move ↑/↓ (1059-1060); footer for active layer: Opacity slider 0-100→`clamp01/100` (1068), Blend `Select` (1072).
- **frames** (1079-1104): FPS `InputNumber 1..60` (1084); Add/Delete/Duplicate (1086-1088); frame chips `#n` highlighted (1092-1096); Duration `InputNumber 20..5000 step 10` (1100).
- **properties** (1105-1153): Size W/H `InputNumber 1..4096` + **Apply** → `changeSize` (resizes all buffers nearest + updates doc) (1108-1114, 811-826); Background select (transparent/white/black/custom) + custom `PsColorPicker` (1115-1130); Import fit select (Fit/Stretch/Original) + File button (1131-1135) + canvas image-node list (1136-1145); Export PNG / Sheet buttons (1146-1150).
- **preview** (1154-1160): `<canvas ref={previewRef}>` showing the current frame composite at true size, `image-rendering:pixelated`, max 100%.

**Dock layout engine** (`dock-layout.ts` / `dock-panel.tsx`):
```
dock-layout.ts
 L6  DockPanelDef = { id, labelKey, icon, dock:"left"|"right"|"bottom", hidden? }
 L22 DockLayout  = { groups:{left,right,bottom: DockGroup[]}, sizes:{...}, split:{left,right} }
 L31-39 DOCK_MIN_SIZE 168 / MAX 640 / bottom min120 max420; default sizes left288 right288 bottom208
 L34 DOCK_MAX_GROUPS { left:2, right:2, bottom:1 }   // side edges may stack 2 groups
 L41 storage prefix "open-canvas:studio-dock:v4:"
 L84-114 dockLoadLayout (localStorage, validated); L116 dockSaveLayout
 L134-167 reveal/toggle/move/setActive/setSize/setSplit
dock-panel.tsx
 L81-97  useDockLayout(studio, defs) → {layout,toggle,reveal,move,activate,resize,split,reset}
 L99-287 DockArea: left col | children (centre) | right col; bottom col; splitters (resize + split);
        drag-to-dock with 6px threshold, 1/4-edge + 1/4-bottom drop zones (167-196, 261-284)
 L70-74  responsive width steps; fitSize caps dock width
```
Top bar (`pixel-studio.tsx:1188-1238`): Back, title; **File** (Import Image…, Export PNG, Export Spritesheet 1164-1169), **Edit** (Undo/Redo/Clear Layer/Deselect 1170-1176), **View** (Pixel Grid, Mirror X/Y, Zoom In/Out, Fit, Actual Size 1177-1186), DockWindowMenu (1217); right side Zoom-out / `%` / Zoom-in / Fit / Play-Pause / Export (1220-1237).

---

## 4. Canvas rendering, zoom/pan, checker, snap, playback

**View model**: `View = { x, y, k }` (`pixel-studio-shared.ts:19`). `clampZoom 0.05..64` (L48); `nextZoom` (L49) snaps through 1× and steps by 25%/2× (multiplicative below 1, additive above 1).

**Main render effect** (`pixel-studio.tsx:258-375`), runs every state change:
1. DPR-aware backing size `round(cw*dpr)` (262-268); `setTransform(dpr)`; `imageSmoothingEnabled=false` (271-272).
2. One-time **auto-fit** per `projectId:WxH`: `k0=min(cw/W,ch/H)*0.9`, floor to int when ≥1, else ≥0.05; centers (274-284).
3. **Checkerboard** background: cached 16×16 pattern (`#ffffff`/`#d0d0d0`, created 212-224), filled over doc rect (288-291).
4. **Layer composite** (292-305): collect ready layer buffers for `activeFrameId`; missing ones trigger `loadCel`; if `W*H ≤ RENDER_LIMIT (16,777,216)` → `compositeLayers(...)` → `bufferToCanvas` → `docCanvasRef` → `drawImage(x,y,W*k,H*k)`.
5. **Preview panel** canvas redrawn to doc size (306-318).
6. **Draft** (shape live preview) drawn over composite (319).
7. **Pixel grid** when `showGrid && k>=4`, 1px `rgba(127,127,127,0.35)` lines aligned to `+0.5` (320-335).
8. **Selection** marching-ants: black then offset-white dashed `strokeRect` (336-345).
9. **Symmetry** guide lines pink `rgba(236,72,153,0.6)` (346-363).

**Coordinates**: `docPoint` = `floor((client - rect - view.x)/view.k)` (377-381). `zoomAt(clientX,Y,factor)` keeps cursor point fixed (383-391).

**Pan**: pointer-down when middle button / hand tool / space held → `setPointerCapture`, capture `panRef` (571-577); move adds delta (617-628); up ends (670-676). Cursor reflects `grab` vs `crosshair` (1251).

**Wheel zoom**: non-passive listener (470-494); starts a gesture, zoom at cursor via `nextZoom(k, deltaY<0?1.25:0.8)`, debounced 160ms then `endGesture`.

**Snap canvas gesture optimization** (`snapRef`, `checkerOverlayRef`):
- `beginGesture(base)` (393-428): copies current `docCanvas` into `snapRef`, positions it + a CSS checker overlay at `base`, **hides the main canvas**.
- `applyGesture(next)` (430-445): transforms snap + overlay via `translate/scale` (CSS), recomputes checker tile/offset — avoids full recompositing every frame.
- `endGesture`/`cancelGesture` (447-468): commit new view / restore.
- During gesture the render effect resets/hides snap+overlay only when not active (364-374).

**Frame playback** (66, 248-255, 1231-1233): `playing` boolean; `setTimeout` (max(20, frame.durationMs)) advances `activeFrameId` cyclically; loops. No `requestAnimationFrame`; fps field (doc.fps) is only used for display/export metadata, playback uses per-frame `durationMs`.

**Onion skin: NOT IMPLEMENTED.** Grep for `onion` finds no match in any pixel file — the only multi-frame affordances are the frame chips, Play/Pause, and the single-frame Preview panel. (Matches for "animate/onion" live in the unrelated texture studio.) Add onion skinning as new work in the Rust port if desired.

**Grid/snap**: There is a pixel **grid overlay** (`showGrid`, drawn at k≥4) but **no snapping** of tools/shapes to grid — brush coordinates are integer pixels inherently.

---

## 5. Raster ops, cel storage, compositing, export, history

### 5a. Raster primitives — `web/src/lib/canvas/pixel/raster.ts`
```
L3   Rgba = [r,g,b,a]
L5   clamp = round + 0..255
L7   hexToRgba: "transparent"→[0,0,0,0]; #rgb/#rgba/#rrggbb/#rrggbbaa
L24  rgbaToHex: #rrggbb (+aa if a<255)
L30  makeBuffer(w,h) = Uint8ClampedArray(w*h*4)
L34/40 getPixel/setPixel (bounds-checked)
L49  drawLine: Bresenham
L74  drawRect: filled or outline
L93  drawEllipse: midpoint ellipse, filled or outline, degenerates to line/dot
L143 floodFill: stack scan, target match within `tolerance` (per-channel max-abs), returns count; no-op if seed==color
L172 blit: RAW copy of overlapping src region into dst (no alpha blend) — used by Move
L189 extractRegion, L205 resizeNearest (both nearest)
L223 blendChannel (normal/multiply/screen/overlay/add)
L233 compositeLayers(layers[{buffer,opacity,blend,visible}], w,h, background):
       fills background if != "transparent";
       per layer: sourceAlpha = (a/255)*opacity;
       outAlpha = sa + ba*(1-sa);
       channel = (blend(cb,cs)*sa + cb*ba*(1-sa)) / outAlpha   // source-over w/ blend
```

### 5b. Cel storage (`storageKey`)
- A cel with art = `{ storageKey: "image:<nanoid>" }`; empty cel = `{}` (`types/canvas.ts:97`, `document.ts:91-95`).
- Bitmaps live in **localforage** store `open-canvas/image_files`, keyed `image:<nanoid>` (`image-storage.ts:18,47-64`). `uploadImage` stores PNG blob; `resolveImageUrl`/`getImageBlob` read it back (L176-185, 216-218).
- In-memory editor keeps a decoded RGBA cache: `buffersRef: Map<"frameId:layerId", Uint8ClampedArray>` and `pendingRef` dedup (`pixel-studio.tsx:43-44,94-119`).
- `loadCel(frameId,layerId)` (94-119): cache → pending → resolve `storageKey`→URL→fetch→`blobToBuffer(w,h)`; fallback all-zero buffer.
- `flushDirty()` (121-141): for each dirty cel key, `bufferToPngBlob` → `uploadImage` → `setFrameCel(doc, frameId, layerId, storageKey)` → `persist` → push history. `dirtyRef` keyed by `celKey` (`shared:47`).
- Cleanup: `cleanupUnusedCanvasImages` counts pixel projects as live refs (`image-storage.ts:262-277, 279-288`) so Garbage-collection keeps cel bitmaps.

### 5c. Frame compositing & PNG export
- `compositeFrame(frameId)` (`pixel-studio.tsx:744-755`): loads all layer buffers for that frame → `compositeLayers(...)`.
- **Export PNG** (`doExport("png")` 757-784): `compositeFrame(activeFrameId)` → `bufferToPngBlob` → `file-saver saveAs(..., title+".png")`.
- **Export spritesheet** (764-777): canvas `(W*frames) × H`, blits each frame's composite at `index*W`, `toBlob` → `title-sheet.png`.
- **Agent/headless export** uses `export.ts:compositePixelFrameFile(doc, frameId, name)` (26 lines): reads cels from image store, `compositeLayers`, returns a PNG `File`. `bufferToDataUrl`/`regionToPngDataUrl` (io.ts:75-86) provide scaled dataURLs (scale via `resizeNearest`).
- **Import** (`doImport` 786-803): `importImageToBuffer(source, W, H, fit)` (io.ts:70-73) → `buffer.set(imported)` on active cel → flush. Fit modes: `stretch` (fill), `contain` (letterbox, default), `nearest` (pixel-exact, centered) — `io.ts:38-63`.

### 5d. History model — `web/src/lib/canvas/pixel/history.ts`
- `createPixelHistory<T>(limit=100)` (L11-37): snapshot **stack** + cursor index; `push` truncates redo tail, caps to last `limit`; `undo`/`redo` move cursor and return snapshot; `canUndo/canRedo`; `reset(snapshot)`.
- Wired at `pixel-studio.tsx:46`; reset on project change (183-194); **full-document snapshots** are pushed on `flushDirty` (139), `applyDoc` (168), `changeSize` (823). `undo`/`redo` (144-161) persist the snapshot and clear buffers/pending/docCanvas.
- Snapshots capture only `CanvasPixelDoc` metadata + `storageKey`s; the actual pixels are re-`loadCel`'d from the image store, so undo is O(doc) not O(pixels).

### 5e. Agent / headless raster pipeline
- `pixel-agent-ops.ts`: pure fold `applyPixelAgentOps` (106-125) applies structural ops only; async/bitmap op types listed at L48 (`pixels.set`, `region.set`, `region.get`, `fill`, `shape`, `image.load`, `export`) with JSON schemas 67-99.
- `headless.ts:runHeadlessPixelOps(projectId, ops)` (34-172): replays ops against `usePixelStore` + image store with **no UI**; `loadBuffer`/`dirty`/upload loop (47-169) mirrors the editor.

---

## Key porting notes for Rust+Slint

1. **Document** = plain serde-able structs; `layers[0]` is the **bottom** layer, but the Layers panel renders **reversed** (`pixel-studio.tsx:1053`).
2. **Cel storage is indirection**: `Doc → Cel.storageKey → blob store`. For Rust you likely want an in-process RGBA store keyed by an id, with PNG-on-disk only at persistence boundaries. The reducer (`document.ts`) never touches pixels — keep that separation.
3. **Raster backend**: everything is simple integer 2D raster (Bresenham line, midpoint ellipse, stack flood fill, nearest resize, source-over compositor). No anti-aliasing, no GPU needed. A `Vec<[u8;4]>` / `Vec<u8>` RGBA8 buffer is a 1:1 match for `Uint8ClampedArray`; consider `tiny-skia` only for PNG encode/decode, not drawing.
4. **Compositor** is the one non-trivial formula (`raster.ts:233-259`) — port it exactly, per-channel blend inside source-over.
5. **Tool state machine** = `PixelTool` + pointer phases. Pencil/eraser mutate the cel live; line/rect/ellipse use a throwaway "draft" buffer; select/move operate on a region copy. Symmetry multiplies each stamp/shape.
6. **View/gesture**: port `View{x,y,k}`, `nextZoom`, `zoomAt`, and the CSS snap-canvas trick (offload pan/zoom during drag; recomposite on release).
7. **Onion skin is absent** — decide whether the Rust port adds it.
8. **History** = bounded full-doc snapshot stack (100); cheap because pixels are external.
9. Keyboard: B/E/G/L/U/O/I/M/V/H, Space=pan, Ctrl/Cmd+Z, Ctrl/Cmd+Shift+Z, Ctrl/Cmd+Y. Mouse: middle-drag pans.

<results>
<files>
- D:\Dev\open-canvas\web\src\components\canvas\workspace\pixel-studio.tsx - The whole editor (1262 LOC): tool state machine, canvas render loop, all 6 panels, pointer/keyboard, import/export, agent + screenshot bridge
- D:\Dev\open-canvas\web\src\components\canvas\workspace\pixel-studio-shared.ts - TOOLS/hotkeys, BLENDS, PIXEL_DOCK_PANELS, View/zoom helpers, shared types (50 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\pixel\document.ts - Pure doc reducer: create + layer/frame/cel/palette/size/fps/background CRUD (111 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\pixel\raster.ts - All pixel algorithms + blend modes + compositeLayers (260 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\pixel\io.ts - Buffer↔Canvas↔PNG/dataURL, image import fit (87 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\pixel\history.ts - Bounded snapshot undo/redo stack (38 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\pixel\export.ts - Doc/frame → PNG File (26 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\pixel\headless.ts - UI-less op runner for agent (172 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\pixel-agent-ops.ts - Flat agent op schema + pure structural fold (126 LOC)
- D:\Dev\open-canvas\web\src\stores\use-pixel-store.ts - PixelProject/Group store + persistence (149 LOC)
- D:\Dev\open-canvas\web\src\types\canvas.ts - CanvasPixelDoc/Layer/Cel/Frame/Blend types + palette (L86-117)
- D:\Dev\open-canvas\web\src\pages\pixel\index.tsx - Library page, size presets, mounts editor (358 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\ps-color-picker.tsx - HSB/RGB/hex picker (185 LOC)
- D:\Dev\open-canvas\web\src\lib\canvas\ps-color.ts - Color conversions (62 LOC)
- D:\Dev\open-canvas\web\src\services\image-storage.ts - Cel bitmap blob store (uploadImage/resolveImageUrl/getImageBlob) (297 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\dock\dock-panel.tsx - Dock engine + DockArea (287 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\dock\dock-layout.ts - Dock layout model/persistence (168 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\dock\dock-tabs.tsx - Dock tab strip (94 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\dock\dock-window-menu.tsx - Panel show/hide menu (39 LOC)
- D:\Dev\open-canvas\web\src\components\canvas\workspace\studio-chrome.ts - Shared className constants (17 LOC)
</files>

<answer>
Pixel Studio is a self-contained Aseprite-like editor over an immutable, pure-JS document model. The three-layer separation is clean and maps well to Rust:
(1) data — CanvasPixelDoc/Layer/Frame/Cel in types/canvas.ts with a pure reducer in document.ts; (2) raster engine — integer-only RGBA8 primitives + a source-over/5-blend compositor in raster.ts; (3) UI — pixel-studio.tsx drives a tool state machine (10 tools, B/E/G/L/U/O/I/M/V/H, Space-pan, Ctrl+Z/Y undo/redo), a per-frame cel RGBA cache backed by PNG blobs in localforage, and a 6-panel dock (tools+palette left, layers+frames+properties right, preview bottom). Zoom/pan use a CSS snapshot-canvas gesture trick to avoid recompositing. History is a bounded (100) full-document snapshot stack — cheap because pixels live outside the doc behind `storageKey`. PNG + horizontal-spritesheet export compose frames via compositeLayers→canvas.toBlob. Notably, onion skinning is NOT implemented (no matches anywhere in pixel code), so it is net-new if you want it.
</answer>

<next_steps>
1. Define Rust structs mirroring types/canvas.ts (serde camelCase), with Cel holding a storage key/id NOT pixels; implement the document.ts reducer 1:1 (including layers[0]=bottom and min-1 layer/frame guards).
2. Choose the raster backend: a raw Vec<u8> RGBA8 buffer is a direct match; use `tiny-skia`/`image` only for PNG encode/decode, not drawing.
3. Port raster.ts algorithms verbatim (Bresenham, midpoint ellipse, stack flood fill, nearest resize) and the compositeLayers formula exactly.
4. Model the tool state machine as PixelTool enum + pointer-gesture phases (live paint vs draft-buffer shape preview vs region select/move), with symmetry applied as a stamp multiplier.
5. Recreate the Slint panel layout from PIXEL_DOCK_PANELS and the DockArea semantics (left/right stack 2 groups, bottom 1; default sizes 288/288/208; v4 persistence) — or a simplified fixed layout if you don't need drag-to-dock.
6. Decide persistence (blob store + PNG) and whether to add onion skinning (absent in the web version).
</next_steps>
</results>