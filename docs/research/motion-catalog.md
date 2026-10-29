[CONTEXT] I am porting D:\Dev\open-canvas\web (React canvas app) to Rust and must preserve ALL UX motion/animation exactly. Look across web/src/components/canvas/**, web/src/components/layout/**, web/src/styles/globals.css, web/tailwind.config.*, and web/package.json.

[GOAL] Produce a complete catalog of every animation/transition/micro-interaction so I can reproduce them in a Rust UI (Slint property animations/transitions/states or manual interpolation).

[DOWNSTREAM] I will map each motion to a Rust animation spec: property, duration, delay, easing curve, trigger.

[REQUEST] Enumerate with file:line provenance:
1. Animation libraries used (framer-motion? motion? react-spring? auto-animate?) — versions from package.json and every usage site.
2. Every transition/animation defined via CSS (transition-*, duration-*, ease-*, animate-*, keyframes in globals.css/tailwind config): element, property, duration, easing.
3. Hover/active/focus feedback patterns (hover:bg-hover, scale, shadow lift, opacity), and any color transitions.
4. Canvas-specific motion: node drag, panel/dock open-close, toolbar popover in/out, minimap, zoom/pan inertia, selection marquee, generation loading/progress, toast/notification enter-exit, modal enter-exit.
5. Any spring/tween parameters (stiffness/damping) or cubic-bezier curves with exact numbers.
Return a table: trigger | element | property | from->to | duration | easing | source. Be exhaustive; skip tests.
<!-- OMO_INTERNAL_INITIATOR -->

---

<analysis>
**Literal Request**: Enumerate every animation/transition/micro-interaction across web/src/components/canvas/**, web/src/components/layout/**, web/src/styles/globals.css, web/tailwind.config.*, and web/package.json, with file:line provenance.

**Actual Need**: A complete motion spec (property, from->to, duration, delay, easing, trigger) so it can be re-created in Rust/Slint — meaning CSS classes AND library usage AND inline styles all must be captured, not just one category.

**Success Looks Like**: A single exhaustive table keyed by trigger, backed by exact file:line, plus library versions and keyframe definitions, so the porter can map each row directly to an animation spec without further searching.
</analysis>

I'll begin by mapping the project structure and locating all the key config/style files in parallel.

---

Let me read the core config files and the one library usage site, plus hunt for the tailwind config and all animation utility classes.

---

No Tailwind config file (v4 uses CSS-first config). The animation library is `motion` v12 (`motion/react`). Now let me exhaustively gather all motion patterns, AntD motion tokens, and the canvas-theme definitions in parallel.

---

Motion library is used only in `canvas-side-panel.tsx`. Now let me extract all CSS transition/animation utility usages and AntD motion config.

---

Now the canvas-specific motion: node drag/return, batch entrance, marquee, zoom/pan. Let me read the key files and hunt for easing/inertia math and remaining animate utilities.

---

Let me read the viewport/canvas interaction core, minimap, connections, drag-preview hook, and the toolbar popovers.

---

Now for marquee/selection, dock, popovers, and toast/notification motion. Let me search project.tsx and the dock/toolbar files.

---

Let me read the side-panel store, hover toolbar, selection/floating toolbars, rulers, and AntD App provider config.

---

Let me capture the remaining layout components and the canvas toolbar popover triggers, plus any inline animation in project.tsx.

---

Let me do final verification sweeps: explicit durations/easings/active-focus states, and the direct-DOM signal stores (to confirm 1:1 drag with no easing).

---

Let me confirm the trigger logic for node entrance/return/dim and the drag ghost chip.

---

I have a complete picture. Here is the exhaustive catalog.

<results>
<files>
- D:\Dev\open-canvas\web\package.json - declares `motion` ^12.38.0, `tailwindcss` ^4, `tw-animate-css` ^1.4.0, `antd` ^6.4.2 (all animation sources)
- D:\Dev\open-canvas\web\src\styles\globals.css - all `@keyframes`, canvas named animations, glass classes, reduced-motion guards
- D:\Dev\open-canvas\web\src\components\canvas\canvas-side-panel.tsx - the ONLY `motion/react` (spring/tween) usage
- D:\Dev\open-canvas\web\src\stores\use-canvas-side-panel-store.ts - side-panel motion duration + unmount timing
- D:\Dev\open-canvas\web\src\components\canvas\canvas-node.tsx - node enter/return/dim, batch-child stagger, spinners, hover toolbars
- D:\Dev\open-canvas\web\src\components\canvas\open-canvas.tsx - pan/zoom (rAF composited, no easing), zoom marker
- D:\Dev\open-canvas\web\src\pages\canvas\project.tsx - drag ghost, marquee selection, toolbars (NOT in scope dir but is the canvas host)
- D:\Dev\open-canvas\web\src\components\canvas\dock\dock-panel.tsx - dock resize/split/drag previews + splitters
- D:\Dev\open-canvas\web\src\components\canvas\canvas-mini-map.tsx - minimap (no transitions)
- D:\Dev\open-canvas\web\src\components\canvas\canvas-connections.tsx - connection label entry animation + active draw
- D:\Dev\open-canvas\web\src\lib\app-theme.ts - AntD theme (NO motion tokens overridden)
- D:\Dev\open-canvas\web\src\components\layout\app-providers.tsx - AntD `App`/`ConfigProvider` wrapper (motion = AntD defaults)
- D:\Dev\open-canvas\web\src\components\canvas\canvas-floating-toolbar.tsx / canvas-selection-toolbar.tsx / canvas-node-hover-toolbar.tsx - floating bar + tooltip hover feedback
</files>

<answer>

# Motion catalog: open-canvas/web

## 0. Scope facts
- **No `tailwind.config.*` exists.** Tailwind v4 is CSS-first; the only config is `web/src/styles/globals.css` (`@import "tailwindcss"` + `@theme inline`).
- **Only one JS animation library: `motion` v12.38.0** (`motion/react`), used in exactly one file (side panel). No framer-motion, react-spring, auto-animate, motionone.
- **`tw-animate-css` v1.4.0** (globals.css:2) supplies `animate-in/fade-in/zoom-in/slide-in-*` utilities (used in `components/ui/select.tsx:74` and `pages/image/index.tsx:470`).
- **AntD v6.4.2** supplies Modal/Drawer/Dropdown/Popover/Tooltip/message animations with **unmodified library defaults** (`app-theme.ts` sets no `motion` token; `app-providers.tsx` wraps in `<App>`).
- Tailwind `transition` default = duration **150ms**, easing **cubic-bezier(0.4, 0, 0.2, 1)**, props: color/background/border/text-decoration/fill/stroke/opacity/box-shadow/transform/filter/backdrop-filter.
- Tailwind built-in `animate-spin` = **1s linear infinite**.

## 1. Easing curves / parameters found (exact numbers)
| Name | Value | Used by | Source |
|---|---|---|---|
| Canvas standard | `cubic-bezier(0.2, 0.85, 0.18, 1)` | node enter/return, batch child, connection label | globals.css:749,776,843; canvas-node.tsx:657 |
| Side-panel ease | `cubic-bezier(0.22, 1, 0.36, 1)` | motion `transition.ease` | canvas-side-panel.tsx:12,53,60 |
| Tailwind default | `cubic-bezier(0.4, 0, 0.2, 1)` | all bare `transition` | globals.css:1 (tailwind) |
| ease-out | `cubic-bezier(0, 0, 0.2, 1)` | popover-in, drag-ghost-in | globals.css:739,808 |
| ease / ease-in-out / linear | standard | prompt-filter-tag, aurora, spinners | globals.css:244,287 |

**No springs exist anywhere.** No `stiffness`/`damping`/`type:"spring"`; the only motion tween is duration+ease. No custom `cubic-bezier` beyond the two above.

## 2. `@keyframes` + named animation classes (all in globals.css)
| Trigger | Element/Class | Property | from -> to | Duration | Easing | Source |
|---|---|---|---|---|---|---|
| Always (AI page title) | `.ai-title-aurora` | background-position, filter drop-shadow | 0%: pos 0%, shadow 0 → 50%: pos 100%, shadow 0 12 28 → 100% | 7000ms, infinite | ease-in-out | globals.css:238-264 |
| Image node content becomes available | `.canvas-node-enter` | opacity, transform | opacity 0 + scale .88 + translateY(10px) → 70%: scale 1.015 → none | 420ms | cubic-bezier(.2,.85,.18,1) | globals.css:748-767; trigger canvas-node.tsx:158-165 (460ms flag) |
| Node drag cancelled (returns) | `.canvas-node-return` | transform translate | (--node-return-from-x/y) → (--node-return-to-x/y) | 200ms | cubic-bezier(.2,.85,.18,1) | globals.css:775-787; canvas-node.tsx:335,343-346 |
| Batch images/text expand | inline animation `canvas-batch-child-in` | opacity, transform translate/rotate/scale | opacity 0 + translate(--batch-from) + rotate(--batch-from-rotate) + scale(.72) → 72%: rotate(-1deg) scale(1.015) → scale(1) | 320ms, delay `index*35ms` | cubic-bezier(.2,.85,.18,1) | globals.css:563-578; canvas-node.tsx:654-657 |
| Popover opens | `.canvas-image-settings-popover`, `.canvas-node-layer-popover`, `.canvas-smart-canvas-settings-popover`, `.canvas-text-style-popover` | opacity, transform | opacity 0 + translateY(4px) + scale(.98) → none | 160ms | ease-out | globals.css:735-740,795-805 |
| Drag-out ghost chip appears | `.canvas-drag-ghost` | opacity, scale | opacity 0 + scale .8 → opacity 1 + scale 1 | 150ms | ease-out | globals.css:807-821; project.tsx:2159 |
| Connection reference label appears | `.canvas-connection-label-in` | opacity, transform scale | opacity 0 + scale .6 → none | 220ms | cubic-bezier(.2,.85,.18,1) | globals.css:842-858; canvas-connections.tsx:58 |
| Reference chip drag (CSS transition, not keyframe) | `.canvas-prompt-ref-chip` / `-dragging` | transform, opacity | normal → translateY(-3px) scale(1.08), opacity .55 | 160ms | ease | globals.css:833-840; prompt-node-panel.tsx:93-101 |
| Search-filter tag hover/active | `.prompt-filter-tag` | background, border-color, box-shadow, color | → hover rgba(28,25,23,.06); active #292524 | 150ms | ease | globals.css:287-305 |
| Hover scroll hint | `.hover-scrollbar-hint::after` | opacity | 0 → 1 | 180ms | ease | globals.css:374-390 |
| Agent code-block actions hover | `.agent-streamdown [data-streamdown=code-block-actions]` | opacity | 0 → .58 | 150ms | ease | globals.css:652-658 |
| Zoom/drag active | `[data-canvas-zooming]/[data-canvas-dragging] .glass-*` | backdrop-filter | blur → none (instant) | 0 | — | globals.css:218-226,742-746 |
| Reduced motion | multiple selectors | all | disabled | — | — | globals.css:769-773,789-793,823-831,860-869 |
| View transitions | `::view-transition-old/new(root)` | animation | disabled | — | — | globals.css:582-586 |

## 3. motion/react (side panel) — the only JS animation
| Trigger | Element | Property | from -> to | Duration | Easing | Source |
|---|---|---|---|---|---|---|
| Panel open/close | `motion.div` wrapper | width, opacity | {width:0,opacity:0} → {width: `width+1`, opacity:1} (0 when closed) | 500ms (`CANVAS_SIDE_PANEL_MOTION_MS`); 0 while resizing or reduced-motion | cubic-bezier(0.22,1,0.36,1) | canvas-side-panel.tsx:11-12,49-55; store:3 |
| Panel close slide | `motion.aside` | x | initial -48 → 0 (open); → -28 while closing | same 500ms / 0 on reduced | same ease | canvas-side-panel.tsx:56-61 |
| Unmount | store | mounted flag | closing true → setTimeout(500ms) → unmount | 500ms | — | use-canvas-side-panel-store.ts:44-51 |

## 4. Generic Tailwind transition / micro-interaction patterns (canvas + layout)
Applied at ~150 sites; all use Tailwind default 150ms / cubic-bezier(0.4,0,0.2,1) unless noted.

| Trigger | Element | Property | from -> to | Duration | Easing | Source (representative) |
|---|---|---|---|---|---|---|
| Hover any button/row | `hover:bg-hover` | background-color | transparent → `var(--hover)` (brand tint 8%/14%) | 150ms | default | ~120 sites; canvas-switcher.tsx:13, canvas-top-bar.tsx:65, ps-layers-panel.tsx:33-35 |
| Hover icon-only | `hover:text-foreground` / `hover:text-white` | color | muted → foreground | 150ms | default | canvas-account-usage.tsx:91, user-status-actions.tsx:20, app-top-nav.tsx:43 |
| Hover card title | `group-hover:text-brand` | color | foreground → brand | 150ms | default | canvas-project-row.tsx:106 |
| Hover reveal | `group-hover:opacity-100` / `focus-within:opacity-100` | opacity | 0 → 1 | 150ms | default | canvas-project-row.tsx:142,153,181; assets-node-content.tsx:259,308,404; image-stack-preview.tsx:47; image-stack-node-content.tsx:81 |
| Hover reveal | `group-hover:pointer-events-auto` | pointer-events | none → auto | — | — | image-stack-preview.tsx:47 |
| Hover scale (button lift) | `hover:scale-[1.02]` | transform scale | 1 → 1.02 | 150ms | default | canvas-node.tsx:29,536 |
| Hover scale (grip dot) | `hover:scale-125` | transform scale | 1 → 1.25 | (transition-all) 150ms | default | canvas-node.tsx:851 |
| Hover opacity | `hover:opacity-90` / `-80` | opacity | 1 → .9/.8 | 150ms | default | canvas-config-node-panel.tsx:72; ps-asset-panels.tsx:88; image-stack-node-content.tsx:64 |
| Hover underline (title edit) | `hover:border-current` | border-color | transparent → currentColor | 150ms | default | canvas-node.tsx:384 |
| Batch expand chevron | `transition-transform` + `rotate-90` | transform rotate | 0deg → 90deg | 150ms | default | canvas-node.tsx:625 |
| Batch frame slide | `transition-all duration-300 group-hover/batch:translate-x-1` | transform translateX | 0 → 4px (on hover) / all props | 300ms | default | canvas-node.tsx:803 |
| Resize corner opacity | `transition-opacity duration-150` | opacity | .95 → 1 (active) | 150ms | default | canvas-node.tsx:836,846 |
| Node shadow/opacity | inline `transition: box-shadow 200ms ease, opacity 160ms ease` + `transition-shadow duration-200` | box-shadow, opacity | selected lift / drag dim to opacity .25 | 200ms shadow / 160ms opacity | ease | canvas-node.tsx:335,341,146 (dragDimmed) |
| Splitter hover | `before:transition hover:before:bg-hover` | background | transparent → hover | 150ms | default | dock-panel.tsx:68 |
| Dock tab hover (JS) | inline background set on mouseenter/leave | background | transparent ↔ `theme.toolbar.itemHover` | 150ms (transition class) | default | dock-tabs.tsx:59-67 |
| Dock tab active | inline style | background/color/box-shadow | → accentBg/accentText + inset ring | 150ms | default | dock-tabs.tsx:60 |
| Left-nav active pill | inline style | background/color/box-shadow | → accentSoft/accent + inset ring | 150ms | default | app-top-nav.tsx:61-70 |
| Align menu row hover (JS) | inline background | background | transparent ↔ itemHover | 150ms | default | canvas-selection-toolbar.tsx:137-141 |
| Disabled states | `disabled:opacity-*`, `has-[:disabled]:opacity-35` | opacity | 1 → .25–.40 | 150ms | default | canvas-config-node-panel.tsx:72, ps-layers-panel.tsx:33 |
| Drag affordance | `active:cursor-grabbing` | cursor | grab → grabbing | — | — | assets-node-content.tsx:244,300,396 |
| Selection / active token | `theme.node.accentSoft` bg + `theme.node.accent` text + inset ring | background/color/box-shadow | per class | 150ms | default | canvas-node-layer-popover.tsx:107, ps-paths-panel.tsx:96, ps-asset-panels.tsx:289 |
| Spinner (all loading) | `animate-spin` | rotate | 0 → 360deg | 1000ms, infinite, linear | linear | canvas-node.tsx:523,687; image-stack-preview.tsx:82; image-stack-node-content.tsx:58; canvas-toolbar.tsx:134; canvas-import-dialog.tsx:124; canvas-account-usage.tsx:73; ps panels |

## 5. Canvas-specific motion
| Trigger | Element | Property | from -> to | Duration | Easing | Source |
|---|---|---|---|---|---|---|
| Wheel zoom | canvas content transform + grid bg | translate/scale, background-size/position | direct DOM write, factor `1.1^(Δ/100)`, k clamped 0.05–5, rAF-throttled | 0 (per frame, 1:1) | none | open-canvas.tsx:111-154 |
| Wheel zoom marker | container dataset `canvasZooming` | enable flag (kills glass blur) | set on wheel, cleared after idle | 160ms debounce | — | open-canvas.tsx:121-127 |
| Drag pan | canvas content transform | translate | 1:1 with pointer delta (threshold 3px), rAF-throttled | 0 | none | open-canvas.tsx:156-239 |
| Pan cursor | container | cursor | grab → grabbing | — | — | open-canvas.tsx:256,178 |
| Node drag | `.node-element` transform translate | x,y | preview position written via signal; 1:1 | 0 | none | project.tsx:1083-1085; canvas-node.tsx:338 |
| Node drag dim | node root | opacity | 1 → .25 (`dragDimmed` when ghost-dragging) | 160ms | ease | canvas-node.tsx:146,335,341 |
| Drag ghost chip | `.canvas-drag-ghost` fixed | left/top follow pointer | -9999,-9999 → pointer x,y | 0 (follow) + 150ms entry | ease-out (entry) | project.tsx:1088-1092,2156-2169 |
| Batch-expand stagger | expanded cards | see §2 batch-child | — | 320ms + i·35ms | cubic-bezier(.2,.85,.18,1) | canvas-node.tsx:657 |
| Marquee selection | `<rect>` in SVG overlay | x,y,w,h | updated per pointermove | 0 (no transition) | none | project.tsx:1974-1984 |
| Marquee visuals | rect | fill/stroke/dash | `selectionFill`, `selectionStroke`, dash `6/k 4/k` (static) | — | — | project.tsx:1984 |
| Multi-select box | SVG rect | rx/ry 16, dash `7 5` (static) | — | — | — | canvas-selection-toolbar.tsx:78-91 |
| Active connection draw | `<path>` | strokeDasharray `5,5` (static, no marching) | — | — | — | canvas-connections.tsx:98 |
| Dock panel resize | column width/height | size | pointer 1:1 via local `preview` state | 0 | none | dock-panel.tsx:127-141,218 |
| Dock split move | group flex-basis | ratio | pointer 1:1 via `splitPreview`, clamped 0.2–0.8 | 0 | none | dock-panel.tsx:143-165 |
| Dock panel drag | drop zones + floating label | background / position | zone highlight on target; label follows pointer at +12,+12 | 0 | none | dock-panel.tsx:176-196,261-284 |
| Minimap pan | minimap viewport rect | x,y position | 1:1 pointer drag, no transition | 0 | none | canvas-mini-map.tsx:85-138 |
| Rulers | ticks | position | re-render per viewport signal, no transition | 0 | none | canvas-rulers.tsx |
| Node/connection drop target | outline/boxShadow | outline, box-shadow | 0 → 2px outline / 1px ring | 150ms | default | canvas-node.tsx:398-406 |

## 6. Overlays / modal / toast enter-exit
| Trigger | Element | Motion | Source |
|---|---|---|---|
| AntD message (App.useApp) | `.ant-message-notice` | library default (fade + slide-down), default 3s; image-tool progress uses `duration:0` | app-providers.tsx:49; project.tsx:1528-1670; canvas-import-dialog.tsx:39-91 |
| AntD notification | `.ant-notification-notice` | library default (slide-in from edge + fade) | globals.css:545 (border override only) |
| AntD Modal (default) | `.ant-modal` | library default zoom + fade (motion `zoom-big`) | canvas-import-dialog.tsx:107; canvas-node-resolution-dialog.tsx:173; canvas-image-analysis-dialog.tsx:67 |
| Modal with animation SUPPRESSED | crop / mask-edit / split dialogs | `transitionName="" maskTransitionName=""` → instant | canvas-node-crop-dialog.tsx:74; canvas-node-mask-edit-dialog.tsx:255; canvas-node-split-dialog.tsx:153 |
| AntD Drawer | `.ant-drawer` | library default slide from placement (left) | mobile-nav-drawer.tsx:18 |
| AntD Popover/Dropdown/Tooltip | `.ant-popover`/`.ant-dropdown`/`.ant-tooltip` | library default zoom/fade; Tooltip `mouseEnterDelay=0.2` | canvas-account-usage.tsx:80; canvas-floating-toolbar.tsx:26; canvas-project-row.tsx:119; dock-tabs.tsx:75 |
| Custom canvas popovers | image-settings / node-layer / smart-canvas-settings / text-style | `canvas-popover-in` 160ms ease-out (§2) | globals.css:735-740 |
| Radix select (ui) | `data-open/closed` | tw-animate-css `animate-in fade-in-0 zoom-in-95` + `slide-in-from-*` (200ms default) | components/ui/select.tsx:74 |
| Fade-in badge | `animate-in fade-in` | tw-animate-css default | pages/image/index.tsx:470 |
| AntD segmented/tabs/slider | — | library default (tabs ink-bar, segmented thumb slide) | throughout canvas dialogs |

## Notes for the Rust port
- Two custom beziers to reproduce exactly: **(0.2,0.85,0.18,1)** (canvas node/batch/connection) and **(0.22,1,0.36,1)** (side panel).
- Tailwind default **(0.4,0,0.2,1) @150ms** is the baseline for nearly every hover/color transition.
- Pan/zoom/marquee/minimap/dock-resize are **1:1 with pointer (no easing, no tween)** — in Slint use direct property binding, not `animate`.
- The node entrance (`.canvas-node-enter`, 420ms) and image-stack/batch stagger are the signature canvas motions.
- AntD defaults (Modal/Drawer/Dropdown/message) must be re-created manually in Rust; sources are library, not repo, so exact numbers aren't in code — replicate AntD v6 defaults (Modal fade+zoom-big, Drawer slide, message fade+slide-down, 3s auto-close).
</answer>

<next_steps>
1. Start the Rust port with the named `@keyframes` in globals.css (lines 238-264, 563-578, 748-787, 795-821, 842-858) — these are the only bespoke keyframes and each maps cleanly to a Slint `PropertyAnimation` or a `states` transition.
2. For the side panel, use `duration: 500ms`, `easing: cubic-bezier(0.22, 1, 0.36, 1)`, animating `width: 0 -> W+1`, `opacity: 0 -> 1`, `x: -48 -> 0` (closing x -> -28), with reduced-motion => duration 0.
3. Reproduce the ~150 Tailwind hover transitions as a shared 150ms/`cubic-bezier(0.4,0,0.2,1)` color/opacity/shadow spec; map `hover:bg-hover` to the `--hover` token values (light `rgba(216,92,58,.08)`, dark `rgba(245,136,90,.14)`).
4. Implement pan/zoom/drag/marquee/minimap as direct property updates (no interpolation), with the zoom factor `1.1^(deltaY/100)` and k clamp `[0.05, 5]`, plus the 160ms "zooming" flag that disables backdrop blur.
5. For overlays, port AntD v6 default motions (Modal zoom-big+fade, Drawer slide, Dropdown/Popover/Tooltip zoom-fade, message fade+slide-down 3s); three dialogs (crop/mask/split) intentionally have NO enter animation.
6. Verify the two remaining unread hosts if you need toolbar popover behavior in detail: `canvas-toolbar.tsx` (createMenu/extPanel/zoomPanel are absolutely-positioned conditionals, no enter animation) and `canvas-switcher.tsx` (list reveals via group-hover).
</next_steps>
</results>

Note: all paths above are Windows-absolute (`D:\...`) because the workspace is on `win32`; that is the actionable absolute path in this environment.