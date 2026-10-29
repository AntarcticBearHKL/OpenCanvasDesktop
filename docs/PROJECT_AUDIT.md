# Project audit — unused files, dead code, vendored reachability

Date: 2026-10-24 · Repo: `open-canvas-desktop` · Scope: everything under `crates/`
(excluding `target*`), plus the vendored imaging crates analysed read-only.

This is the input for a follow-up **build-time / performance** pass. It records what
was measured, what was deleted, and what was deliberately left alone with a reason.

Gates at the end of this pass (all from a clean-ish tree):

| gate | command | result |
|---|---|---|
| check | `cargo check --workspace --all-targets` | pass (implied by `clippy`) |
| build | `cargo build --workspace` | pass (194 s from a check-warm `target/`) |
| lints | `cargo clippy --workspace --all-targets -- -D warnings` | pass (189 s) |
| tests | `cargo test --workspace` | pass, 2 284 passed / 0 failed (558 s) |

Re-run after the vendored gate of §4.6: same results, same 2 284 tests (the gated modules
still compile and run — see §4.6 for how).

---

## 0. Headline numbers

| metric | value |
|---|---|
| `.rs` files | 450 (182 582 lines) |
| `.slint` files | 42 (22 712 lines) |
| product Rust (excl. every `#[cfg(test)]` block and test target) | ≈125 500 lines |
| test Rust (inline test modules + `*_tests.rs` + `tests/` + `examples/`) | ≈57 000 lines (31 %) |
| unreachable `.rs` / `.slint` files | **0 / 0** |
| files deleted | **0** (nothing was provably unreachable) |
| dead `pub` items deleted | **13** (+2 private helpers and 2 imports that became unused), 138 lines |
| vendored code reachable from the app, `algo` | 25 of 71 modules (+`lib.rs`); the other 42 modules + 4 test-only ones are behind `feature = "full"` (§4.6) |
| vendored code gated out of the app build | `algo` 34 376 lines (61 % of the crate's source), `psd` 6 891 (46 %); both re-enabled for their own tests by a self dev-dependency |
| cold build win measured | `cargo clean -p open-canvas-algo` + `cargo build --workspace`: 113.4 s → 54.2 s (median of 3); `algo` rlib 52.2 → 11.4 MB; release exe 59 059 200 → 59 046 912 B (§4.6) |
| vendored code still estimated gateable | `cms` ≈0.8 k lines (17 %) — gated in the follow-up pass (§4.3) |

---

## 1. Crate inventory

"Product" = the file compiles into the crate outside `cfg(test)` **and** outside a
`tests/`, `examples/`, `benches/` target. "Inline tests" are `#[cfg(test)] mod` blocks
inside product files, so `product − inline tests` is the volume that ships.

| crate | purpose | `.rs` files | `.rs` lines | `.slint` files | `.slint` lines | product lines | of which inline tests | separate test files | separate test lines | other targets (files/lines) |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `core` | canvas domain: nodes, geometry, connections, undo history, document/library envelope | 18 | 5 838 | – | – | 5 838 | 2 040 | 0 | 0 | – |
| `ai` | OpenAI-compatible HTTP client + provider/model config (image, text-SSE, video, speech) | 17 | 6 459 | – | – | 5 519 | 1 411 | 0 | 0 | 2 / 940 |
| `ps` | Image-Studio domain: PS document model, compositor, adjustments, filters, PSD mapping, `algo_bridge` | 28 | 13 836 | – | – | 13 836 | 3 054 | 0 | 0 | – |
| `psd` | vendored PSD/PSB codec, plus ABR/ACO/ASE/ATN/GRD/SHC/TIFF readers | 43 | 18 026 | – | – | 15 109 | 4 204 | 0 | 0 | 12 / 2 917 |
| `geom` | vendored rects, tiles, warp mesh geometry | 2 | 1 666 | – | – | 1 666 | 430 | 0 | 0 | – |
| `cms` | vendored pure-Rust ICC v2/v4 colour management | 18 | 5 871 | – | – | 4 780 | 492 | 0 | 0 | 4 / 1 091 |
| `color` | vendored pixel formats, colour models, blend-mode math | 3 | 1 495 | – | – | 1 495 | 557 | 0 | 0 | – |
| `raster` | vendored sparse copy-on-write tiled surfaces | 2 | 2 060 | – | – | 2 060 | 778 | 0 | 0 | – |
| `algo` | vendored CPU filter engine (Photoshop's Filter menu) | 146 | 57 655 | – | – | 54 863 | 9 675 | 7 | 1 362 | 17 / 1 430 |
| `plugins` | plugin manifests, registry, host bridge, official plugins | 30 | 5 083 | – | – | 4 159 | 409 | 0 | 0 | 5 / 924 |
| `app` | Slint UI + application state (binary) | 134 | 63 020 | 42 | 22 712 | 47 397 | 9 360 | 46 | 15 620 | 1 / 3 |
| `cli` | headless CLI (`convert`, `filter`, `inspect`, `batch`) | 9 | 1 573 | – | – | 1 320 | 84 | 0 | 0 | 1 / 253 |
| **total** | | **450** | **182 582** | **42** | **22 712** | **158 042** | **32 494** | **53** | **16 982** | **42 / 7 558** |

Reading notes:

* `ps`, `ai`, `core`, `plugins` keep their unit tests **inline**, so their "separate test
  files" column is 0 — the test mass is in "inline tests".
* `app` is the only crate with Slint: 42 `.slint` files / 22.7 k lines, and it takes
  59.5 s of the 63 s `cargo build` measured here (a rebuild with dependencies warm) —
  that time is Slint codegen, not Rust volume.
* `psd` and `cms` line counts include writers/generators that only their own tests and
  examples use (see §4).
* the `app` row is the **pre-split** inventory. After §6.1 it is 107 `.rs` files / 56 143
  lines, and the new `crates/app-core` (`open-canvas-app-core`) holds 29 files / 6 605
  lines of it (6 519 moved, the rest `lib.rs` + split headers). `app` is still the only
  crate with `build.rs` / `slint::include_modules!()`.

---

## 2. File reachability (step 1)

**Result: no unreachable file exists. Nothing deleted.**

Two independent passes, which agree exactly:

1. **Module graph.** Walked every `mod` item (`mod x;`, `#[path = "…"] mod x;`, inline
   `mod x { … }`) from each crate root (`crates/*/src/{lib,main}.rs`), resolving
   `foo.rs` → `foo/` for children and honouring `#[path]` relative to the declaring
   file's directory (the Rust rule that makes `#[path = "app_tests/x.rs"]` inside
   `app.rs` resolve to `src/app_tests/x.rs`). 408 `.rs` files were reached; the other
   42 are standalone Cargo targets (`tests/`, `examples/`, `benches/`, `build.rs`).
2. **rustc dep-info.** After `cargo check --workspace --all-targets`, the 47 freshly
   written `target/debug/deps/*.d` files list every source file rustc actually read
   (they include the `#[cfg(test)]` targets). Every `.rs` under `crates/` appeared,
   except the example binaries that had not been rebuilt — i.e. no source file is on
   disk without being compiled into some target.

Slint: all 42 files are imported transitively from `crates/app/ui/app.slint`
(`import … from "…"`); 0 orphans. Every `import` target exists.

Dynamic-reachability traps that were checked explicitly, and where they apply:

| trap | where | consequence |
|---|---|---|
| `include_bytes!` / `include_str!` | `algo/src/faces/cnn.rs` → `data/libfacedetection.bin` (104 KB); `app/src/image/text_raster.rs` + `ruler_text.rs` → `assets/fonts/Baloo2-Variable.ttf`; `cms/src/builtin.rs` → `profiles/open-canvas-coated-cmyk.icc` | assets are reachable only through the module that embeds them — gating `algo::faces` also drops the 104 KB blob |
| `include_str!` of `.slint`/`.rs` sources | ~40 sites in `app/src/**/*_tests.rs` (UI-consistency tests that grep source text) | these are why some UI files look "only used by tests"; they are real product files, keep |
| serde tag dispatch ("reflection") | `ps/src/algo_bridge.rs` builds `FilterParams` via `serde_json::from_value` from a **string tag** (`"filter": "…"`) | reachability of `algo` filters cannot be read off Rust call sites; §4 enumerates the tags instead |
| Slint codegen | `slint::include_modules!()` in `app/src/main.rs` | everything under `ui/` is consumed by build.rs, not by `mod` |

The reason this repo has no orphan files: test files are attached with
`#[cfg(test)] #[path = "…"] mod …;` (69 such files, all gated — verified). A naive
audit that only looks for `mod name;` in `main.rs` will mis-report ~40 of them as
orphans; the `#[path]` lines live next to the module they test.

---

## 3. Dead `pub` surface (step 2)

Method: indexed every identifier in the workspace, then matched each of the **4 073
`pub fn/struct/enum/trait/const/static/type` definitions** against all other files.
"Zero references" below means the identifier occurs exactly once in the whole
workspace — not even a doc-comment link or a re-export names it, so it cannot be
reached by any code path, test, or macro.

`#[allow(dead_code)]` / `#[expect(dead_code)]`: **none in the workspace**, so
`clippy -D warnings` already covers private items; only the `pub` surface needed this
pass.

### 3.1 Deleted (provable dead, 10 files, −138/+1 lines)

| crate | item | why unused |
|---|---|---|
| `core` | `Alignment::is_distribute` | zero references; `as_str`/`from_id` cover the enum |
| `ai` | `config::model_option_label` | port of `modelOptionLabel`; no caller |
| `ai` | `image::normalize_quality` (+ private `QUALITY_ALIASES`) | port of `normalizeQuality`; callers use `quality_base` |
| `ps` | `adjustments::ps_copy_params` | `PsParams` is a `BTreeMap`; callers clone inline |
| `ps` | `board::smart_canvas_fill` | returns a **CSS** `color-mix()` string for the web build; no Rust caller |
| `ps` | `board::move_ps_layer` | port of `movePsLayer`; reordering uses `layer_ops::move_ps_layer_to` instead |
| `ps` | `layer_ops::layer_centre` | port of `psLayerCentre`; no caller |
| `ps` | `selection::path_to_selection` (+ 2 imports) | port of `psPathToSelection`; `selection_polygon` is called directly where needed |
| `ps` | `text::text_font`, `text::text_small_caps_font` (+ `SMALL_CAPS_SCALE`) | build CSS font-shorthand strings; text is rasterised by `text_raster`, which composes its own font |
| `plugins` | `official::html::{EDITOR_FONT_SIZE, EDITOR_LINE_HEIGHT}` | the comment says "kept for the Slint editor", but Slint cannot reference Rust consts and no Rust code does; `ui/components/plugin-manager.slint` hard-codes its own `font-size` instead |
| `plugins` | `official::sticky_note::StickyNote::with_content` | builder method with no call sites |

All of these are stragglers of the web→Rust port (each carries a `web/src/…` provenance
comment). **This repo has no commits**, so the deleted bodies are not recoverable from
git: appendix A keeps the exact removed text.

### 3.2 Found dead, left alone (with reason)

| crate | item | reason left |
|---|---|---|
| `algo` | 13 zero-reference `pub` items (`artistic::uses_colours`, `content_aware::{polygon_mask,grow_mask,default_brush_size}`, `lib::{makes_transparency,sizes_by_canvas}`, `matting::region_surface`, `perspective::{unit_corners,plane_point}`, `render2::tree_type_names`, `segment::{grabcut::OBJECT_AT_WORK_PX,quick::select_working}`, `warp::place_source_projective`) | vendored, out of scope for this task |
| `cms` | 6 (`builtin::linear_gray`, `curve::is_monotonic`, `gamut::{DEFAULT_THRESHOLD,out_of_gamut}`, `profile::{can_be_destination,supports_intent}`) | vendored |
| `psd` | 7 (`file::unreadable_layer_block`, `layer::OBSOLETE`, `path::{CLIPBOARD,FLAG_NOT_LINKED}`, `patterns::block_key`, `slices::SLICES`, `tiff::transcode_layer_info`) | vendored |
| `geom` | 3 (`Rect::from_size`, `warp::{max_segment,control_bounds}`) | vendored |
| `raster` | 3 (`try_take_tiles`, `try_default_tile`, `is_hdr`) | vendored |
| `color` | 1 (`TRANSPARENT`) | vendored |
| `ai` | `ProviderPreset::text_only` field | only read by `is_text_only_base_url`, which is kept (next row); it is data mirroring `provider-presets.ts`. Delete with the next port-parity sweep. |
| `ai` | `config::is_text_only_base_url` (+ private `normalize_base_url`) | zero references, but `docs/GAP_ANALYSIS.md:468` names it as the implementation of "text-only base URL forces text capability". Kept so the gap report stays true; retire both together. |
| `plugins` | `manifest::PluginManifest::description_for` | zero references, but `docs/GAP_REPORT_V2.md:601` cites it as the partial i18n implementation (only `name_for` is consumed). Kept for the same reason; it is ~6 lines. |
| `algo` | 15 of 61 `FilterParams` variants are never constructed by the bridge: `GaussianBlur`, `BoxBlur`, `MotionBlur`, `RadialBlur`, `UnsharpMask`, `AddNoise`, `DustAndScratches`, `Emboss`, `FindEdges`, `Solarize`, `Pinch`, `Spherize`, `Ripple`, `Crystallize`, `LensFlare` | the enum is public and deserialised from tags; the match arms in `kernel()` must stay until the whole enum is gated (§4.1). Every one of their modules is needed anyway for a sibling variant (`blur` by `SurfaceBlur`, `sharpen` by `SmartSharpen`, …), so gating them buys only intra-module pruning. |
| `ps`, `ai`, `plugins`, `core` | ≈**372** `pub` items never referenced outside their own crate (`ps` 114, `ai` 102, `plugins` 90, `core` 66 — approximate: counts include name collisions) | not dead, just over-exported. Sinking them to `pub(crate)` is a mechanical, separately reviewable refactor (§6.4). |

### 3.3 Test-only code in the binary

None: all 69 `*_tests.rs` / `tests.rs` / `*_test_support.rs` files are declared under
`#[cfg(test)]`. They cost **`cargo test`** build time only (16 982 lines in separate
files + 32 494 lines in inline modules), which is expected.

---

## 4. Vendored crates: reachability and gateable subsets

Measured, then **gated** in a follow-up pass (§4.6): `algo` behind `feature = "full"`,
`psd` behind `feature = "extra-formats"`, both off by default. Method: module table from
each `lib.rs`, then a use-graph between modules (`a::` / `super::a::` / `use crate::a`),
seeded from what other crates actually name, transitively closed.

Entry points used by the app:

* `algo` — **only** `open_canvas_algo::{apply_in, FilterParams, GalleryEffect,
  GalleryFilter, GalleryParam, GalleryParamKind, GALLERY_CATEGORIES}`, all from
  `ps/src/algo_bridge.rs`.
* `psd` — 12 items, all flat re-exports: `BlendMode, ColorMode, GroupSpec, LayerSpec,
  MaskSpec, PixelData, PsdBuilder, PsdFile, Rect` (from `ps::psd_io`) and
  `LayerNode, LayerRecord, PsdFile` (from `cli::inspect`).
* `cms` — via `color` (profiles, transforms).
* `color` — `blend`, `convert` (from `algo`, `raster`, `ps`).
* `raster` — whole crate: `lib.rs` (`Surface`) + `interrupt.rs`.
* `geom` — `lib.rs` + `warp` (from `algo`, `raster`, `ps`).

### 4.1 `algo` (57.6 k lines, 146 files) — the big one

Call chain: **catalogue → tag → `FilterParams` → `kernel()` arm → module**.

* `ps/src/algo_bridge.rs` publishes **42 direct filters** (`filter(id, label, group,
  tag, …)`) + **47 gallery filters** (`GalleryFilter::ALL`, all sent as `filterGallery`)
  = 89 menu entries. One direct entry, `displace`, is declared `supported: false` (it
  needs a second image the document model has no slot for), so 88 are runnable.
* Those map to **43 distinct serde tags** (`"filter": "<tag>"`: the 42 catalogue tags +
  `filterGallery`) and therefore 43 of the 61 `FilterParams` variants; 42 of them are
  constructible (`displace` is rejected before `build_params`).
* `apply_in` → `apply_tiled_with_impl` → `kernel()` dispatches those 43 variants into
  **19 filter modules**, plus 7 shared/infrastructure modules (`image`, `fxutil`,
  `photo_util`, `params_ext`, `resample`, `shape_blur`, `artistic`).

Reachable (must compile as-is): 25 modules, ≈13 266 lines — plus `lib.rs` itself
(1 351 lines), which holds the dispatch. `resample` (871 lines in the table below) was in
this set in the first pass, but nothing reachable uses it: its only callers are
`transform`, `warp` and `sharpen`'s test module, which are all gated, so it moved into
the gated set.

| module | lines | role |
|---|---:|---|
| `blur` (8 files: `gaussian`, `box`, `motion*`, `radial`, `surface`, `box_widths`) | 3 560 | `surfaceBlur`, and the box/motion/radial fast paths called for **every** filter |
| `artistic_fx` | 1 443 | `filterGallery` (47 gallery effects) |
| `sharpen` (7 files: `unsharp`, `smart`, `high_pass`, `exact`, `fft`) | 1 318 | `smartSharpen`, `highPass` (+ FFT fast path for all) |
| `pixelate` (5 files) | 853 | `colorHalftone`, `facet`, `fragment`, `mezzotint`, `pointillize` |
| `stylize2` (4 files) | 834 | `diffuse`, `extrude`, `tiles`, `traceContour`, `wind` |
| `relight` / `gallery` (3) / `blur2` / `render` | 592 / 540 / 488 / 331 | `relight`; tilt-shift+iris+field+spin+path blur; `lensBlur`,`shapeBlur`,`smartBlur`; `clouds`,`fibers`,`lightingEffects` |
| `params_ext`, `photo_util`, `noise`, `other`, `fxutil`, `artistic` | 289/282/281/266/252/251 | halo/metadata tables, parallel row helpers, `median`, `minimum`/`maximum`/`offset`, CMYK polarity, gallery core |
| `color_to_alpha`, `distort`, `stylize`, `oil`, `denoise`, `image`, `other2`, `distort2`, `shape_blur`, `video` | 236/206/201/199/180/180/141/130/123/90 | the remaining per-filter implementations and the pixel buffer |

Not reachable from any product path: **42 modules, 34 376 lines (61 % of the crate's
source)**, plus the four `#[cfg(test)]`-only modules; every one of them is still referenced
by `algo`'s own unit tests, so nothing here is dead code — it is unused *surface*.

| module | lines | what it would be |
|---|---:|---|
| `segment` (15 files) | 4 715 | magic-wand / grabcut / quick-select / maxflow |
| `inpaint` (3) | 2 776 | content-aware fill |
| `selection` (4) | 2 256 | mask algebra, marquee, distance transforms |
| `camera_raw` (3) | 2 168 | RAW pipeline (local tone, white/black points) |
| `panorama` | 1 996 | stitching |
| `matting` (4) | 1 672 | brush/morphology alpha matting |
| `puppet` | 1 590 | puppet warp |
| `render2` | 1 533 | frames/flames renderer |
| `liquify` | 1 491 | liquify |
| `content_aware` | 1 129 | patch-based fill (`par::` consumer) |
| `nonlocal` | 1 015 | non-local-means denoise |
| `dff` | 990 | deep-focus / depth-of-field maps |
| `quantize` (2) | 948 | palette quantisation |
| `pyramid` | 885 | laplacian pyramid helpers |
| `seam` / `transform` / `resample` | 874 / 874 / 871 | seam carving; 4-point transform; sampler the gated geometry filters use |
| `selection_blur` | 837 | selection feathering |
| `tone` (2) / `magnetic` (2) | 804 / 797 | shadows/highlights; magnetic lasso |
| `features`, `tests_ext`, `warp`, `lens` (2), `direct`, `vanishing`, `hdr`, `stack`, `wideangle`, `redeye`, `faces` (2), `paint`, `poisson`, `retouch`, `perspective`, `remove`, `exif`, `scancrop`, `trap`, `histogram`, `tests_mosaic`, `erase`, `tests_selection_mix`, `par`, `vectorscope` | 767…72 | assorted editors and analysers |

**Gated (this pass): 42 modules, 34 376 lines** behind `[features] full = []` (off by
default), each declared `#[cfg(any(test, feature = "full"))]` in `lib.rs`; the four
modules that were already test-only (`tests`, `tests_ext`, `tests_mosaic`,
`tests_selection_mix`) keep their `#[cfg(test)]`.

1. Why `full` at all: `cfg(test)` alone covers the in-crate unit tests, but the
   integration tests (`tests/retouch_blur_exact.rs`, `tests/warp_clipped.rs`) and the 15
   examples exercise the same modules without `cfg(test)`. A self dev-dependency
   (`open-canvas-algo = { path = ".", features = ["full"] }`) enables `full` for
   `cargo test` / `cargo clippy --all-targets`, while `cargo build --workspace` — which
   does not build dev-dependencies — gets the gated subset. Nothing had to be marked
   `required-features`, so no test or example is skipped.
2. Coupling handled: `par` (content_aware, faces::cnn), `poisson` (inpaint, nonlocal,
   remove), `transform` (direct, features, panorama, perspective, puppet), `warp`
   (puppet), `segment`/`selection`/`matting` (content_aware, erase), `pyramid`/`lens`
   (dff), `tone` (hdr), `selection_blur` (selection), `resample` (transform, warp) and
   `remove`↔`nonlocal` are all inside the gate, so no gated module is half-compiled. The
   `photo_util` helpers used only by gated modules (least-squares solvers, `hash01`, the
   sampler, SplitMix64) carry the same gate, so the production build is warning-free.
3. **Not** gateable without behaviour risk, and left untouched: the 43 live tags' arms
   and the always-called fast paths (`blur::box_widths`, `blur::motion_apply::apply`,
   `blur::radial::Plan`, `sharpen::fft::apply`, `sharpen::exact::*`).
4. **Face model:** gating `faces` takes `data/libfacedetection.bin` (106 643 B) out of the
   *compiled* crate. The pre-change release exe did not contain those bytes either — the
   linker had already dead-stripped them: a 64-byte marker from the blob is absent from
   both exes, and both name only `blur`, `sharpen` and `stylize` under
   `crates/algo/src/<module>` panic paths. The measured exe delta is 12 288 B (§4.6), not
   104 KB.
5. Keep in mind: the tag set is **serde-driven**, so the check is
   "which tags does `ps::algo_bridge` emit", not "which functions are called". Re-derive
   that list from `crates/ps/src/algo_bridge.rs::build_catalogue` if the catalogue
   changes (42 `filter(…, "tag", …)` + `gallery_entries`).

### 4.2 `psd` (18 k lines)

| reachable via `ps`+`cli` (16 modules, 7 168 lines; `testgen` also appears in the closure, through `builder`'s tests) | not reachable (13 modules, 7 457 lines — of which `fixtures` 566 is already gated) |
|---|---|
| `blend builder compression descriptor error file filter_effects header image_data io layer path pixels resources tagged tree` | `tiff` 2 546 · `abr` 944 · `patterns` 776 · `fixtures` 566 · `slices` 496 · `atn` 466 · `ase` 439 · `aco` 341 · `grd` 299 · `shc` 251 · `act` 143 · `metadata` 97 · `hdr` 93 |

`fixtures` and `testgen` are already `#[cfg(any(test, feature = "testgen"))]`; the other
twelve are now gated the same way behind `[features] extra-formats = []` (off by
default): **6 891 lines / 46 %** of the crate's 15 129 source lines. `psd`'s own tests
and examples opt in through the self dev-dependency (`features = ["testgen",
"extra-formats"]`), so nothing dropped out of `cargo test --workspace`, while
`cargo build --workspace` compiles the 16 modules the codec needs. `tiff.rs` alone is
2 546 lines and only `transcode_layer_info` is referenced, from `tiff` itself. The two
`io.rs` helpers only the gated readers use (`utf16_units`, `Reader::new_little`) carry
the same gate, so the production build stays warning-free.

### 4.3 `cms` (5.9 k lines)

Reachable: `builtin clut curve math pipeline profile synth transform write` (3 873
lines). Not reachable: `lutfile` 446, `lut3d` 143, `gpu_matrix_shaper` 131, `gamut` 78 →
**798 lines (17 %)** gateable; all four are exercised only by `cms/tests/*`
(`gpu_matrix_shaper.rs` even exists as its own test target).

**Gated (follow-up pass).** Re-checked against `color/src/convert.rs` and the app before cutting
anything: `write` is product code (`Profile::content_hash` → `write::encode`, called by
`CmykSpace::build`), and `synth` is reached from `color`'s and `cms`'s tests (`regen.rs`), so
both stay ungated — they are the "profile writing / synthetic CMYK" helpers in name only.
That leaves the four modules above, now declared
`#[cfg(any(test, feature = "extra-profiles"))]` in `lib.rs` (`extra-profiles` off by default,
self dev-dependency for this crate's tests, same pattern as `algo`/`psd`). One coupling had to
move: `transform.rs` built its table sizes from `GpuMatrixShaper::INPUT_INTERVALS` /
`OUTPUT_ENTRIES`, so the two constants moved into `transform.rs` (`INPUT_INTERVALS`,
`OUTPUT_ENTRIES`, both 4 096 — same values) and the gated struct re-publishes them; the
`Transform::gpu_matrix_shaper` method carries the same gate. Default build: 10 of 14 source
files (`target/debug/deps/open_canvas_cms-*.d` has no `lut*`/`gamut`/`gpu_matrix_shaper`
entry), feature build: 14.

Method: `cargo clean -p open-canvas-cms` then `cargo build -p open-canvas-cms`, median of 3,
`CARGO_TARGET_DIR=target/gate-bench` (deleted afterwards):

| measurement | before | after | delta |
|---|---:|---:|---:|
| `cms` cold crate build (median of 3) | 5 141 ms | 4 230 ms | **−911 ms (−18 %)** |
| runs | 5 223 / 5 141 / 5 105 | 4 162 / 4 230 / 4 482 | |

The before/after runs were separate bursts and the machine drifted between them (a first
"after" burst measured 5 516 / 5 434 / 5 446, i.e. *slower*); the trustworthy figure is an
interleaved A/B on the final tree, 3 rounds alternating the two feature sets:
**full (`--features extra-profiles`) 5 037 ms vs gated 4 230 ms** (runs 5 037/5 062/4 983 vs
4 162/4 230/4 482) → −807 ms, −16 %. The full number agrees with the pre-change tree (5 141),
so the row keeps the pre-change measurement as "before".

### 4.4 `color` (1.5 k lines)

Three files: `lib.rs`, `blend.rs`, `convert.rs`; `blend` and `convert` are both used by
`algo`/`raster`/`ps`. Nothing gateable (the third "module" a naive scan reports is the
inline `#[cfg(test)] mod tests`, 557 lines).

### 4.5 `raster` (2 k lines)

`lib.rs` (`Surface`, `Tile`, format conversion) + `interrupt.rs` (`Interrupt`,
`Cancelled`). Both used by `algo` and `ps`; nothing gateable. 778 lines of it are inline
tests. Note the whole crate is one 1 952-line file — the natural split (tile / surface /
conversion / tests) is a readability change, not a build-time one.

### 4.6 What the gate buys, measured

Both crates now have a non-default feature (`algo`: `full`, `psd`: `extra-formats`) and
declare the unreachable modules `#[cfg(any(test, feature = "…"))]` in `lib.rs`. The gate
is `any(test, …)` so each crate's unit tests keep their subjects; a self dev-dependency
(`open-canvas-algo = { path = ".", features = ["full"] }`, likewise for `psd`) turns the
feature on for `cargo test` and `--all-targets`, so the integration tests and examples run
unchanged, while `cargo build --workspace` — which never builds dev-dependencies —
compiles the subset. Nothing was marked `required-features`, so no test or example is
skipped, and the gated build is warning-free (`cargo check -p open-canvas-algo
-p open-canvas-psd`), which is why no `#[allow(dead_code)]` was needed.

| crate | feature | modules gated | lines gated | shipped by the app build |
|---|---|---:|---:|---|
| `algo` | `full` | 42 (+4 that were already test-only) | 34 376 of 56 296 (61 %) | 25 modules + `lib.rs` |
| `psd` | `extra-formats` | 12 (+2 that were already `testgen`) | 6 891 of 15 129 (46 %) | 16 modules |
| `cms` | `extra-profiles` | 4 | 798 of 4 780 (17 %) | 10 of 14 source files |

Method: `cargo clean -p <crate>` then `cargo build --workspace`, 3 runs each, median, with
`CARGO_TARGET_DIR=target/gate-bench` so the normal `target/` stayed untouched. Wall clock,
warm dependency graph, same machine, runs interleaved only with each other:

| measurement | before | after | delta |
|---|---:|---:|---:|
| `algo`: clean + `cargo build --workspace` (median of 3) | 113 430 ms | 54 235 ms | **−59 195 ms (−52 %)** |
| `algo` runs | 108 514 / 115 191 / 113 430 | 53 680 / 54 235 / 55 546 | |
| `psd`: clean + `cargo build --workspace` (median of 3) | 34 264 ms | 38 385 ms | +4 121 ms (noise — see below) |
| `psd` runs | 34 231 / 34 264 / 35 687 | 38 385 / 37 223 / 40 167 | |
| release `open-canvas-app.exe` | 59 059 200 B | 59 046 912 B | **−12 288 B** |
| release section split | `.text` 40 476 160 · `.rdata` 16 103 936 · `.pdata` 2 321 920 | `.text` 40 470 016 · `.rdata` 16 097 792 · `.pdata` 2 321 408 | `.text` −6 144 · `.rdata` −6 144 · `.pdata` −512 |
| `algo` compiled artifact (`lib*.rlib`, dev) | 52 216 022 B | 11 390 072 B | **−78 %**, face model present → absent |
| `psd` cold crate build (`clean -p` + `build -p open-canvas-psd`, median of 3) | 4 255 ms | 2 751 ms | **−1 504 ms (−35 %)** |

Why `algo` halves the build: it is the largest of the vendored crates the dev profile pins
to `opt-level = 2` (56 k source lines; `cms` 5.9 k, `raster` 2 k, `color` 1.5 k are the
others), so it was the most expensive per-line compile in the tree and 61 % of it is now
gone. The rest of the `--workspace` build (psd → ps → app/Slint) is unchanged, so the delta
is `algo`'s own compile.

The `psd` figure is inside the noise floor and is reported as such: `psd` itself is ~2 s of
a ~35 s rebuild whose tail is `app`'s Slint codegen. The ranges overlap (34.2–35.7 s
before, 37.2–40.2 s after; the after runs went second, with the machine busier), and 46 %
fewer lines in a 2 s step cannot be resolved through that. On its own the crate does
compile faster — `cargo clean -p open-canvas-psd` + `cargo build -p open-canvas-psd`,
median of 3: **4 255 ms → 2 751 ms** (−1 504 ms, −35 %) — that ~1.5 s is simply invisible
under the app's codegen. Treat `psd` as "no measurable change at workspace level, 46 %
fewer lines compiled".

The 12 KB exe delta is smaller than the 106 643 B asset by design of the linker, not of the
gate: the pre-change release exe never contained the blob's bytes or the gated modules'
panic paths (see §4.1 item 4), so gating removes the *compilation*, not linked bytes. Of
the 12 288 B that did go, the section table shows `.text` −6 144, `.rdata` −6 144 and
`.pdata` −512 (≈42 unwind records) — code the linker had kept despite no app call site; the
last 512 B of the total is header/file-alignment padding. The exact symbols were not chased
down, and part of it is build-to-build variance (two builds of the identical pre-change
tree, `target-rel2` vs the bench dir, differ by 512 B).

The pruning is visible in the compiled artifact instead: the gated
`libopen_canvas_algo-*.rlib` from `cargo build --workspace` is **11 390 072 B** and does not
contain a 64-byte marker taken from `data/libfacedetection.bin`; the `full` rlib from
`cargo test` is **52 216 022 B** and does. That ~41 MB of object code for modules no binary
links is what a cold build was paying to produce.

For comparison, the first pass's isolated `cargo check -p <crate>` cold-crate figures
(warm deps, whole crate rechecked) were:

| crate | lines | check time |
|---|---:|---:|
| `app` (Rust + Slint codegen) | 63 k + 22.7 k slint | **106 s** (59.5 s of the full `cargo build`) |
| `ps` | 13.8 k | 12.0 s |
| `algo` | 57.6 k | 9.9 s |
| `psd` | 18 k | 1.9 s |

Conclusion for prioritisation, unchanged: the vendored gate is worth the ~1 min it gives
on a cold workspace build, but `app` (Slint's generated module, one unit, single-threaded
codegen) remains an order of magnitude larger and is the only place left where a build-time
change moves the number by minutes (§6.1).

---

## 5. Duplicated helpers worth merging

Detected by normalising function bodies (whitespace/comments stripped, then
identifier+literal-blind for the near-duplicates) across `crates/app/src`.

| # | duplicate | copies | files |
|---|---|---:|---|
| 1 | `fn init_backend()` — installs the Slint testing platform with the software renderer | **15** (one already shared) | `config_test_support.rs` (the canonical one), `render_tests.rs`, `library_export_tests.rs`, `home_bar_tests.rs`, `library_layout_tests.rs`, `node_card_tests.rs`, `widget_tests.rs`, `app_tests/{frame_budget,no_dim,top_bar_tab,wire_label}_tests.rs`, `image/{ai_render,dock_render,render_layout,studio_tool,tools_render}_tests.rs` |
| 2 | `fn differing_pixels(a, b)` — pixel-diff count for render snapshots | 7 | `render_tests.rs`, `library_export_tests.rs`, `app_tests/glass_chrome_tests.rs` (as `differing`), `image/{ai_render,dock_render,render_layout,tools_render}_tests.rs` |
| 3 | `fn render_editor(...)` — open the editor on a snapshot | 2 | `image/ai_render_tests.rs`, `image/tools_render_tests.rs` |
| 4 | `fn app_with_board() -> (App, TempDir)` | 2 | `image/editor.rs`, `image/filter.rs` (both in inline `mod tests`) |
| 5 | `fn composite(&App)` / `board_pixels` — composite a board to RGBA bytes | 3 | `image/editor.rs`, `image/masks.rs`, `image/text.rs` |
| 6 | `fn hwnd_of(window) -> Option<isize>` — **product** code, not a test helper | 2 | `app/src/file_drop.rs` (pub) and `app/src/main.rs` (private) — `main.rs` should call `file_drop::hwnd_of` |

1 and 2 are worth doing: they already differ (there are two variants of `init_backend`,
279 vs 318 chars) so the copies have started to drift, and `config_test_support.rs`
already exists as the home for both.

**Done (follow-up pass).** `crates/app/src/test_support.rs` (declared `#[cfg(test)]`) now
holds the canonical helpers; the local copies are gone:

| helper | before | after |
|---|---:|---:|
| `init_backend` | 16 definitions (15 copies + the one in `config_test_support.rs`) | 1 `init_backend` (Result) + 1 `init_backend_allow_existing` (tolerates an existing per-thread platform) |
| `differing_pixels` | 10 (7 `Result`, 2 `usize`, 1 slice/alpha variant) | 1 `Result` canonical + 1 local specialised `differing_rgba` (compares alpha, takes slices) in `app_tests/no_dim_tests.rs` |
| `render_editor` | 4 (2 fresh-temp-dir copies, 1 shared-temp-root, 1 explicit-store) | 1 `render_editor` + 1 `render_editor_with_store`, plus a 3-line `render_editor_shared` wrapper in `render_tests.rs` (shared-temp-root store: a different fixture, kept local) |
| `app_with_board` | 8 copies of the dir/store/`App`/`enter_image` setup | 1 `app_with_image(project, bitmaps)`; the 8 fixtures stay local as `*_board_app()` wrappers (different board sizes/pixels) |
| `composite` / `board_pixels` | 3 identical bodies | 1 `composite` |

`config_test_support::init_backend` is now a re-export of the canonical one, so the settings
tests are unchanged. No assertion moved: the only call-site edits are `?` where a `usize`
diff became the `Result` variant (size check added, sizes equal by construction) and the
renames above.

---

## 6. Prioritised refactors for the next pass

Ordered by (build-time or clarity win) ÷ risk. Sizes: XS < 30 min, S < 2 h, M < 1 day,
L multi-day.

| # | item | size | win | risk / notes |
|---|---|---|---|---|
| 1 | **Split `app` into `app-core` (state, store, doc, service, i18n) + `app` (Slint wiring)** | L | **done (§6.1), partly:** 23 modules / 6 519 lines moved into `open-canvas-app-core` (Slint-free, compiles in parallel, no `build.rs`). Clean build 706 → 689 s (−2.4 %), app rebuild after a UI edit 33 → 33 s (unchanged), but the domain loop is now 1–3 s (`check`/`build`/`test -p open-canvas-app-core`) instead of 21–29 s through the Slint crate | the remaining 56 k lines are Slint-entangled by signature (`&AppWindow`, generated rows); `store`, `media`, `i18n`, `ruler_text` and the whole `image/*` tree stayed — see §6.1 for each reason and the next lever |
| 2 | **Gate the vendored `algo` surface** (`feature = "full"`, off by default, `#[cfg(any(test, feature = "full"))]` on 42 modules) | M | **done (§4.6):** 34 376 lines (61 %) stop compiling; cold `cargo clean -p open-canvas-algo` + `cargo build --workspace` 113.4 → 54.2 s; exe −12 288 B; "what the app can do" is legible from `lib.rs` | self dev-dependency keeps tests/examples on the full engine, so no CI change is needed |
| 3 | **Collapse the 15 `init_backend` / 7 `differing_pixels` copies into `config_test_support.rs`** | S | **done (§5):** 16 → 2 backend definitions, 10 → 2 diff helpers, 8 → 1 board app, 3 → 1 composite; ~250 lines deleted | purely test code; no product behaviour; the two existing `init_backend` variants are reconciled as `init_backend` + `init_backend_allow_existing` |
| 4 | **Sink over-exported `pub` to `pub(crate)`** in `ps` (114), `ai` (102), `plugins` (90), `core` (66) | M | smaller documented API surface; makes future dead-code detection trustworthy (`clippy` cannot flag `pub` items) | mechanical but wide; one crate per commit; count is approximate (name collisions), so re-derive per item |
| 5 | **Gate `psd`'s unused readers** (`tiff`, `abr`, `aco`, `ase`, `atn`, `grd`, `shc`, `act`, `slices`, `patterns`, `metadata`, `hdr`) behind `extra-formats`, mirroring the existing `testgen` feature | S | **done (§4.6):** 6 891 lines (46 %) stop compiling; no measurable change at workspace level (inside the Slint-codegen noise) | tests/examples opt in through the self dev-dependency; do not gate `tiff` if TIFF import is ever wired to the UI — check with the product owner |
| 6 | **Gate `cms`'s unused modules** (`lutfile`, `lut3d`, `gpu_matrix_shaper`, `gamut`) | XS | **done (§4.3):** 798 lines (17 %) stop compiling; cold crate build 5 141 → 4 230 ms (−807 ms, −16 %, interleaved A/B) | tests only; `write`/`synth` verified reachable and left alone |
| 7 | **Delete the 33 zero-reference `pub` items inside the vendored crates** (§3.2) | XS | tidiness; no build win | must be done at the next vendoring sync, or it will be re-broken by the next drop |
| 8 | **Merge `hwnd_of`** (`file_drop.rs` vs `main.rs`) | XS | removes a real product duplicate (window-handle extraction is easy to fix twice) | 5-line change, `#[cfg(windows)]` on both sides |
| 9 | **Retire `ProviderPreset::text_only` together with `is_text_only_base_url`** (§3.2) | XS | one less field mirroring a TS concept the app does not use | 2 preset literals; do it with the gap-report row |
| 10 | **Split `raster/src/lib.rs`** (1 952 lines, 40 % tests) into `tile`/`surface`/`convert` | S | readability only | no build win — listed so it is not mistaken for one |

Do **not** spend time on: further file-level splitting inside `app/src/image/*` (four
files over 1 500 lines) — the crate is one codegen unit, so it buys review comfort at
best; and on `serde`-derive trimming — derive cost is spread and small compared with
Slint.

### 6.1 `app` → `app-core`: the split, as done

**Mechanism (low churn).** `crates/app-core` (`open-canvas-app-core`) holds the part of
the app that compiles with `open-canvas-core` / `open-canvas-ai` alone; `crates/app`
keeps every module that names a Slint type (`AppWindow`, the generated row structs,
`slint::Image`, the `I18n`/`Theme` globals, `slint::Timer`, `slint::Window`). The moved
files kept their names, their internal `crate::…` paths and their inline tests, because
`crates/app` re-exports the modules at its crate root:

```rust
pub use open_canvas_app_core::{
    asset_folder, clipboard, connect, coords, cost, doc, doc_edit, doc_project, drop_bindings,
    drop_files, gesture, glass, inline_edit, input, local_models, log_view, nav, overlay, panels,
    paths, storage_scan, system_clipboard, wires,
};
```

So no `crate::doc::…` / `crate::coords::…` call site in the 56 k lines that stayed had to
change, and the moved files needed no `use` rewrite either. `crates/app` is still the only
crate with `build.rs` / `slint::include_modules!()`.

Three things had to move with the files, all mechanical:

* `Doc`'s fields and `Doc::assemble`, `doc_edit::free_output_spot`, `overlay::dashed_polyline`
  were `pub(crate)`: across a crate boundary that is `pub` (this crate is an implementation
  detail of the app binary, not a published API).
* `library::EditorMeta` (the five strings the save shape needs) now lives next to
  `Doc::to_project` as `doc_project::EditorMeta` and is re-exported by `library`, so
  `crate::library::EditorMeta` still resolves.
* `Doc::demo()` and `Doc::connections_mut()` are test fixtures. A library dependency never
  sees `cfg(test)`, so they are gated `#[cfg(any(test, feature = "test-fixtures"))]` and
  `crates/app`'s **dev-dependency** enables `test-fixtures`; no non-test build does.

Two test modules could not travel: `cost_source_tests.rs` (asserts on the Slint rows
`config_tabs::cost_rows` produces) is now declared from `main.rs`, since its old parent
module `cost.rs` moved; the `app_tests/*` suite (drives `AppWindow`) keeps its
declarations in `app.rs`. `doc_tests.rs` did travel -- its only non-domain dependency was
`EditorMeta`.

**What moved** (lines, as they left `crates/app`):

| module | lines | why it is UI-independent |
|---|---:|---|
| `drop_bindings` | 893 | drop-target resolution + node geometry; `open-canvas-core` only |
| `glass` | 417 | Win32 backdrop / rounding / timer resolution; its own `mod win`, zero Slint types |
| `wires` | 423 | wire hit test, cut stroke, relation labels |
| `connect` | 381 | connection drag + drop-target legality |
| `doc` | 225 + 237 (`doc_tests`) | document state, selection, undo (`open-canvas-core`) |
| `doc_edit` | 364 | demo scene, node creation/deletion |
| `doc_project` | 48 + 8 (`EditorMeta`) | `CanvasProject` ⇄ `Doc` conversion |
| `asset_folder` | 349 | bound-folder scan + path-escape guard |
| `clipboard` | 320 | node copy/paste envelopes |
| `cost` | 304 + 131 (`cost_tests`) | generation-cost records (`open-canvas-ai`) |
| `overlay` | 272 | marquee/connection dash geometry |
| `inline_edit` | 261 | title/body draft state machine |
| `gesture` | 229 | resize geometry, drag/resize gestures |
| `paths` | 223 | dot grid / wire / guide path builders |
| `storage_scan` | 174 + 115 (`storage_scan_tests`) | data-directory size inspector |
| `nav` | 166 | wheel zoom, pan, marquee |
| `panels` | 140 | ruler tick geometry |
| `coords` | 121 | `WorldPos`/`ScreenPos`/`Viewport` |
| `log_view` | 116 + 152 (`log_view_tests`) | runtime-log filter/projection rows |
| `drop_files` | 122 + 87 (tests) | extension classification, drop-path dedupe, client→world map — **split out of `file_drop.rs`**, which keeps the Win32 drop target, the queue and the `App` hook-up |
| `local_models` | 92 + 63 (`local_models_tests`) | local model descriptors + folder scan |
| `input` | 66 | pointer/key/modifier payloads |
| `system_clipboard` | 20 | `arboard` bridge |
| **total** | **6 519** (of which 785 are test files) | |

`crates/app` went from 63 020 to 56 143 `.rs` lines (6 877 fewer); `crates/app-core` is
6 605 lines (the 6 519 moved + a 55-line `lib.rs` + ~30 lines of split headers). Dependencies of the new crate are exactly the ones the
moved modules already used: `open-canvas-core`, `open-canvas-ai`, `serde`, `serde_json`,
`chrono`, `arboard` (+ `tempfile` for tests). Nothing new in the graph.

**What stayed, and why** (each checked, not assumed):

| module | lines | blocker |
|---|---:|---|
| `ai_service` | 2 189 | `AppWindow` + `App` + `image` (pushes results into the studio) |
| `image/*` | 27 863 | every handler takes `&mut AppWindow`; `render.rs` builds generated rows |
| `library*` (`library`, `_export`, `_ops`, `_wire`) | 2 971 | `AppWindow` + `impl App` |
| `store` | 738 | pure persistence **except** `load_image`/`save_image` (`image::ImageFile`) and `load_dock_file` (`image::dock_engine::DockFile`) — see the next lever |
| `media` | 344 | the LRU it exists for stores `slint::Image`; the decode half alone is not worth a split |
| `i18n` | 460 | owns the Slint `I18n` global |
| `ruler_text` | 308 | rasterizes glyphs into a `slint::Image` |
| `vm` | 668 | builds the generated row structs |
| `plugin_host`, `plugin_*` | 1 426 | `plugin_host` caches `slint::Image` for plugin SVG |
| `file_drop` (rest) | 423 | the Win32 drop target takes `&AppWindow`; the pure half moved to `drop_files` |
| `app`, `events`, `handlers`, `sync/*`, `save`, `shortcuts` | 2 303 | `Rc<RefCell<App>>` + `AppWindow` throughout |
| `*_tests.rs` / `app_tests/*` / `plugin_tests/*` | ~5 000 | the harness builds an `AppWindow` and asserts rendered pixels |

**Build matrix** (same machine, `.cargo/config.toml` `jobs = 4`, `debug = 0`,
`incremental = true`, `target/` cold each time):

| metric | before | after | delta |
|---|---:|---:|---|
| `cargo clean` + `cargo build --workspace` | 706 s | 689 s | **−17 s (−2.4 %)** |
| `cargo build -p open-canvas-app` after `touch ui/app.slint` | 33 s | 32.2 s | −1 s (−2 %) |
| `cargo build -p open-canvas-app` after `touch src/main.rs` | 29 s | 30.1 s | +1 s (+4 %) |
| `cargo check -p open-canvas-app`, cold (`cargo clean -p`, deps warm) | 143 s | 144 s | +1 s (+0.7 %) |
| `cargo check -p open-canvas-app`, warm | 21 s | 20 s | −1 s (−5 %) |
| `touch app-core/src/doc.rs` + `cargo build -p open-canvas-app` (new) | — (was the 29 s row above) | 28.8 s | 0 |
| `touch app-core/src/doc.rs` + `cargo build -p open-canvas-app-core` (new) | — | **1.3–2.5 s** | **~12× faster domain loop** |
| `touch app-core/src/doc.rs` + `cargo check -p open-canvas-app-core` (new) | — | **0.8–3.4 s** | — |
| `cargo test -p open-canvas-app-core` (95 tests, no GUI link) | — | **0.03 s** | — |

**Verdict.** The split does **not** make `cargo build` faster: the app crate's rebuild is
one rustc invocation over the 392 k-line Slint-generated module plus the ~40 k lines of
Slint wiring, and 6.6 k lines of domain code were never the cost. What it buys is a real
**inner loop for domain work** (`check`/`build`/`test app-core` in 1–3 s instead of 21–29 s
through the Slint crate, with no `build.rs` re-run), a **unit-test target that does not
link the GUI** (95 tests in 0.03 s), and a crate boundary the compiler now enforces
(`app-core` cannot grow a Slint dependency without the build failing). It also costs
nothing at runtime: `app-core` is a plain library, `cargo tree` gains one path dependency.

**Where the cold build actually goes** (`cargo build --timings`, same run): 455
units, and the last one is `open-canvas-app` at **142.5 s** of the 689 s wall — it is the
critical path. `open-canvas-app-core` compiles in **2.7 s**, in parallel with the app's
build script, so it is free. The other headline units are Slint's own compiler crate
(`i-slint-compiler`, 159.6 s), and the opt-level-2 dependencies (`algo` 80.1 s,
`i-slint-core` 54.0 s, `image` 48.1 s twice, `windows` 47.9 s). That is why moving 6.6 k
lines out of `app` moved the clean build by 2 % and no more: the Rust that moved was
never on the critical path, the 392 k-line generated module is.

**Gates on the split tree** (all from the same tree, `cargo build --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`):
build pass; clippy pass, 0 warnings (152 s); tests **2 284 passed / 0 failed / 26 ignored**
(550 s) — the same total as before the split, with 95 of them now the `app-core` unit
tests (0.03 s, no GUI link) and 551 left in the app binary. No `.slint` file was edited
and no test was added or removed.

**Profile tuning (step 7): one change tried, rejected, none kept.**

* `debug = 0` and `incremental = true` were already right; `[profile.dev.package."*"]`
  keeps dependencies out of incremental state. No change.
* `[profile.dev.package.open-canvas-app] codegen-units = 16` was measured and **rejected**:
  app rebuild after a `main.rs` touch went 29 s → 66–97 s and after an `app.slint` touch
  33 s → 50–61 s. The generated module wants the dev default (256 units) to feed rustc's
  codegen threads. The number is recorded in `Cargo.toml` so nobody re-tries it.
* The release profile was not touched; `target-rel2/` and `target/release/` were not
  rebuilt.

**Remaining big lever.** `crates/app/src/image/*` — 27 863 lines, 44 % of the crate and
the majority of what is left — is entangled only through its handler signatures
(`&mut AppWindow`, plus `image::render` building generated row structs). Extracting a
view-model boundary there (studio state + operations in `app-core`, one render module
left in `app`) is the next order-of-magnitude move; it is a multi-day refactor of ~2 000
call sites and should be done module by module (`channels`, `masks`, `transform`,
`history` first). Second, smaller: `store.rs` (738 lines of tested persistence) can move
once `ImageFile` + `DockFile` and their `ImageAssets` / `DockLayout` envelopes are lifted
out of `image/mod.rs` and `image/dock_engine.rs` into plain-data modules — that is the
only thing keeping `store` on the UI side, and it would also make the persistence path
testable without the GUI.


---

## 7. Method and reproducibility

* Module graph and file inventory: script over `crates/**/*.rs` (parse `mod`, `#[path]`,
  inline blocks; resolve `foo.rs` → `foo/`, `#[path]` → the declaring file's directory),
  cross-checked against `target/debug/deps/*.d` produced by
  `cargo check --workspace --all-targets`.
* Slint graph: `import { … } from "…"` closure from `ui/app.slint`.
* Dead `pub`: identifier index over all 450 files; a name occurring once in the
  workspace is unreachable (doc-comment mentions count as occurrences, so the test is
  conservative).
* Vendored reachability: per-crate module table + intra-crate use graph, seeded from
  the items other crates name (`use open_canvas_x::…`, `open_canvas_x::a::b`), transitively
  closed. Over-approximates (counts comments/test code), so the gateable lists are lower
  bounds.
* Duplication: brace-matched function bodies, min length 220 chars (100 for the
  product-only sweep), compared then re-checked with identifiers/literals normalised.
* Timings: `cargo build --workspace --timings` for the full build;
  `cargo check -p <crate>` immediately after touching that crate's root for per-crate
  figures (warm dependencies, cold crate — the shape of the edit/verify loop).
* §6.1 matrix: cold column = `cargo clean` + `cargo build --workspace [--timings]`, one
  run per side (±5 s run-to-run); the touch rows are the median of 3 consecutive
  `cargo build -p open-canvas-app` runs after `touch <file>`, dependencies warm; the
  `app-core` rows are the median of 2. Everything at `.cargo/config.toml`'s `jobs = 4`,
  nothing else running.

### What this pass did not do

* Except for the §4.6 gate, did not touch any vendored crate: `algo` and `psd` only
  gained the non-default `full` / `extra-formats` features and the matching `cfg`
  attributes; `color`, `raster` and `geom` are untouched. (The follow-up pass added the
  same gate to `cms` — §4.3 — and collapsed the duplicated `app` test helpers — §5; still
  no behaviour, UI, profile or dependency change.)
* Did not change behaviour, UI, build profiles, or add dependencies.
* Did not pursue the `app` split — the largest lever, but out of scope for a deletion
  pass. (A later pass did it, partly: §6.1.)

---

## 8. Runtime hot spots (measured)

Date: 2026-10-24 · Same tree as §1–§7, plus the changes below. Scope: the interactive
paths a canvas pan, a node drag and a studio slider tick actually execute.

Method: `crates/app/src/perf_tests.rs` (new, `#[cfg(test)]`, no external crate) times the
real functions with `std::time::Instant` on realistic fixtures — 200 nodes / 400
connections (text, prompt, image, image-stack, video-prompt, generation, assets and
plugin cards, wired as prompt/image/generation triples) and a 1024x768 board with four
painted layers. Every figure is the **median of 3 samples** of one batch (60 pans, 200-row
build, 20 strokes, …), taken with `--test-threads=1`, on the `dev` profile — where every
workspace crate except the vendored imaging crates is `opt-level = 0`, so absolute numbers
are several times the release ones; only the ratios matter here. Run them with:

```
cargo test -p open-canvas-app --bin open-canvas-app -- --nocapture --test-threads=1 perf_tests::
```

| hot spot | entry point | before | after | change |
|---|---|---:|---:|---|
| studio sync while panning (1024x768, 4 layers), 60 frames | `App::sync_image` | 60 board composites = **280 s** (derived: 60 × the 4.67 s measured single composite) | **1.12 s** | −99.6 % |
| unchanged studio sync, 60 frames | `ImageState::cached_display` | 4 559 ms/frame | **0.006 ms/frame** (0.38–0.41 ms per batch) | −100 % (work skipped) |
| one board composite, unchanged (reference path) | `render::display_of` | 4 559 ms | 4 670 ms | none — not optimised |
| canvas pan frame, 60 pans (200 nodes / 400 wires) | `App::sync_view` | 103.9 ms | **28.8–30.0 ms** | −72 % |
| wire labels for one pan (400 wires) | `vm::wire_label_rows` | 101.2 ms / 60 | **27.3–32.0 ms / 60** | −70 % |
| wire path string rebuild (400 wires) | `paths::wires_commands` | 54.5 ms / 60 | **29.5–32.4 ms / 60** | −42 % |
| full node-row rebuild (200 nodes, 400 wires) | `App::rows_for_nodes(None)` | 8.46–9.13 ms | **6.0–7.9 ms** | −15…−34 % (noisy, consistently lower) |
| one-node row refresh (200 nodes on screen) | `App::rows_for_nodes(Some(..))` | 0.35 ms | 0.32–0.34 ms | −9 % (noise) |
| layer-panel thumbnails, 60 syncs | `ImageState::thumb_cache` | not separable: the studio batch moved 1.17 s → 1.12 s, inside its run-to-run spread (4 layers x 28 px only) | ~0 per repeat | folded into the studio row |
| 20 brush strokes incl. history (1024x768) | `ImageState::begin_edit` + `commit` | 20.4–39.9 ms | unchanged | none — not optimised |
| 10 filter-preview slider ticks (1024x768 layer) | `image_filter_param_core` | 9 660–9 845 ms | unchanged | none — not optimised (see §8.6) |

### 8.1 The studio composite dominated everything (fixed)

`App::sync_image` ran `image_display()` on **every** studio event: every pointer move, every
zoom step, every panel edit. `image_display` cloned the whole `ImageProject`, composited
1024x768 from all four layers through `open_canvas_ps::render_document_with_text`, applied
the channel view and copied the result into a Slint image. A pan therefore paid a full
board composite per frame.

Two changes, neither visible:

1. **`ps::Surface` gained a content stamp** (`crates/ps/src/surface.rs`): a `u64` handed out
   by a process-wide counter at creation and replaced by `set`, `fill` and `data_mut` (the
   only three ways to write the private pixel buffer; `PartialEq`/`Eq` still compare pixels
   only, and a clone keeps its source's stamp). It answers "are these still the same
   pixels?" in O(1) instead of hashing 12 MB.
2. **`ImageState::display_cache`** (`image/render.rs`) holds the last composite plus its
   `DisplayKey` — the project the composite was built from (mask view already resolved
   through `mask_view_project`), a digest of the bitmap store keyed by those stamps, and
   the channel view. An equal key returns the cached surface and `sync_image` skips both
   the composite and the 3 MB `set_image_canvas_image` copy. `image_display` became the
   free `render::display_of` (the uncached reference the cache is tested against).

The cache is exact, not heuristic: any pixel write, project edit, bitmap insert/remove,
mask-view change or channel switch changes the key. `cached_display_matches_the_uncached_composite`
asserts the cached surface equals `display_of` and that one written pixel invalidates it.

### 8.2 Wire labels: quadratic → linear (fixed)

`sync_wire_labels` rebuilt every label on every pan (labels are screen-space, so a pan must
move them). Per wire it did two `nodes.iter().find` scans and called
`wires::reference_index`, which calls `mention_references` — a graph walk — so the pass was
quadratic in connections and re-formatted *every* label string. It also built the bezier
path (`connection_geometry`) only to read the two endpoints for the midpoint, discarding a
`format!`ed path per wire.

Now (`vm::wire_label_rows`): one `id -> node` map per pass (first occurrence wins, exactly
like the `find` it replaces), reference numbers computed once per prompt target and cached,
and the midpoint computed from the two endpoint positions — no path string. Output is
byte-identical: the relation branch, the "first occurrence" reference rule and the
`Reference N` fallback to `relation_label` are unchanged.

### 8.3 Wire paths (fixed)

`paths::wires_commands` scanned the node list twice per connection (quadratic on a busy
board). Same one-map fix; first-occurrence semantics preserved.

### 8.4 Node rows (fixed)

`vm::node_row` is called once per visible node per refresh. Three costs were pure waste:

* `plugin_host::upstream_nodes` cloned **every upstream node** (metadata, text lists, image
  refs) before `plugin_cell`, which returns immediately for a built-in node and never looks
  at them. It is now computed only for plugin cards.
* `can_generate`, the reference-image chips and the video-slot titles each scanned the node
  list per row; they now use `vm::NodeIndex`, one `id -> node` map per refresh (`RowContext`
  carries it, the four construction sites build it).
* `App::rows_for_nodes` cloned **every** row id into a `Vec<(String, f32)>` and then did a
  linear search per node just to preserve the enter-animation value. It now keeps only the
  ids it is about to rebuild in a map (`None` = full rebuild, unchanged).

### 8.5 Panel thumbnails (fixed)

`sync_image_rows` resampled every full-size layer bitmap (and every mask) down to 28 px on
every sync — a 1024x768 → 28 px bilinear pass per layer per pointer move.
`ImageState::thumb_cache` keys the derived `slint::Image` by storage key, size and the
surface stamp, so a repeat is a refcount clone. On a four-layer board the win is small
(a few ms) and hides inside the studio batch's noise; it is kept because it scales with
layer count and board size, and the stamp that makes it exact is the same one the
composite cache uses.

### 8.6 Left unoptimised, with the reason

| item | measured | why left |
|---|---:|---|
| the studio's own view-model push (rest of `sync_image` after the cache) | 18.6 ms/frame in dev (≈2 ms estimated in release, ~10x) | It is ~20 `VecModel`/`Scalar` pushes (`sync_image_rows`, `_fields`, `_extra`, `_dock`) rebuilt every sync. Skipping the unchanged ones needs a revision per panel (assets, gradients, brushes, paths, channels, editor, filter), i.e. a state-ownership refactor, not a local change. Nothing here is pixel work; the composite (the part that was 4.7 s) is gone. |
| filter-dialog preview | 985 ms per slider tick in dev (~80 ms release) | `refresh_preview` runs the chosen filter on the **full-resolution** layer and only then downscales to the 320 px preview. Downscaling first would be ~10x cheaper but changes the previewed pixels (blur radius, noise seeds, halos) — a visual change. A debounce would move the preview *after* the tick, which changes when the dialog updates and would break the synchronous `*_core` tests. Left as is. |
| brush-stroke history | 1.0–1.7 ms per stroke (1024x768) | `begin_edit` clones the pre-edit bitmap once per key (not per event; `before` keeps the first) and `commit` clones the project. That is a 3 MB memcpy per stroke; making it free needs copy-on-write `ps::Surface` tiles, a much larger change to the compositor's contract. Not a hot spot at this cost. |
| pan/marquee/node-drag allocations in `handlers.rs` | rolled into the 29.5 ms/60-pan and 0.32 ms/node-row figures | Each move allocates one `Vec<String>` of dragged ids and one `Vec<String>` for the refresh; both are O(selected), not O(document). The measured cost is below the noise floor of the row rebuild it feeds. |

### 8.7 Gates for this pass

| gate | command | result |
|---|---|---|
| build | `cargo build --workspace` | pass |
| lints | `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| tests | `cargo test --workspace` | pass, 2 296 passed / 0 failed (2 284 before; +11 perf guards + 1 `Surface` stamp test) |

No behaviour, UI, build-profile or dependency change: every pre-existing test still passes
unmodified except the four `RowContext` construction sites (they now build a `NodeIndex`)
and the three channel tests that called `App::image_display` (now `render::display_of`).

### 8.8 Canvas pan/zoom: input coalescing and the tiled dot grid (second pass)

Same day, same tree plus the changes below. The complaint: the canvas feels laggy while
panning and zooming with the wheel. §8 had already cut the *per-sync* cost (`sync_view`
0.48–0.50 ms/frame in dev), so the remaining cost was **event volume** plus the dot-grid
tessellation:

* every pointer move and every wheel event ran the *whole* viewport sync (`App::sync_view`:
  scalars, wire labels, ruler ticks + label bitmaps, grid patch, dock panel, node layer)
  and asked for a redraw **immediately**. A 1000 Hz mouse therefore did that work 1000
  times per second and claimed 1000 frames/s — 8x more than the 125 Hz tick can present,
  and 16x more than a 60 Hz panel shows;
* the background was **one `Path` with a dot segment per world intersection**
  (`paths::grid_patch`: 1 376 segments at 100 %, 5 292 at 25 %). Slint re-tessellates a path
  whenever its viewbox moves, so every pan/zoom frame re-tessellated thousands of
  `M x y l 0.25 0` sub-paths.

Three changes, no visual change (same 48-unit lattice, same 2.5 px dot, same
`Theme.canvas-dot` ink):

1. **Coalesced input.** Pointer moves and wheel events now only *record* state
   (`App::note_pan` / `App::note_wheel`: latest pointer position, accumulated wheel delta,
   latest cursor anchor) and the 8 ms frame tick applies it once
   (`App::view_tick` → `apply_pending_view` + one hot sync). At most 125 viewport
   applications and exactly one redraw per tick at any input rate. A wheel burst becomes
   one `zoom_at(latest_cursor, summed_delta)`.
2. **Two-tier viewport sync.** `sync_view_hot` is the only per-frame work: `vp-x`, `vp-y`,
   `scale`, the zoom read-out and one `request_redraw` (node cards, wires and the tiled
   grid follow `scale`/`vp-x`/`vp-y` through `.slint` bindings). `sync_view_follow` (wire
   labels, ruler ticks/label bitmaps, gesture overlays) runs at 60 Hz from the tick while a
   gesture is live. `sync_view_cold` (labels/rulers again, grid tile, dock panel, node
   layer) runs 100 ms after the last input, and immediately on structural changes
   (`sync_all`, resize, `Ctrl+0`/`Ctrl+1`).
3. **Pre-rendered dot tile.** One bitmap holds `dots x dots` anti-aliased dots
   (`grid_tile::rasterize`: 2.5 px circles, `Theme.canvas-dot` at the layer's 0.65 opacity,
   premultiplied so the tile's own resampling cannot dim or shrink a dot) and the canvas
   repeats it with a lattice of `Image` elements (`grid_indices`, 20–25 at any zoom). The
   lattice offsets are `Math.mod(vp - spacing/2, tile)` bindings, so a **pan touches no Rust
   code and rasterises nothing**; the bitmap is regenerated only when the on-screen spacing
   leaves a 2 % bucket (`paths::quantize_spacing`, ≤1 % rescale of a 2.5 px dot), the theme
   changes or the device scale changes. The bucket is taken on the *spacing*, not on the
   zoom: quantising the zoom can land on the other side of the stride boundary
   (`GRID_MIN_SCREEN_STEP`) and generate a bitmap for twice the spacing the lattice shows.

Measured with `cargo test -p open-canvas-app --bin open-canvas-app -- --nocapture
--test-threads=1 perf_tests::` (dev profile; medians of 3; the *whole-window* software
snapshot is ~30 ms of unrelated chrome, so the delta between the two columns is the grid):

| metric | before | after | change |
|---|---|---:|---|
| viewport applications per second of 1000 Hz input | **1 000** | **≤ 125** (one per 8 ms tick) | −87.5 % |
| redraw requests per second of 1000 Hz input | **1 000** | **≤ 125** (exactly one per applied tick) | −87.5 % |
| coalesced pan frame, 240 moves + 60 wheels over 30 frames | 300 applications | **30 applications, 30 redraws** (asserted) | −90 % |
| full pan frame cost (same board as §8.1, 200 nodes / 400 wires) | 0.48–0.50 ms **per input event** | **0.28–0.69 ms per frame** (hot + every-other-tick follow; spread is machine load) | ~1–1.7x less work/frame and 8x fewer frames |
| grid dot segments re-tessellated per pan frame @100 % | 1 376 | **0** | −100 % |
| grid dot segments re-tessellated per pan frame @25 % | 5 292 | **0** | −100 % |
| grid command-string build per pan frame @100 % / @25 % | 0.7–1.4 / 2.8–5.2 ms | 0 (pan) — 2.0–3.2 ms **once per 2 % zoom bucket** | −100 % on pan |
| full-window render per pan frame @100 % (median of 10) | 32–83 ms | 36–87 ms | ±10 % (software only, see below) |
| full-window render per pan frame @25 % (median of 10) | 44–88 ms | **33–59 ms** | −21…−33 % |
| tile lattice | 1 Path, 1 376–5 292 segments | **20–25 `Image` quads, one 288 px / 252 px bitmap** | constant per frame |

The render delta grows with zoom-out (0.88–1.07x @100 %, 1.27–1.49x @25 %; the ranges are
run-to-run load on a shared machine): the path's segment count grows 3.8x while the tile
lattice stays at ~20 quads. In the *software* snapshot the tile is
slightly behind at 100 % because its fill (20 quads of 288² logical px, clipped to the
canvas) costs a little more than tessellating 1 376 short segments — a CPU rasteriser pays
per covered pixel, and the tile covers the whole background with mostly transparent texels.
On femtovg (the shipped renderer) the tessellation is per-frame CPU work plus vertex
uploads while the quads are twenty textured draws of one already-uploaded 288 px texture, so
the software figure is the pessimistic one; the decisive, renderer-independent cost is the
per-frame command string (0.73–2.77 ms of allocation + formatting per pan, now zero).

**Visual equivalence** (`perf_tests::the_tiled_grid_draws_the_same_lattice_as_the_path`,
headless `take_snapshot` at 100 %): the tiled background and the old path agree on the
lattice phase to **0.28 px** in x and 0.34 px in y (the path's own AA estimation noise), a
17.37 px pan moves the tiled lattice by exactly 17.37 px (measured 23.17 against an expected
22.60 absolute window phase, the path measuring 22.41 — i.e. both track the pan the same
way), the *darkest* dot sample is identical (`190.11` from the same ink and layer opacity),
panning does not change it (no shimmer), and the total dot ink is within 14 %
(33 075 vs 28 927 ink units: the tile's exact circle vs the path's 2.5 x 2.75 px round-capped
stub). The tile's peak alpha is asserted against `Theme.canvas-dot x 0.65` in
`grid_tile::tests`, and the bitmap is premultiplied so the tile's own 1 % resampling cannot
dim or shrink a dot at a fractional offset.

**Wheel zoom anchoring** (`a_coalesced_wheel_burst_is_cursor_anchored_and_reversible`,
`the_latest_cursor_anchors_the_coalesced_zoom`): ten wheel events in one frame produce one
application, `k = 1.1` exactly (|Δ| < 1e-9), the world point under the cursor is unchanged
(< 1e-9), and zooming back out returns to the identical viewport (`x`, `y`, `k` all
< 1e-9). The chord test moves the cursor between events and checks the *latest* one is the
anchor.

**One redraw per tick** is enforced three ways: the coalescing test asserts
`view_stats.applications == 1 && view_stats.redraws == 1` on every one of 30 frames;
`only_the_hot_tier_requests_a_redraw` counts `.request_redraw()` call sites in
`sync/view.rs` (exactly one, inside `sync_view_hot`), `handlers.rs` (zero) and `main.rs`
(zero); and the release frame is a single hot sync (asserted).

New tests: 6 in `app_tests/view_coalescing_tests.rs` (30-frame budget, exact release,
cursor-anchored + reversible wheel, latest-cursor anchor, no tile regeneration while
panning, single redraw site), 5 in `grid_tile.rs` (dot centring, ink/opacity, bitmap reuse
per bucket, bounded bitmap, degenerate layout), 5 in `app-core::paths` (lattice coverage at
every pan offset, exact spacing with a small lattice, spacing-bucket absorption, bounded
HiDPI bitmap) and 3 perf guards (`the_tiled_grid_draws_the_same_lattice_as_the_path`,
`the_tiled_grid_is_cheaper_than_the_path`, `a_coalesced_pan_frame_stays_in_budget`). The
pre-rendered background is opt-out via `OC_LEGACY_GRID=1` (the old path, used by the
before/after benchmark).

#### 8.8.1 Gates for this pass

| gate | command | result |
|---|---|---|
| build | `cargo build --workspace` | pass |
| lints | `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| tests | `cargo test --workspace` | pass, 2 326 passed / 0 failed |

No dependency, build-profile or visual change. Behaviour changes are limited to *when* the
viewport is applied (once per 8 ms frame instead of per event) and *when* the label/ruler
tiers catch up (60 Hz during a gesture, exact 100 ms after it); the release position is
applied exactly.

### 8.9 Canvas node drags: coalesced frames, incremental rows and wires (third pass)

2026-10-26, same tree plus the changes below. Complaint: dragging a node on a busy board is
laggy. §8.8 had already coalesced pan/zoom, but node gestures still took the per-event path:
every pointer `move` called `Doc::node_drag_to`, `sync_rows_of` (a full `NodeItem` rebuild
for each dragged card: media lookups, plugin cells and every `SharedString`), `sync_wires`
(400 path strings + the whole label model, including the reference-graph walk) and
`sync_overlays`, then requested a redraw. A 240 Hz mouse therefore did that work 240 times
per second; a 1000 Hz mouse could not be absorbed at all.

Four changes, no behaviour change:

1. **Coalesced gesture input.** Pointer moves during a node drag, resize or connect only
   *record* the latest screen position (`PendingNodeDrag`, `App::note_node_drag`). The 8 ms
   frame tick applies it once (`App::node_drag_tick`): `node_drag_to`, one row pass, one wire
   refresh, the drag overlays, and exactly one redraw. A move batch costs one frame whatever
   the event rate. `PendingNodeDrag::has_input` (the old per-frame early-out that was never
   wired up) is gone; the tick consumes the `Option` directly.
2. **Incremental rows.** `App::sync_drag_rows` scans the row model once and writes only the
   dragged rows' `world-x`, `world-y`, `width` and `height` with `set_row_data`; nothing is
   rebuilt and no other row is touched. The release does one `sync_all` (one full row
   rebuild) after applying the exact release point.
3. **Dirty wires only.** `WireCache` keeps each connection's path segment in document order.
   A tick regenerates only the segments touching a dragged node (`sync_drag_wires`), joins
   the cached rest once, and pushes the one `wires` string the UI property needs. Wire
   labels are moved only for dirty wires, never rebuilt; a wire between two dragged nodes
   keeps its midpoint and is not written. Starting a node drag clears the stale selected-wire
   overlay on the first frame. Ruler ticks and label bitmaps are not touched at all: a node
   drag does not change the viewport.
4. **Overlays that move with the drag only.** Snap guides, the drop-target scalars, the drag
   ghost, the hover bar and the multi-selection bounds are updated per frame. The side panel,
   ruler models, wire-label text and everything else stay on their existing cadence.

Release flush: pointer-up drops any pending sample, applies the release position exactly
(`Doc::node_drag_to`), resolves the drop, ends the gesture, and runs the **one** full sync.
The release works even when the last move was never ticked (the test releases after 40
unticked moves).

Measured by `perf_tests::a_coalesced_node_drag_frame_stays_in_budget` on the 200-node /
400-wire fixture with five cards selected, dev profile, whole-window cost excluded, three
runs (`per_event_drag` is the reconstructed old path: `sync_rows_of` + `sync_wires` +
`sync_overlays` + one redraw per move):

| metric (240 pointer moves, 30 drag frames) | before (per event) | after (coalesced) |
|---|---:|---:|
| applied positions | 240 | **30** |
| redraw requests | 240 | **30** |
| row-model rebuilds | 240 partial (`sync_rows_of` reconstructs 5 `NodeItem`s each = 1 200 row builds) | **0 live**; 1 full rebuild on release |
| row-data writes | 1 200 | **110–120** (only the 5 dragged rows; snapped frames that do not move a row are skipped) |
| wire path segments regenerated | 96 000 (400 per event) | **600** (20 dirty wires x 30 frames) |
| whole `wires` strings pushed | 240 | **30** (one per frame) |
| wire-label model rebuilds | 240 | **0 live**; one full sync on release |
| label rows moved | 240 x 400 recomputed | **440–480** (dirty midpoints only) |
| drag work for the 240 moves (dev) | **412–486 ms** = 1.72–2.03 ms/event | **29–40 ms** = 0.96–1.35 ms/frame |
| work per second at the input rate | **1 717–2 026 ms/s** at 1 000 Hz (cannot keep up) | **120–169 ms/s** at 125 frames/s |
| stated frame budget | — | **3.0 ms** per drag frame (dev), against an 8.33 ms 120 fps frame |

At 120 fps the tick (8 ms) is the limiter: the measured drag work leaves 6.98 ms of the
8.33 ms frame for the renderer, and the test's 3 ms budget still leaves 5.33 ms. The
`before` column is the work the same 240 moves caused under the old per-event path; the
`after` column is the whole 30-frame batch including all 240 input recordings, the ticks
and the one redraw each (median of three passes). The release is outside that count: it
applies the exact position, ends the drag and runs one `sync_all` (one row rebuild plus
one paint).

Tests: 5 functional tests in `app_tests/drag_coalescing_tests.rs` — moves wait for the tick
and the release is exact (including an unticked last move, compared against a directly
dragged reference document), only dragged rows and dirty wires change (the pushed wire
string and every label are compared with a full rebuild), multi-select and snapping intact,
coalesced resize lands the exact bounds, coalesced connect lands on the release target, and
the stale wire selection is cleared. The perf test asserts `row_rebuilds == 0` during the
drag and `== 1` after release, `redraws == 30`, at most five row writes per frame, only the
20 dirty wire paths per frame, and every node position exactly equal to the reference drag.
`only_the_hot_tier_requests_a_redraw` was updated for the shared redraw site: one raw
`request_redraw` in `sync/view.rs`, used by the viewport hot tier and the node-gesture tick;
`handlers.rs` and `main.rs` contain none.

#### 8.9.1 Gates for this pass

| gate | command | result |
|---|---|---|
| build | `cargo build --workspace` | pass |
| lints | `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| tests | `cargo test --workspace` | pass, 2 403 passed / 0 failed (26 ignored) |

No dependency, build-profile, UI or visual change. Behaviour is identical at the gesture
boundaries: the drag follows the pointer (exact release), snapping and multi-select are
untouched, and resize/connect end in the same state as before; only *when* the intermediate
frame is applied (once per 8 ms tick) and *how much* is rewritten (dragged rows and dirty
wires only) changed.

### 8.10 Canvas mouse mapping and 120 fps wheel zoom (fourth pass)

2026-10-26, same tree plus the changes below. Two complaints: the canvas mouse mapping did
not match the intended layout, and wheel zoom still felt laggy on a busy board.

#### 8.10.1 Final mouse mapping

| input on the empty background | action |
|---|---|
| left drag | move the OS window (manual begin/tick/end; the 4 ms cursor sampler runs the tick) |
| left click (no window movement) | clear the node/wire selection |
| Shift + left drag | marquee selection |
| middle drag | pan the canvas |
| right drag | pan the canvas |
| Ctrl + right drag | wire-cut stroke |
| wheel | cursor-anchored zoom (one coalesced step per 8 ms frame) |

A left press on a node never reaches the background layer: the card keeps select / node
drag / resize / connect. A press on a wire still selects that wire; only an empty-background
press arms the window move. The hatch is explicit: the Rust handler classifies the press
(wire / Shift / background) and sets `canvas-window-drag`, and the `.slint` background starts
the manual move only when that flag is set. `root.panning` still drives the `grabbing` cursor
for a middle/right pan, and `space-held` keeps the `grab` cursor.

#### 8.10.2 Why zoom was more expensive than pan

The wheel path was already coalesced (`note_wheel` recorded the summed delta and the 8 ms
tick applied one `zoom_at`), but a wheel burst still ran the **follow tier every other
tick**: `sync_wire_labels` (400 screen-space label rows), `sync_rulers` (tick model plus
per-label raster lookups) and `sync_overlays` (which also calls `sync_panel`). Ten wheel
events per frame over 60 frames therefore paid 30 label/ruler/panel passes, and the wheel
gesture never reached the hot-only path that the task expected. The other suspected costs
were checked and are **not** on the wheel path:

| suspected cost | finding |
|---|---|
| dot-grid tile rebuild | none: the grid is screen-constant and `sync_grid` only runs in the cold pass; the bitmap key has no viewport in it |
| ruler label image regeneration | cached per label/ink/scale; the model rebuild was the real repeat cost, now cold-only |
| `sync_wire_labels` midpoints | 400 rows per follow pass, now once per gesture |
| `image_fit` / `auto_fit` | not called by canvas zoom (studio-only paths) |
| per-zoom `sync_all` | none: `apply_pending_view` only applies the viewport and the hot tier |

#### 8.10.3 The fix

`PendingView` gained a `hot_only` flag. A wheel event sets it; a pan clears it. While it is
set, `view_tick_at` runs `sync_view_hot` (scale, `vp-x`, `vp-y`, zoom read-out, one
`request_redraw`) and **nothing else**; the follow tier is skipped, including the idle ticks
between the last wheel event and the cold deadline. The cold pass runs once, 100 ms after
the burst, and calls labels/rulers/grid/panel/node-layer exactly once. The hot tier also
stops writing `panning` when it did not change, and the zoom badge only writes
`zoom-badge-visible` on the first frame of a burst.

The accumulator contract is unchanged and now asserted per frame: any number of wheel
events inside one 8 ms frame is one `zoom_at(latest_cursor, summed_delta)`, so the world
point under the cursor is fixed and zoom-in + zoom-out round-trips exactly. The perf test
drives 10 events per frame for 60 frames (600 events); the functional test also drives
40-events-per-frame trackpad batches and checks one application/redraw per frame.

#### 8.10.4 Measured before/after

`perf_tests::a_coalesced_wheel_zoom_frame_stays_in_budget`, 200 nodes / 400 wires, dev
profile, median of 3 passes, 10 wheel events per 8 ms frame for 60 frames plus the one cold
settle tick:

| metric | before | after | change |
|---|---:|---:|---|
| wheel events applied | 600 | 600 | — |
| hot applications | 60 | **60** (one per frame) | unchanged (correct) |
| redraw requests | 60 | **60** (one per frame) | unchanged (correct) |
| follow-tier passes during the burst | 30 | **0** | −100 % |
| wire-label passes (total) | 31 | **1** (cold) | −97 % |
| ruler passes (total) | 31 | **1** (cold) | −97 % |
| cold passes | 1 | **1** | unchanged |
| dot-tile rebuilds | 0 | **0** | unchanged |
| per-frame work (incl. amortised cold pass) | **0.285 ms** | **0.009–0.012 ms** | **−96…−97 %** |
| frame budget | — | 3.0 ms (dev) | 0.009–0.012 ms measured |

The whole 60-frame burst fell from ~17.1 ms to ~0.6 ms of Rust work on the 200-node
fixture; the remaining 8.33 ms of a 120 fps frame belongs to the renderer. Applications and
redraws stay at one per frame, and the label/ruler/panel layer now does exactly one pass
when the gesture ends.

Tests: the mapping is covered by 9 tests in `app_tests/canvas_input_tests.rs` (left press
classification and click-clear, left drag not panning, middle and right drags panning and
the grabbing cursor, Ctrl+right cut, Shift marquee, moved window drag keeping the
selection), and the wheel contract by
`a_wheel_burst_runs_only_the_hot_tier_until_the_cold_pass` plus the existing cursor-anchor /
reversibility / latest-cursor tests in `app_tests/view_coalescing_tests.rs`. The perf test
asserts 60 hot applications, 60 redraws, zero follow passes, one cold pass, one label and
one ruler pass, an unchanged grid-tile key and a per-frame cost under 3 ms.

#### 8.10.5 Gates for this pass

| gate | command | result |
|---|---|---|
| build | `cargo build --workspace` | pass |
| lints | `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| tests | `cargo test --workspace` | pass, 2 413 passed / 0 failed (26 ignored) |

No dependency, build-profile or pan/zoom-math change. The only interaction change is the
button mapping above; zoom stays one cursor-anchored application per frame, and its labels,
rulers, panel and node layer settle once, 100 ms after the wheel burst.

---

## Appendix A. Removed text

Exact deleted bodies, so the deletions above can be reviewed or reverted without git
history. Nothing else in the tree changed.

### `crates/ai/src/config.rs`

```rust
/// `modelOptionLabel` (`use-config-store.ts:333`): `"<channel>/<model>"` when encoded.
pub fn model_option_label(config: &AiConfig, value: &str) -> String {
    let Some((channel_id, model)) = decode_channel_model(value) else {
        return value.to_string();
    };
    match config.channels.iter().find(|item| item.id == channel_id) {
        Some(channel) => format!("{}/{}", channel.name, model),
        None => model,
    }
}
```

### `crates/ai/src/image.rs`

```rust
/// `QUALITY_ALIASES` (`image.ts:125`).
const QUALITY_ALIASES: [(&str, &str); 3] = [("1k", "low"), ("2k", "medium"), ("4k", "high")];
/// `normalizeQuality` (`image.ts:138`).
pub fn normalize_quality(quality: &str) -> Option<String> {
    let value = quality.trim().to_ascii_lowercase();
    let normalized = QUALITY_ALIASES
        .iter()
        .find(|(alias, _)| *alias == value)
        .map_or_else(|| value.clone(), |(_, target)| (*target).to_string());
    QUALITY_BASE
        .iter()
        .any(|(name, _)| *name == normalized)
        .then_some(normalized)
}
```

### `crates/core/src/alignment.rs`

```rust
    pub fn is_distribute(self) -> bool {
        matches!(self, Self::DistributeX | Self::DistributeY)
    }
```

### `crates/plugins/src/official/html.rs`

```rust
/// Source-editor metrics (`html/index.tsx:9-10`), kept for the Slint editor.
pub const EDITOR_FONT_SIZE: f64 = 12.0;
pub const EDITOR_LINE_HEIGHT: f64 = 20.0;
```

### `crates/plugins/src/official/sticky_note.rs`

```rust
    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }
```

### `crates/ps/src/adjustments.rs`

```rust
pub fn ps_copy_params(params: &PsParams) -> PsParams {
    params.clone()
}
```

### `crates/ps/src/board.rs`

```rust
/// `smartCanvasFill` (smart-canvas.ts:79): CSS `color-mix` when translucent.
pub fn smart_canvas_fill(color: &str, opacity: f64) -> String {
    if opacity >= 1.0 {
        color.to_string()
    } else {
        format!(
            "color-mix(in srgb, {color} {}%, transparent)",
            (opacity * 100.0).round()
        )
    }
}

/// `movePsLayer` (smart-canvas.ts:207): swap one step in the flat array.
pub fn move_ps_layer(
    layers: &[CanvasPsLayer],
    layer_id: &str,
    forward: bool,
) -> Vec<CanvasPsLayer> {
    let index = layers.iter().position(|layer| layer.id == layer_id);
    let Some(index) = index else {
        return layers.to_vec();
    };
    let target = if forward {
        index as i64 + 1
    } else {
        index as i64 - 1
    };
    if target < 0 || target as usize >= layers.len() {
        return layers.to_vec();
    }
    let mut next = layers.to_vec();
    next.swap(index, target as usize);
    next
}
```

### `crates/ps/src/layer_ops.rs`

```rust
/// `psLayerCentre` (layer-ops.ts:27).
pub fn layer_centre(layers: &[CanvasPsLayer], layer: &CanvasPsLayer) -> Point {
    let box_ = layer_box(layers, layer);
    Point::new(box_.x + box_.width / 2.0, box_.y + box_.height / 2.0)
}
```

### `crates/ps/src/selection.rs`

```rust
use crate::doc::{CanvasPsLayer, CanvasPsPath};
use crate::path::polyline;
/// `psPathToSelection` (path-ops.ts:7): fill a path's flattened outline.
pub fn path_to_selection(
    path: &CanvasPsPath,
    target: &mut Surface,
    feather: f64,
    anti_alias: bool,
    mode: SelectionMode,
) {
    let points = polyline(path, 24);
    selection_polygon(target, &points, feather, anti_alias, mode);
}
```

### `crates/ps/src/text.rs`

```rust
const SMALL_CAPS_SCALE: f64 = 0.8;
/// `psTextFont` (ps-text.ts:40): a CSS font shorthand.
pub fn text_font(layer: &CanvasPsLayer, scale: f64) -> String {
    let style = if layer.faux_italic == Some(true) {
        "italic "
    } else {
        ""
    };
    let weight = if layer.faux_bold == Some(true) {
        "bold "
    } else {
        "normal "
    };
    let family = layer
        .font_family
        .clone()
        .unwrap_or_else(|| "sans-serif".to_string());
    format!(
        "{style}{weight}{}px {family}",
        (text_font_size(layer) * scale).max(1.0)
    )
}

/// `psTextSmallCapsFont` (ps-text.ts:46).
pub fn text_small_caps_font(layer: &CanvasPsLayer, scale: f64) -> String {
    let weight = if layer.faux_bold == Some(true) {
        "bold "
    } else {
        "normal "
    };
    let family = layer
        .font_family
        .clone()
        .unwrap_or_else(|| "sans-serif".to_string());
    format!(
        "{weight}{}px {family}",
        (text_font_size(layer) * scale * SMALL_CAPS_SCALE).max(1.0)
    )
}
```
