<results>
<files>
- D:\Dev\open-canvas\web\src\pages\texture\index.tsx - Route page: library view (groups/projects) + editor mount
- D:\Dev\open-canvas\web\src\pages\texture\components\texture-studio.tsx - Editor shell: state/history/render loop, toolbar, export menu, panel wiring
- D:\Dev\open-canvas\web\src\pages\texture\components\params-panel.tsx - Layer/type/transform/param panel
- D:\Dev\open-canvas\web\src\pages\texture\components\layer-list.tsx - Layer list (visibility, reorder, add/delete)
- D:\Dev\open-canvas\web\src\pages\texture\components\effects-panel.tsx - Post-effects panel + shared rows (SliderRow/CheckRow/ColorRow/Section)
- D:\Dev\open-canvas\web\src\pages\texture\components\gradient-editor.tsx - Gradient/ramp editor + 16 ramp presets
- D:\Dev\open-canvas\web\src\pages\texture\components\output-panel.tsx - Resolution + animation/GIF panel
- D:\Dev\open-canvas\web\src\pages\texture\components\type-browser.tsx - 117-type browser modal (search/category/favorites)
- D:\Dev\open-canvas\web\src\pages\texture\components\use-texture-thumbnails.ts - 64px thumbnail renderer/cache
- D:\Dev\open-canvas\web\src\pages\texture\components\samples-modal.tsx - Built-in sample gallery (128px thumbs)
- D:\Dev\open-canvas\web\src\pages\texture\components\preset-library-modal.tsx - User preset save/load/import/export (localStorage)
- D:\Dev\open-canvas\web\src\pages\texture\components\sheet-export-dialog.tsx - Sprite-sheet export config
- D:\Dev\open-canvas\web\src\pages\texture\components\channel-pack-dialog.tsx - Channel-pack config
- D:\Dev\open-canvas\web\src\pages\texture\components\texture-studio-shared.ts - Dock panel definitions
- D:\Dev\open-canvas\web\src\lib\texture\types.ts - Document model + defaults + blend modes + channel config
- D:\Dev\open-canvas\web\src\lib\texture\renderer.ts - WebGL2 renderer (ping-pong FBOs, gradient LUT, 2 programs)
- D:\Dev\open-canvas\web\src\lib\texture\shaders\vertex.ts - Fullscreen-quad vertex shader
- D:\Dev\open-canvas\web\src\lib\texture\shaders\texture.ts - Per-layer shape/type + composition fragment shader
- D:\Dev\open-canvas\web\src\lib\texture\shaders\post.ts - Post-effects fragment shader
- D:\Dev\open-canvas\web\src\lib\texture\registry.ts - Type order / ID map / per-type param specs / defaultParams
- D:\Dev\open-canvas\web\src\lib\texture\presets.ts - 12 built-in samples (full EditorState presets)
- D:\Dev\open-canvas\web\src\lib\texture\export.ts - PNG/normal/channel/sheet/GIF/canvas-node export
- D:\Dev\open-canvas\web\src\lib\texture\history.ts - Undo/redo stack + share-link decode
- D:\Dev\open-canvas\web\src\stores\use-texture-store.ts - Zustand project/group store (localforage-persisted)
- D:\Dev\open-canvas\web\src\router.tsx - Routes /texture and /texture/:id
</files>

<answer>

# Texture Studio - Port Map

## 0. High-level shape

- Route: `/texture` (library) and `/texture/:id` (editor) — `web/src/router.tsx:29-30`.
- Library vs editor switch: `web/src/pages/texture/index.tsx:89-105` (editor when `:id`), library grid at `:107-323`.
- Editor is `TextureStudio` (`texture-studio.tsx:91`), receives `project` + `onUpdate` and owns an ephemeral `EditorState` that is written back into the store.
- Rendering: WebGL2, two programs (per-layer + post), ping-pong FBOs. No offscreen intermediate beyond the ping-pong buffers, except export paths which create their own renderer/canvas.
- Persistence: projects/groups in Zustand+localforage; user presets and favorites in `localStorage`.
- Everything shape-related is GPU; everything image-analytic (normal map, channel pack, sheet layout, GIF) and all data/UI is CPU.

---

## 1. Document model

Defined in `web/src/lib/texture/types.ts`.

**Project/group (store)** — `use-texture-store.ts:8-21`:
```
TextureProject { id, title, state: EditorState, createdAt, updatedAt, groupId }
TextureGroup   { id, name, createdAt }
```
Actions (`:23-35`): createProject/updateProject/renameProject/deleteProject, createGroup/renameGroup/deleteGroup/setProjectGroup. Persist key `open-canvas:texture_projects` (`:119`), storage = debounced localforage (`:39`).

**EditorState** — `types.ts:88-102`:
`resolution, time, animate, animSpeed, checkerboard, blackBackground, gifFps, gifDuration, gifSeamless, postEffects: PostEffects, activeLayerId, layerCounter, layers: LayerState[]`. Defaults `createDefaultState()` `:176-192` (512px, one "Circle" layer).

**LayerState** — `types.ts:21-44`:
`id, name, type, blendMode, opacity, polarConversion, invertEnable, visible, typeParams: number[16], offsetX/offsetY, scaleX/scaleY, rotation(deg), scrollX/scrollY, gradEnable, gradStops: GradientStop[], solidColorEnabled, solidColor: RGB`. Default `createDefaultLayer()` `:151-174`.

**GradientStop** — `types.ts:16-19` `{ position:0..1, color:"#rrggbb" }`; default B/W `:104-107`.

**Blend modes** — `types.ts:6-9`: `normal|add|multiply|screen|mask` → shader ints `0..4` (`BLEND_MODE_ID`).

**Resolution** — `types.ts:11-12`: `64|128|256|512|1024|2048`.

**PostEffects** — `types.ts:46-86` (defaults `:109-149`). 17 toggleable effects with params:
- blur (strength), sharpen (strength), pixelation (pixelSize), chromaticAberration (amount)
- vignette (strength,size,color), scanline (density,speed,strength,color), kaleidoscope (segments,rotation), mirrorTile (x,y)
- swirl (strength,radius), edgeDetection (thickness,color), toon (dark,light), vignetteMask (enabled only)
- bloom (strength), color (shadow/midtone/highlight RGB)

**Channel packing (export-only)** — `types.ts:194-222`: `CHANNEL_SOURCES` 10 sources (baseR/G/B/A, baseLuma, normalX/Y/Z, black, white), `ChannelConfig{r,g,b,a}`, `CHANNEL_PRESETS` (base, normal_xy, normal_xyz, luma_alpha).

**Share/preset formats**:
- Share link `#tc=<base64url(EditorState JSON)>` decoded by `decodeTextureShare` — `history.ts:59-78`.
- Preset file `{ version:1, ...EditorState }` — `preset-library-modal.tsx:53-62`; stored list `TexturePreset{id,name,createdAt,updatedAt,state}` (`:11-17`), max 40 (`:9`).

---

## 2. Shader pipeline (WebGL2 / GLSL ES 3.00)

Renderer: `web/src/lib/texture/renderer.ts` (class `TextureRenderer`).

### Init / resources
- Context `webgl2` with `preserveDrawingBuffer:true, alpha:true` — `renderer.ts:131-137`.
- Two programs: `program` = VERTEX+TEXTURE (`:139`), `postProgram` = VERTEX+POST (`:140`).
- Fullscreen quad: 4 verts `[-1,-1, 1,-1, -1,1, 1,1]`, `TRIANGLE_STRIP`, attrib `a_position` — `:145-155, :325`.
- Ping-pong: 2 textures + 2 FBOs, RGBA8 (`UNSIGNED_BYTE`), `NEAREST`, `CLAMP_TO_EDGE` — `:194-214`. Recreated on `resize` (`:216-222`).
- Gradient LUT: 256x1 RGBA texture, `LINEAR`, `CLAMP_TO_EDGE` — `:224-240`; updated via `updateGradientLUT` (`:242-247`).

### Shader A — vertex (`shaders/vertex.ts:2-9`)
`v_uv = a_position*0.5+0.5; gl_Position = vec4(a_position,0,1)`. Shared by both programs.

### Shader B — texture / per-layer (`shaders/texture.ts`, 2668 lines)
Uniforms (`texture.ts:8-35`): `u_type`(int), `u_time`, `u_polarConversion`, `u_params[16]`, `u_transform[5]`=(offX,offY,scaleX,scaleY,rot), `u_scroll`(2), `u_invertEnable`, `u_gradEnable`, `u_gradTex`(LUT), `u_isBaseLayer`, `u_backTex`(previous layer result), `u_blendMode`(0..4), `u_opacity`, `u_blackBackground`, `u_solidColorEnabled`, `u_solidColor`(vec3).

Structure:
- Utilities/noise: `hash1`, `hash1v2`, `hash2v2`, `fbm` — `texture.ts:40-170`.
- ~120 shape functions `typeXxx(vec2 uv[, float mode])` — `texture.ts:171-2423` (e.g. `typeCircle:171`, `typeRing:247`, `typePerlinNoise:537`, `typeFbmNoise:547`, `typeVoronoiNoise:557`, `typeFire:633`, `typeLava:947`, `typeStar:977`, `typeBricks:1692`, `typeHalftone:1809`, `typeGeoRelief:2386` ...).
- Dispatch split into **four** functions to stay under GLSL compiler limits: `getVal0:2425`, `getVal1:2464`, `getVal2:2495`, `getVal3:2530`; each maps `u_type` int → `typeXxx(...)`, returns `-1.0` when unmatched.
- `main()` — `texture.ts:2564-2667`:
  1. Polar conversion (`:2568-2573`).
  2. Transform: center, scale, rotate, offset, time-scroll — `:2575-2592`.
  3. `val = getVal0; if val<-0.5 getVal1; ... getVal3; else 0` — `:2595-2599`.
  4. Clamp, optional invert — `:2602-2605`.
  5. Gradient LUT sample `texture(u_gradTex, vec2(val,0.5))` else grayscale — `:2607-2613`.
  6. Solid-color multiply — `:2615-2617`.
  7. **Composition**: alpha = `val`; premultiply with `u_opacity` (`:2622-2627`); if base layer emit (composited on black if `u_blackBackground`) else blend with `u_backTex` by `u_blendMode` — Normal/Add/Multiply/Screen/Mask — `:2629-2666`.

Per-layer invocation loop and uniform wiring: `renderer.ts:249-330`; ping-pong index swap `:327-329`.

### Shader C — post (`shaders/post.ts`, 210 lines)
Uniforms `post.ts:5-60`: `u_mainTex`, `u_time`, `u_resolution`, then all 17 effect toggles/params mirroring `PostEffects` (names in `MAIN`/`POST_UNIFORMS` lists at `renderer.ts:29-72`).
Helpers: `swirl:64`, `kaleidoscope:77`, `mirrorTile:88`.
`main()` — `post.ts:95-208`, order:
1. UV manip: mirrorTile → kaleidoscope → swirl → pixelation (`:99-114`).
2. Sample `u_mainTex` (`:116`).
3. chromatic aberration (`:118-123`), blur 3x3 (`:125-134`), bloom 5x5 luma→add (`:136-149`), sharpen laplacian (`:151-159`), edge detection (`:161-170`), scanline (`:172-176`), toon posterize (`:178-182`), color grade shadow/mid/high via luma (`:184-193`), radial vignette-mask (`:195-199`), vignette color (`:201-205`).

### Composition / resolution / passes
- Layer order: `renderer.ts:255` filters `visible`; index 0 = base (`:270`); layers composited bottom→top.
- Each layer renders to FBO[write], reading previous result as `u_backTex` from FBO[read] (`:317-319`).
- Final post pass renders ping-pong[read] to the default framebuffer (`:332-385`).
- Empty-layer case clears to black/transparent (`:256-261`).
- **Preview** is always 512x512 (`texture-studio.tsx:62,142-143,726`), scaled by CSS transform (`:718`).
- **Export/full-res** uses `state.resolution` via a fresh offscreen `TextureRenderer` — `export.ts:26-55`.
- `readPixels` flips vertically (GL bottom-up → top-down) — `renderer.ts:388-404`.

---

## 3. Effect / type registry + presets

### Registry (`web/src/lib/texture/registry.ts`, 4575 lines — AUTO-GENERATED)
- `ParamSpec` `{ key, en, ja, min, max, step, default, index }` — `:3-12`.
- `PARAM_SLOTS = 16` (`:14`) — `u_params[16]`; `defaultParams(type)` fills by `spec.index` (`:4569-4574`).
- `TYPE_ORDER: string[]` — **117 types**, `:17-135`.
- `TYPE_ID: Record<string,number>` — **117** name→shader int, `:138-256`.
- `TYPE_COUNT = 117` (`:258`).
- `PARAM_SPECS: Record<string, ParamSpec[]>` — **117** blocks, `:261-4566` (e.g. `Circle` `:262-293`, `Vignette` `:294-325`, `LensFlare` `:326-357`).

Full type list (verbatim `TYPE_ORDER`, `registry.ts:18-134`):
Circle, Vignette, LensFlare, Sun, SolarGlow, Ring, Crescent, Flash, EnergyRing, AuraRing, Halo, Ripple, Concentric, Pulse, MetaBalls, WaveRingSine, WaveRingNoisy, WaveRingSquare, WaveRingDouble, Star, Polygon, HexGridRadial, Rectangle, Checker, GradientChecker, RoundChecker, DiamondChecker, Spark, Flare, Cross, Glare, StarFlare, RayBurst, Burst, ImpactLines, RadialLines, SpiralV2, Swirl, GodRay, StarBurst, Flower, Spiral, Energy, Crack, Bokeh, Shimmer, VoronoiFluid, Speckle, CrossGrid, SquareGrid, PyramidPattern, RandomTiles, SquareGridDash, Dots, SquareGridPolka, DotMatrix, Zigzag, Crosshatch, TriGrid, Bricks, Scanline, FlowLines, Fabric, PolarDots, Weave, Halftone, SweepGradient, GradationLinear, GradationReflect, GradationRepeat, BevelSquare, Grain, PerlinNoise, FbmNoise, DistortionWave, StripeNoise, ToxicCloud, GeoRelief, Smoke, WaterTurbulence, Electric, SimplexNoise, Lava, Wrinkle, Crystal, AbsNoise, FractalCamo, PlasmaV2, Squiggles, Grunge, GrungeV2, CellularEdge, Twirl, CosmicPortal, Wormhole, Plasma, MarbleNoise, Fire, Cloud, Caustics, Aurora, Flame, PixelNoise, AnalogGlitch, CyberBlock, Mosaic, LaserBeam, GlitchBlock, VoronoiCell, Matrix, Wood, SparkBurst, VoronoiNoise, Cell, Lightning, Kaleido, SymmetricNoise.

UI categories derive from `TYPE_ID` ranges (`type-browser.tsx:31-37`): `<=199` Basic, `<=260` Pattern, else FX.

### Preset libraries (two distinct things)
- **Built-in samples** — `web/src/lib/texture/presets.ts` (5065 lines): `TextureSample{id,name,description,state:EditorState}` (`:4-9`), `BUILTIN_SAMPLES` = **12** full-state presets (`:11+`): magic-energy-ring, fire-burst, water-vortex, impact-shockwave, cyber-portal, frost-nova, toxic-pulse, holy-flare, dark-singularity, electric-storm, hologram-shield, aurora-field. Consumed by `samples-modal.tsx:4,66`.
- **User presets** — localStorage `oc_texture_presets` (`preset-library-modal.tsx:8`), see §1.
- **Gradient ramp presets** — 16 hardcoded ramps (B/W, Fire, Water, Plasma, Lava, Ice, Nature, Neon, Sunset, Gold, Rainbow, Toxic, Aurora, Blood, Hologram, Desert) — `gradient-editor.tsx:8-173`.

---

## 4. Panels / UI inventory

Dock panel defs `TEXTURE_DOCK_PANELS` — `texture-studio-shared.ts:6-12`: `layers`(left), `params`(left), `output`(right), `effects`(right), `gradient`(right). Rendering switch — `texture-studio.tsx:638-660`. Dock system is shared: `@/components/canvas/dock/dock-panel` (`texture-studio.tsx:6,133,695,707`).

| Panel/Modal | File | Notes |
|---|---|---|
| Layer list | `layer-list.tsx` (110) | Eye/reorder/add/delete; reversed display `:32-35` |
| Params panel | `params-panel.tsx` (163) | Layer name/blend/opacity/solid; type select; transform; dynamic param sliders from `PARAM_SPECS` `:57,147-157` |
| Effects panel | `effects-panel.tsx` (147) | 17 post-effect blocks; exports shared `SliderRow/CheckRow/ColorRow/Section` |
| Output panel | `output-panel.tsx` (34) | Resolution + animation/GIF (uses shared rows) |
| Gradient editor | `gradient-editor.tsx` (429) | Stop add/drag/dbl-click-delete, color picker, 16 presets, canvas-2D color sampling `:201-213` |
| Type browser | `type-browser.tsx` (157) | Search/category/favorites, 64px thumbs |
| Thumbnails hook | `use-texture-thumbnails.ts` (68) | One shared renderer, sequential 64px per type, in-memory cache |
| Samples modal | `samples-modal.tsx` (96) | 12 built-ins, 128px thumbs, apply full state |
| Preset library modal | `preset-library-modal.tsx` (194) | Save/load/delete/import/export JSON presets |
| Sheet export dialog | `sheet-export-dialog.tsx` (69) | mode/frames/cell/columns/padding/save-metadata |
| Channel pack dialog | `channel-pack-dialog.tsx` (80) | 4 channel source selects + presets |
| Output modal | `@/components/studio/studio-output-modal` (shared) | Download / save-to-canvas — `texture-studio.tsx:757-766` |
| GIF progress modal | inline `texture-studio.tsx:767-772` | |
| Library page | `index.tsx` (326) | Group sidebar, project cards, create/rename/delete/move |

Toolbar menus: Edit (undo/redo/reset), View (bg/checker/zoom/browse/samples/presets), Export (png/normal/channel/sheet/gif/save-to-canvas) — `texture-studio.tsx:607-704`.

Editor-side state/history:
- `TextureHistory` (50 snapshots, JSON-deep-equal dedupe) — `history.ts:5-57`.
- Debounced record 350ms (`texture-studio.tsx:71,244-251`), rAF-coalesced state commits (`:294-312`).
- Animation loop advances `time` while `animate` — `:354-379`.
- Share-link `#tc=` ingestion — `:399-410`.
- Zoom/pan model `{x,y,k}`, MIN/MAX 0.05/64 — `:64-69,146-225`.

---

## 5. GPU (shader) vs CPU (JS math) — port classification

### Must be reimplemented on GPU (wgpu shaders)
- Vertex fullscreen quad — `shaders/vertex.ts`.
- Per-layer shape generation + transform/polar/invert/gradient-LUT/solid-color — `shaders/texture.ts:2564-2667` and all `typeXxx` (`:171-2423`) + `getVal0-3` dispatch (`:2425-2562`).
- Layer compositing / blend modes / premultiplied alpha — `shaders/texture.ts:2619-2666`.
- All post effects — `shaders/post.ts:95-208`.
- Ping-pong layer accumulation and final blit — `renderer.ts:249-385`.

### CPU — port as plain Rust (no shader)
- Gradient LUT rasterization: `buildGradientLUT` uses canvas-2D `createLinearGradient` (`renderer.ts:88-103`); in Rust = simple per-pixel stop interpolation. Fallback grayscale ramp `:74-86`.
- **Normal map** Sobel: `generateNormalMap` (`export.ts:67-109`) — pure per-pixel math.
- **Channel packing**: `channelValue`/`packChannels` (`export.ts:119-170`), incl. `baseLuma` 0.2126/0.7152/0.0722 (`:130`).
- **Sprite sheet layout**: `exportSheet` (`export.ts:263-328`); colors mode hue rotation uses full HSL conversion `rgbToHsl`/`hslToRgb`/`rotateHue` (`:206-256`); animation mode re-renders per frame with `time_i = time + 2*animSpeed*i/frameCount` (`:305`).
- **GIF encoding**: `gif.js` (vendored vendor script `/gif/gif.js`, loaded at runtime `export.ts:343-359`); frame flip + seamless cross-fade blend (`:396-417`). NOTE: the `/gif/gif.js` and `/gif/gif.worker.js` assets were **not found** under `web/public` in this snapshot — verify before porting.
- Undo/redo, share decode, presets/samples data, favorites, store — all pure data (`history.ts`, `presets.ts`, `use-texture-store.ts`).
- Thumbnails orchestration (`use-texture-thumbnails.ts`, `samples-modal.tsx:24-58`) — CPU-scheduled but GPU-rendered.
- PNG download / blob / canvas-node export glue (`export.ts:33-61,454-483`) — replaced by Rust image encoding.

### GPU resources to replicate in wgpu
- 2x RGBA8 ping-pong color targets (per resolution) `renderer.ts:194-214`.
- 256x1 linear-filtered gradient LUT sampled by `val` `renderer.ts:224-247`.
- Readback with vertical flip `renderer.ts:388-404`.

---

## 6. Rough LOC per file (core vs secondary)

| File | LOC | Tier |
|---|---:|---|
| `lib/texture/registry.ts` | 4575 | Core data (generated) |
| `lib/texture/presets.ts` | 5065 | Core data (generated) |
| `lib/texture/shaders/texture.ts` | 2668 | Core GPU (generated GLSL) |
| `lib/texture/renderer.ts` | 430 | Core GPU orchestration |
| `lib/texture/export.ts` | 484 | Core CPU export |
| `lib/texture/shaders/post.ts` | 210 | Core GPU (generated GLSL) |
| `lib/texture/types.ts` | 223 | Core model |
| `pages/texture/components/texture-studio.tsx` | 776 | Core editor shell |
| `pages/texture/components/gradient-editor.tsx` | 429 | Secondary UI |
| `pages/texture/index.tsx` | 326 | Secondary (library) |
| `pages/texture/components/preset-library-modal.tsx` | 194 | Secondary UI |
| `pages/texture/components/params-panel.tsx` | 163 | Core panel |
| `pages/texture/components/type-browser.tsx` | 157 | Secondary UI |
| `pages/texture/components/effects-panel.tsx` | 147 | Core panel |
| `pages/texture/components/layer-list.tsx` | 110 | Core panel |
| `pages/texture/components/samples-modal.tsx` | 96 | Secondary UI |
| `pages/texture/components/channel-pack-dialog.tsx` | 80 | Secondary UI |
| `pages/texture/components/sheet-export-dialog.tsx` | 69 | Secondary UI |
| `pages/texture/components/use-texture-thumbnails.ts` | 68 | Secondary util |
| `lib/texture/history.ts` | 80 | Core util |
| `stores/use-texture-store.ts` | 143 | Core store |
| `pages/texture/components/output-panel.tsx` | 34 | Secondary panel |
| `pages/texture/components/texture-studio-shared.ts` | 13 | Config |
| `lib/texture/shaders/vertex.ts` | 10 | Core GPU (generated GLSL) |
| **Total (~)** | **~16,410** | |

Generated/auto files (do not hand-port literally, regenerate/translate): `registry.ts`, `presets.ts`, `shaders/*.ts` (each carries an "AUTO-GENERATED from texture-create (gameanimation.info)" header).

## Notes for the wgpu port
- The shader is one mega-program per layer with a 117-way `if/else` dispatch split across 4 functions (`getVal0-3`) — a direct port can either keep one WGSL shader with a big switch, or (better) split into per-type entry points / a pipeline cache. Dynamic indexing `u_params[16]` is allowed in WGSL as a uniform array.
- Post FX are a single fullscreen pass with many `#ifdef`-style branches keyed by uniforms — directly portable, but consider splitting into a small pass chain for perf.
- Blend modes are implemented manually with premultiplied alpha in-shader (`texture.ts:2619-2666`), NOT via fixed-function blending; keep that logic in WGSL.
- Resolution handling is trivial (square only, FBOs resized to canvas). Preview fixed at 512; export at `state.resolution`.
</answer>

<next_steps>
1. Define Rust model first: `EditorState`, `LayerState`, `PostEffects`, `GradientStop`, `BlendMode`, `Resolution` from `types.ts` (straight 1:1). Add `Project/Group` + `Preset`/`Sample` types from the store and preset/sample formats.
2. Decide the shader strategy: single WGSL port of `texture.ts` (with the 117-type dispatch) vs. per-type functions. Translate `getVal0-3` mapping + `typeXxx` bodies; the type↔id map is `registry.ts:138-256`.
3. Port the CPU pieces as plain Rust: Sobel normal map (`export.ts:67-109`), channel pack (`:119-170`), sheet+hue rotation (`:206-328`), gradient LUT (`renderer.ts:88-103`). Use an image crate for PNG; a GIF crate (e.g. `gif`/`image`) to replace gif.js — verify the missing `/gif/*.js` assets first.
4. Rebuild the two GPU resources: double RGBA8 ping-pong targets + 256x1 LUT, plus vertical-flip readback.
5. Scope panels using the LOC table above; `effects-panel.tsx` and `params-panel.tsx` are the shared-row pattern (Section/SliderRow/CheckRow/ColorRow) to reuse across panels in Slint.
6. Confirm the 12 built-in samples + 16 gradient ramps are treated as static data (they can be embedded/parsed from JSON rather than hardcoded).
</next_steps>
</results>