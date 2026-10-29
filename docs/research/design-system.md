[CONTEXT] I am porting the web frontend at D:\Dev\open-canvas\web to a Rust desktop app and must preserve ALL UI design, colors, fonts and motion 1:1. The canvas surfaces live in web/src/components/canvas/, the canvas theme in web/src/lib/canvas-theme.ts and web/src/lib/app-theme.ts, global CSS in web/src/styles/globals.css, Tailwind/Theme config in web/tailwind.config.*, web/components.json, web/src/styles/.

[GOAL] Produce a complete, exact design-system extraction so I can rebuild it as Rust code (Slint theme globals) with identical values. I need literal values, not descriptions.

[DOWNSTREAM] I will hand these values to an implementer to build the Rust design tokens (colors, fonts, radii, shadows, spacing) and verify pixel fidelity.

[REQUEST] Extract and return, with file:line provenance:
1. Every canvas theme token from canvas-theme.ts (canvasThemes): full token tree with exact color values for BOTH light and dark (hex/rgba). Include node.*, toolbar.*, panel.*, canvas bg, grid, selection, connection/wire, minimap, ruler, context menu, chip, badge, overlay tokens. Group by light/dark.
2. app-theme.ts: AntD alias tokens + component tokens (dropdown/select/menu background/hover/selected), any color overrides.
3. globals.css: root variables, dark-mode selectors, font-face declarations, base resets, the --hover variable and any other custom properties, keyframes.
4. Fonts: exact font-family stacks used for UI text, headings, and any monospace; where they are declared (@font-face, imported webfonts, next/font, or system stack). List every font file/URL referenced (check index.html, globals.css, any fonts folder, package.json deps like @fontsource).
5. Tailwind theme extension: the named colors (text-foreground, bg-brand, bg-brand-soft, border-brand, text-success/warning/danger/info, hover:bg-hover, etc.) and what they map to.
Keep it exhaustive and literal. Prefer grouped tables/lists of `token = value  (file:line)`. Skip tests.
<!-- OMO_INTERNAL_INITIATOR -->

---

<analysis>
**Literal Request**: Extract every design token (colors, fonts, radii, shadows) with exact hex/rgba values and file:line provenance from canvas-theme.ts, app-theme.ts, globals.css, Tailwind config, components.json, and font declarations in D:\Dev\open-canvas\web.

**Actual Need**: A complete, literal design-token inventory to rebuild as Rust/Slint theme globals with pixel-identical fidelity. They cannot tolerate descriptions or approximations—they need copy-pasteable values.

**Success Looks Like**: Grouped tables/lists of `token = value (file:line)` covering canvas themes (light+dark), AntD aliases, CSS custom properties, font stacks/faces, and Tailwind color mappings, such that an implementer can transcribe them directly without opening the source.
</analysis>

Let me locate and read all relevant files in parallel.

---

No `tailwind.config.*` at that path—likely a monorepo or Vite project. Let me read the core files and hunt for Tailwind config in parallel.

---

Tailwind v4 (CSS-first config, no `tailwind.config.*`). Now let me find additional theme tokens, the theme store, font assets, and hardcoded canvas-surface colors.

---

Now reading the theme store, providers, hooks, and node registry, plus scanning for hardcoded canvas-surface colors and any other token groups.

---

Found `selectionBlue`. Let me scan all canvas component color literals and read the toolbar/node surfaces, plus locate context-menu/chip/badge/overlay implementations.

---

Let me resolve the webfont faces and inspect the chip/badge/overlay implementations.

---

I have everything needed. Below is the complete literal design-system extraction.

---

# OpenCanvas Web Design-System Extraction

**Key structural fact:** This is a **Tailwind v4 CSS-first** project — there is **no `tailwind.config.*` file**. The Tailwind theme is defined in `globals.css` via `@theme inline`. `components.json` confirms `"config": ""`. All color tokens ultimately resolve to CSS variables that **mirror** `canvasThemes` (comment in `globals.css:64,114`).

Note a **path discrepancy**: `components.json:8` declares `"css": "src/app/globals.css"`, but the real file is `web/src/styles/globals.css` (imported at `main.tsx:5`).

---

## 1. `canvas-theme.ts` — `canvasThemes` full token tree

File: `D:\Dev\open-canvas\web\src\lib\canvas-theme.ts`

### 1A. LIGHT (`canvasThemes.light`, lines 5–56)

| Token | Value | Line |
|---|---|---|
| canvas.background | `#F4F1EB` | 7 |
| canvas.dot | `rgba(58,48,40,.22)` | 8 |
| canvas.line | `rgba(58,48,40,.10)` | 9 |
| canvas.selectionStroke | `#8C8175` | 10 |
| canvas.selectionFill | `rgba(28,25,23,.07)` | 11 |
| canvas.accentFill | `rgba(216,92,58,.10)` | 12 |
| node.label | `#4A443C` | 15 |
| node.fill | `#FFFFFF` | 16 |
| node.panel | `#FFFEFC` | 17 |
| node.stroke | `#E8E2D8` | 18 |
| node.activeStroke | `#3A342D` | 19 |
| node.placeholder | `#9A9288` | 20 |
| node.text | `#26221D` | 21 |
| node.muted | `#6B635A` | 22 |
| node.faint | `#7E766C` | 23 |
| node.secondary | `#3A342D` | 24 |
| node.primary | `#F6EAE0` | 25 |
| node.primaryText | `#8A3F22` | 26 |
| node.blocked | `#b91c1c` | 27 |
| node.danger | `#b91c1c` | 28 |
| node.dangerSoft | `#fbe7e7` | 29 |
| node.accent | `#D85C3A` | 30 |
| node.accentText | `#B8451F` | 31 |
| node.accentSoft | `#FBEAE2` | 32 |
| node.success | `#15803d` | 33 |
| node.successSoft | `#e4f4e8` | 34 |
| node.warning | `#96620a` | 35 |
| node.warningSoft | `#fbf0dc` | 36 |
| node.info | `#0f766e` | 37 |
| node.infoSoft | `#e2f2ef` | 38 |
| toolbar.panel | `rgba(255,254,252,.86)` | 41 |
| toolbar.border | `rgba(38,34,29,.12)` | 42 |
| toolbar.item | `#3A342D` | 43 |
| toolbar.itemHover | `#F1ECE4` | 44 |
| toolbar.activeBg | `#EFE9DF` | 45 |
| toolbar.activeText | `#26221D` | 46 |
| toolbar.accentBg | `#FBEAE2` | 47 |
| toolbar.accentText | `#B8451F` | 48 |
| toolbar.surface | `#FFFEFC` | 49 |
| toolbar.surfaceStroke | `rgba(28,25,23,.20)` | 50 |
| toolbar.surfaceShadow | `0 14px 34px rgba(28,25,23,.26), 0 2px 6px rgba(28,25,23,.18)` | 51 |
| toolbar.itemText | `#3A342D` | 52 |
| toolbar.divider | `rgba(28,25,23,.14)` | 53 |
| chart[0..4] | `["#D85C3A", "#0f766e", "#96620a", "#15803d", "#b91c1c"]` | 55 |

### 1B. DARK (`canvasThemes.dark`, lines 57–108)

| Token | Value | Line |
|---|---|---|
| canvas.background | `#15120E` | 59 |
| canvas.dot | `rgba(245,242,238,.26)` | 60 |
| canvas.line | `rgba(245,242,238,.11)` | 61 |
| canvas.selectionStroke | `#d6d1c8` | 62 |
| canvas.selectionFill | `rgba(250,249,247,.10)` | 63 |
| canvas.accentFill | `rgba(245,136,90,.16)` | 64 |
| node.label | `#E7E1D8` | 67 |
| node.fill | `#201D18` | 68 |
| node.panel | `#1B1813` | 69 |
| node.stroke | `#3A342C` | 70 |
| node.activeStroke | `#efe9df` | 71 |
| node.placeholder | `#9c958b` | 72 |
| node.text | `#faf8f5` | 73 |
| node.muted | `#C4BDB2` | 74 |
| node.faint | `#a8a29e` | 75 |
| node.secondary | `#dcd6cc` | 76 |
| node.primary | `#E7C9A8` | 77 |
| node.primaryText | `#2a2118` | 78 |
| node.blocked | `#f87171` | 79 |
| node.danger | `#f87171` | 80 |
| node.dangerSoft | `rgba(248,113,113,.16)` | 81 |
| node.accent | `#F5885A` | 82 |
| node.accentText | `#FBAE82` | 83 |
| node.accentSoft | `rgba(245,136,90,.16)` | 84 |
| node.success | `#4ade80` | 85 |
| node.successSoft | `rgba(74,222,128,.16)` | 86 |
| node.warning | `#fbbf24` | 87 |
| node.warningSoft | `rgba(251,191,36,.16)` | 88 |
| node.info | `#2dd4bf` | 89 |
| node.infoSoft | `rgba(45,212,191,.16)` | 90 |
| toolbar.panel | `rgba(27,24,19,.86)` | 93 |
| toolbar.border | `rgba(255,255,255,.14)` | 94 |
| toolbar.item | `#dcd6cc` | 95 |
| toolbar.itemHover | `#201D18` | 96 |
| toolbar.activeBg | `#2E2921` | 97 |
| toolbar.activeText | `#faf8f5` | 98 |
| toolbar.accentBg | `rgba(245,136,90,.16)` | 99 |
| toolbar.accentText | `#F5885A` | 100 |
| toolbar.surface | `#1E1A15` | 101 |
| toolbar.surfaceStroke | `rgba(255,255,255,.24)` | 102 |
| toolbar.surfaceShadow | `0 16px 40px rgba(0,0,0,.62), 0 2px 8px rgba(0,0,0,.5)` | 103 |
| toolbar.itemText | `#efe9df` | 104 |
| toolbar.divider | `rgba(255,255,255,.16)` | 105 |
| chart[0..4] | `["#F5885A", "#2dd4bf", "#fbbf24", "#4ade80", "#f87171"]` | 107 |

### 1C. Shared constants
| Constant | Value | Line |
|---|---|---|
| `frostedSurfaceClass` | `"backdrop-blur-xl backdrop-saturate-150"` | 113 |
| `frostedSurfaceLargeClass` | `"backdrop-blur-2xl backdrop-saturate-150"` | 114 |
| Active theme selector | `useThemeStore` default `"light"`, key `open-canvas:theme_store` | `use-theme-store.ts:14,17` |
| Theme hook | `canvasThemes[theme]` | `use-canvas-theme.ts:5` |

### 1D. Token groups that DO NOT exist in `canvas-theme.ts`
There is **no** `panel.*`, `minimap.*`, `ruler.*`, `contextMenu.*`, `chip.*`, `badge.*`, or `overlay.*` group. Those surfaces are **composed** from `canvas.*` / `node.*` / `toolbar.*` plus glass utilities. Compositions found in code:

| Surface | Composition | File:line |
|---|---|---|
| Toolbar dock | `glass-surface` + `toolbar.border`/`toolbar.item` | `canvas-toolbar.tsx:115,70` |
| Toolbar popovers (create/extensions/zoom = context menus) | `glass-raised` + `toolbar.border`/`toolbar.item`; hover `toolbar.itemHover`; active `toolbar.activeBg`/`activeText` | `canvas-toolbar.tsx:269,299,329,280,441` |
| Dock tooltip | bg `node.text`, color `node.panel` | `canvas-toolbar.tsx:458` |
| Top bar | `frostedSurfaceClass` + `toolbar.panel`/`toolbar.border`, h-14 | `canvas-top-bar.tsx:57,8` |
| Minimap container | `glass-card` + `toolbar.border`; node blobs `minimapColor` else `node.muted` @ opacity 0.8; viewport rect border `node.activeStroke`, fill `${node.activeStroke}18` | `canvas-mini-map.tsx:121,101,111-112,138` |
| Ruler | surface `{background: toolbar.panel, borderColor: toolbar.border, color: node.text}`; tick text `node.muted` fontSize 11; thickness 20px; top offset 56px | `canvas-rulers.tsx:27,33,40,6-9` |
| Wire / connection | `node.activeStroke` (active) else `node.faint`; width 3/2, opacity 1/0.82; label fill active `node.activeStroke` else `node.muted`, halo `canvas.background`; active drag `node.activeStroke` dashed `5,5` | `canvas-connections.tsx:49-51,65-67,98` |
| Chip (reference) | `toolbar.panel` bg, `node.stroke` border, `node.text`; rounded-md, h-6 | `canvas-prompt-chip-input.tsx:350-351` |
| Mention menu (overlay) | `glass-raised` + `toolbar.border`/`node.text`; active row `toolbar.activeBg`/`activeText` | `canvas-prompt-chip-input.tsx:286-287,298` |
| Selection toolbar popover | `glass-raised`, w-44, rounded-xl | `canvas-selection-toolbar.tsx:128` |
| Modal (all canvas dialogs) | `glass-raised` class + `background: var(--glass-strong)` | e.g. `canvas-node-crop-dialog.tsx:74`, `canvas-image-analysis-dialog.tsx:67` |

### 1E. Hardcoded canvas-surface literals (not in theme tokens)
| Value | Purpose | File:line |
|---|---|---|
| `#2f80ff` (`selectionBlue`) | node selection border, active/related node glow, drop outline | `canvas-node.tsx:25,179,401,403,405` |
| `#2f80ff` | mention reference highlight (`bg-[#2f80ff]/16`, `ring-[#2f80ff]/24`) | `canvas-resource-mention-textarea.tsx:206` |
| `#2563eb` (`maskOverlayColor`) | mask-edit overlay | `canvas-node-mask-edit-dialog.tsx:22` |
| `#eab308` / `#fb923c` / `#10b981` / `#f97316` / `#f472b6` / `#fb7185` / `#64748b` | per-node-type minimap colors (Prompt, VideoPrompt, Image, Video, ImageGeneration, VideoGeneration, Assets) | `builtin-nodes.tsx:22-29` |
| `#f59e0b55` / `#f59e0b14` | plugin warning banner border/bg | `canvas-plugin-manager-modal.tsx:218` |
| `#ffffff` / `#000000` | smart-canvas bg presets | `smart-canvas-settings-popover.tsx:105-106` |
| checker `#c8c8c8`/`#ffffff`; HSB gradient `#000000,#ffffff,#ff0000,#ffff00,#00ff00,#00ffff,#0000ff,#ff00ff` | color picker | `ps-color-picker.tsx:18,79,90` |

---

## 2. `app-theme.ts` — AntD alias + component tokens

File: `D:\Dev\open-canvas\web\src\lib\app-theme.ts`. Consumed by `ConfigProvider` at `app-providers.tsx:48`. `componentSize` not set.

### 2A. `neutral.light` (lines 7–31) — aliased to canvas light
| Alias | Resolves to | Value | Line |
|---|---|---|---|
| brand | `canvasThemes.light.node.accent` | `#D85C3A` | 8 |
| brandHover | literal | `#C24A2C` | 9 |
| brandActive | literal | `#A83D22` | 10 |
| onBrand | literal | `#ffffff` | 11 |
| layoutBg | `light.canvas.background` | `#F4F1EB` | 12 |
| containerBg | `light.node.panel` | `#FFFEFC` | 13 |
| elevatedBg | `light.node.panel` | `#FFFEFC` | 14 |
| border | `light.node.stroke` | `#E8E2D8` | 15 |
| borderSecondary | `light.node.fill` | `#FFFFFF` | 16 |
| text | `light.node.text` | `#26221D` | 17 |
| textSecondary | `light.node.label` | `#4A443C` | 18 |
| textTertiary | `light.node.muted` | `#6B635A` | 19 |
| error | `light.node.danger` | `#b91c1c` | 20 |
| success | `light.node.success` | `#15803d` | 21 |
| warning | `light.node.warning` | `#96620a` | 22 |
| info | `light.node.info` | `#0f766e` | 23 |
| itemHoverBg | `light.canvas.selectionFill` | `rgba(28,25,23,.07)` | 24 |
| itemSelectedBg | `light.node.accentSoft` | `#FBEAE2` | 25 |
| itemSelectedHoverBg | literal | `#F6DCD2` | 26 |
| itemText | `light.node.text` | `#26221D` | 27 |
| itemSelectedText | `light.node.accentText` | `#B8451F` | 28 |
| tableSelectedBg | `light.node.accentSoft` | `#FBEAE2` | 29 |
| tableSelectedHoverBg | literal | `#F6DCD2` | 30 |

### 2B. `neutral.dark` (lines 32–56)
| Alias | Resolves to | Value | Line |
|---|---|---|---|
| brand | `dark.node.accent` | `#F5885A` | 33 |
| brandHover | literal | `#FBA57C` | 34 |
| brandActive | literal | `#FDC0A0` | 35 |
| onBrand | literal | `#1e1c19` | 36 |
| layoutBg | `dark.canvas.background` | `#15120E` | 37 |
| containerBg | `dark.node.panel` | `#1B1813` | 38 |
| elevatedBg | `dark.node.fill` | `#201D18` | 39 |
| border | `dark.node.stroke` | `#3A342C` | 40 |
| borderSecondary | `dark.node.fill` | `#201D18` | 41 |
| text | `dark.node.text` | `#faf8f5` | 42 |
| textSecondary | `dark.node.label` | `#E7E1D8` | 43 |
| textTertiary | `dark.node.muted` | `#C4BDB2` | 44 |
| error | `dark.node.danger` | `#f87171` | 45 |
| success | `dark.node.success` | `#4ade80` | 46 |
| warning | `dark.node.warning` | `#fbbf24` | 47 |
| info | `dark.node.info` | `#2dd4bf` | 48 |
| itemHoverBg | `dark.canvas.selectionFill` | `rgba(250,249,247,.10)` | 49 |
| itemSelectedBg | `dark.node.accentSoft` | `rgba(245,136,90,.16)` | 50 |
| itemSelectedHoverBg | literal | `rgba(245,136,90,.24)` | 51 |
| itemText | `dark.node.text` | `#faf8f5` | 52 |
| itemSelectedText | `dark.node.accentText` | `#FBAE82` | 53 |
| tableSelectedBg | `dark.node.accentSoft` | `rgba(245,136,90,.16)` | 54 |
| tableSelectedHoverBg | literal | `rgba(245,136,90,.24)` | 55 |

### 2C. `token` (global alias, lines 65–96)
| AntD token | Value | Line |
|---|---|---|
| algorithm | `darkAlgorithm` / `defaultAlgorithm` | 63 |
| cssVar.key | `open-canvas-dark` / `open-canvas-light` | 64 |
| fontFamily | `"Baloo 2", "Resource Han Rounded SC", "Yuanti SC", "YouYuan", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif` | 66 |
| fontSize | `14` | 67 |
| fontSizeSM | `13` | 68 |
| fontSizeLG | `16` | 69 |
| borderRadius | `8` | 70 |
| borderRadiusSM | `6` | 71 |
| borderRadiusLG | `12` | 72 |
| borderRadiusXS | `4` | 73 |
| colorPrimary | `brand` | 74 |
| colorPrimaryHover | `brandHover` | 75 |
| colorPrimaryActive | `brandActive` | 76 |
| colorInfo | `info` | 77 |
| colorSuccess | `success` | 78 |
| colorWarning | `warning` | 79 |
| colorError | `error` | 80 |
| colorLink | `brand` | 81 |
| colorLinkHover | `brandHover` | 82 |
| colorLinkActive | `brandActive` | 83 |
| colorTextLightSolid | `onBrand` | 84 |
| colorText | `text` | 85 |
| colorTextSecondary | `textSecondary` | 86 |
| colorTextTertiary | `textTertiary` | 87 |
| colorBgLayout | `layoutBg` | 88 |
| colorBgContainer | `containerBg` | 89 |
| colorBgElevated | `elevatedBg` | 90 |
| colorBorder | `border` | 91 |
| colorBorderSecondary | `borderSecondary` | 92 |
| controlItemBgHover | `itemHoverBg` | 93 |
| controlItemBgActive | `itemSelectedBg` | 94 |
| controlItemBgActiveHover | `itemSelectedHoverBg` | 95 |

### 2D. `components` overrides (lines 97–128)
| Component token | Value | Line |
|---|---|---|
| Button.primaryShadow | `none` | 99 |
| Dropdown.colorBgElevated | `elevatedBg` | 102 |
| Dropdown.colorText | `itemText` | 103 |
| Dropdown.controlItemBgHover | `itemHoverBg` | 104 |
| Dropdown.controlItemBgActive | `itemSelectedBg` | 105 |
| Dropdown.controlItemBgActiveHover | `itemSelectedHoverBg` | 106 |
| Menu.popupBg | `elevatedBg` | 109 |
| Menu.itemActiveBg | `itemSelectedBg` | 110 |
| Menu.itemHoverBg | `itemHoverBg` | 111 |
| Menu.itemSelectedBg | `itemSelectedBg` | 112 |
| Menu.itemSelectedColor | `itemSelectedText` | 113 |
| Menu.darkPopupBg | `neutral.dark.elevatedBg` = `#201D18` | 114 |
| Menu.darkItemHoverBg | `neutral.dark.itemHoverBg` = `rgba(250,249,247,.10)` | 115 |
| Menu.darkItemSelectedBg | `neutral.dark.itemSelectedBg` = `rgba(245,136,90,.16)` | 116 |
| Menu.darkItemSelectedColor | `neutral.dark.itemSelectedText` = `#FBAE82` | 117 |
| Select.optionActiveBg | `itemHoverBg` | 120 |
| Select.optionSelectedBg | `itemSelectedBg` | 121 |
| Select.optionSelectedColor | `itemSelectedText` | 122 |
| Table.rowSelectedBg | `tableSelectedBg` | 125 |
| Table.rowSelectedHoverBg | `tableSelectedHoverBg` | 126 |

---

## 3. `globals.css` — variables, resets, keyframes

File: `D:\Dev\open-canvas\web\src\styles\globals.css`

### 3A. Imports & variant (lines 1–6)
| Directive | Value | Line |
|---|---|---|
| `@import` | `"tailwindcss"` | 1 |
| `@import` | `"tw-animate-css"` | 2 |
| `@import` | `"shadcn/tailwind.css"` | 3 |
| `@source` | `"../../../node_modules/streamdown/dist/*.js"` | 4 |
| `@custom-variant dark` | `(&:is(.dark *))` | 6 |

### 3B. `@theme inline` (Tailwind token mapping, lines 8–61)
| Tailwind token | Maps to | Line |
|---|---|---|
| --color-background | `var(--background)` | 9 |
| --color-foreground | `var(--foreground)` | 10 |
| --color-border | `var(--border)` | 11 |
| --color-ring | `var(--ring)` | 12 |
| --radius-sm | `calc(var(--radius) - 4px)` | 13 |
| --radius-md | `calc(var(--radius) - 2px)` | 14 |
| --radius-lg | `var(--radius)` | 15 |
| --radius-xl | `calc(var(--radius) + 4px)` | 16 |
| --font-heading | `var(--font-sans)` | 17 |
| --font-sans | `"Baloo 2", "Resource Han Rounded SC", "Yuanti SC", "YouYuan", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif` | 18 |
| --color-sidebar-ring | `var(--sidebar-ring)` | 19 |
| --color-sidebar-border | `var(--sidebar-border)` | 20 |
| --color-sidebar-accent-foreground | `var(--sidebar-accent-foreground)` | 21 |
| --color-sidebar-accent | `var(--sidebar-accent)` | 22 |
| --color-sidebar-primary-foreground | `var(--sidebar-primary-foreground)` | 23 |
| --color-sidebar-primary | `var(--sidebar-primary)` | 24 |
| --color-sidebar-foreground | `var(--sidebar-foreground)` | 25 |
| --color-sidebar | `var(--sidebar)` | 26 |
| --color-chart-5 … 1 | `var(--chart-5)` … `var(--chart-1)` | 27–31 |
| --color-input | `var(--input)` | 32 |
| --color-destructive | `var(--destructive)` | 33 |
| --color-accent-foreground | `var(--accent-foreground)` | 34 |
| --color-accent | `var(--accent)` | 35 |
| --color-muted-foreground | `var(--muted-foreground)` | 36 |
| --color-muted | `var(--muted)` | 37 |
| --color-secondary-foreground | `var(--secondary-foreground)` | 38 |
| --color-secondary | `var(--secondary)` | 39 |
| --color-primary-foreground | `var(--primary-foreground)` | 40 |
| --color-primary | `var(--primary)` | 41 |
| --color-brand | `var(--brand)` | 42 |
| --color-brand-foreground | `var(--brand-foreground)` | 43 |
| --color-brand-soft | `var(--brand-soft)` | 44 |
| --color-success | `var(--success)` | 45 |
| --color-success-soft | `var(--success-soft)` | 46 |
| --color-warning | `var(--warning)` | 47 |
| --color-warning-soft | `var(--warning-soft)` | 48 |
| --color-danger | `var(--danger)` | 49 |
| --color-danger-soft | `var(--danger-soft)` | 50 |
| --color-info | `var(--info)` | 51 |
| --color-info-soft | `var(--info-soft)` | 52 |
| --color-hover | `var(--hover)` | 53 |
| --color-popover-foreground | `var(--popover-foreground)` | 54 |
| --color-popover | `var(--popover)` | 55 |
| --color-card-foreground | `var(--card-foreground)` | 56 |
| --color-card | `var(--card)` | 57 |
| --radius-2xl | `calc(var(--radius) * 1.8)` | 58 |
| --radius-3xl | `calc(var(--radius) * 2.2)` | 59 |
| --radius-4xl | `calc(var(--radius) * 2.6)` | 60 |

**`--font-mono` is NOT overridden** → Tailwind v4 default: `ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace`.

### 3C. `:root` (light, lines 63–111)
| Variable | Value | Line |
|---|---|---|
| --radius | `0.625rem` (=10px) | 65 |
| --glass | `rgba(255, 254, 252, 0.72)` | 66 |
| --glass-strong | `rgba(255, 254, 252, 0.9)` | 67 |
| --border | `#E8E2D8` | 68 |
| --ring | `#a8a29e` | 69 |
| --background | `#F4F1EB` | 70 |
| --foreground | `#26221D` | 71 |
| --card | `#FFFEFC` | 72 |
| --card-foreground | `#26221D` | 73 |
| --popover | `#FFFEFC` | 74 |
| --popover-foreground | `#26221D` | 75 |
| --primary | `#D85C3A` | 76 |
| --primary-foreground | `#ffffff` | 77 |
| --brand | `#D85C3A` | 78 |
| --brand-foreground | `#ffffff` | 79 |
| --brand-soft | `#FBEAE2` | 80 |
| --success | `#15803d` | 81 |
| --success-soft | `#e4f4e8` | 82 |
| --warning | `#96620a` | 83 |
| --warning-soft | `#fbf0dc` | 84 |
| --danger | `#b91c1c` | 85 |
| --danger-soft | `#fbe7e7` | 86 |
| --info | `#0f766e` | 87 |
| --info-soft | `#e2f2ef` | 88 |
| **--hover** | **`rgba(216, 92, 58, 0.08)`** | 89 |
| --secondary | `#F1ECE4` | 90 |
| --secondary-foreground | `#26221D` | 91 |
| --muted | `#F1ECE4` | 92 |
| --muted-foreground | `#6B635A` | 93 |
| --accent | `#F1ECE4` | 94 |
| --accent-foreground | `#26221D` | 95 |
| --destructive | `#b91c1c` | 96 |
| --input | `#E8E2D8` | 97 |
| --chart-1..5 | `#D85C3A`, `#0f766e`, `#96620a`, `#15803d`, `#b91c1c` | 98–102 |
| --sidebar | `#FFFEFC` | 103 |
| --sidebar-foreground | `#26221D` | 104 |
| --sidebar-primary | `#D85C3A` | 105 |
| --sidebar-primary-foreground | `#ffffff` | 106 |
| --sidebar-accent | `#F1ECE4` | 107 |
| --sidebar-accent-foreground | `#26221D` | 108 |
| --sidebar-border | `#E8E2D8` | 109 |
| --sidebar-ring | `#a8a29e` | 110 |

### 3D. `.dark` (lines 113–160)
| Variable | Value | Line |
|---|---|---|
| --glass | `rgba(27, 24, 19, 0.68)` | 115 |
| --glass-strong | `rgba(27, 24, 19, 0.88)` | 116 |
| --background | `#15120E` | 117 |
| --foreground | `#faf8f5` | 118 |
| --border | `#3A342C` | 119 |
| --ring | `#a8a29e` | 120 |
| --card | `#1B1813` | 121 |
| --card-foreground | `#faf8f5` | 122 |
| --popover | `#1B1813` | 123 |
| --popover-foreground | `#faf8f5` | 124 |
| --primary | `#F5885A` | 125 |
| --primary-foreground | `#1e1c19` | 126 |
| --brand | `#F5885A` | 127 |
| --brand-foreground | `#1e1c19` | 128 |
| --brand-soft | `rgba(245, 136, 90, 0.16)` | 129 |
| --success | `#4ade80` | 130 |
| --success-soft | `rgba(74, 222, 128, 0.16)` | 131 |
| --warning | `#fbbf24` | 132 |
| --warning-soft | `rgba(251, 191, 36, 0.16)` | 133 |
| --danger | `#f87171` | 134 |
| --danger-soft | `rgba(248, 113, 113, 0.16)` | 135 |
| --info | `#2dd4bf` | 136 |
| --info-soft | `rgba(45, 212, 191, 0.16)` | 137 |
| **--hover** | **`rgba(245, 136, 90, 0.14)`** | 138 |
| --secondary | `#201D18` | 139 |
| --secondary-foreground | `#faf8f5` | 140 |
| --muted | `#201D18` | 141 |
| --muted-foreground | `#c8c2b8` | 142 |
| --accent | `#201D18` | 143 |
| --accent-foreground | `#faf8f5` | 144 |
| --destructive | `#f87171` | 145 |
| --input | `#3A342C` | 146 |
| --chart-1..5 | `#F5885A`, `#2dd4bf`, `#fbbf24`, `#4ade80`, `#f87171` | 147–151 |
| --sidebar | `#1B1813` | 152 |
| --sidebar-foreground | `#faf8f5` | 153 |
| --sidebar-primary | `#F5885A` | 154 |
| --sidebar-primary-foreground | `#242119` | 155 |
| --sidebar-accent | `#201D18` | 156 |
| --sidebar-accent-foreground | `#faf8f5` | 157 |
| --sidebar-border | `#3A342C` | 158 |
| --sidebar-ring | `#a8a29e` | 159 |

`--radius` is **not** redefined in `.dark` (single global `0.625rem`).

### 3E. Base resets (`@layer base`, lines 162–193)
| Selector | Rule | Line |
|---|---|---|
| `*` | `@apply border-border outline-ring/50` | 164 |
| `html, body` | `height:100%; overflow:hidden; user-select:none; -webkit-user-select:none` | 167–171 |
| `input,textarea,select,[contenteditable="true"],.select-text` | `user-select:text; -webkit-user-select:text` | 173–180 |
| `body` | `@apply bg-background text-foreground` | 182 |
| `textarea` | `resize:none` | 185 |
| `textarea::-webkit-resizer` | `display:none` | 188 |
| `html` | `@apply font-sans` | 191 |

### 3F. Utilities — glass / scrollbars / keyframes
| Utility | Definition | Line |
|---|---|---|
| `.glass-card` | bg `var(--glass)`; blur(16px) saturate(150%); border `color-mix(in srgb, currentColor 12%, transparent)` | 196–201 |
| `.glass-surface` | bg `var(--glass)`; blur(20px) saturate(160%); border 12% currentColor | 203–208 |
| `.glass-raised` | bg `var(--glass-strong)`; blur(24px) saturate(160%); border 14%; `box-shadow: 0 12px 34px color-mix(in srgb, currentColor 12%, transparent)` | 210–216 |
| zoom/drag glass disable | `backdrop-filter:none !important` on `[data-canvas-zooming]`/`[data-canvas-dragging]` | 218–226 |
| `@media (prefers-reduced-transparency: reduce)` | glass → `background: var(--card)`, no blur | 228–236 |
| `.ai-title-aurora` | `linear-gradient(90deg,#111827 0%,#111827 36%,#6b7280 54%,#111827 72%,#111827 100%)`; size 220% 100%; 7s ease-in-out infinite | 238–245 |
| `.dark .ai-title-aurora` | `linear-gradient(90deg,#fafaf9,#fafaf9 36%,#a8a29e 54%,#fafaf9 72%,#fafaf9)` | 247–252 |
| `.hide-scrollbar` | scrollbar-width:none | 266–273 |
| `.prompt-filter-tag.ant-tag-checkable` | radius 6px; padding 5px 10px; color `#57534e`; font-size 14px; transitions 0.15s | 275–292 |
| ↳ hover | bg `rgba(28,25,23,0.06)`; color `#1c1917` | 294–297 |
| ↳ active/checked | bg `#292524`; border `#292524`; color `#fbfaf7`; font-weight 600 | 299–305 |
| ↳ `.dark` | color `#e7e5e4`; hover bg `rgba(255,255,255,0.1)` color `#fff` | 307–314 |
| ↳ `.dark` active/checked | bg `rgba(250,250,249,0.96)`; border `rgba(255,255,255,0.88)`; shadow `0 0 0 1px rgba(255,255,255,.18), 0 6px 18px rgba(0,0,0,.22)`; color `#0c0a09`; weight 700 | 316–325 |
| `.thin-scrollbar` | thumb `rgba(120,113,108,0.45)`; 4px | 327–340 |
| `.hover-scrollbar` | thumb `rgba(120,113,108,0.5)`; track `rgba(120,113,108,0.12)`; 6px | 342–368 |
| `.hover-scrollbar-hint::after` | `rgba(120,113,108,0.42)`; h3px; bottom4/left12/right12 | 370–390 |
| `.canvas-control-select/number` borders | `rgba(120,113,108,0.32)`; focus `#a8a29e`; dark `rgba(214,211,209,0.2)` / focus `#78716c` | 392–499 |
| `.canvas-image-settings-popover .ant-popover-inner` | radius 18px; shadow `0 18px 54px rgba(28,25,23,0.16)`; padding 18px (dark: `rgba(0,0,0,0.45)`) | 501–523 |
| Ant overlay border | `1px solid color-mix(in srgb, currentColor 10%, transparent)` (modal/drawer/dropdown/select/popover/picker/menu/message/notification) | 537–547 |

**Keyframes (exact):**
| Keyframe | Value | Line |
|---|---|---|
| `ai-title-aurora` | 0/100%: pos 0% 50%, drop-shadow 0; 50%: pos 100% 50%, `drop-shadow(0 12px 28px rgba(28,25,23,0.08))` | 254–264 |
| `canvas-batch-child-in` | from: opacity0, translate(var --batch-from-x/y) rotate(var--batch-from-rotate) scale(0.72); 72%: opacity1, translate(0,0) rotate(-1deg) scale(1.015); to: opacity1 none | 563–578 |
| `canvas-node-enter` | from: opacity0 scale(0.88) translateY(10px); 70%: opacity1 scale(1.015); to: none | 752–767 |
| `canvas-node-return` | from translate(var--node-return-from-x/y) → to translate(var--node-return-to-x/y) | 779–787 |
| `canvas-popover-in` | from opacity0 translateY(4px) scale(0.98) → to none | 795–805 |
| `canvas-drag-ghost-in` | from opacity0 scale(0.8) → to opacity1 scale(1) | 811–821 |
| `canvas-connection-label-in` | from opacity0 scale(0.6) → to none; transform-origin center | 848–858 |

Motion timing literals: node enter `0.42s cubic-bezier(0.2,0.85,0.18,1)` (748); node return `200ms` same curve (776); popover-in `0.16s ease-out` (739); drag-ghost `0.15s ease-out` (808); connection label `220ms cubic-bezier(0.2,0.85,0.18,1)` (843); chip transition `160ms ease` (834); batch child `320ms` delay `index*35ms cubic-bezier(.2,.85,.18,1)` (`canvas-node.tsx:657`). All have `prefers-reduced-motion` disable blocks (769–773, 789–793, 823–831, 860–870).

`.agent-streamdown` block (lines 588–733) is markdown/code-block styling (radii `0.4rem`/`0.3rem`, code font-size `0.75rem`, line-height `1.15rem`, etc.), plus `[data-streamdown]` modal (`rgb(28 25 23 / 24%)` scrim, blur 8px, panel radius `1rem`, shadow `0 20px 60px rgb(28 25 23 / 18%)`).

`::view-transition-old/new(root)` → `animation:none; mix-blend-mode:normal` (582–586).

---

## 4. Fonts

### 4A. Family stacks (UI / heading / mono)
| Purpose | Stack | Declared at |
|---|---|---|
| UI sans (Tailwind `--font-sans`, `html`, body) | `"Baloo 2", "Resource Han Rounded SC", "Yuanti SC", "YouYuan", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif` | `globals.css:18`; `app-theme.ts:66` |
| Headings (`--font-heading`) | `var(--font-sans)` (same stack) | `globals.css:17` |
| Body inline (JS) | `"Baloo 2","Resource Han Rounded SC","Yuanti SC","YouYuan","PingFang SC","Microsoft YaHei",system-ui,sans-serif` | `main.tsx:15` |
| Monospace — Tailwind `font-mono` | `ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace` (v4 default, **not overridden**) | no override in `globals.css` |
| Text-node config "System" | `system-ui, -apple-system, "Segoe UI", Roboto, sans-serif` | `text-style.ts:16` |
| Text-node "Sans" | `ui-sans-serif, Helvetica, Arial, sans-serif` | `text-style.ts:17` |
| Text-node "Serif" | `Georgia, "Times New Roman", serif` | `text-style.ts:18` |
| Text-node "Mono" | `ui-monospace, SFMono-Regular, Menlo, Consolas, monospace` | `text-style.ts:19` |
| Text-node "Rounded" | `ui-rounded, "SF Pro Rounded", "Hiragino Maru Gothic ProN", Quicksand, sans-serif` | `text-style.ts:20` |

Photoshop text layer families (`ps-text.ts:8`): `sans-serif, serif, monospace, Arial, Helvetica, Georgia, Times New Roman, Verdana, Trebuchet MS, Courier New, Impact`. Default `sans-serif`. Export renderer default `sans-serif` (`ps-text.ts:43,48`; `smart-canvas.ts:219`).

### 4B. Font sources (all remote; **no local font files**, no `@fontsource`, no `@font-face` anywhere)
Declared in `index.html`:
| Source | URL | Line |
|---|---|---|
| Preconnect | `https://fonts.googleapis.com` | 8 |
| Preconnect | `https://fonts.gstatic.com` (crossorigin) | 9 |
| Preconnect | `https://fontsapi.zeoseven.com` (crossorigin) | 10 |
| Google Fonts — **Baloo 2** wght 400–800 | `https://fonts.googleapis.com/css2?family=Baloo+2:wght@400..800&display=swap` | 11 |
| ZeoSeven 989 **main** (Regular) | `https://fontsapi.zeoseven.com/989/main/result.css` | 12 |
| ZeoSeven 989 **sc-bold** (Bold) | `https://fontsapi.zeoseven.com/989/sc-bold/result.css` | 13 |

Verified via fetch: ZeoSeven 989 serves `FontFamilyName: Resource Han Rounded SC` — Regular (`main`) and Bold (`sc-bold`); fonts split via `cn-font-split` (many `.woff2` chunks referenced inside those two CSS files; PostScript names `Resource-Han-Rounded-SC-Regular` / `-Bold`). `Yuanti SC`, `YouYuan`, `PingFang SC`, `Microsoft YaHei` are **system fallbacks** (no files).

---

## 5. Tailwind named-color mapping (`@theme inline`)

All values for these named utilities are resolved from `@theme inline` in `globals.css` (§3B) → CSS vars (§3C/§3D). Raw value per theme:

| Utility / token | Maps to var | Light | Dark |
|---|---|---|---|
| `text-foreground` / `bg-background` | `--foreground` / `--background` | `#26221D` / `#F4F1EB` | `#faf8f5` / `#15120E` |
| `border-border` | `--border` | `#E8E2D8` | `#3A342C` |
| `ring-ring` | `--ring` | `#a8a29e` | `#a8a29e` |
| `bg-brand` / `text-brand` / `border-brand` | `--brand` | `#D85C3A` | `#F5885A` |
| `bg-brand-foreground` | `--brand-foreground` | `#ffffff` | `#1e1c19` |
| `bg-brand-soft` | `--brand-soft` | `#FBEAE2` | `rgba(245,136,90,0.16)` |
| `text-success` / `bg-success-soft` | `--success` / `--success-soft` | `#15803d` / `#e4f4e8` | `#4ade80` / `rgba(74,222,128,0.16)` |
| `text-warning` / `bg-warning-soft` | `--warning` / `--warning-soft` | `#96620a` / `#fbf0dc` | `#fbbf24` / `rgba(251,191,36,0.16)` |
| `text-danger` / `bg-danger-soft` | `--danger` / `--danger-soft` | `#b91c1c` / `#fbe7e7` | `#f87171` / `rgba(248,113,113,0.16)` |
| `text-info` / `bg-info-soft` | `--info` / `--info-soft` | `#0f766e` / `#e2f2ef` | `#2dd4bf` / `rgba(45,212,191,0.16)` |
| `hover:bg-hover` | `--hover` | `rgba(216,92,58,0.08)` | `rgba(245,136,90,0.14)` |
| `bg-primary` / `text-primary-foreground` | `--primary` / `--primary-foreground` | `#D85C3A` / `#ffffff` | `#F5885A` / `#1e1c19` |
| `bg-secondary` / `text-secondary-foreground` | `--secondary` / `--secondary-foreground` | `#F1ECE4` / `#26221D` | `#201D18` / `#faf8f5` |
| `bg-muted` / `text-muted-foreground` | `--muted` / `--muted-foreground` | `#F1ECE4` / `#6B635A` | `#201D18` / `#c8c2b8` |
| `bg-accent` / `text-accent-foreground` | `--accent` / `--accent-foreground` | `#F1ECE4` / `#26221D` | `#201D18` / `#faf8f5` |
| `bg-card` / `text-card-foreground` | `--card` / `--card-foreground` | `#FFFEFC` / `#26221D` | `#1B1813` / `#faf8f5` |
| `bg-popover` / `text-popover-foreground` | `--popover` / `--popover-foreground` | `#FFFEFC` / `#26221D` | `#1B1813` / `#faf8f5` |
| `bg-destructive` | `--destructive` | `#b91c1c` | `#f87171` |
| `border-input` | `--input` | `#E8E2D8` | `#3A342C` |
| chart-1..5 | `--chart-1..5` | `#D85C3A,#0f766e,#96620a,#15803d,#b91c1c` | `#F5885A,#2dd4bf,#fbbf24,#4ade80,#f87171` |
| sidebar tokens (8) | `--sidebar*` | see §3C 103–110 | see §3D 152–159 |

**Radii mapping (Light & Dark identical, `--radius = 0.625rem = 10px`):**
`radius-sm = 6px`, `radius-md = 8px`, `radius-lg = 10px`, `radius-xl = 14px`, `radius-2xl = 18px`, `radius-3xl = 22px`, `radius-4xl = 26px` (globals.css:13–16,58–60). AntD radii: XS 4, SM 6, base 8, LG 12 (`app-theme.ts:70–73`). Node cards use `rounded-3xl` = 22px (`canvas-node.tsx:399`).

**`components.json` (shadcn):** style `radix-nova`, rsc true, tsx true, tailwind config `""` (no JS config), css `src/app/globals.css` (stale path), baseColor `neutral`, cssVariables true, prefix `""`, iconLibrary `lucide`, rtl false, menuColor `default`, menuAccent `subtle`, registry `@magicui`. Aliases: `@/components`, `@/lib/utils`, `@/components/ui`, `@/lib`, `@/hooks`.

**Shadows (literal, non-Tailwind):** toolbar.surfaceShadow (canvas-theme §1); glass-raised `0 12px 34px color-mix(in srgb, currentColor 12%, transparent)`; image-settings popover `0 18px 54px rgba(28,25,23,0.16)` / dark `rgba(0,0,0,0.45)`; streamdown modal `0 20px 60px rgb(28 25 23 / 18%)`.

---

<results>
<files>
- D:\Dev\open-canvas\web\src\lib\canvas-theme.ts - Primary canvas token source; canvasThemes light+dark (canvas/node/toolbar/chart), frosted classes
- D:\Dev\open-canvas\web\src\lib\app-theme.ts - AntD alias tokens, global token set, Dropdown/Menu/Select/Table/Button component overrides
- D:\Dev\open-canvas\web\src\styles\globals.css - Tailwind v4 @theme mapping, :root/.dark CSS vars, --hover, glass utilities, all keyframes
- D:\Dev\open-canvas\web\index.html - Font sources: Google Fonts Baloo 2 + ZeoSeven Resource Han Rounded SC; dark-mode bootstrap
- D:\Dev\open-canvas\web\src\main.tsx - Inline body font-family stack
- D:\Dev\open-canvas\web\src\stores\use-theme-store.ts - Theme state; default "light"; persist key open-canvas:theme_store
- D:\Dev\open-canvas\web\src\hooks\use-canvas-theme.ts - canvasThemes[theme] selector
- D:\Dev\open-canvas\web\src\components\layout\app-providers.tsx - ConfigProvider binding of getAntThemeConfig(dark)
- D:\Dev\open-canvas\web\components.json - shadcn config (radix-nova, lucide, baseColor neutral); stale css path
- D:\Dev\open-canvas\web\src\components\canvas\canvas-node.tsx - selectionBlue #2f80ff; node card composition; resize/handle/tooltip colors
- D:\Dev\open-canvas\web\src\components\canvas\canvas-toolbar.tsx - Toolbar dock + create/extensions/zoom context menus composition
- D:\Dev\open-canvas\web\src\components\canvas\canvas-top-bar.tsx - Top bar surface composition
- D:\Dev\open-canvas\web\src\components\canvas\canvas-connections.tsx - Wire stroke/label token usage
- D:\Dev\open-canvas\web\src\components\canvas\canvas-rulers.tsx - Ruler surface + dimensions (20px/56px)
- D:\Dev\open-canvas\web\src\components\canvas\canvas-mini-map.tsx - Minimap surface + node color/opacity
- D:\Dev\open-canvas\web\src\components\canvas\nodes\builtin-nodes.tsx - Hardcoded minimapColor values per node type
- D:\Dev\open-canvas\web\src\components\canvas\canvas-prompt-chip-input.tsx - Chip + mention-menu (overlay) token composition
- D:\Dev\open-canvas\web\src\lib\canvas\text-style.ts - Text-node font stacks + size/lineheight bounds
- D:\Dev\open-canvas\web\src\lib\canvas\ps-text.ts - Photoshop text font family list
- D:\Dev\open-canvas\web\src\components\canvas\canvas-overlays.ts - Overlay target selector (Ant modal/popover/dropdown/select/tooltip)
</files>

<answer>
All values above are literal and provenance-tagged. Three things the request assumed that are NOT true and worth flagging before Rust implementation:

1. There is NO separate `panel.*`, `minimap.*`, `ruler.*`, `contextMenu.*`, `chip.*`, `badge.*`, or `overlay.*` token group in `canvas-theme.ts`. Only four groups exist: `canvas`, `node`, `toolbar`, `chart`. Those UI surfaces are composed from those tokens + the glass utilities + a handful of hardcoded literals (§1D/§1E). Build the Rust theme globals around canvas/node/toolbar, and encode the compositions as component styles.

2. Tailwind is v4 CSS-first: there is no `tailwind.config.*`. The "theme extension" is the `@theme inline` block in globals.css (§3B), which maps named utilities to the `:root`/`.dark` CSS vars (§3C/§3D). Port §3B+§3C+§3D together as the token layer.

3. Fonts are 100% remote (Baloo 2 via Google Fonts; Resource Han Rounded SC Regular+Bold via ZeoSeven 989). No local font files, no `@font-face`, no @fontsource. For an offline Rust app you must download/embed: Baloo 2 (400–800) and Resource Han Rounded SC (Regular + Bold). The exact UI stack is `"Baloo 2", "Resource Han Rounded SC", "Yuanti SC", "YouYuan", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif`; headings reuse it; `font-mono` is the Tailwind v4 default since it is never overridden.

Core palette anchor: brand `#D85C3A` (light) / `#F5885A` (dark); canvas bg `#F4F1EB` / `#15120E`; node panel `#FFFEFC` / `#1B1813`; selection accent is the hardcoded `#2f80ff` (nodes) and `canvas.selectionStroke` (`#8C8175`/`#d6d1c8`, marquee). `--hover` is theme-branded: `rgba(216,92,58,0.08)` light, `rgba(245,136,90,0.14)` dark.
</answer>

<next_steps>
1. Transcribe §1A/§1B directly into Slint theme globals (two theme structs keyed light/dark) — these are the authoritative canvas values.
2. Transcribe §3C/§3D as the global CSS-var equivalent, and port §3B as the utility→var alias table so `bg-brand`, `bg-brand-soft`, `border-brand`, `text-success/warning/danger/info`, `hover:bg-hover` resolve identically.
3. Port §2A/§2B/§2C/§2D as the AntD-equivalent token layer; if the Rust app does not use AntD, still keep the semantic alias names (brand/brandHover/brandActive, elevatedBg, itemHoverBg, itemSelectedBg...) since canvass components reference them indirectly through globals.css.
4. Bundle the two fonts (§4B) locally and register Baloo 2 + Resource Han Rounded SC (Regular/Bold); keep the fallback tail.
5. Encode the glass recipes (`.glass-card`/`.glass-surface`/`.glass-raised`) and §3F keyframes/timings as Rust animation specs; carry over the `prefers-reduced-motion` / `prefers-reduced-transparency` guards.
6. Reproduce §1D compositions (toolbar dock, popovers, minimap, ruler, wire, chip, mention menu, modals) and §1E hardcoded literals (`#2f80ff` selection blue, node minimap colors, checker/gradients) as explicit constants.
No follow-up needed — the inventory is complete for all five requested areas.
</next_steps>
</results>