# OpenCanvas Desktop — Rust Port Spec

把 `D:\Dev\open-canvas\web`（React + Vite canvas 应用）移植为原生 Rust 桌面程序，
**1:1 保留 UI 设计、配色、字体、UX 动效**。新工程：`D:\Dev\open-canvas-desktop`。

> 状态：Phase 0（规格 + 骨架）。实现分 5 阶段，见 §6。
> 逐字段的原始证据在同目录 `research/` 下（四份侦察报告，带 file:line 出处）。

---

## 1. 源项目事实（已核实）

| 维度 | 结论 |
|---|---|
| 框架 | React 19 + Vite 7 + TypeScript；`web/src/**`（~200 源文件） |
| 样式 | **Tailwind v4 CSS-first**——**没有 `tailwind.config.*`**；token 在 `web/src/styles/globals.css` 的 `@theme inline` + `:root`/`.dark` CSS 变量 |
| 组件库 | Ant Design 6（Dropdown/Menu/Select/Modal/Popover/Tooltip），token 在 `web/src/lib/app-theme.ts` |
| 状态 | Zustand（`web/src/stores/**`），localforage/IndexedDB 持久化 |
| 图标 | lucide-react（SVG） |
| 动效 | **`motion` v12**（仅 `canvas-side-panel.tsx` 一处 tween）+ `tw-animate-css` + Tailwind `transition`（~150 处）+ 少量 `@keyframes` |
| 字体 | **Baloo 2**（Google，400–800）+ **Resource Han Rounded SC**（zeoseven 989，Regular+Bold）；全部远程，无本地字体文件 |

### 1.1 画布渲染方式（关键）

**不是 canvas-2D，也不是 ReactFlow——是自定义 DOM + SVG：**

- **平移/缩放**：手势期间把 `transform: translate() scale()` 直接写到 DOM（不逐帧 re-render），松手才提交状态；缩放因子 `1.1^(deltaY/100)`，`k` 钳制 **0.05–5**，光标锚定。
- **网格**：CSS `background-image`（点/线/空白），逐手势更新 `backgroundSize/Position`；`GRID_SIZE = 48`（画布）/ `CANVAS_GRID_SIZE = 16`（吸附）。
- **节点**：变换层内绝对定位 `<div>`，`transform: translate(x,y)`；卡片 `rounded-3xl`（22px）、`border-2`。
- **连线/选择框/吸附线**：单个 10000×10000 `<svg>` 图层，三次贝塞尔路径。
- 侧栏 / dock / 弹层用 `motion/react` 与 CSS 过渡。

> 这决定了 Rust 侧：需要「保留式场景 + 可绘制自身变换的容器」（平移缩放直接改属性，**不要**用补间动画）；连线用矢量路径；节点是可复用卡片组件。

---

## 2. 设计系统（必须逐值复刻）

权威来源：`research/design-system.md`（含 light/dark 全量 token 与 file:line）。下列为速查锚点。

### 2.1 画布 token（`web/src/lib/canvas-theme.ts` → `canvasThemes`，仅 4 组：`canvas` / `node` / `toolbar` / `chart`）

| token | light | dark |
|---|---|---|
| canvas.background | `#F4F1EB` | `#15120E` |
| canvas.dot | `rgba(58,48,40,.22)` | `rgba(245,242,238,.26)` |
| canvas.line | `rgba(58,48,40,.10)` | `rgba(245,242,238,.11)` |
| canvas.selectionStroke | `#8C8175` | `#d6d1c8` |
| canvas.selectionFill | `rgba(28,25,23,.07)` | `rgba(250,249,247,.10)` |
| node.fill | `#FFFFFF` | `#201D18` |
| node.panel | `#FFFEFC` | `#1B1813` |
| node.stroke | `#E8E2D8` | `#3A342C` |
| node.activeStroke | `#3A342D` | `#efe9df` |
| node.text | `#26221D` | `#faf8f5` |
| node.muted | `#6B635A` | `#C4BDB2` |
| node.label | `#4A443C` | `#E7E1D8` |
| node.accent | `#D85C3A` | `#F5885A` |
| node.accentText | `#B8451F` | `#FBAE82` |
| node.accentSoft | `#FBEAE2` | `rgba(245,136,90,.16)` |
| node.success/warning/info/danger | `#15803d`/`#96620a`/`#0f766e`/`#b91c1c` | `#4ade80`/`#fbbf24`/`#2dd4bf`/`#f87171` |
| toolbar.panel | `rgba(255,254,252,.86)` | `rgba(27,24,19,.86)` |
| toolbar.activeBg / activeText | `#EFE9DF` / `#26221D` | `#2E2921` / `#faf8f5` |
| toolbar.surfaceShadow | `0 14px 34px rgba(28,25,23,.26), 0 2px 6px rgba(28,25,23,.18)` | `0 16px 40px rgba(0,0,0,.62), 0 2px 8px rgba(0,0,0,.5)` |

（`node.*` 还含 primary/primaryText/placeholder/faint/secondary/blocked 及各自 `*-soft`；完整见 research 文档。）

### 2.2 全局 / Tailwind 命名色（`globals.css`）

| 语义 | light | dark |
|---|---|---|
| background / foreground | `#F4F1EB` / `#26221D` | `#15120E` / `#faf8f5` |
| brand / brand-foreground | `#D85C3A` / `#ffffff` | `#F5885A` / `#1e1c19` |
| brand-soft | `#FBEAE2` | `rgba(245,136,90,.16)` |
| **hover（交互反馈统一色）** | **`rgba(216,92,58,.08)`** | **`rgba(245,136,90,.14)`** |
| border / input | `#E8E2D8` | `#3A342C` |
| card / popover | `#FFFEFC` | `#1B1813` |
| muted / muted-foreground | `#F1ECE4` / `#6B635A` | `#201D18` / `#c8c2b8` |
| glass / glass-strong | `rgba(255,254,252,.72)` / `.9` | `rgba(27,24,19,.68)` / `.88` |

**圆角**：`--radius = 10px`；sm6 / md8 / lg10 / xl14 / 2xl18 / 3xl22 / 4xl26。AntD：XS4 / SM6 / base8 / LG12。节点卡 `rounded-3xl`=22px。
**交互态统一**：选中 = `accentSoft` 底 + `accent` 文字（需要描边用 `inset 0 0 0 1px accent`）；hover = `--hover`。
**硬编码常量**（不在 token 里，必须照抄）：节点选中蓝 `#2f80ff`；各节点小地图色（text `#`none / prompt `#eab308` / video-prompt `#fb923c` / image `#10b981` / video `#f97316` / image-generation `#f472b6` / video-generation `#fb7185` / assets `#64748b`）。

### 2.3 玻璃（glass）配方
- `.glass-card`：bg `--glass`，`blur(16px) saturate(150%)`，边框 `color-mix(currentColor 12%, transparent)`。
- `.glass-surface`：blur(20px) saturate(160%)，边框 12%。
- `.glass-raised`：bg `--glass-strong`，blur(24px) saturate(160%)，边框 14%，`box-shadow 0 12px 34px color-mix(currentColor 12%, transparent)`。
- 缩放/拖拽时 `backdrop-filter: none`；`prefers-reduced-transparency` 时退化为纯色 `--card`。
- 等价 Slint 说明：无原生 backdrop-blur，用**半透明填充 + 近似阴影**复刻（记录为已知偏差）。

### 2.4 字体
- UI/标题栈：`"Baloo 2", "Resource Han Rounded SC", "Yuanti SC", "YouYuan", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif`。
- `font-mono` 未覆盖 → Tailwind v4 默认 `ui-monospace, SFMono-Regular, Menlo, ...`。
- 文本节点可选族：System / Sans / Serif / Mono / Rounded（`lib/canvas/text-style.ts`）。
- 需**本地内嵌** Baloo 2（400–800）与 Resource Han Rounded SC（Regular/Bold）；中文栈靠系统回退。

---

## 3. UI 结构（移植蓝本）

权威来源：`research/ui-inventory.md`。画布屏（`pages/canvas/project.tsx`）分区与 z 序：

```
user-layout (flex h-dvh)
├─ AppTopNav            sticky h-14 glass-surface —— 编辑器路由下自动隐藏
└─ CanvasWorkspace
   ├─ CanvasTopBar      absolute top-0 h-14 z-80  —— 左：侧栏开关/Home/可编辑标题；中：工作区页签；右：用户操作
   ├─ CanvasSidePanel   左列 width 150±, z-60, glass-surface, 可拖拽改宽（220–480），含 CanvasSwitcher
   └─ Canvas section (flex-1)
      ├─ OpenCanvas     网格 + 变换层(节点) + SVG 层(连线/框选/吸附线)
      ├─ CanvasToolbar  absolute bottom-5 居中 h-14 dock（导出/工具/撤销/重做/输入组/提示词组/生成组/扩展/缩放/节点列表/删除）
      ├─ CanvasRulers   z-75（左 20px + 顶，顶栏下 56px）
      ├─ Minimap        bottom-24 left-6 z-50, 240×160
      ├─ NodeListPanel  bottom-84 居中 w-250
      ├─ Hover/Selection toolbar  z-70
      └─ Modal/Popover 栈
```

**节点卡片解剖**（`canvas-node.tsx`）：标题在卡上方（媒体节点改用 `MediaInfoBar`）；卡体 `rounded-3xl border-2`（非媒体为玻璃卡）；内容按 type 分派；4 角缩放手柄（解锁时）+ 右下 grip（hover/选中）；左右连接点（`size-12` 命中区）；角标（锁/批次）；节点下 `w-600` 面板。
**连线**（`canvas-connections.tsx`）：右侧中点 → 左侧中点，曲率 `max(|dx|*0.5, 50)`；激活 drag 虚线 `5,5`。
**节点面板**：prompt / config / prompt-node / video-prompt(帧槽) / assets / image-stack，及插件面板。

**核心 vs 次要**：核心=顶栏/侧栏/画布/工具栏/节点卡/连线/悬浮条/节点面板/预览弹窗/Mention 输入/节点注册表/插件宿主/配置弹窗（最小 providers+models）/主题 token。次要=标尺/小地图/节点列表/多选工具栏/裁剪等图像编辑弹窗/版本弹窗/移动端抽屉。**Phase 5**=Image Studio（12 dock 面板）、Pixel Studio（6 面板）、Texture Studio（WebGL）、插件运行时、AI 生成。

---

## 4. 动效规范（必须复刻）

权威来源：`research/motion-catalog.md`。要点：

- **两条自定义缓动 + 一条默认**：画布标准 `cubic-bezier(.2,.85,.18,1)`；侧栏 `cubic-bezier(.22,1,.36,1)`；Tailwind 默认 `cubic-bezier(.4,0,.2,1)` @150ms（几乎所有 hover/颜色过渡）。
- **无 spring**——只有 duration+ease 的 tween。
- 关键 keyframes：`canvas-node-enter`（420ms）、`canvas-node-return`（200ms）、`canvas-batch-child-in`（320ms + i·35ms）、`canvas-popover-in`（160ms ease-out）、`canvas-drag-ghost-in`（150ms ease-out）、`canvas-connection-label-in`（220ms）。
- 侧栏开合：500ms `cubic-bezier(.22,1,.36,1)`，width `0→W+1`、opacity `0→1`、x `-48→0`；关闭 x→-28；reduced-motion → 0。
- **1:1 直连（不要补间）**：平移/缩放/节点拖拽/框选/小地图/dock 改尺寸——直接绑定属性。
- 遮罩层动效 = AntD v6 默认（Modal zoom-big+fade、Drawer slide、Dropdown/Popover/Tooltip zoom-fade、message fade+slide-down 3s）；crop/mask/split 三个弹窗**刻意无入场动画**（`transitionName=""`）。
- 全局 hover 反馈统一 `hover:bg-hover`（`--hover`）；载体 ~150 处。

> Rust 侧：把上表逐行映射为「属性动画 / 状态过渡 / 直接绑定」三类。direct 类用属性绑定，其余用带缓动的动画。

---

## 5. 领域模型（UI 无关，可单测）

权威来源：`research/state-and-domain.md`。要点：

- 文档 = `CanvasProject { id, title, createdAt, updatedAt, groupId?, nodes: Vec<CanvasNode>, connections: Vec<Connection>, viewport: Viewport{x,y,k}, scope? }`。
- `CanvasNode { id, type: NodeTypeId, title, position, width, height, metadata: Option<NodeMetadata> }`；`type` = 9 个内置 + 插件 `"<pluginId>:<name>"` 开放串。
- `NodeMetadata` = 一个大结构，涵盖所有 kind 字段（status/prompt/images/videoSlots/生成参数/交互标量 flags…）——**逐字段对齐 `types/canvas.ts:209–261` 以保证 serde 无损**。
- `Connection { id, fromNodeId, toNodeId, relation? }`；方向规则：generation 是 sink，Image 不能作 target；z 序 = 数组顺序。
- 尺寸规则：`NODE_SPECS`（image 340×240…assets 360×320）；`fitNodeSize(maxW/H=640)`；`nodeSizeFromRatio`（比例 0.25–4）。
- 历史：快照仅 `{nodes, connections}`，180ms 防抖提交上一态，上限 50；undo/redo 后清空选择。
- 持久化：项目库 `open-canvas:canvas_store`；二进制 blob `image:<id>` / `<prefix>:<id>`；导出 ZIP = `projects.json`（`app:"open-canvas"`, `version:3`）+ `projects/<id>/files/**`。
- 交互：工具 `select`/`pan`（默认 pan，Ctrl/Space 临时切换）；滚轮缩放、背景拖拽平移/框选、手柄拖拽连线、Ctrl+右键笔画切断连线；快捷键 Ctrl+Z/Shift+Z/Y、Ctrl+A/C/V、Delete/Backspace、Escape。
- 外部服务（Phase 5）：图片/视频/视觉-OCR 生成（浏览器直连 OpenAI 兼容 HTTP，key 本地）；本地计算：imgly 去背景 + MobileSAM 分割（WASM）、裁剪/拆分/放大、PS/Pixel 合成。

**Rust 建议布局**：`CanvasNodeType` = 内置 enum + 插件 newtype 兜底；`NodeMetadata` 全 `Option`；`History` 用 `VecDeque`；持久化 = 每项目一个 JSON 文件 + `<project>/files/` blob 目录（导出改回 ZIP）。

---

## 6. 分阶段计划

| 阶段 | 内容 | 验收 |
|---|---|---|
| **P0 骨架** | Cargo 工程、本 spec、字体内嵌、主题 token 模块、CI 可编译 | `cargo run` 起窗口，light/dark 可切 |
| **P1 外壳** | 主题系统（light/dark）、AppTopNav/CanvasTopBar/侧栏（含动画）/工具栏 dock、画布网格 + 平移缩放 | 与网页并排截图，配色/字体/圆角一致 |
| **P2 画布** | 节点卡解剖、9 内置 kind 渲染、选中/拖拽/缩放、贝塞尔连线、框选、吸附线、节点入场动效 | 能建/移/连/删节点 |
| **P3 面板与浮层** | 节点 prompt/config 面板、设置 popover、悬浮/多选工具条、小地图、标尺、节点列表、预览弹窗、mention 输入 | 交互与动效对拍 |
| **P4 数据** | 项目库、保存/加载 JSON、undo/redo、快捷键、导出/导入 | 文档往返无损 |
| **P5 后续** | Image/Pixel/Texture Studio、插件运行时、AI 服务 | 逐块追加 |

### 6.1 P5 细分（已定）

| 阶段 | 内容 | 状态 |
|---|---|---|
| **P5a** | AI 服务层（`crates/ai`：reqwest + tokio + SSE + 轮询 + 重试）+ provider/model 配置 + 设置弹窗 + Generate 接线 | 完成 |
| **P5b** | Pixel Studio（`crates/pixel` 栅格引擎 + Pixel 工作区 UI + 持久化 + PNG/精灵表导出） | 进行中 |
| **P5c** | Image Studio（PS 式，~9.7k LOC：图层/通道/路径/历史/调整/滤镜/颜色 + dock） | 待做 |
| **P5d** | Texture Studio（117 类型 + 17 后效，GLSL→wgpu 重写；CPU 部分：梯度 LUT / Sobel 法线 / 通道打包 / 精灵表 / GIF） | 待做 |
| **P5e** | 原生插件/节点 ABI + 原生移植 5 个官方插件（html/markdown/svg/sticky-note/panorama）；`rquickjs` 仅用于 model-plugin 请求脚本 | 待做 |

> **验证规则（用户要求，全局生效）**：一律用代码（headless）验证——单测/集成测试 + 在 Rust 内渲染成缓冲/PNG 后对像素与尺寸断言；**不驱动 GUI、不截图**。

---

## 7. 保真度风险（先声明）

1. **玻璃虚化**：Slint/egui 无 `backdrop-filter`，用半透明+近似阴影替代（已知偏差）。
2. **文本排版/CJK shaping**：无浏览器排版引擎，换行/字距/子像素抗锯齿会有差异。
3. **富文本**：streamdown(markdown)、tiptap、codemirror 在 native 无对等物，需自研或降级。
4. **图标**：lucide 是 SVG（~1000 个），需预栅格化或选支持 SVG 的方案。
5. **视频**：native 无内置播放，需 ffmpeg/GPU 路径。
6. **WebGL 效果**：texture studio 着色器需 wgpu 重写。
7. **AntD 默认动效**数值不在仓库内，需按 AntD v6 观感复刻。
8. **芯片输入（mention chips，P3c）**：Slint `TextInput` 无内联子元素，引用芯片按网页同样的序列化写回文本（标签内联），并在输入框上方以芯片条呈现；节点卡参考芯片的 160ms 抬升效果由悬停回放（Web 为 HTML5 拖拽 `canvas-prompt-ref-chip-dragging`）；输入芯片的图像缩略图降级为文字 pill；`metadata.references` 存被引用节点 id（与 `videoSlots` 的「存节点 id 不存载荷」约定一致；网页该字段存图片 URL，供 P5 生成使用）。
9. **视频节点内容**：native 无视频解码器，**视频播放明确不实现（out of scope）**。节点卡渲染设计过的海报位：`metadata.thumbnail`（可解码的 data URL）优先，回落 `metadata.content`（同 `CanvasImage` 的 `thumbnail || content` 顺序），海报以 `contain` 叠在 letterbox 底色上，中央为胶片图标，底部信息条给出时长（`metadata.durationMs`）与状态词（Ready/Generating/Failed）；无海报时显示空视频占位（`Empty video node`）。src/tests 见 `crates/app/src/render_tests.rs` 的像素断言。
10. **图片节点位图**：`metadata.content`（data URL 或裸 base64）经 `open-canvas-pixel::decode_png` 解码为 RGBA，按内容哈希缓存（最多 48 张，单张文本 ≤ 12 MB，命中即复用，行重建不重复解码）；`freeResize` 用 `fill`、否则 `contain`（`canvas-node.tsx:699`）。不可解码/超限的内容显示空状态而不阻塞主线程；`metadata.thumbnail` 可解码时优先（缓存同时覆盖缩略图与全图）。
11. **Assets 节点内容**：桌面端没有文件夹绑定与浏览器扩展缓存数据源，列表按 `metadata.assetStudioProjects` 在本地项目库中解析（名称 + 分组 + 图像/像素类型图标），已删除项目以 `Missing project` 降级显示；资产自带位图时渲染缩略图，否则用类型图标（对应 web 的 studio 源）。文件夹/缓存源与预览弹窗不在本次范围。
12. **批量文本（`metadata.texts`）**：文本卡显示主文本 + 数量徽标（`N options`），web 的 `BatchFrame` 展开多卡与「设为 primary」暂未实现。
13. **图片信息条**：`标题 · W x H · 字节 · 成本` 与 web `MediaInfoBar` 一致，替代有内容图片节点的标题（`!hasImageContent` 才显示标题）；桌面端没有供应商成本记录，成本一栏恒为客户端估价（`~$x.xx`，`open-canvas-ai::generation_cost`，与 web 的 estimate 源同格式），未定价模型不显示该段。
14. **国际化（i18n）**：本轮引入 `Locale`（`en-US` / `zh-CN`）+ `src/i18n.rs` 字符串表 + `t(key)` 回退（缺键回退 en-US，再回退键名），运行时切换并持久化在 `ui.json`。当前覆盖范围为**设置弹窗全部 10 个标签页、顶栏与画布底部工具栏/缩放弹层/快捷键表**（最高频 chrome）；`crates/app` 其余工作区文案（像素/图像/纹理工作台、库、toast、节点面板等）仍为英文硬编码，按后续轮次增量迁移——新增文案应优先走 `t()` 与 `I18n` 全局，不要再写死字符串。
15. **主题跟随系统**：web 只有 light/dark 两档；桌面新增 `follow-system`，通过 `HKCU\...\Themes\Personalize\AppsUseLightTheme`（`reg query`，无新依赖）判定，仅在选中该档位时每 2s 轮询一次；`light` 是默认值（与 web 一致）。切换即时生效（与 web 的 CSS 变量翻转一致，见 motion-catalog §6），无自造缓动。
16. **本地模型 / 成本查询 / Agent 审计**：桌面无本地推理运行时与 Agent 桥接，因此「本地模型」标签页只扫描 `<data-root>\models` 的真实文件并展示 not installed / no runtime 状态（不做下载、不伪造进度）；「Agent」标签页保持诚实空态；「成本」标签页写入真实记录（完成生成时用 `generation_cost` 估价写入 `costs.json`，上限 `GENERATION_COST_RECORD_LIMIT`=200，另可粘贴 generation id 走 `fetch_generation_cost` 查询 API 计费）。

---

## 8. 框架选型

**倾向：Rust + Slint**（与已完成的 `D:\Dev\asset-library-desktop` 一致；声明式语法最接近 CSS，支持自定义字体、`animate`/`states` 过渡、`Path` 画连线、`Flickable` 平移缩放）。

备选（未采用）：
- **egui/eframe**：即时模式 painter 对节点图/连线更顺手，但 CSS 式布局与动效需全手写，**面板/弹窗保真更差**；本应用 74 个文件依赖 AntD 式组件，重写成本更高。
- **Tauri（webview 包壳）**：保真度 100%（直接跑现有 Web UI），但**不是原生移植**，与「前端转 Rust」目标不符。

**已定稿：Slint。** 依据：源 UI 是 DOM+CSS+SVG（非 canvas-2D），声明式保留式 UI 比即时模式更贴合；Slint 支持自定义字体、`animate`/`states` 缓动过渡、`Path` 画贝塞尔连线、属性绑定做 1:1 平移缩放；且与 `asset-library-desktop` 技术栈一致。

已知需在实现中验证/近似（记录为偏差）：`backdrop-blur`（玻璃虚化，用半透明+近似阴影）、SVG 图标（lucide，需预栅格化）、CJK 字体嵌入、富文本与视频（native 无对等物）。

---

## 9. 参考

- `research/design-system.md` — 全量设计 token（light/dark）、AntD alias、globals、字体、Tailwind 映射（带 file:line）
- `research/ui-inventory.md` — 全部可见 UI 面与组件树、core/secondary 划分
- `research/motion-catalog.md` — 全部动效（触发/属性/时长/缓动/来源）
- `research/state-and-domain.md` — 领域模型、store、持久化、交互、外部服务
