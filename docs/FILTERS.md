# Filters

The Image Studio's Filter menu reaches **112 filters**: the 22 native kernels in
`open_canvas_ps::filters` and all 90 filters of the vendored engine exposed by
`open_canvas_ps::algo_bridge`. Every menu entry opens the same dialog, previews
on the active layer, applies through `App::image_filter_apply`, and becomes the
"Repeat Last Filter" target.

- Menu model: `crates/app/src/image/filter.rs` (`filter_menu_rows`)
- Dialog: `crates/app/src/image/filter.rs` + `ui/components/image-dialogs.slint`
- Menu UI: `ui/components/image.slint` (`StudioScrollMenu`)
- Engine catalogue: `crates/ps/src/algo_bridge.rs` (`available()`)

## Identity and parameters

`FilterChoice` is the unified identity:

```rust
pub enum FilterChoice {
    Native(PsFilterType),
    Algo(&'static FilterEntry),
}
```

`FilterChoice::from_id` resolves native ids first, then the engine catalogue, so
a recorded step always round-trips. `FilterParams` is the unified parameter bag:
native filters keep a `PsParams` (`BTreeMap<String, CanvasPsParamValue>`), engine
filters keep a `BTreeMap<String, ParamValue>`.

The recorder writes `filter` steps with the filter id and a deterministic
parameter string: the native `key=value;...` encoding, or a JSON object for
engine filters (sorted keys, tagged values, stable number formatting). Replay
decodes with `FilterParams::parse_for`.

Engine filters that take a seed run with their own `seed` parameter when the
schema has one; otherwise they use the fixed `0x5EED` seed, so the preview and
the applied result are identical and repeatable.

## Dialog controls

Rows are built from the filter's schema (native control lists are unchanged):

| Schema control | Dialog row |
| --- | --- |
| `Number` | Slider with a numeric readout |
| `Integer` | Slider (integral range) with a numeric readout |
| `Choice` | Dropdown |
| `Toggle` | Checkbox |
| `Text` | Text field |
| `Color` | `#rrggbb` text field |
| `Numbers` | Comma-separated number field |

Reset restores every schema default. Cancel closes without applying. Apply
commits the bitmap, records the filter step, pushes a `Filter` history entry and
closes the dialog. Liquify stays native-only: its preview drag pushes pixels
through `ps_liquify_push` and the apply commits that working copy.

## Menu groups

The menu is data-driven and built once per session. It starts with **Repeat Last
Filter**, then one header per non-empty Photoshop group, then **Filter Gallery**
with its folders:

1. Blur
2. Distort
3. Noise
4. Pixelate
5. Render
6. Sharpen
7. Stylize
8. Other
9. Video (empty in the current catalogue, so it is not shown)
10. Filter Gallery — Artistic, Brush Strokes, Distort, Sketch, Stylize, Texture

The menu is wrapped in a `Flickable` capped at 540px, so all 128 rows are
reachable. Unsupported entries stay visible, dimmed, with the reason as
secondary text.

## Filter table

| Id | Label | Group |
| --- | --- | --- |
| `gaussian-blur` | Gaussian Blur | Blur |
| `box-blur` | Box Blur | Blur |
| `motion-blur` | Motion Blur | Blur |
| `radial-blur` | Radial Blur | Blur |
| `tilt-shift` | Tilt-Shift | Blur |
| `iris-blur` | Iris Blur | Blur |
| `surface-blur` | Surface Blur | Blur |
| `lens-blur` | Lens Blur | Blur |
| `shape-blur` | Shape Blur | Blur |
| `smart-blur` | Smart Blur | Blur |
| `field-blur` | Field Blur | Blur |
| `spin-blur` | Spin Blur | Blur |
| `path-blur` | Path Blur | Blur |
| `ripple` | Ripple | Distort |
| `pinch` | Pinch | Distort |
| `spherize` | Spherize | Distort |
| `liquify` | Liquify | Distort |
| `twirl` | Twirl | Distort |
| `wave` | Wave | Distort |
| `polar-coordinates` | Polar Coordinates | Distort |
| `shear` | Shear | Distort |
| `zig-zag` | ZigZag | Distort |
| `displace` | Displace | Distort |
| `add-noise` | Add Noise | Noise |
| `despeckle` | Despeckle | Noise |
| `dust-scratches` | Dust & Scratches | Noise |
| `median` | Median | Noise |
| `minimum` | Minimum | Noise |
| `maximum` | Maximum | Noise |
| `reduce-noise` | Reduce Noise | Noise |
| `pixelate` | Pixelate | Pixelate |
| `crystallize` | Crystallize | Pixelate |
| `mosaic` | Mosaic | Pixelate |
| `color-halftone` | Color Halftone | Pixelate |
| `facet` | Facet | Pixelate |
| `fragment` | Fragment | Pixelate |
| `mezzotint` | Mezzotint | Pixelate |
| `pointillize` | Pointillize | Pixelate |
| `difference-clouds` | Difference Clouds | Render |
| `fibers` | Fibers | Render |
| `lighting-effects` | Lighting Effects | Render |
| `relight` | Relight | Render |
| `sharpen` | Sharpen | Sharpen |
| `unsharp-mask` | Unsharp Mask | Sharpen |
| `smart-sharpen` | Smart Sharpen | Sharpen |
| `high-pass` | High Pass | Sharpen |
| `emboss` | Emboss | Stylize |
| `find-edges` | Find Edges | Stylize |
| `solarize` | Solarize | Stylize |
| `clouds` | Clouds | Stylize |
| `lens-flare` | Lens Flare | Stylize |
| `invert` | Invert | Stylize |
| `desaturate` | Desaturate | Stylize |
| `diffuse` | Diffuse | Stylize |
| `extrude` | Extrude | Stylize |
| `oil-paint` | Oil Paint | Stylize |
| `tiles` | Tiles | Stylize |
| `trace-contour` | Trace Contour | Stylize |
| `wind` | Wind | Stylize |
| `offset` | Offset | Other |
| `custom` | Custom | Other |
| `hsb-hsl` | HSB/HSL | Other |
| `color-to-alpha` | Color to Alpha | Other |
| `de-interlace` | De-Interlace | Other |
| `ntsc-colors` | NTSC Colors | Other |
| `gallery-colored-pencil` | Colored Pencil | Artistic |
| `gallery-cutout` | Cutout | Artistic |
| `gallery-dry-brush` | Dry Brush | Artistic |
| `gallery-film-grain` | Film Grain | Artistic |
| `gallery-fresco` | Fresco | Artistic |
| `gallery-neon-glow` | Neon Glow | Artistic |
| `gallery-paint-daubs` | Paint Daubs | Artistic |
| `gallery-palette-knife` | Palette Knife | Artistic |
| `gallery-plastic-wrap` | Plastic Wrap | Artistic |
| `gallery-poster-edges` | Poster Edges | Artistic |
| `gallery-rough-pastels` | Rough Pastels | Artistic |
| `gallery-smudge-stick` | Smudge Stick | Artistic |
| `gallery-sponge` | Sponge | Artistic |
| `gallery-underpainting` | Underpainting | Artistic |
| `gallery-watercolor` | Watercolor | Artistic |
| `gallery-accented-edges` | Accented Edges | Brush Strokes |
| `gallery-angled-strokes` | Angled Strokes | Brush Strokes |
| `gallery-crosshatch` | Crosshatch | Brush Strokes |
| `gallery-dark-strokes` | Dark Strokes | Brush Strokes |
| `gallery-ink-outlines` | Ink Outlines | Brush Strokes |
| `gallery-spatter` | Spatter | Brush Strokes |
| `gallery-sprayed-strokes` | Sprayed Strokes | Brush Strokes |
| `gallery-sumi-e` | Sumi-e | Brush Strokes |
| `gallery-diffuse-glow` | Diffuse Glow | Distort |
| `gallery-glass` | Glass | Distort |
| `gallery-ocean-ripple` | Ocean Ripple | Distort |
| `gallery-bas-relief` | Bas Relief | Sketch |
| `gallery-chalk-charcoal` | Chalk & Charcoal | Sketch |
| `gallery-charcoal` | Charcoal | Sketch |
| `gallery-chrome` | Chrome | Sketch |
| `gallery-conte-crayon` | Conté Crayon | Sketch |
| `gallery-graphic-pen` | Graphic Pen | Sketch |
| `gallery-halftone-pattern` | Halftone Pattern | Sketch |
| `gallery-note-paper` | Note Paper | Sketch |
| `gallery-photocopy` | Photocopy | Sketch |
| `gallery-plaster` | Plaster | Sketch |
| `gallery-reticulation` | Reticulation | Sketch |
| `gallery-stamp` | Stamp | Sketch |
| `gallery-torn-edges` | Torn Edges | Sketch |
| `gallery-water-paper` | Water Paper | Sketch |
| `gallery-glowing-edges` | Glowing Edges | Stylize |
| `gallery-craquelure` | Craquelure | Texture |
| `gallery-grain` | Grain | Texture |
| `gallery-mosaic-tiles` | Mosaic Tiles | Texture |
| `gallery-patchwork` | Patchwork | Texture |
| `gallery-stained-glass` | Stained Glass | Texture |
| `gallery-texturizer` | Texturizer | Texture |

Counts: 22 native + 43 direct engine filters + 47 Filter Gallery effects = **112
enabled filters**. With the repeat row, 8 group headers, the Filter Gallery
header and its 6 folders, the menu has 128 rows.

## Unsupported

| Id | Label | Group | Why |
| --- | --- | --- | --- |
| `displace` | Displace | Distort | Needs a displacement map (a second image); the document model has no second-image parameter. `algo_bridge::apply_filter` returns `PsError::UnsupportedFilter`. |

It stays in the menu as a dimmed row with that reason as secondary text, and
`image_filter_open_core` refuses it with the same message.

## Engine filters that are neutral at their defaults

These produce no pixel change with schema defaults on a flat test bitmap; the
dialog still works and the parameters change the result:

- `smart-blur` — only averages neighbours within its threshold; use Edge Only /
  Overlay Edge or a larger threshold for a visible result.
- `shear` — the default curve is a straight line (identity).
- `offset` — default horizontal/vertical are 0.
- `custom` — the default 5×5 kernel is the identity.
- `hsb-hsl` — default input and output are both RGB.
- `ntsc-colors` — in-gamut colours map to themselves.

## Tests

`crates/app/src/image/filter.rs` covers:

- every native and every supported engine id opens with a bounded 320px
  preview and a warning-free pixel target, applies, changes the bitmap, records
  a `Filter` history step and becomes repeat-last;
- native and engine schema rows map onto the dialog's controls;
- number parameters rerender the preview and reset restores the defaults (native
  and `oil-paint`);
- choices, toggles, engine text/colour/list edits route into the parameter bag;
- repeat-last works for a native (`sharpen`) and an engine (`oil-paint`) filter;
- a Filter Gallery effect (`gallery-watercolor`) opens, previews and applies;
- the menu lists every native and engine id exactly once, in Photoshop group
  order, marks `displace` disabled with its reason, and every row id resolves
  through `FilterChoice::from_id`.
