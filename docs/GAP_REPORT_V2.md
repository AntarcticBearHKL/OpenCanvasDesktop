# Open Canvas 原生移植 · GAP 报告 V2（三域合并 · 三轴复核）

> **基准与范围**：**Web 是唯一事实来源**（source of truth）——`D:\Dev\open-canvas\web\src`（React 19 + Tailwind v4 + AntD 6）；原生端口为 `D:\Dev\open-canvas-desktop`（Rust + Slint）。本报告是 **W1–W8 之后的重新核实（post-W1–W8 re-verification）**，全部判定以当前代码逐文件核对为准（读代码，未运行程序）。
> **取代关系**：旧 `docs/GAP_ANALYSIS.md` 已被本报告**取代（superseded）**，仅可作历史清单；其多条结论已过期。
> **合并来源**：三份已完成的子审计报告——A《画布工作区 + 全局外壳》、B《Image Studio + Pixel Studio》、C《Texture Studio + 插件系统 + AI 服务》。三份原文逐字收录于「领域报告」（仅将源标题层级整体下移、去掉会话性开场白，表格逐行保留）。
> **状态词**：一致 / 部分 / 缺失 / 偏差 / 不适用。
> **设计性偏差**（平台不可能项，如无 JS 引擎 / 无 webview / 无 3D 渲染器 / 无 ONNX / 无 backdrop-filter 等）不计入差距，统一列于文末。

---

## 执行摘要

- **总体**：三份报告合计 **381 条**逐行核对 → **一致 165 / 部分 62 / 缺失 52 / 偏差 92 / 不适用 10**。功能轴 251 条中一致 114；UI 轴 61 条中偏差 40（视觉规格几乎全面偏移，多数为受限或刻意）；UX 轴 69 条中一致 35。
- **画布工作区 + 全局外壳**：主干功能（9 类节点、连线、历史/剪贴板、库、配置 10 页签、主题/i18n）高度对齐（功能 31 一致 / 74）；但**媒体管线整段断裂**——上传/拖入、图片工具族、图片预览、导出载荷全部缺失。**这是全文最重要的发现。**
- **Image / Pixel Studio**：功能覆盖度全库最高（Image 31/54 一致；Pixel 17/28 一致 + 4 项净新增：`.gpl`/`.pal` 调色板、洋葱皮、图层高级操作、GIF 导出）；缺口集中在坞布局、图层样式 fx 参数、文本/形状编辑细节。
- **Texture / 插件 / AI**：纹理生成器 **117/117** 全量渲染对齐（分类 10/49/58 与 web 相同，有测试断言）；缺口形态高度一致——「**库层已实现、应用层未接线**」（视频帧槽、账号用量、音频、插件事件总线/插件存储、插件状态持久化、分享链接 `#tc=`）。
- **跨域共性**（三份报告共同出现）：全局无 Tooltip、玻璃模糊缺失（设计性）、坞布局不可调/不持久、无 reduced-motion、toast/长任务反馈弱化。
- **最严重单点**：画布无媒体上传/文件拖入——本地素材无法进入画布任何节点，且图集空态仍显示 "Drop images or files here" 死文案。
- **数据完整性风险**：插件安装/启停状态不持久化（重启回默认、URL 安装记录丢失）与导出/导入丢媒体载荷（`files: []`）——跨会话/跨机场景会静默丢数据。
- **AI 链路**：请求库层保真度极高（含 HTTP 集成测试），但 UI→请求有 **3 处覆盖断链**：`reasoningEffort` 面板可写但请求不读、`videoMode` 端到端缺失、文本模式 `metadata.count` 不生效。
- **设计性偏差**：约 19 类平台不可能项（无 JS 引擎 / webview / 3D 渲染器 / ONNX 运行时 / backdrop-filter / 原生文件对话框 / localStorage / agent 运行时等），不计入差距。
- **结论**：与「可日常使用」的距离已不在功能广度，而在**媒体管线**与**接线完整度**——大量已实现能力（含测试）挂不到 UI 与请求上。

---

## 三轴统计

**计数规则**：按三份源报告的表格**逐行计数**，每行只计一次，共 381 行（不含各报告的「Top 10 差距」「已达成」「设计性偏差」段落）。归类优先取「差距/差异」列：含状态词或明确差异描述者按描述归类（描述为「无」时取「原生现状」列状态词）；「净新增/设计性」条目按报告标注归入 偏差 或 不适用；多状态单元格（如「偏差/缺失」）取更严重者（缺失 > 偏差 > 部分 > 一致）。画布报告 1 条「未验证」（行高，无 token 可核）计入 不适用。

| 领域 / 轴 | 一致 | 部分 | 缺失 | 偏差 | 不适用 |
|---|---:|---:|---:|---:|---:|
| 画布工作区 + 全局外壳 · 功能 | 31 | 15 | 15 | 13 | 0 |
| 画布工作区 + 全局外壳 · UI | 7 | 1 | 0 | 11 | 1 |
| 画布工作区 + 全局外壳 · UX | 14 | 1 | 4 | 2 | 1 |
| **画布 小计** | **52** | **17** | **19** | **26** | **2** |
| Image Studio · 功能 | 31 | 7 | 7 | 9 | 0 |
| Pixel Studio · 功能 | 17 | 3 | 2 | 2 | 4 |
| 工作室 · UI | 5 | 0 | 0 | 16 | 0 |
| 工作室 · UX | 8 | 4 | 6 | 3 | 2 |
| **工作室 小计** | **61** | **14** | **15** | **30** | **6** |
| Texture Studio · 功能 | 17 | 4 | 3 | 12 | 0 |
| 插件系统 · 功能 | 3 | 14 | 4 | 4 | 1 |
| AI 服务 · 功能 | 15 | 7 | 7 | 4 | 0 |
| 纹理 / 插件 / AI · UI | 4 | 3 | 0 | 13 | 0 |
| 纹理 / 插件 / AI · UX | 13 | 3 | 4 | 3 | 1 |
| **纹理组 小计** | **52** | **31** | **18** | **36** | **2** |
| **按轴合计 · 功能** | 114 | 50 | 38 | 44 | 5 |
| **按轴合计 · UI** | 16 | 4 | 0 | 40 | 1 |
| **按轴合计 · UX** | 35 | 8 | 14 | 8 | 4 |
| **总计** | **165** | **62** | **52** | **92** | **10** |

---

## 跨域 Top 20 差距

> 跨三份报告合并、去重后按**用户可见影响**排序（同影响下优先低成本项）。〔〕内为来源与重叠标注。

1. **画布无媒体上传 / 文件拖入路径** ｜ 画布 · 功能 ｜ **回归 vs web** ｜ 本地图片/视频无法进入任何节点；图集空态仍写 "Drop images or files here"（死文案）。〔canvas Top2〕
2. **悬停条图片快捷工具全族 + 画布图片对话框族缺失**（裁剪/蒙版/切分/分辨率/分析/分割/角度；含 OCR、多角度重生成、AI 放大未接线） ｜ 画布 + AI 服务 · 功能 ｜ **回归 vs web** ｜ 图片节点只能生成/展示，无法加工——web 的主交互入口在原生无对应。〔重叠：canvas Top1 × texture Top9〕
3. **插件安装/启停状态不持久化** ｜ 插件系统 · 功能 ｜ **回归 vs web** ｜ 重启后官方插件一律回到 enabled、URL 安装记录丢失（状态丢失）。〔texture Top1〕
4. **视频生成帧槽 / 参考媒体 / videoMode 未接线 + 每节点覆盖断链（reasoningEffort、文本 count）** ｜ AI 服务 + 画布 · 功能 ｜ **回归 vs web** ｜ 拖拽绑定的首/尾帧在生成时被静默忽略；推理强度与出图数量设置不生效。〔重叠：canvas 视频槽位行 × texture Top2 + Top5〕
5. **导出 / 导入丢媒体载荷** ｜ 画布 · 功能 ｜ **回归 vs web** ｜ 导出 JSON 信封 `files: []`、无拖放、导入不跳转；跨机恢复工程丢图片载荷。〔canvas Top4〕
6. **图片预览弹窗仍是 "Preview unavailable" 占位 + 图集预览画廊动作全缺** ｜ 画布 · 功能 ｜ **回归 vs web** ｜ 无法大图查看、设封面、取出、拆分。〔canvas Top3〕
7. **图层样式（fx）参数缺 4 类** ｜ Image Studio · 功能 ｜ **回归 vs web** ｜ Bevel 的 style/direction/高光色/阴影色、全部样式 blendMode、PatternOverlay patternKey 不可编辑。〔studios Top2〕
8. **Dock 不可拖拽 / 缩放 / 拆分 / 页签 / 持久化** ｜ 工作室×2 + 纹理 · UI/UX ｜ **回归 vs web** ｜ 面板布局固定（270/288/178、288/288/172），重启不恢复；web 可拖 168–640 并持久化。〔重叠：studios Top1 × texture Top4；画布 dock 本体视觉一致〕
9. **ModalShell 固定 520px，类型浏览器网格退化为列表** ｜ 纹理 + 插件 · UI ｜ **回归 vs web** ｜ 类型浏览器 880→520、插件管理器 640→520、样本 880→520，浏览与管理首屏观感明显不同。〔texture Top3〕
10. **全局无 Tooltip** ｜ 画布 + 工作室 + 纹理 · UX ｜ **回归 vs web** ｜ dock/顶栏/浮动条/库卡/工作室控件无悬停提示，可发现性显著低于 web。〔重叠：canvas Top5 × studios Top3 × texture Dock 提示缺失〕
11. **插件运行时面缺失**（安装=登记；`setup`/CSS/`Content`/`Panel`/`toolbar`/`onDoubleClick` 无等价物；事件总线与插件存储已实现零接线） ｜ 插件系统 · 功能 ｜ **设计性偏差**（其中事件总线/插件存储/声明式视频 provider 的「未接线」可闭合） ｜ 第三方插件装了不能用；官方插件仅靠原生重绘。〔texture Top10〕
12. **玻璃模糊整体缺失（无 backdrop-filter）** ｜ 跨域 · UI ｜ **设计性偏差**（Slint 能力限制） ｜ `.glass-*` 降级为纯 alpha 填充，浮层层次感更弱。〔重叠：canvas Top10 × studios Top5 × texture UI〕
13. **顶栏与账号/语音入口缺口**（画布标题不可就地重命名、无全局 logo 导航、无账号用量气泡、无语音入口） ｜ 画布 + AI 服务 · 功能 ｜ **回归 vs web** ｜ 顶栏功能面窄；OpenRouter 用量与音频服务在库不在应用。〔重叠：canvas Top7 × texture Top7〕
14. **图层行与排版尺度偏差** ｜ 工作室 + 纹理 · UI ｜ **回归 vs web** ｜ 名称 14→12px、行高 36→32px、图标 20→28px、面板标题 11px/700 全大写带字距、整体小 1–3px。〔重叠：studios Top4 × texture 面板标题/排版行〕
15. **单节点图层弹层缺失** ｜ 画布 · 功能/UX ｜ **回归 vs web** ｜ 节点「图层」按钮改开全局节点列表，单节点层级调整多绕一步。〔canvas Top6〕
16. **侧栏项目切换器功能面窄** ｜ 画布 · 功能 ｜ **回归 vs web** ｜ 无 New/Load、无拖拽排序、无逐行删除、无节点数、无分组名。〔canvas Top8〕
17. **文本批量备选不渲染 + 文本样式弹层缺失** ｜ 画布 · 功能 ｜ **回归 vs web** ｜ `metadata.texts` 不可展开/设主/取出；字体/粗斜体/对齐/颜色不可用。〔canvas Top9〕
18. **Image Studio 编辑面缺口**（形状图层属性不可编辑、渐变选项不全、路径/图案标签缺失、画布内实时文本编辑缺失） ｜ Image Studio · 功能 ｜ **回归 vs web** ｜ 形状无法改类型/填充/描边/圆角；文本只能在面板改。〔studios Top6 + Top10 + Top7〕
19. **成本只记估算、无实际成本** ｜ AI 服务 + 画布 · 功能 ｜ **回归 vs web** ｜ 生成结束固定写估算值，不读响应 `usage.cost`；账单数字偏乐观。〔重叠：canvas Cost 页签行 × texture Top6〕
20. **纹理缩略图无渐进渲染 + 历史 350ms 合并窗口缺失** ｜ 纹理 · UX ｜ **回归 vs web** ｜ 打开类型浏览器即同步渲染全部（最多 117 项），首开可能冻结；拖滑杆产生密集撤销点。〔texture Top8〕

---

## 修复优先级建议

> 分组口径：**P0 = 破坏核心流程**（用户无法完成预期主任务或发生数据丢失）；**P1 = 明显质量差距**（流程可用但观感/能力明显弱于 web）；**P2 = 打磨**。设计性不可闭合项标注「不可闭合」，给出缓解方案。

### P0 — 破坏核心流程（7 项）

1. **画布媒体上传 / 文件拖入** — 修复：接入文件选择与画布 drop 事件 → 调 `media.rs` 导入并落成 image/video 节点；同步替换图集空态死文案。
2. **图片快捷工具族 + 画布图片对话框族** — 修复：在 `bars.rs` / `floating.slint` 悬浮条挂对话框入口，复用 `ps` / `image_editor` 域算子；先做裁剪/蒙版/切分/分辨率 4 项 MVP，再补 OCR / 多角度 / AI 放大（API 可复用 `request_image_question`）。
3. **图片预览弹窗接位图 + 图集画廊动作** — 修复：`modals.slint:243-316` 接 `media.rs` 位图；图集预览补设封面/取出/移除/重试/下载/全部拆分。
4. **视频帧槽 / 参考媒体 / videoMode 接线** — 修复：`ai_service.rs` 从 `metadata.videoSlots` 构造 `VideoRequest`（`video_request.rs` 已全保真并有测试），补 videoMode 解析与 UI 切换。
5. **每节点覆盖断链 3 处** — 修复：`ai_service.rs:218-256` 读取 `reasoningEffort`；文本模式应用 `metadata.count`；补 `isOpenRouterVideoModel` 特例。
6. **插件安装/启停状态持久化** — 修复：`store.rs` 增 `load/save_plugins`，启动恢复 `InstalledRegistry`（serde 往返已具备）；卸载补二次确认。
7. **导出/导入媒体载荷** — 修复：`library_export.rs` 改为 ZIP（含媒体载荷）替代 `files: []`；导入解包并跳转打开；接拖放。

### P1 — 明显质量差距（13 项）

1. **Dock 布局（工作室×2 + 纹理）** — 修复：实现可拖宽/分栏/页签 + 布局持久化（对齐 `dock-layout.ts` v4 语义）；最低限度先做可调宽 + 持久化。
2. **ModalShell 520px 上限** — 修复：`modals.slint` 支持逐弹窗宽度（类型浏览器/样本 880、插件管理器 640）；类型浏览器恢复网格 + 棋盘格衬底。
3. **图层样式 fx 缺参** — 修复：`image_editor.rs` 补 Bevel style/direction/高光色/阴影色、全样式 blendMode、PatternOverlay patternKey，面板加字段。
4. **全局 Tooltip** — 修复：实现 Slint tooltip 原语（扩展 `hover-button.slint`），在 dock/顶栏/浮动条/库卡/工作室控件补挂。
5. **顶栏与入口缺口** — 修复：`top-bar.slint` 标题接 InlineEditor；`account.rs` 用量气泡接顶栏；`audio.rs` 接语音入口。
6. **单节点图层弹层** — 修复：`bar_events.rs:125` 恢复打开单节点 popover（层数/上移/下移）。
7. **侧栏切换器** — 修复：`side-panel.slint` 补 New/Load、拖拽排序、逐行删除、节点数、分组名。
8. **文本批量备选 + 文本样式弹层** — 修复：`node-card.slint` 渲染 `metadata.texts`（展开/设主/取出）；补字体/粗斜体/对齐/颜色弹层。
9. **Image Studio 编辑面** — 修复：形状属性面板（type/fill/stroke/width/radius/sides）、渐变 blend/opacity/stop/前后景拾色、路径行 SVG 缩略图、画布内 textarea 文本编辑。
10. **成本实际值** — 修复：生成结束读响应 `usage.cost` 落账（`cost.rs` 已留 api/lookup 来源枚举）；接 `estimate_token_cost`。
11. **纹理缩略图渐进渲染 + 历史合并** — 修复：缩略图异步分批 + spinner/"…" 占位；加 350ms 历史合并窗口。
12. **玻璃模糊（不可闭合）** — 缓解：提高 alpha 层次 + 统一 raised 阴影 token（`theme.slint` 增 shadow-raised），补偿无 blur 观感。
13. **插件未接线能力闭合** — 修复：接 `event_bus.rs`、`PluginStorage` 持久化后端、`plugin_video.rs` 声明式 provider 到 app 调用点。

### P2 — 打磨（13 项）

1. **蚂蚁线动画 + 光标覆盖** — 修复：`image_render.rs` 加 120ms 相位推进；补 move/nwse/nesw/路径锚点光标。
2. **无 reduced-motion** — 修复：接系统动效偏好（Windows 动画设置）并给动效加 0ms 路径。
3. **toast / 长任务反馈** — 修复：多 toast 堆叠 + 自动消失；导出类补进度条（GIF 已有可复用）。
4. **纹理 animSpeed 无 UI** — 修复：输出面板加 0–5 滑杆写入 `anim_speed`。
5. **资源池拖拽入板** — 修复：`image_assets.rs` 接拖放落板；来源改为画布图像节点。
6. **插件管理器提示** — 修复：补琥珀警告横幅、官方刷新按钮/加载态、升级圆点与 旧→新 文案。
7. **运行时日志** — 修复：上限 200→500、落盘持久化、补分类筛选/复制/下载/明细折叠。
8. **预设库语义** — 修复：同名覆盖更新、空名拒绝提示、导入=应用并关闭。
9. **画板** — 修复：补 2:3/3:2/9:21 比例、自定义背景色拾色、自动撑大、排列布局接线（`board.rs` 引擎已有）。
10. **键盘可达性（类型浏览器）** — 修复：TouchArea 加焦点 + Enter/Space 选择。
11. **虚假快捷键 + 分享链接** — 修复：View 菜单 `+/-` 实现或移除；`project.rs` `#tc=` 编解码接入口。
12. **边界值与语义偏差** — 修复：GIF 时长上限 0.1–30 vs 0.5–10（按 AGENTS.md 先确认）；精灵表 metadata JSON 改勾选；面板标题大小写/字距对齐 web。
13. **画布视觉规格打包** — 修复：图片内容去 16px 内缩、卡片去常驻描边、浮动条胶囊化、字号（标题 18px/页签 14px）、阴影层次、配置弹窗 980×560、库卡片响应式网格。

---

## 领域报告

> 以下三份报告为源报告**逐字收录**（表格逐行保留，未删改）。为维持合并文档层级，源报告的标题层级整体下移两级（`#`→`####` 等）；已移除各报告开头的会话性开场白。三份源报告均完整、无截断。

### 领域报告 A — 画布工作区 + 全局外壳

**源标题：** Open Canvas 画布工作区 + 全局外壳 · 三轴对比报告（本轮复核版）

#### Open Canvas 画布工作区 + 全局外壳 · 三轴对比报告（本轮复核版）

> 结论口径：**Web 为基准**；所有判定均以当前代码为准（`GAP_ANALYSIS.md` 仅作清单，未沿用其结论）。`未验证` 表示本轮未在源码中找到可判定依据。平台性不可移植项（无 webview / 无视频解码器 / 无 3D 渲染器 / 无 JS 插件运行时 / 桌面无 localStorage）在文末统一列为**设计性偏差**，不重复计入差距。

---

##### 功能 (functional)

| 项目 | Web（文件） | 原生现状 | 差距 | 严重度 |
|---|---|---|---|---|
| 9 种内建节点 + 插件节点类型/注册表 | `types/canvas.ts`、`lib/canvas/node-registry.ts` | `crates/core/src/node.rs`、`crates/plugins/src/builtins.rs` | 一致 | — |
| 节点工厂（中心落点/id/元数据） | `lib/canvas/canvas-node-factory.ts` | `node.rs::create_canvas_node`、`plugin_manager.rs::create_plugin_node` | 一致 | — |
| 创建入口：dock 输入/提示词/生成器分组 | `canvas-toolbar.tsx:147-203` | `toolbar.slint:317-340`、`plugin_manager.rs:27-49` | 一致（分组筛选菜单已落地） | — |
| 扩展（插件）类型弹层 | `canvas-toolbar.tsx:204-325` | `create-menu.slint`、`plugin_views.rs` | 偏差：并入统一创建菜单，无独立弹层 | 低 |
| 空白处右键/双击锚点创建（点击放置） | 无（web 无右键菜单） | `handlers.rs:63-67,162`、`plugin_manager.rs:53` | 偏差（净新增，改进） | 低 |
| 图片节点位图渲染（缩略图优先） | `canvas-node.tsx:693-745` | `media.rs`、`node-card.slint:750-769` | 一致 | — |
| 图片空态 | `canvas-node.tsx:704-714` | `node-card.slint:756-762` | 一致（文案同源） | — |
| 图集节点接触印相（叠卡/计数片） | `nodes/image-stack-node-content.tsx` | `canvas_content.rs`、`node-card.slint:851-995` | 一致 | — |
| 视频节点：海报帧 + 时长/状态条 | `canvas-node.tsx:747-757` | `node-card.slint:775-847` | 部分：无播放（设计性），有海报 | 低 |
| 资产节点内容 | `nodes/assets-node-content.tsx` | `canvas_content.rs::asset_rows`、`node-card.slint:1000-1062` | 部分：只读引用列表，无文件夹浏览/文件预览 | 中 |
| 文本节点渲染 + 双击就地编辑 | `canvas-node.tsx:572-610` | `inline_edit.rs`、`node-card.slint:1066-1149` | 一致 | — |
| 文本批量备选（`metadata.texts` 展开/设主/取出） | `canvas-node.tsx:612-690` | 仅计数片 `node-card.slint:1070-1103` | 缺失 | 中 |
| 提示词节点内容（内联 chip 编辑） | `nodes/prompt-node-content.tsx`、`canvas-prompt-chip-input.tsx` | `node-card.slint:1153-1197`（chip 条） | 部分：chip 不内联；提示词节点用通用 composer，无标签编辑器 | 中 |
| 视频提示词槽位（首/尾/参考）拖放绑定 | `canvas-video-slots.ts`、`nodes/video-prompt-node-content.tsx` | `drop_bindings.rs::bind_video_slot`、`node-card.slint:1243-1264` | 部分：无 videoMode 切换；槽位仅显示标题/数量 | 中 |
| 生成配置节点空态 | `canvas-node.tsx:716-725` | `node-card.slint:1201-1241` | 一致（文案同源） | — |
| 插件节点内容（便签/markdown/svg/html/全景） | 各官方插件 `Content` | `plugin_content.rs`、`node-card.slint:1266-1416` | 部分：html 沙箱无 webview、全景无 3D（设计性） | 低 |
| 缺插件占位 | `canvas-node.tsx:552-558` | `node-card.slint:1268-1290` | 一致 | — |
| 标题就地重命名（双击） | `canvas-node.tsx:362-396` | `content_events.rs:69`、`node-card.slint:582-668` | 一致 | — |
| 媒体信息条（W×H·字节·成本·info） | `canvas-node.tsx:759-791` | `canvas_content.rs:141-166`、`node-card.slint:269-318` | 一致（成本为本地估价） | — |
| 锁定徽标 | `canvas-node.tsx:464-468` | `node-card.slint:1421-1443` | 一致 | — |
| 关联/聚焦高亮（节点） | `project.tsx:570-588`、`canvas-node.tsx:179,401-405` | `canvas_content.rs::related_ids`、`app.rs:491-503`、`vm.rs:116,182` | 一致 | — |
| 关联连线高亮 | `project.tsx:639`（related connectionIds） | `app.rs:685` 仅 selected 为 active | 缺失 | 中 |
| 缩放手柄（4 角+握把）/连接圆点 | `canvas-node.tsx:476-484,820-854` | `node-handles.slint`、`vm.rs:151-152` | 一致 | — |
| 保持比例缩放 + 自由变形开关 | `canvas-node.tsx:699`（freeResize） | `gesture.rs` 仅比例 | 缺失（无 free-resize 开关） | 低 |
| 平移/缩放/框选/多选拖动/吸附/连线 | `open-canvas.tsx`、`project.tsx` | `handlers.rs`、`gesture.rs`、`connect.rs`、`core/geometry.rs` | 一致 | — |
| Shift 点选语义 | `project.tsx`（已选再点取消） | `doc.rs::select_node`（只加不减） | 偏差 | 低 |
| 连线选择/删除 + 关系标签 + Ctrl 右键裁线 | `canvas-connections.tsx`、`canvas-pointer-tools.ts` | `wires.rs`、`handlers.rs:57-62,100-131` | 一致 | — |
| 节点拖放绑定 + 拖拽幽灵 | `canvas-drop-bindings.ts`、`project.tsx:2156-2169` | `drop_bindings.rs`、`drag-ghost.slint` | 一致 | — |
| 复制/剪切/粘贴（含系统剪贴板） | `project.tsx:1223-1233`、`use-canvas-document.ts` | `clipboard.rs`、`edit_events.rs:20-62`、`system_clipboard.rs` | 部分：仅 JSON 信封文本，无图片/纯文本→节点粘贴 | 中 |
| 画布文件拖入/上传媒体 | `use-canvas-insertion.ts:105-121`、`project.tsx:2064` | 无任何上传/文件拖放路径 | 缺失（图集空态仍写"Drop images or files here"，为死文案） | 高 |
| 背景模式 dots/lines/blank + 弹层 + 持久化 | `open-canvas.tsx:259`、`app-config-modal.tsx:76-86` | `toolbar.slint:16-44`、`paths.rs`、`config_tabs.rs:311` | 一致 | — |
| 缩略图/标尺/缩放面板/重置/适配/快捷键弹窗 | `canvas-toolbar.tsx:327-378`、`canvas-mini-map.tsx`、`canvas-rulers.tsx` | `minimap.slint`、`rulers.slint`、`toolbar.slint:101-220`、`shortcuts-modal.slint` | 一致（Ctrl+0/1 为净新增） | — |
| 节点列表面板：过滤/表头/批量重命名/勾选 | `canvas-node-layer-popover.tsx:29-140` | `node-list.slint`、`node_list.rs`、`app.rs:608-661` | 一致（chip 过滤 vs `<select>`） | — |
| 单节点图层弹层（层数/上移/下移） | `canvas-node-hover-toolbar.tsx:184-185`、`canvas-node-layer-popover.tsx:57-86` | `bar_events.rs:125`（改开全局节点列表） | 缺失 | 中 |
| 悬停工具栏基础动作（info/复制/字号/删除） | `canvas-node-hover-toolbar.tsx:139-143,159-160` | `bars.rs:106-133`、`floating.slint` | 一致 | — |
| 悬停工具栏图片快捷工具（上传/下载/复制/裁剪/蒙版/去背/切分/分辨率/分析/OCR/分割/角度/重试） | `canvas-node-hover-toolbar.tsx:144-165`、`canvas-image-toolbar-tools.tsx` | 无 | 缺失 | 高 |
| 悬停工具栏视频工具（抽帧/下载/替换/任务查询） | `canvas-node-hover-toolbar.tsx:145-162` | 无（`grep` 无 capture/query 命中） | 缺失 | 中 |
| 文本样式弹层（字体/字号/粗斜体/对齐/颜色） | `canvas-text-style-popover.tsx` | 无（仅 ±2 字号） | 缺失 | 中 |
| 悬停工具栏"显示标签"开关 | `canvas-node-hover-toolbar.tsx:183` | 无 | 缺失 | 低 |
| 节点信息弹窗（Info/JSON） | `canvas-node-hover-toolbar.tsx:192-261` | `modals.slint:170-239`、`bars.rs:145-197` | 部分：缺图片体积行与错误块（`error` 属性未渲染） | 中 |
| 图片预览弹窗 | `project.tsx:2102-2114` | `modals.slint:243-316`（仍为 "Preview unavailable" 占位） | 偏差/缺失 | 高 |
| 图集预览画廊（设封面/取出/移除/重试/下载/复制/全部拆分） | `image-stack-preview.tsx` | 无（仅占位三格） | 缺失 | 高 |
| 画布图片对话框族（裁剪/蒙版/切分/分辨率/分析/分割/角度） | `canvas-node-*-dialog.tsx` | 无 | 缺失 | 高 |
| 加载/错误/重试三态 | `canvas-node.tsx:519-546,684-690` | `node-card.slint:675-747`、`content_events.rs:24-39` | 一致 | — |
| 节点面板（composer/config，600px） | `canvas-node-prompt-panel.tsx`、`canvas-config-node-panel.tsx` | `node-panel.slint`、`node_panel.rs` | 部分：无"展开编辑器"弹窗、无输入徽标细节 | 中 |
| 图片/视频/文本设置弹层（质量/尺寸/数量/秒数/音频…） | `canvas-image-settings-popover.tsx` 等 | `panel-popovers.slint` | 一致 | — |
| 撤销/重做（快照+180ms 防抖+50 上限） | `hooks/use-canvas-history.ts` | `core/history.rs` | 一致 | — |
| 顶栏：侧栏开关/Home/工作区页签/齿轮 | `canvas-top-bar.tsx:58-146`、`app-top-nav.tsx` | `top-bar.slint` | 部分：无全局 logo 导航；无账号用量弹层（`canvas-account-usage.tsx`） | 中 |
| 顶栏标题就地重命名 | `canvas-top-bar.tsx:79-101` | `top-bar.slint:92-99`（静态 Text） | 缺失 | 中 |
| 工作区页签路由 | `canvas-top-bar.tsx:105-141`（跳工作室页） | `library.rs:409-422`（打开已开项目，否则按 kind 过滤库） | 偏差 | 低 |
| 侧栏显隐/拖拽调宽（220–480）/持久化 | `canvas-side-panel.tsx:26-44`、`use-canvas-side-panel-store.ts` | `side-panel.slint:124-139`、`edit_events.rs:118-150`、`store.rs:482-547` | 一致 | — |
| 侧栏项目切换器：New/Load/拖拽排序/删 除确认/节点数/分组名 | `canvas-switcher.tsx:62-183` | `side-panel.slint:94-121`（仅标题行） | 缺失 | 中 |
| 库：分组 CRUD（内联改名/确认删除） | `pages/canvas/index.tsx:80-160` | `library.slint:340-400`、`library_ops.rs` | 一致 | — |
| 库卡片：打开/改名/删除/导出 | `canvas-project-row.tsx:37-170` | `library.slint:240-380`、`library_ops.rs` | 一致 | — |
| 库卡片：移动到分组/多选/拖拽排序 | `canvas-project-row.tsx:44-135` | 无 | 缺失 | 中 |
| 库卡片统计行（节点/连线数） | `canvas-project-row.tsx:108` | 改为更新时间 + kind 徽标（`library.slint:406-450`） | 偏差 | 低 |
| 库 kind 徽标 + 按 kind 过滤 | 无（web 无 kind 概念） | `library.rs:29-76,239-247` | 偏差（净新增） | 低 |
| 导出/导入 | `lib/canvas/canvas-export.ts`、`canvas-import-dialog.tsx`（ZIP+blob+file-saver、拖放） | `library_export.rs`（JSON 路径输入框，`files: []`，无拖放） | 偏差：导出的图片载荷丢失，导入不跳转 | 高 |
| 配置弹窗 10 个页签 | `app-config-modal.tsx:37-173` | `config-modal.slint:394-463`、`config_tabs.rs` | 一致（页签齐） | — |
| Providers 页签 | `config-providers.tsx`（Collapse/预设/密码框/模型能力） | `config-modal.slint:487-650` | 部分：API Key 明文输入，无折叠、无通道排序 | 中 |
| Models 页签（5 能力 + ModelPicker） | `config-models.tsx` | `config-modal.slint` capabilities 行 | 部分：能力默认值 + 手动增删模型 | 低 |
| Appearance 页签（语言/主题/画布背景） | `app-config-modal.tsx:50-90` | `appearance.rs`、`config_tabs.rs:54-97` | 偏差：主题多 follow-system、语言多 zh-CN（净新增） | 低 |
| Generation 页签（默认出图数 1–15） | `app-config-modal.tsx:91-109` | `appearance.rs:203-231` | 一致 | — |
| Local models 页签 | `config-local-models.tsx` | `local_models.rs`、`config-modal.slint` | 偏差：无本地推理运行时，仅扫描/打开目录（设计性） | 低 |
| Local storage 页签 | `config-local-storage.tsx`（localforage 用量） | `storage_scan.rs`（数据目录按类统计） | 偏差：对象不同（本地目录 vs 浏览器存储） | 低 |
| Cost 页签（记录/合计/清空/按 id 查询） | `config-generation-cost.tsx`、`use-generation-cost-store.ts` | `cost.rs`、`config_tabs.rs:174-213` | 部分：无 API `usage.cost` 来源（仅 estimate/lookup） | 低 |
| Agent 页签 | `config-agent-audit.tsx`（桥接/目标/MCP/审计） | 空态占位（`i18n.slint:116-118`） | 偏差：无 agent 桥（设计性） | 低 |
| Runtime logs 页签（过滤/清空） | `config-runtime-logs.tsx` | `log_view.rs`、`config-modal.slint:1089-1141` | 一致 | — |
| About 页签（版本 + 快捷键） | `app-config-modal.tsx:140-172` | `config-modal.slint:1143-1169`、`shortcuts-list.slint` | 部分：无版本更新检查/发布弹窗（`version-release-modal.tsx`） | 低 |
| 主题持久化 | `use-theme-store.ts`（localStorage） | `appearance.rs:127-168`、`store.rs:482-547` | 一致（+follow-system 轮询 OS，净新增） | — |
| i18n | `i18n/index.ts`（强制 en-US，配置只给 en-US） | `i18n.rs`（en-US+zh-CN，覆盖顶栏/dock/配置弹窗） | 偏差：原生为超集，但仅外壳被本地化 | 低 |
| 提示（toast） | `app-providers.tsx` AntD message（3s 自动消失） | `app.slint:2196-2250`（单条、手动关闭） | 部分 | 低 |
| 快捷键 | `project.tsx:1197-1260`（无 Ctrl+X、无 Ctrl+0/1） | `shortcuts.rs:12-63`（多 Ctrl+X、Ctrl+0/1） | 偏差（净新增） | 低 |
| 插件管理器 | `canvas-plugin-manager-modal.tsx` | `plugin-manager.slint`、`plugin_manager.rs` | 部分：仅登记，无获取/执行（设计性） | 低 |

---

##### UI 视觉 (visual)

| 项目 | Web 的规格（token/数值/文件） | 原生实现 | 差异 | 严重度 |
|---|---|---|---|---|
| 画布/节点/工具栏色板（浅+深） | `canvas-theme.ts:4-109`（canvas/node/toolbar 全量 token） | `theme.slint:10-70` 逐值转录 | 一致（逐值比对相同） | — |
| 全局 globals 变量 | `globals.css:63-160`（`--radius .625rem`、`--hover`、`--glass`…） | `theme.slint:57-70` | 一致 | — |
| 选中蓝字面量 | `#2f80ff`（project.tsx/selectionBlue） | `theme.slint:73-74` | 一致 | — |
| 玻璃/背景模糊 | `.glass-surface` blur(20px) saturate(160%)、`.glass-raised` blur(24px)+阴影；缩放/拖拽时 `backdrop-filter:none`（`globals.css:195-236,742-746`） | `theme.slint:69-70` 仅 rgba 半透明，Slint 无 backdrop-filter | 偏差：无模糊/饱和度，无"缩放时去模糊"路径 | 中 |
| 浮层阴影 | `toolbar.surfaceShadow` 双层 `0 14px 34px rgba(28,25,23,.26), 0 2px 6px rgba(28,25,23,.18)`（`canvas-theme.ts:51,103`） | `floating.slint:33-35` 单层 drop-shadow α.12/blur34/offset12 | 偏差：层次感更弱 | 低 |
| 圆角 token | `--radius` 系 6/8/10/14/18/22/26（`globals.css:13-16,58-60`） | `theme.slint:157-163` 同值 | 一致 | — |
| 节点卡 | `rounded-3xl border-2`，边框色仅 active/related/drop 时着色，否则 `transparent`（`canvas-node.tsx:398-406`） | `node-card.slint:537-547` 常驻 `node.stroke` 2px | 偏差：静止态多一圈可见描边 | 低 |
| 图片内容布局 | 满幅 `rounded-[inherit]`（22px，无内边距）（`canvas-node.tsx:697-701`） | `node-card.slint:670-768` 统一 16px 内边距 + `radius-2xl`(18px) | 偏差：图片被内缩、圆角更小 | 中 |
| 浮动工具栏（悬停/选择） | `h-11 rounded-full` + `toolbar.surface/surfaceStroke/surfaceShadow`（`canvas-floating-toolbar.tsx:7-16`） | `floating.slint:8-38` 44px + `radius-xl`(14px) + glass-strong | 偏差：胶囊 vs 圆角矩形、面板色不同 | 中 |
| dock | `h-14 rounded-xl px-2 glass-surface`、按钮 `h-8 w-8`（`canvas-toolbar.tsx:115`） | `toolbar.slint:263-273` 56px/radius-xl/glass、按钮 32px | 一致 | — |
| 顶栏 | `h-14` 霜化、`absolute` 覆盖层（`canvas-top-bar.tsx:56-57`） | `top-bar.slint:63-71` 56px 实为布局行 | 偏差：结构（覆盖层 vs 行）不同，视觉等价 | 低 |
| 排版字体族 | `"Baloo 2","Resource Han Rounded SC","Yuanti SC","YouYuan","PingFang SC","Microsoft YaHei",system-ui`（`globals.css:18`） | `app.slint:157` 内嵌 Baloo 2 可变字重，CJK 交系统回退 | 部分：CJK 回退链不可枚举 | 低 |
| 字号/字重 | 顶栏标题 `text-lg`(18px/600)（`canvas-top-bar.tsx:89`）；页签 `text-sm`(14px)（:110）；节点标题 `text-sm font-medium` 且用 `node.text`（`canvas-node.tsx:384-385`） | 顶栏 14px/600（`top-bar.slint:92-99`）；页签 13px（:36-42）；节点标题 14px/500 用 `node-label`（`node-card.slint:646-655`） | 偏差：标题小 4px、页签小 1px、节点标题色号不同 | 中 |
| 行高 | `text-sm`→20px / `text-[15px] leading-6`（`canvas-node-prompt-panel.tsx:151`） | Slint 无 line-height token，默认行距 | 未验证（无对应 token 可核） | 低 |
| 间距/8pt 栅格 | p-4/px-3/gap-2 等（`canvas-node.tsx:572`、`canvas-toolbar.tsx:115`） | 16px 卡内距、6/8/10/12px 面板间距（`node-card.slint:671`、`floating.slint:435`） | 基本一致；库/弹窗有 14/18/20 非 8 倍数 | 低 |
| 图标 | lucide-react SVG，`size-4/4.5`，stroke 2（`canvas-toolbar.tsx:4`） | `icons.slint` 手绘 Path（24 viewbox，stroke 1.5–2，齿轮/月亮/罗盘等为简化形） | 偏差：形态近似但非 1:1 | 低 |
| 关键尺寸 | minimap 240×160（`canvas-mini-map.tsx:11-12`）、标尺 20px、节点面板 600px、AntD 弹窗 520px | `minimap.slint:18-19`、`rulers.slint:27`、`node-panel.slint:75`、`modals.slint:91` | 一致 | — |
| 配置弹窗尺寸 | 980×560 正文（`app-config-modal.tsx:17,187-195`） | 900×620（`config-modal.slint:320-321`） | 偏差 | 低 |
| 库卡片网格 | 响应式 1/2/3/4 列、卡高自适应（`pages/canvas/index.tsx:189`） | 固定 300×170、最多 6 列（`library.rs::place_cards`） | 偏差 | 低 |
| 节点列表面板 | 250px、`bottom-[84px]`、`glass-card`（`project.tsx:2059`、`canvas-node-layer-popover.tsx:56`） | 268px、高度上限 430（`node-list.slint:121-126`） | 偏差 | 低 |

---

##### UX 交互 (interaction)

| 项目 | Web 的行为/动效（时长·缓动·文件） | 原生实现 | 差异 | 严重度 |
|---|---|---|---|---|
| 节点入场 | 420ms `cubic-bezier(.2,.85,.18,1)`，opacity+scale.88+translateY(10)（`globals.css:748-767`） | `node-card.slint:511-534` 420ms 同曲线 | 一致 | — |
| 弹层入场 | `canvas-popover-in` 160ms ease-out，opacity+4px+scale.98（`globals.css:795-805`） | `floating.slint:13-17`、`node-panel.slint:65-90`、`toolbar.slint:117-124` | 一致 | — |
| 拖拽幽灵 | 150ms ease-out，scale .8→1（`globals.css:807-821`） | `drag-ghost.slint:11-25` 150ms ease-out | 一致 | — |
| 连线标签入场 | 220ms `cubic-bezier(.2,.85,.18,1)`（`globals.css:842-858`，仅带编号引用） | 标签静态（`app.rs:665-688` 无动画） | 缺失 | 低 |
| 侧栏开关 | 500ms `cubic-bezier(.22,1,.36,1)`，width 0↔W、opacity、x −48/−28；拖拽/降动效时 0ms（`canvas-side-panel.tsx:49-61`、store:3） | `side-panel.slint:71-85` 500ms 同曲线、resizing→0ms | 一致（x 位移用 −28 起手） | — |
| 悬停反馈统一 150ms | Tailwind 默认 `cubic-bezier(.4,0,.2,1)`（motion-catalog §4，约 150 处） | 各组件 `animate background` 150ms 同曲线（`hover-button.slint:22-25` 等） | 一致 | — |
| 拖拽/缩放/框选/小地图 1:1 无缓动 | motion-catalog §5 | 直接属性写（`handlers.rs`），无 animate | 一致 | — |
| 悬停意图（14px 间隙保持） | `canvas-node-hover-toolbar.tsx:171-175` + 浮动条 | `bar_events.rs:19-73` 160ms grace + 桥接 | 一致 | — |
| Tooltip | dock `DockTip`（`canvas-toolbar.tsx:456-462`）、浮动条 AntD Tooltip 0.2s 延迟（`canvas-floating-toolbar.tsx:26`）、顶栏/库卡多处 `title` | 全部缺失（`grep tooltip` 无命中） | 缺失 | 中 |
| 禁用/加载态 | 按钮 `disabled:opacity-35`、导出按钮 Loader2 转圈（`canvas-toolbar.tsx:119,420`） | `HoverButton.opacity 0.35`（`hover-button.slint:19`）；导出无 spinner | 部分 | 低 |
| 空态 | 库 Spin/Empty（`pages/canvas/index.tsx:176-201`）；节点空态文案齐全 | 空态文案一致（`library.slint:663-730`、`node-card.slint`）；库无加载态（同步读盘） | 一致（加载态不适用） | — |
| 错误/重试 | 节点 ErrorContent + 悬停条 retry（`canvas-node.tsx:684-690`） | `node-card.slint:692-747` + `content_events.rs` | 一致 | — |
| 反馈（toast） | AntD message，3s 自动消失、可多条 | 单条、位置底部居中、需手动关（`app.slint:2193-2250`） | 偏差 | 低 |
| 键盘：复制粘贴/剪切/全选/撤销 | `project.tsx:1197-1260` | `shortcuts.rs:46-61` | 一致（原生多 Ctrl+X） | — |
| 键盘：缩放重置/适配 | 无 | Ctrl+0 / Ctrl+1（`shortcuts.rs:58-59`） | 偏差（净新增） | 低 |
| 焦点管理 | 输入自动 focus/select（`canvas-node.tsx:199-203`、`canvas-top-bar.tsx:81`） | `InlineEditor` focus+select-all（`node-card.slint:168-179`）、`FocusScope` 收键盘（`app.slint:2645-2659`） | 一致 | — |
| 指针光标 | grab/grabbing、crosshair、col-resize、nwse/nesw-resize（`open-canvas.tsx:256`、`canvas-node.tsx:822-825,846`、`canvas-side-panel.tsx:72`） | 同套（`app.slint:1091`、`node-handles.slint:48,105`、`side-panel.slint:134`） | 一致 | — |
| 滚轮/缩放 | 滚轮以光标为锚 1.1^(Δ/100)，clamp 0.05–5（`open-canvas.tsx:111-154`） | `coords.rs::zoom_at`、`handlers.rs:141-152` | 一致 | — |
| 面板/弹层外点与 Esc 关闭 | `canvas-toolbar.tsx:91-110` | `bars.rs::close_overlays`、`shortcuts.rs:33-43` | 一致 | — |
| 拖拽来源光标 | `active:cursor-grabbing`（`assets-node-content.tsx:244`） | 无对应（原生无 HTML5 拖拽） | 不适用 | — |
| 悬停条动作标签 | 图片工具可切"显示标签"（`canvas-node-hover-toolbar.tsx:183`） | 无 | 缺失 | 低 |
| 减少动效（prefers-reduced-motion） | `globals.css:769-831` 多守卫 | 无系统级降动效处理 | 缺失 | 低 |

---

##### 已达成（本轮核实）

- 画布媒体渲染：图片位图（缩略图优先）、图集接触印相 + 计数片、视频海报帧 + 时长条、资产节点只读列表（`media.rs` / `canvas_content.rs` / `node-card.slint`）。
- 文本/标题就地编辑（双击、Enter/Esc/blur 提交语义、单条历史）（`inline_edit.rs` / `content_events.rs`）。
- 生成中 spinner（1000ms linear）、错误态 + Retry、媒体信息条（`title · W x H · bytes · ~$cost`）、锁徽标（`node-card.slint`）。
- 关联高亮（节点 + 聚焦变暗 160ms）、连线关系/编号标签、连线点选删除、Ctrl+右键裁线（`canvas_content.rs` / `wires.rs` / `handlers.rs`）。
- 网格三模式 + dock 背景弹层 + 配置 Appearance 持久化（`toolbar.slint` / `paths.rs` / `config_tabs.rs`）。
- 复制/剪切/粘贴（内部寄存器 + 系统剪贴板 JSON 信封、连接重映射、单条历史）（`clipboard.rs` / `edit_events.rs`）。
- 缩放面板（5–500% 滑杆、重置、适配、小地图、快捷键入口）+ Ctrl+0/1 + 快捷键弹窗（17 行）（`toolbar.slint` / `shortcuts-list.slint`）。
- 节点拖放绑定（assets 收集 / stack 吸收 / 视频槽位，含模型帧数限制与总量限制）+ 拖拽幽灵（`drop_bindings.rs` / `drag-ghost.slint`）。
- 侧栏拖拽调宽 220–480 + `ui.json` 持久化（宽度/开合/主题/语言）（`edit_events.rs` / `store.rs`）。
- 节点列表：类型过滤、层数/选中表头、勾选、批量重命名（按列表序编号、单条历史）（`node_list.rs` / `node-list.slint`）。
- 配置弹窗 10 页签（providers/models/appearance/generation/local-models/local-storage/cost/agent/logs/about）（`config-modal.slint` / `config_tabs.rs`）。
- 主题持久化 + follow-system（2s 轮询注册表）+ 顶栏月亮键（`appearance.rs`）。
- i18n 脚手架（en-US + zh-CN，顶栏/dock/配置弹窗/快捷键描述）（`i18n.rs` / `i18n.slint`）。
- 工作区页签：有项目则回工作区，无项目落到按 kind 过滤的库（`library.rs:409-422`）。
- 点击放置：右键/双击空白处锚点创建菜单 + dock 三组分组按钮（`plugin_manager.rs:27-71`）。

---

##### Top 10 差距

| # | 差距 | 轴 | 性质 | 说明 |
|---|---|---|---|---|
| 1 | 悬停工具栏图片快捷工具全族 + 画布图片对话框族（裁剪/蒙版/切分/分辨率/分析/分割/角度/OCR/去背/上采样）完全缺失 | 功能 | 回归 | 图片节点只能生成/展示，不能加工；web 的主交互入口（`canvas-node-hover-toolbar.tsx:144-165`）在原生无对应 |
| 2 | 画布无媒体上传/文件拖入路径 | 功能 | 回归 | 本地图片/视频无法进入任何节点；图集空态仍显示"Drop images or files here"（死文案） |
| 3 | 图片预览弹窗仍是 "Preview unavailable" 占位；图集画廊动作全缺 | 功能 | 回归 | `modals.slint:243-316` 未接位图，`image-stack-preview.tsx` 无对应 |
| 4 | 导出/导入丢媒体：JSON 信封 `files: []`、路径输入框、无拖放、导入不跳转 | 功能 | 回归 | `library_export.rs:96-121`；跨机恢复工程会丢图片载荷 |
| 5 | 全局无 Tooltip（dock/顶栏/浮动条/库卡） | UX | 回归 | `grep tooltip` 在 `crates/app/ui` 零命中；可发现性显著低于 web |
| 6 | 单节点图层弹层缺失（层数、上移/下移），"图层"按钮改开全局节点列表 | 功能/UX | 回归 | `bar_events.rs:125`；web 的 `canvas-node-layer-popover.tsx:57-86` |
| 7 | 顶栏标题不可就地重命名 + 无账号用量弹层 + 无全局 logo 导航 | 功能 | 回归 | `top-bar.slint:92-99` 静态文本；`canvas-account-usage.tsx` 无对应 |
| 8 | 侧栏切换器功能面窄：无 New/Load、无拖拽排序、无逐行删除、无节点数、无分组名 | 功能 | 回归 | `side-panel.slint:94-121` vs `canvas-switcher.tsx:62-183` |
| 9 | 文本批量备选（`metadata.texts`）不渲染/不可展开/不可设主；文本样式弹层（粗斜体/字体/对齐/颜色）缺失 | 功能 | 回归 | `node-card.slint:1070-1103` 仅计数片；`canvas-text-style-popover.tsx` 无对应 |
| 10 | 视觉/交互规格偏差打包：无 backdrop 模糊、浮动条为圆角矩形而非胶囊、图片内容被 16px 内缩、卡片常驻描边、字号偏小、toast 不自动消失、无 reduced-motion | UI/UX | 设计性偏差（部分为回归） | `theme.slint` / `floating.slint` / `node-card.slint` vs `globals.css` / `canvas-floating-toolbar.tsx` |

**设计性偏差（不计为差距，一次性列出）**：无 webview（html 节点沙箱）、无视频解码（海报代播放）、无 3D（全景占位）、无 JS 插件运行时（插件仅登记/官方插件内容原生重绘）、无 localStorage/localforage（改用 `library.json`/`ui.json`/`config.json`）、无 BroadcastChannel 多窗口同步、无浏览器级系统剪贴板图片读取（仅文本信封）、无 OS 级"浏览器存储用量"（改为数据目录扫描）、桌面无移动端抽屉/移动断点、Agent 桥与本地 ONNX 推理运行时未移植。

### 领域报告 B — Image Studio + Pixel Studio

**源标题：** Image Studio / Pixel Studio — 三轴对比报告（web → 原生 Rust+Slint，本轮重新核实）


---

#### Image Studio / Pixel Studio — 三轴对比报告（web → 原生 Rust+Slint，本轮重新核实）

**路径前缀**：`W/` = `D:\Dev\open-canvas\web\src\components\canvas\workspace\`；`N/` = `D:\Dev\open-canvas-desktop\`。
**核实方式**：逐文件读代码（引擎模块 + Slint 组件 + 回调接线），未运行程序。旧 `docs/GAP_ANALYSIS.md` 仅当清单使用；其中「masks / transform / text panel / drag-reorder / guides / SV 面板 / bitmap import / output modal / pixel SV+onion+GIF+palette io+image-node import+持久化+agent 域」等条目**已过期**。
**设计性不可实现（只列一次）**：无 ONNX 运行时（背景移除 / MobileSAM 分割 / AI 放大不移植）；无 JS 引擎（插件脚本、`model-plugin`、视频插件任务不移植）；无 agent 运行时（image agent 命名空间、agent 审计日志保持空——`N/crates/app/src/i18n.rs:214` 明示）；Slint 无 `backdrop-filter`（玻璃模糊降级为纯 alpha）；桌面无原生文件对话框（一律「输入路径」提示框）。

---

##### 功能 (functional)

状态词：一致 / 部分 / 缺失 / 偏差 / 不适用

###### Image Studio

| 项目 | Web（文件） | 原生现状 | 差距 | 严重度 |
|---|---|---|---|---|
| **工具 rail：16 工具 + zoom in/out** | `W/image-studio.tsx:99-116,1646-1669` | 一致 `N/crates/app/src/image.rs:64-107`、`N/crates/app/ui/components/image-panels.slint:51-95` | 无 | — |
| 工具热键 V/H/P/⇧P/M/⇧M/L/⇧L/W/A/B/E/G/⇧G/U/I | `W/image-studio.tsx:124,462-479` | 一致 `N/crates/app/src/image_ops.rs:1099-1102`、`N/crates/app/src/image.rs:131-139` | 无 | — |
| `[`/`]` 笔刷尺寸、Space 临时抓手、Enter 闭合多边形、Esc 退出 | `W/image-studio.tsx:408-497` | 一致 `N/crates/app/src/image_ops.rs:1051-1110` | 无 | — |
| Ctrl+A/D/⇧I/Z/Y/T/⇧T（web 未绑 Ctrl+F） | `W/image-studio.tsx:416-442` | 一致 `N/crates/app/src/image_ops.rs:1057-1069` | 无 | — |
| 笔刷 size/hardness/opacity 选项 | `W/image-studio.tsx:1555-1566` | 一致 `N/crates/app/ui/components/image-panels.slint:172-200`、`N/crates/app/src/image_actions.rs:156-171` | 无 | — |
| 选区 feather + antialias | `W/image-studio.tsx:1567-1568` | 一致 `image-panels.slint:226-243`、`image_actions.rs:162,146` | 无 | — |
| 魔棒 tolerance/contiguous/sampleAll | `W/image-studio.tsx:1569-1575` | 一致 `image-panels.slint:202-224`、`image_actions.rs:144-145` | 无 | — |
| 油漆桶 tolerance | `W/image-studio.tsx:1576` | 一致 `image-panels.slint:202-224` | 无 | — |
| 渐变选项 type/reverse | `W/image-studio.tsx:1577-1584` | 部分 `image-panels.slint:245-263`、`image_actions.rs:147-148` | **缺 blend mode / opacity / stop / 选项栏内 fg-bg 拾色器** | 中 |
| 形状选项 type/fill/stroke/strokeWidth | `W/image-studio.tsx:1586-1593` | 部分 `image-panels.slint:265-306`、`image_actions.rs:149,163` | 单 fill/stroke 开关、色取自前景，**无独立描边色** | 中 |
| 变换模式分段（free/skew/distort/perspective/warp + Done） | `W/image-studio.tsx:1604-1611` | 一致 `image-panels.slint:136-170`、`image_actions.rs:61-73` | 无 | — |
| 路径/图案 active 标签 + 清除图案 | `W/image-studio.tsx:1595-1603` | 缺失（`image-panels.slint` 选项栏无此行） | 无标签、无清除按钮 | 低 |
| 无像素层落笔时自动建层 + 提示 | `W/image-studio.tsx:672,1412-1413` | 偏差 `N/crates/app/src/image_ops.rs:572-585`、`N/crates/app/src/image.rs:649-678` | 仅像素/图像层可画；文本/形状/组上落笔静默无效 | 中 |
| 新建 image/text/pixel/shape/adjustment 图层 | `W/ps-layers-panel.tsx:64-83` | 一致 `image_actions.rs:374-394`、`image-layers.slint:1002-1005`（调整） | 无 | — |
| 复制 / 删除 / 上移 / 下移 | `W/ps-layers-panel.tsx:140-161` | 一致 `image_actions.rs:395-439` | 无 | — |
| 编组 / 解组 | `W/ps-layers-panel.tsx:124-139` | 部分 `image_actions.rs:440-455` | 解组只在 Edit 菜单，面板头无按钮 | 低 |
| 拖拽重排 before/after/into + drop hint | `W/ps-layers-panel.tsx:97-111,244-254` | 一致 `image-layers.slint:80-228`、`image_actions.rs:330-360` | 无 | — |
| 组折叠 / 展开 | `W/ps-layers-panel.tsx:256-274` | 一致 `image-layers.slint:241-269`、`image.rs:865-869` | 无 | — |
| 图层缩略图 / 蒙版缩略图 / fx 徽标 | `W/ps-layers-panel.tsx:384-400` | 一致 `image_render.rs:963-994`、`image-layers.slint:273-370` | 无 | — |
| 可见 / 锁定 / 行内重命名 | `W/ps-layers-panel.tsx:275-351` | 一致 `image_actions.rs:456-499`、`image-layers.slint:343-383` | 无 | — |
| 不透明度 + 16 混合模式 | `W/ps-layers-panel.tsx:168-191`、`lib/canvas/blend-modes.ts:7` | 一致 `image-layers.slint:397-440`、`image_actions.rs:271-293`、`N/crates/ps/src/blend.rs:7-24` | 无 | — |
| 行双击 → 打开 fx | `W/ps-layers-panel.tsx:239-243` | 偏差 `image-layers.slint:211-213` | 改为进入重命名；fx 变成独立坞面板 | 低 |
| 蒙版 add/apply/delete/目标绘制/蒙版视图 | `W/ps-properties-panel.tsx:37-46,130-143`、`W/image-studio.tsx:925-944` | 一致 `N/crates/app/src/image_masks.rs:41-151`、`image-layers.slint:306-341,890-911` | 无 | — |
| 图层样式 10 种：增/启停/删/编辑 | `W/ps-fx-panel.tsx:41-122` | 部分 `N/crates/app/src/image_editor.rs:342-411`、`image_actions.rs:500-579` | **Bevel 缺 style/direction/highlightColor/shadowColor；全部样式缺 blendMode；PatternOverlay 缺 patternKey** | 高 |
| fx 呈现方式 | `W/ps-fx-panel.tsx:131`（380px Modal） | 偏差 `image-layers.slint`（ImageFxPanel 坞面板） | 弹窗 → 坞面板 | 低 |
| 15 种调整 + 参数编辑器 | `W/ps-adjustments-panel.tsx`、`W/ps-adjustment-editor.tsx` | 一致 `image_editor.rs:20-315`、`image-panels-extra.slint:561-590` | 无 | — |
| 曲线 / 渐变停靠点编辑器 | `W/ps-param-editors.tsx:20-141` | 一致 `image_editor.rs:645-831` | 无 | — |
| 22 滤镜 + 分组 + 重复上次 | `W/lib/canvas/ps/filters.ts:41-62` | 一致 `N/crates/ps/src/filters.rs:457-557`、`N/crates/app/src/image_filter.rs:475-487` | 无 | — |
| 滤镜对话框（320 预览/重置/液化拖拽/栅格化警告） | `W/ps-filter-dialog.tsx` | 一致 `image-dialogs.slint:49-188`、`image_filter.rs` | 无 | — |
| 选区：6 工具 + 4 组合模式 + feather/invert/all/deselect | `W/image-studio.tsx:367-401,758-828` | 一致 `image_ops.rs:700-780`、`image_actions.rs:802-842` | 无 | — |
| 蚂蚁线动画（120ms 相位 +2，dash 8） | `W/ps-overlays.tsx:20-64` | 偏差 `image_render.rs:222-247`、`image.slint:625-636` | 静态双色虚线，无相位动画 | 中 |
| 通道：RGB/R/G/B 行 + 可见性 + alpha 列表 + 存选区 + 预览隔离 | `W/ps-channels-panel.tsx:116-241` | 一致 `N/crates/app/src/image_channels.rs:126-236`、`image-panels-extra.slint:101-236` | 无 | — |
| 路径：新建/复制/删除/可见/重命名/激活/生成选区/填充/描边/颜色 | `W/ps-paths-panel.tsx` | 部分 `image_channels.rs:246-390`、`N/crates/ps/src/path_ops.rs` | 路径行无 SVG 缩略图 | 低 |
| 文本引擎（字体/字号/色/粗斜下删/字距/字偶/行距/基线/缩放/大小写/段落/路径/变形） | `W/ps-text-panel.tsx:62-186` | 一致 `N/crates/app/src/image_text_panel.rs:309-530`、`N/crates/ps/src/text.rs:87-316` | 无 | — |
| 文本面板呈现（Character / Paragraph 页签） | `W/ps-text-panel.tsx:49-61` | 偏差 `image-layers.slint:913-957` | 平铺行列表、无页签；字体为 chip 条；仅 1 种嵌入字体 | 中 |
| 画布内实时文本编辑（textarea 覆盖层） | `W/image-studio.tsx:1866-1876` | 缺失 `N/crates/app/src/image_wire.rs:978-987` | 双击画布只处理多边形套索 | 中 |
| 菜单 Select/Edit/Image/Filter/View + Dock | `W/ps-menus.tsx:101-171` | 一致 `image.slint:1157-1450`、`image_actions.rs:24-121` | 无 | — |
| 模态：图像大小/画布大小/旋转/数值变换 | `W/ps-menus.tsx:197-287` | 一致 `N/crates/app/src/image_menu.rs:38-327`、`image-dialogs.slint:191-318` | 无 | — |
| 画板设置浮层（比例/分辨率/背景/不透明度） | `W/smart-canvas-settings-popover.tsx:28,103-180` | 部分 `image.slint:1733-1876`、`image_actions.rs:192-228` | 比例 6/9（缺 2:3、3:2、9:21）；背景无自定义色拾色器 | 低 |
| 画板自动撑大以容纳溢出图层 | `W/image-studio.tsx:270-281` | 缺失 | 不自动；只按比例重算尺寸 | 低 |
| 画板排列布局 grid/row/column/feature | `W/lib/canvas/smart-canvas.ts:265` | 缺失 `N/crates/ps/src/board.rs:557`（引擎有，未接线） | 无 UI/菜单 | 低 |
| 12 个坞面板 | `W/image-studio.tsx:125-138` | 偏差 `N/crates/app/src/image.rs:269-283`（13 个，含 fx） | fx 独立成面板 | 低 |
| 坞：拖拽/缩放/拆分/页签 | `W/components/canvas/dock/dock-panel.tsx`、`dock/dock-layout.ts:22-44` | 缺失 `image.slint:745,866,991`（固定 178/288/270） | 固定列宽、无页签、不可拖拽 | 中 |
| 坞布局持久化 | `dock-layout.ts:41,116-122`（v4 localStorage） | 缺失 `N/crates/app/src/store.rs:482-496`（只存 `pixel_panels`） | 图像坞布局不持久 | 低 |
| 位图导入 | `W/ps-layers-panel.tsx:64-70`（仅画布图像节点） | 一致 `N/crates/app/src/image_output.rs:83-112` | 原生新增真实文件导入；入口为路径提示 | 低 |
| 资源池来源 + 拖拽入画板 | `W/ps-resource-pool-panel.tsx:28-46,75-78` | 偏差 `N/crates/app/src/image_assets.rs:1012-1066` | 来源=其它图像项目（web=画布图像节点）；无拖拽落板 | 中 |
| PNG 导出 | `W/pages/image/index.tsx`（composeSmartCanvas+下载） | 偏差 `N/crates/app/src/image_export.rs:16-76` | 写盘路径提示代替浏览器下载 | 低 |
| 输出模态（download / canvas） | `W/components/studio/studio-output-modal.tsx:7-145` | 部分 `image_output.rs:115-187`、`image-dialogs.slint:322-396` | 无目标画布选择、节点标题、放置位置 | 低 |
| 画布叠加：棋盘/网格/标尺/参考线/吸附线/草稿/路径/变换手柄 | `W/ps-overlays.tsx` | 一致 `image_render.rs:106-430`、`image.slint:594-741` | 无 | — |
| 参考线：从标尺拖出 / 拖动 / 越界删除 / 清除 | `W/image-studio.tsx:1308-1352` | 一致 `N/crates/app/src/image_guides.rs:21-103` | 无 | — |
| 缩放：滚轮@光标 / fit / actual，clamp 0.05–8，step 1.2 | `W/image-studio.tsx:92-94,344-365` | 偏差 `N/crates/app/src/image.rs:39-42` | 上限 64（web 8）、步进 1.25（web 1.2） | 低 |
| image agent 命名空间（ops 应用 + 截图提供者） | `W/lib/canvas/image-agent-ops.ts`、`W/image-studio.tsx:1196-1241` | 缺失（`N/crates/app/src/config_tabs.rs:11-12` 明示无 agent 桥） | 设计性缺失 | 低 |
| 历史面板（快照/上限 50/跳转） | `W/ps-history-panel.tsx` | 一致 `N/crates/app/src/image.rs:318-388`、`image-layers.slint:960-972` | 无 | — |
| 动作面板（录制/停止/保存/播放/重命名/删除） | `W/ps-actions-panel.tsx` | 一致 `image_channels.rs:419`、`image-panels-extra.slint:362-465` | 无 | — |

###### Pixel Studio

| 项目 | Web（文件） | 原生现状 | 差距 | 严重度 |
|---|---|---|---|---|
| 10 工具 + 热键 B/E/G/L/U/O/I/M/V/H | `W/pixel-studio-shared.ts:26-38` | 一致 `N/crates/app/src/pixel.rs:38-49`、`pixel_ops.rs:467-490` | 无 | — |
| 工具选项 brush 1-32 / tolerance 0-255 / Filled / Mirror X-Y / Grid | `W/pixel-studio.tsx:978-1007` | 一致 `N/crates/app/ui/components/pixel-panels.slint:306-444`、`pixel_actions.rs:132-151` | 无 | — |
| 图形拾色器（SV 方块 + 色相条 + 前后景色对 + 交换/重置） | `W/ps-color-picker.tsx:70-131,138-172` | 一致 `pixel-panels.slint:452-694` | 无（web 的像素工作室不传背景色，原生新增 bg 槽） | — |
| HSB / RGB / hex 数值字段 | `W/ps-color-picker.tsx:102-128` | 一致 `pixel-panels.slint:805-877`、`pixel_actions.rs:546-583` | 无 | — |
| 调色板：默认 16 色 / 添加 / 右键删除 / 8 列网格 | `W/pixel-studio.tsx:1012-1043` | 一致 `pixel-panels.slint:698-780`、`pixel_actions.rs:509-543` | 无 | — |
| 调色板 `.gpl` / `.pal` 导入导出 | —（web 无） | 不适用 `N/crates/pixel/src/palette.rs:117-200`、`pixel_export.rs:177-210` | 净新增 | 不适用 |
| 图层：增/删/可见/上移/下移/不透明度/5 混合模式/倒序 | `W/pixel-studio.tsx:1045-1077` | 一致 `N/crates/app/ui/components/pixel-layers.slint:46-289`、`pixel_actions.rs:170-248` | 无 | — |
| 图层：复制 / 向下合并 / 拼合 / 重命名 / 锁定 | —（web 无） | 不适用 `N/crates/pixel/src/layer_ops.rs`、`pixel_actions.rs:209-288` | 净新增 | 不适用 |
| 帧：增/删/复制/chip 选择/FPS 1-60/时长 20-5000ms | `W/pixel-studio.tsx:1079-1102` | 一致 `pixel-layers.slint:293-503`、`pixel_actions.rs:451-506` | 无 | — |
| 播放/暂停（逐帧 durationMs，min 20ms） | `W/pixel-studio.tsx:248-255` | 一致 `pixel_ops.rs:499-537` | 无 | — |
| 洋葱皮（开关/1-3 邻居/前后着色循环） | —（web 无） | 不适用 `N/crates/pixel/src/onion.rs`、`pixel.rs:226-273`、`pixel-layers.slint:361-446` | 净新增 | 不适用 |
| 画布视图：自动 fit / zoom 0.05–64 / nextZoom / 滚轮@光标 / 平移 / 棋盘 / 网格≥4 / 蚂蚁线 / 对称辅助线 | `W/pixel-studio.tsx:226-363`、`shared.ts:48-49` | 一致 `pixel_ops.rs:70-144`、`pixel_render.rs:44-178` | 无 | — |
| 手势 snap-canvas 优化（缩放/平移期用快照层） | `W/pixel-studio.tsx:393-468` | 缺失 | 仅性能差异（原生每帧重合成） | 低 |
| 画布尺寸最近邻重采样（全部 cel） | `W/pixel-studio.tsx:811-826` | 一致 `pixel_actions.rs:617-644` | 无 | — |
| 撤销/重做（100 上限 + cel 增量）+ 清空层 + 取消选区 | `W/lib/canvas/pixel/history.ts`、`W/pixel-studio.tsx:144-181` | 一致 `pixel.rs:518-566`、`N/crates/pixel/src/history.rs` | 无 | — |
| 6 个坞面板 + File/Edit/View 菜单 + 缩放%/播放/导出 | `shared.ts:39-46`、`W/pixel-studio.tsx:1188-1238` | 一致 `pixel.rs:52-59`、`N/crates/app/ui/components/pixel.slint:486-783` | 无 | — |
| 坞：拖拽/缩放/拆分 | `W/components/canvas/dock/dock-panel.tsx` | 缺失 `pixel.slint:797,998,978`（固定 288/288/172） | 固定列宽 | 中 |
| 坞布局持久化（含尺寸/分组/页签） | `dock-layout.ts:41,116-122` | 部分 `N/crates/app/src/store.rs:482-512`、`pixel_actions.rs:99-113` | 只持久化可见性 | 低 |
| 预览面板（真实尺寸 + pixelated + 100% 上限） | `W/pixel-studio.tsx:1154-1160` | 一致 `pixel-panels.slint:884-908` | 无 | — |
| 属性：尺寸+Apply / 背景 / 导入 fit / File / 导出按钮 | `W/pixel-studio.tsx:1105-1151` | 部分 `pixel-layers.slint:508-811`、`pixel_actions.rs:586-644` | 自定义背景仅 hex 字段（无 SV 拾色器） | 低 |
| 画布图像节点导入列表 | `W/pixel-studio.tsx:1136-1145` | 一致 `pixel_actions.rs:343-403`、`pixel-layers.slint:685-746` | 无 | — |
| 导出 PNG / 横向精灵图 | `W/pixel-studio.tsx:757-777` | 一致 `pixel_export.rs:19-56` | 写盘路径提示 | 低 |
| 动画 GIF 导出对话框 | —（web 无） | 不适用 `N/crates/pixel/src/gif.rs`、`pixel_export.rs:226-345`、`pixel.slint:189-336` | 净新增 | 不适用 |
| 导入图片→像素（Fit/Stretch/Original） | `W/pixel-studio.tsx:786-809` | 一致 `pixel_export.rs:156-175`、`N/crates/pixel/src/png.rs:96` | 路径提示代替文件选择 | 低 |
| 项目持久化 | `W/stores/use-pixel-store.ts`（localforage blob） | 偏差 `pixel.rs:664-691`、`store.rs:252` | JSON + base64 PNG | 低 |
| agent 命名空间 + 截图提供者 + headless runner | `W/lib/canvas/pixel-agent-ops.ts`、`W/lib/canvas/pixel/headless.ts`、`W/pixel-studio.tsx:828-952` | 部分 `N/crates/pixel/src/agent.rs`、`headless.rs:34-77`（域层齐全），app 无接线 | 无运行时 agent 桥（设计性） | 低 |
| 键盘 Ctrl+Z / ⇧Z / Ctrl+Y + 空格 | `W/pixel-studio.tsx:708-740` | 一致 `pixel_ops.rs:467-497` | 无 | — |
| 新建项目尺寸预设（16…4K） | `W/pages/pixel/index.tsx:11,108` | 偏差 `N/crates/app/src/library_ops.rs:184`（固定 64×64） | 无预设（工作台范围外） | 低 |

---

##### UI 视觉 (visual)

| 项目 | Web 的规格（token/数值/文件） | 原生实现 | 差异 | 严重度 |
|---|---|---|---|---|
| 工作室顶栏 | `h-11`(44px) + `glass-surface` + `border-b` `W/image-studio.tsx:1502`、`W/pixel-studio.tsx:1190`、`studio-chrome.ts:5` | 44px `Theme.toolbar-panel` + 1px `Theme.toolbar-border` `N/crates/app/ui/components/image.slint:383-392`、`pixel.slint:486-495` | 无（模糊见下行） | — |
| 顶栏标题排版 | `text-sm font-semibold`(14px) `W/image-studio.tsx:1506` | 13px/600 `image.slint:406-414` | 小 1px | 低 |
| 工具选项栏位置/断点 | 顶栏内联，`hidden … min-[1920px]:flex` `W/image-studio.tsx:1550` | 独立 34px 行，始终显示 `image-panels.slint:121` | 结构不同 + 断点行为不同（有意的可用性改良） | 低 |
| 坞默认/可调尺寸 | 左/右 288、底 208；168–640（底 120–420）`W/components/canvas/dock/dock-layout.ts:31-44` | 图像：左 270 / 右 288 / 底 178；像素：288/288/172，**固定** `image.slint:745,866,991`、`pixel.slint:797,978,998` | 左列与底高不同 + 不可调 | 高 |
| 图像工具 rail | 36px 宽、24px 按钮（`size-6`）、`rounded-md` `W/image-studio.tsx:1645-1669`、`studio-chrome.ts:12` | 42px rail、34×30 按钮 `image-panels.slint:16-18,57` | 按钮 24→34/30，rail 更宽 | 低 |
| 像素工具网格 | 5 列、28px 按钮（`size-7`）、`gap-1` `W/pixel-studio.tsx:967,977` | 5 列、48×32 按钮 `pixel-panels.slint:272-273,326-337` | 按钮明显更大 | 低 |
| 图层行解剖（图像） | `py-1 pl-1 pr-0.5` + 28px 缩略图 + `text-sm`(14px) 名称 + 20px 图标 `W/ps-layers-panel.tsx:224-352` | 32px 行 + 28px 缩略图 + 12px 名称 + 28px 图标 `image-layers.slint:79,274,359,371-383` | 名称 14→12px、图标 20→28px、行高 36→32px | 中 |
| 图层行解剖（像素） | `px-1.5 py-1 text-sm` + 20px 图标 `W/pixel-studio.tsx:1054-1061` | 28px 行 + 12px 名称 + 28px 图标 `pixel-layers.slint:122,163,168-186` | 名称与图标尺寸不同 | 低 |
| 面板标题 | `text-xs`(12px) `font-medium` + lucide 图标 `W/ps-layers-panel.tsx:116-117` | 30px 头 + 11px/700/`letter-spacing .8px`，无图标 `image-layers.slint:87-102` | 字重/字距/字号/图标不同 | 低 |
| 面板行标签/数值列宽 | 标签 `w-[70px]`、值 `w-11` `studio-chrome.ts:15-16` | 标签 52px、值 34px `image-layers.slint:802,837` | 更窄 | 低 |
| 拾色器几何 | 列宽 228px、SV 方块 128px（行内 96px）、色相条 14px、手柄 12px 圆、fg/bg 40px 盒 + 20px 色块 `W/ps-color-picker.tsx:17,71-100,143-160` | SV 方块 120×96、色相条 12px、手柄 12px、fg/bg 40px 盒 + 28/40px 色块 `image-panels.slint:353-434,446-469` | SV 方块 120×96 vs 228×128 | 低 |
| 圆角体系 | `--radius 10px`；sm 6 / md 8 / lg 10 / xl 14 / 2xl 18 / 3xl 22 `W/styles/globals.css:13-16,58-60,65` | sm 6 / md 8 / lg 10 / xl 14 / 2xl 18 / 3xl 22 / 4xl 26 `N/crates/app/ui/theme.slint:157-163` | 无 | — |
| 玻璃与模糊 | `.glass-card` blur16/sat150、`.glass-surface` blur20/sat160、`.glass-raised` blur24/sat160 + `0 12px 34px` `globals.css:196-216` | 纯 alpha 填充（`Theme.glass`/`glass-strong`），弹层用 `drop-shadow` `theme.slint:69-70`、`pixel.slint:124-129` | **无背景模糊**（Slint 无 backdrop-filter） | 中 |
| 阴影 | `toolbar.surfaceShadow` `0 14px 34px / 0 2px 6px` `W/lib/canvas-theme.ts:51` | 菜单 24px/offset 8、模态 24–28px `pixel.slint:128-129`、`image-dialogs.slint:24-26,340-342` | 数值近似 | 低 |
| 排版尺度 | 值 `text-sm`(14px)、标题 `text-xs`(12px)、`tabular-nums` `studio-chrome.ts:5-16` | 标签/数值 11px、名称 12px、标题 13px（全局） | 整体小 1–3px | 中 |
| 菜单几何 | AntD Dropdown `glass-raised`，项 `min-w-[180px]` `W/ps-menus.tsx:45,88-99` | 196px / Dock 176px，`radius-md` 8px `pixel.slint:120-138,731` | 圆角 16→8px | 低 |
| 模态几何 | AntD Modal：fx 380px、导入/输出 ≈520px `W/ps-fx-panel.tsx:131`、`W/ps-menus.tsx:197` | 居中卡片 360px、DialogBackdrop 自适应 `image-dialogs.slint:9-46,322-336` | 宽度/圆角近似 | 低 |
| 明暗 token | `canvasThemes.light/dark` 全量 `W/lib/canvas-theme.ts:4-109` | 逐值镜像 `theme.slint:10-70` | 无 | — |
| 主题切换入口 | 配置模态 Appearance（light/dark/跟随系统）`W/components/layout/app-config-modal.tsx` | `Theme.dark` + ui_state `theme` `N/crates/app/src/store.rs:463-468`、`appearance.rs:127-176` | 无 | — |
| 选中/激活态画法 | `accentSoft` 底 + `accent` 文字 + 左侧 2px accent 条 `W/ps-layers-panel.tsx:229-231` | 同 `image-layers.slint:162,196-202` | 无 | — |
| 像素色块尺寸 | `grid-cols-8 aspect-square` `W/pixel-studio.tsx:1026-1037` | 固定 30×30、间距 4px `pixel-panels.slint:755-780` | 尺寸策略不同 | 低 |

---

##### UX 交互 (interaction)

| 项目 | Web 的行为/动效（时长·缓动·文件） | 原生实现 | 差异 | 严重度 |
|---|---|---|---|---|
| hover 过渡 | 150ms `cubic-bezier(.4,0,.2,1)`（Tailwind 默认，约 150 处）`docs/research/motion-catalog.md:98,142` | `Theme.motion-fast` 150ms + 同缓动 `image-panels.slint:20-23`、`hover-button.slint:22-25`、`pixel-panels.slint:141-144` | 无 | — |
| 侧栏动效 | 500ms `cubic-bezier(.22,1,.36,1)` `motion-catalog.md:133` | `Theme.panel-motion` 500ms `theme.slint:170` | 缓动未在工作室层体现 | 低 |
| 工作室菜单进入 | AntD Dropdown 缩放+淡入 `motion-catalog.md:195` | `StudioMenu` 无 `animate`，即时出现 `pixel.slint:120-138` | 无进入动效 | 低 |
| 画板设置浮层进入 | `canvas-popover-in` 160ms ease-out `motion-catalog.md:119` | 无 `animate` `image.slint:1733` | 无进入动效 | 低 |
| 模态进入 | AntD `zoom-big` + fade `motion-catalog.md:192` | `DialogBackdrop`/`ModalShell` 无 `animate` `image-dialogs.slint:9-46` | 无进入动效 | 低 |
| 蚂蚁线（图像） | 120ms 定时相位 +2、dash 8 `W/ps-overlays.tsx:20-62` | 静态双色虚线 `image_render.rs:222-247` | 无动画 | 中 |
| 蚂蚁线（像素） | 静态 dash 4/3、offset 4 `W/pixel-studio.tsx:336-345` | 静态 `pixel_render.rs:142` | 无 | — |
| 洋葱皮淡入 | —（web 无） | 150ms ease-out `pixel.slint:933-937` | 净新增 | 不适用 |
| 光标变化 | grab/crosshair/move/nwse-resize/nesw-resize/ew/ns `W/image-studio.tsx:1675,1810,1846,1857`、`W/ps-overlays.tsx:97,113,236` | 仅 grab/crosshair + 标尺 ns/ew `image.slint:578,689,716` | 图层移动/缩放手柄/路径锚点无专用光标 | 中 |
| 悬停提示（tooltip） | 全站 `title`/`aria-label` + AntD `Tooltip` `W/image-studio.tsx:1655-1656`、`W/pixel-studio.tsx:966` | 无 tooltip 原语（`hover-button.slint` 无） | 全站无悬停提示 | 中 |
| 禁用态 | `disabled:opacity-40` / `-25` `studio-chrome.ts:11`、`W/ps-layers-panel.tsx:33` | 0.35（图标按钮）/0.45（chip）`pixel-panels.slint:17,136` | 数值近似 | 低 |
| 焦点态 | AntD 焦点环 / `focus-within` | 仅输入框 focus 边框 accent `pixel-panels.slint:234` | 覆盖不足 | 低 |
| 拖拽反馈 | HTML5 DnD + before/after/into 高亮 + 拖拽幽灵 `W/ps-layers-panel.tsx:97-111,224-254`、`motion-catalog.md:174` | 本地拖拽 + 三态 hint `image-layers.slint:169-202` | 无跨面板拖放（资源池→画板） | 中 |
| 空态 | `noBoards` 空态 + 图标 `W/image-studio.tsx:1362-1381` | 不适用（按项目打开） | — | 不适用 |
| 长任务/结果反馈 | AntD `message` 成功/失败（3s，进度 `duration:0`）`motion-catalog.md:190` | 单条可关闭 notice + 模态内错误行 `image_output.rs:73-79`、`image_export.rs:78-83`、`pixel_export.rs:128-135` | 无进度条/无多 toast 堆叠 | 低 |
| 加载态 | 两个工作室均无 spinner（`animate-spin` 仅画布节点/工具栏）`motion-catalog.md:163` | 无 | 无 | — |
| 错误态 | toast | 模态错误行 + toast | 无 | — |
| 键盘全表 | 工具热键/Ctrl 组合/空格/Enter/Esc `W/image-studio.tsx:408-497`、`W/pixel-studio.tsx:708-740` | `image_ops.rs:1051-1110`、`pixel_ops.rs:467-497` | 无 | — |
| 缩放交互 | 滚轮@光标，图像 step 1.2、像素 1.25/0.8 + nextZoom 阶梯 `W/image-studio.tsx:344-365`、`shared.ts:49` | `image_ops.rs:89-99`（1.25）、`pixel_ops.rs:92-117` | 图像步进 1.25 vs 1.2 | 低 |
| 平移 | 空格/中键/手形，1:1 跟随 | `image_ops.rs` Pan 手势、`pixel_ops.rs:144` | 无 | — |
| 播放循环 | 逐帧 `durationMs`、`max(20ms)` `W/pixel-studio.tsx:248-255` | `pixel_ops.rs:499-537` | 无 | — |
| 输入保护 | `isTyping` 显式忽略输入框按键 `W/image-studio.tsx:409-411`、`W/pixel-studio.tsx:709-711` | 依赖 Slint 焦点路由 `pixel.slint:1197-1211` | 无显式守卫（行为等价） | 低 |
| 减弱动效 | `prefers-reduced-motion`/`reduced-transparency` 分支 `globals.css:228-236`、`motion-catalog.md:127` | 无 | 无系统偏好适配 | 低 |

---

##### 已达成（本轮核实）

- **Image Studio**：图层蒙版（新增/应用/删除/蒙版目标绘制/蒙版视图，`image_masks.rs:41-151`）；交互式变换（free 旋转手柄 + skew/distort/perspective/warp 网格，`image_transform.rs:65-142`、`image-panels.slint:136-170`）；完整文本面板（字符/段落/路径/变形全部控件，`image_text_panel.rs:309-530`）；图层拖拽重排 + 组折叠 + 位图缩略图 + 蒙版缩略图 + fx 徽标（`image-layers.slint:80-370`）；标尺拖拽建参考线/移动/越界删除（`image_guides.rs:21-103`）；SV 拾色面板 + 色相条（`image-panels.slint:315-613`）；位图文件导入（`image_output.rs:83-112`）；输出模态（下载 / 存为画布节点，`image_output.rs:115-187`）；`Ctrl+T`/`Ctrl+Shift+T` 已绑定（`image_ops.rs:1065-1066`）。
- **Pixel Studio**：图形拾色器（SV + 色相条 + 前后景 + 交换/重置，`pixel-panels.slint:452-694`）；洋葱皮（`onion.rs`、`pixel-layers.slint:361-446`）；图层复制/向下合并/拼合/重命名/锁定（`layer_ops.rs`、`pixel_actions.rs:209-288`）；调色板 `.gpl`/`.pal` 导入导出（`palette.rs:117-200`）；GIF 导出对话框（`gif.rs`、`pixel.slint:189-336`）；画布图像节点导入源（`pixel_actions.rs:343-403`）；坞可见性持久化（`store.rs:482-512`）；agent/headless 域层（`crates/pixel/src/agent.rs`、`headless.rs`）。

---

##### Top 10 差距

1. **坞不可调整、无页签、无拆分（两个工作室）** — UI/UX — **regression vs web**。Web 可拖拽/缩放/合并成页签并持久化（`dock-layout.ts:22-44,116-122`）；原生固定 270/288/178 与 288/288/172（`image.slint:745,866,991`、`pixel.slint:797,978,998`）。
2. **图层样式（fx）参数缺 4 类** — 功能 — **regression vs web**。Bevel 的 style/direction、highlightColor/shadowColor，7 种样式的 blendMode，PatternOverlay 的 patternKey 全部不可编辑（`image_editor.rs:342-411` vs `W/ps-fx-panel.tsx:41-122`）；且 web 由图层行双击打开 380px Modal，原生改为重命名 + 独立坞面板（`image-layers.slint:211`）。
3. **全站无 tooltip** — UX — **regression vs web**。Web 每个控件都有 `title`/`aria-label`，关键按钮另有 AntD Tooltip；Slint 侧无 tooltip 原语（`W/image-studio.tsx:1655` vs `hover-button.slint:1-60`）。
4. **图层行与面板排版/字号偏差** — UI — **regression vs web**。名称 14→12px、行高 36→32px、图标 20→28px、面板标题 12px/medium→11px/700/字距、标签列 70→52px（`W/ps-layers-panel.tsx:224-352`、`studio-chrome.ts:15` vs `image-layers.slint:79,359,802`）。
5. **玻璃模糊整体缺失** — UI — **designed deviation**（Slint 无 `backdrop-filter`）。Web `glass-*` 有 blur16/20/24 + saturate（`globals.css:196-216`）；原生为纯 alpha 填充（`theme.slint:69-70`）。
6. **形状图层属性不可编辑** — 功能 — **regression vs web**。Web 属性面板有 type/fill/stroke/width/radius/sides（`W/ps-properties-panel.tsx:147-190`）；原生只留一句说明文字（`image-layers.slint:959-965`），且工具选项是单色 + 单 fill/stroke 开关（`image-panels.slint:265-306`）。
7. **画布内实时文本编辑缺失** — 功能 — **regression vs web**。Web 有 textarea 覆盖层（`W/image-studio.tsx:1866-1876`）；原生画布双击只处理多边形套索（`image_wire.rs:978-987`）。
8. **蚂蚁线无动画** — UX — **regression vs web**。Web 120ms 相位推进（`W/ps-overlays.tsx:20-62`）；原生静态（`image_render.rs:222-247`）。
9. **图像工作室光标覆盖不足** — UX — **regression vs web**。缺 move / nwse / nesw / 路径锚点光标（`image.slint:578` 仅 grab/crosshair）。
10. **渐变工具选项不完整** — 功能 — **regression vs web**。缺 blend mode / opacity / stop / 选项栏内前后景拾色器（`image-panels.slint:245-263` vs `W/image-studio.tsx:1577-1584,1612-1617`）。

**次席（未入前十但值得记录）**：无像素层落笔不自动建层（`image_ops.rs:572-585`，中）；资源池来源不同且不可拖拽入板（`image_assets.rs:1012-1066`，中）；文本面板无 Character/Paragraph 页签（`image-layers.slint:913-957`，中）；菜单/浮层/模态无进入动效（低）；画板比例 6/9 且背景无自定义色（`image.slint:1733-1876`，低）；图像缩放上限 64 vs 8（`image.rs:41`，低）；画板不自动撑大（低）。

### 领域报告 C — Texture Studio + 插件系统 + AI 服务

**源标题：** Texture Studio · 插件系统 · AI 服务 — 三轴对比报告（web → 原生 Rust+Slint）


---

#### Texture Studio · 插件系统 · AI 服务 — 三轴对比报告（web → 原生 Rust+Slint）

**核实基准**：web `D:\Dev\open-canvas\web\src`（React 19 + Tailwind v4 + AntD 6）；原生 `D:\Dev\open-canvas-desktop`（Rust + Slint）。旧审计 `docs/GAP_ANALYSIS.md` 仅作清单，全部结论已对当前代码复核。状态词：一致 / 部分 / 缺失 / 偏差 / 不适用。

**一次性设计性不可能（非差距，后文不再重复）**：无 JS 执行引擎（web 插件是 React 组件，`plugin-loader.ts:11-23` Blob `import()`）；无 webview/HTML 引擎（html 插件 iframe、markdown 的 marked 富渲染）；无 3D 渲染器（panorama three.js）；无 ONNX 运行时（抠图/分割本地模型）；无浏览器扩展桥（`browser-cache.ts`）与 URL/分享链接入口（`#tc=`）。

---

##### 功能 (functional)

###### 1. Texture Studio

**生成器注册表分类计数**（web 分类由 `type-browser.tsx:31-37` 的 `TYPE_ID` 区间推导；原生同规则 `crates/texture/src/registry.rs:79-88`）

| 分类 | Web 数量 | 原生数量 | 缺失名称 |
|---|---:|---:|---|
| Basic (id ≤ 199) | 10 | 10 | 无 |
| Pattern (200–260) | 49 | 49 | 无 |
| FX (> 260) | 58 | 58 | 无 |
| **合计** | **117** | **117** | **无** |

原生 `TYPE_ID`/`TYPE_ORDER` 与 web 逐条一致（`registry_data.rs:12-130,139-…`），渲染分派 117 个 `u_type` 分支全覆盖（`renderer/types/mod.rs:30-151`，match 臂 = 117），并有测试断言每个类型 64px 缩略图非空白（`crates/app/src/texture.rs:2329-2344`）。

| 项目 | Web（文件） | 原生现状 | 差距 | 严重度 |
|---|---|---|---|---|
| 类型注册表（117 类型 / 407 参数规格 / `PARAM_SLOTS=16`） | `lib/texture/registry.ts` | 一致：`crates/texture/src/registry_data.rs` 生成表 + `registry.rs` 查询层 | 无 | 低 |
| 参数默认值 / 步进吸附 | `params-panel.tsx:147-157`（AntD Slider step） | 一致：`snap_to_step`（`crates/app/src/texture.rs:546,1208,1229`） | 无 | 低 |
| 参数面板动态滑杆 | `params-panel.tsx:132-159` | 一致：`texture-panels.slint:340-349` | 无 | 低 |
| 类型切换（面板内可搜索 Select） | `params-panel.tsx:93-100` | 偏差：面板只显示静态 `Type: X`（`texture-panels.slint:268-281`），改型只能走浏览器弹窗 | 少一处内联入口 | 低 |
| 类型面板空态（"No adjustable parameters"） | `params-panel.tsx:144-146` | 缺失：无空态文案 | 少文案 | 低 |
| 图层列表（眼睛/名称/类型/上移/下移/删除/新增，顶为首） | `layer-list.tsx:22-107` | 一致：`texture-panels.slint:83-133` + `texture_render.rs` `layer_rows`（`.rev()`，测试 `layer_rows_are_top_first…`）；新增默认 `Circle`（`texture.rs:1039-1042`） | 无 | 低 |
| 混合模式 5 种 / 不透明度 / Polar / Invert / Gradient / Solid+取色 | `params-panel.tsx:65-130` | 一致：`texture-panels.slint:238-317`（HSV 取色 `texture-color.slint:105-194`） | 无 | 低 |
| Transform 7 项 + Reset Transform | `params-panel.tsx:111-130` | 一致：`texture.rs:1223-1249`、`988-1005`；菜单项 `texture.slint:299-310` | 无 | 低 |
| Reset Parameters | `params-panel.tsx:132-142` | 一致：`texture.rs:1005` | 无 | 低 |
| 渐变编辑器：条上点击新增 / 按住 220ms 拖拽 / 双击删除 / Reset / 最少 2 stop | `gradient-editor.tsx:179,259-327` | 一致：`GRADIENT_DRAG_DELAY_MS=220`、`MIN_STOP_DISTANCE=0.02`、`gradient_drag_arm`（`texture.rs:53-57,1423-1476,1540-1546`），测试 `2239-2260` | 无 | 低 |
| 渐变 16 条内置 ramp | `gradient-editor.tsx:8-173` | 一致：`ramps_data.rs:7-124`（16 条逐值相同） | 无 | 低 |
| 后处理 14 效果（含范围/步长/颜色） | `effects-panel.tsx:72-146` | 一致：`post.rs:16` `POST_EFFECT_COUNT=14`、`post_data.rs:48-175` | 无 | 低 |
| 输出分辨率 6 档 64–2048 | `output-panel.tsx:13-19` | 一致：`texture-panels.slint:423-429` | 控件形态（chips vs Select） | 低 |
| 动画开关 / Time | `output-panel.tsx:24-25`（Slider 0–100 step .1） | 部分：Time 为文本框（`texture-panels.slint:461-492`，`texture.rs:1609-1615`） | 无滑杆 | 低 |
| 动画速度 `animSpeed`（0–5） | `output-panel.tsx:26` | **缺失**：`anim_speed` 无任何 UI 写入点（全仓仅 `texture.rs:384,1982`、`texture_export.rs:158,165` 读取），GIF/精灵表时间步长仍依赖它 | 速度不可调 | 中 |
| GIF FPS / Duration | `output-panel.tsx:27-28`（1–60 / 0.5–10） | 部分：文本框；clamp 1–60 / **0.1–30**（`texture.rs:1614-1615`） | 无滑杆 + 时长上限放宽 3× | 低 |
| Seamless Loop | `output-panel.tsx:29`（输出面板） | 偏差：移入 GIF 弹窗（`texture-dialogs.slint:712-718`），同一 `gif_seamless` 状态 | 位置不同 | 低 |
| 预览渲染 512px | `texture-studio.tsx:62,135-144` | 一致：`PREVIEW_SIZE` + `ensure_preview`（`texture.rs:331-344`） | 无（CPU 光栅替代 WebGL2） | 低 |
| 缩放（滚轮 1.25/0.8、clamp 0.05–64）/ 适配 / 100% | `texture-studio.tsx:64-230` | 一致：`texture.rs:47-50,411-449`、`texture_scroll:1862-1866` | 平移无修饰键（web 需中键/Alt+左键） | 低 |
| View 菜单标注的 `+` / `-` 快捷键 | —（web 无此标注） | 偏差：菜单显示 "shortcut: +/−"（`texture.slint:333-343`）但 `texture_key_core` 只处理 Ctrl+Z/Y（`shortcuts.rs:75-85`） | 虚假快捷键提示 | 低 |
| 历史：50 快照 / 深比较去重 / 截断 | `lib/texture/history.ts:5-57` | 一致：`crates/texture/src/history.rs:9-93` | 无 | 低 |
| 历史 350ms 合并窗口 | `texture-studio.tsx:71,244-251` | 偏差：每次 `commit()` 立即入栈（`texture.rs:319-324`），拖滑杆会产生密集快照 | 撤销粒度更细 | 中 |
| 键盘撤销/重做（Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y，输入框内忽略） | `texture-studio.tsx:381-397` | 一致：FocusScope→`texture_key_core` 只驱动自身历史（`shortcuts.rs:66-85`，测试 `texture.rs:2140-2161` 断言不碰画布历史） | 无 | 低 |
| 收藏持久化（web localStorage `oc_texture_type_favorites`） | `type-browser.tsx:10,39-63` | 一致：`store.load/save_texture_favorites`（`texture.rs:817-854`，`store.rs:361-374`） | 无 | 低 |
| PNG 导出 | `export.ts:34-55,426-438` | 一致：`texture_export.rs:191-251` | 无 | 低 |
| 法线图（Sobel strength=2） | `export.ts:67-109` | 一致：`NORMAL_STRENGTH=2.0`（`texture_export.rs:18-31,677-696`） | 无 | 低 |
| 通道打包（4 预设 / 10 源 / normal 实时生成） | `channel-pack-dialog.tsx:4-78` | 一致：`texture-dialogs.slint:468-566`、`texture.rs:1648-1668` | 无 | 低 |
| 精灵表（animation/colors、8192 上限、时长 2、hue 旋转） | `export.ts:263-328`、`sheet-export-dialog.tsx:18-21` | 部分：数值/上限/hue 全一致（`sheet.rs:17-20,132-146`）；**元数据 JSON 恒定写出**（`texture_export.rs:290-296`）而 web 是勾选项；帧数/格子/列/边距为自由文本框，默认 padding 0（web 1，预设 0/1/2/4） | 少勾选、少预设 | 低 |
| GIF 导出（fps×duration 帧数、seamless 尾段 20% 交叉淡化、进度） | `export.ts:361-450` | 一致：`gif_export.rs:22-38`、`texture_export.rs:149-180,401-466`；CPU 编码替代 gif.js(workers=2,quality=10) | 弹窗多一个导出分辨率选择（net-new） | 低 |
| 类型浏览器：搜索 / 分类 / 收藏筛选 / 计数 | `type-browser.tsx:50-152` | 一致：`texture-dialogs.slint:14-141`、`texture.rs:476-494,1276-1288` | 布局为列表而非网格（见视觉轴） | 低 |
| 缩略图 64px | `use-texture-thumbnails.ts:7,41-58`（懒渲染+spinner+"…"） | 偏差：`type_thumb` 同步渲染（`texture.rs:346-363`），打开时一次性渲染全部可见类型（`texture_render.rs:229-241`） | 无进度反馈、可能卡顿 | 中 |
| 12 条内置样本（128px 缩略图） | `samples-modal.tsx:8-58` | 一致：`samples.rs`、`texture.rs:365-375`、`texture-dialogs.slint:144-203` | 布局差异 | 低 |
| 预设库：保存/加载/删除/JSON 导入导出/上限 40 | `preset-library-modal.tsx:8-133` | 部分：`texture.rs:1711-1840`；**无同名（忽略大小写）覆盖更新**（web 会更新同名项）；空名默认 "Preset N"（web 提示"Enter a preset name"并拒绝）；**导入是新增一条预设**，web 是直接应用到编辑器并关闭 | 行为语义不同 | 中 |
| 分享链接 `#tc=` 解码 | `lib/texture/history.ts:59-78` | 偏差：编解码器已移植（`project.rs:89-101`）但无任何 UI/入口调用（`crates/app` 无引用） | 无 URL 面，未接线 | 低 |
| 纹理项目库（分组侧栏+计数、新建弹窗选组、卡片分辨率/日期/Open→、移动分组） | `pages/texture/index.tsx:108-325` | 偏差：并入共享库（`library.slint` 卡片 300×170 + kind 徽章），无分组移动、无分辨率行 | 设计性合并 | 中 |
| 停靠面板：拖拽改宽 168–640 / 分栏 0.2–0.8 / 面板拖动换边 / 页签堆叠 / localStorage v4 持久化 | `dock/dock-layout.ts:31-120`、`dock-panel.tsx` | 缺失：固定 288px 双列（`texture.slint:453-687`），仅显隐 + Reset Layout（`texture.slint:420-439`） | 布局不可调、不持久 | 中 |

###### 2. 插件系统

框架面（manifest / 安装源 / 生命周期 / 存储 / 事件 / CSS / i18n / 版本）

| 项目 | Web（文件） | 原生现状 | 差距 | 严重度 |
|---|---|---|---|---|
| 插件清单（解析/校验/规范化/再输出） | 无对应物（web 直接导出 `CanvasPlugin`，`types/canvas-plugin.ts:138-147`） | 偏差（net-new）：`plugins/src/manifest/{model,validate,convert,error}.rs`（id/semver/type-id/icon/size/color/settings 全量校验，`validate.rs:11-190`），`normalize` 补齐 name/version（`model.rs:168-180`） | 声明式替代 JS 清单 | 低 |
| 节点定义字段面 | `canvas-plugin.ts:101-126` | 部分：`abi.rs:147-173` 覆盖 icon/size/minimapColor/showInCreateMenu/hasSourceHandle/hidePanel/transparentBg/autoOpenPanel/useBuiltinPanel/interactionToggle/forceInteractive/keepAspectRatio/resource/hasCustomPanel；**`Content`/`Panel`/`toolbar`/`onDoubleClick` 无原生等价**（`PanelKind::Plugin`→None，`plugin_views.rs:230-237`） | 运行时组件面缺失（设计性） | 高 |
| render-kind 映射 | 无（组件即渲染） | 偏差（net-new）：7 种声明式 body（markdown/svg/html-sandbox-text/sticky-note/panorama-placeholder/image/text），`render_kind.rs:75-158` | 声明式替代 | 低 |
| 安装源：URL 抓取+求值+缓存 | `plugin-loader.ts:11-68,92-113` | 缺失：**仅登记记录**（URL+将缓存的本地路径），不 fetch、不执行（`plugin_manager_model.rs:143-181`，`plugin_host.rs:6-11`） | 设计性（无 JS 引擎） | 高（设计性） |
| 安装源：本地发现 `/plugins/index.json` | `plugin-loader.ts:117-149` | 缺失 | 设计性 | 中 |
| 安装源：开发插件 `VITE_DEV_PLUGINS` | `plugin-loader.ts:153-170` | 不适用 | — | — |
| 安装源：官方注册表拉取 + 相对 URL 解析 | `plugin-registry.ts:17-32` | 偏差：官方 5 插件编译内置，条目由本地列表生成（`plugin_manager_model.rs:129-141`，`builtin://official/*.js` 伪 URL） | 不拉取远端 | 中 |
| 安装状态模型 + serde 往返 | `use-plugin-store.ts:6-45`（zustand persist/localforage） | 部分：`InstalledRecord`/`InstalledRegistry` 可往返（`manager/install.rs:15-177`，测试 `manager.rs:374-402`），但**应用层未落盘**：`store.rs` 无 `load/save_plugins`，启动时官方插件总是重新装为 enabled（`plugin_host.rs:49-80`） | 启停/URL 记录重启丢失 | 高 |
| 启用 / 禁用（注销节点定义） | `plugin-loader.ts:75-85` | 部分：逻辑一致（`plugin_host.rs:84-105`，`sync_registry` 注册/注销），但不持久化（同上） | 同上一行 | 高 |
| 卸载（Popconfirm 二次确认） | `canvas-plugin-manager-modal.tsx:99-101` | 部分：`uninstall` 直接执行（`plugin_manager.rs:175-196`），无确认；不持久化 | 少确认 | 低 |
| 版本/升级（semver、hasUpgrade、old→new 标签、升级点） | `plugin-registry.ts:36-49`、`modal:107-120,172-186` | 部分：模型全有（`manager.rs:107-114,241-283`；`upgrade_label`/`shows_upgrade_dot`），UI 只有 badge + "Update" 按钮（`plugin-manager.slint:178-197,234-239`），无升级圆点、无 `旧→新` 文案 | 提示弱化 | 低 |
| 生命周期：activate/deactivate（注册/注销定义、替换重装） | `plugin-loader.ts:31-46` | 一致：`plugin_host.rs:84-114`、`PluginManager::install` 原位替换（`manager.rs:161-180`，测试 `294-314`） | 无 | 低 |
| `setup()` / cleanup 钩子 | `canvas-plugin.ts:146`、`plugin-loader.ts:37-39` | 缺失 | 设计性 | 中 |
| CSS 注入（`plugin.css` + `injectCSS(key)` 替换） | `plugin-runtime.ts:17-26`、markdown `styles.css` | 缺失 | 设计性（无 CSS 引擎） | 中 |
| 事件总线（emit/on + payload） | `lib/canvas/canvas-event-bus.ts`（经 `plugin-runtime.ts:37-38`） | 部分：`CanvasEventBus` 完整实现（`event_bus.rs:34-112`，Drop 即退订，测试全），但**应用层无任何使用点**（全仓 grep 仅 plugins crate） | 能力未接线 | 中 |
| 插件私有存储（按插件命名空间隔离） | `canvas-plugin.ts:70-74`、`createPluginStorage` | 部分：`PluginStorage` trait + `NamespacedStorage` + `MemoryPluginStore`（`storage.rs:24-114`），**应用层未接线、无持久化后端** | 未接线 | 中 |
| i18n（清单 `i18n[locale][key]` + 回退） | web 插件为中文硬编码；管理器用 i18n key | 部分：`PluginManifest::localized/name_for/description_for`（`model.rs:182-201`，测试 `tests.rs:267-275`），UI 未消费 | 未接线 | 低 |
| 创建菜单（input/prompt/generator 分组弹窗 + 扩展弹窗） | `canvas-toolbar.tsx:75-88,147-325` | 部分：三组按钮已存在且可用（`toolbar.slint:313-339`、`plugin_manager.rs:19-49`、`plugin_views.rs:67-111`），但弹出的是**同一列表按组过滤**（`create-menu.slint`），非三块独立分组面板；扩展弹窗并入同一菜单 | 结构差异 | 中 |
| 谜题按钮语义 | web = 扩展弹窗（仅当有插件节点） | 偏差：原生 = 直接打开插件管理器（`toolbar.slint:379`、`plugin_manager.rs:109-113`） | 设计性 | 低 |
| 插件管理器弹窗（3 页签 / 警告横幅 / 刷新+加载态 / 错误横幅 / 空态） | `canvas-plugin-manager-modal.tsx:11-226` | 部分：3 页签 + 空态齐全（`plugin-manager.slint:76-298`），缺**琥珀警告横幅、官方刷新按钮/加载态、错误横幅**；页签宽度 520（web 640） | 少三处提示 | 低 |
| 官方插件 · sticky-note（8 预设色+自定义、默认色、hidePanel、空态文案、pluginColor extra、调色板） | `plugins/canvas/sticky-note/src/index.tsx` | 部分：逻辑与配色一致（`official/sticky_note.rs`、`plugin_content.rs:122-135`、`plugin_views.rs:213-222` 调色板项、`theme.slint:153-154` 墨色），但**无卡内右上角色点+弹层**（web 形态），改为悬浮工具条 palette | UI 形态差异 | 低 |
| 官方插件 · markdown（marked HTML 渲染、CSS、resource=text、编辑/预览开关） | `plugins/canvas/markdown/src/index.tsx` | 部分：`to_text` 纯文本展示（`plugin_content.rs:136-144`）、`to_html` 子集渲染器已实现但**未被卡片使用**（`official/markdown/mod.rs:29-76`）；resource=text 一致（`mod.rs:91,106-108`）；编辑切换由**双击节点**驱动（`bar_events.rs:131-136`），非工具条按钮 | 富文本缺失（设计性）+ 入口不同 | 中 |
| 官方插件 · svg（透明底、上游 SVG 采纳、校验、渲染） | `plugins/canvas/svg/src/index.tsx` | 一致：校验器 + 上游采纳（`official/svg.rs:38-124`）、`resvg` 光栅化（≤720px，`plugin_content.rs:184-209`）、透明底、缓存 | 无 | 低 |
| 官方插件 · html（`{{input}}` 替换、沙箱 iframe、interactionToggle/forceInteractive） | `plugins/canvas/html/src/index.tsx` | 部分：替换逻辑逐字符一致（`official/html.rs:26-84`，含 `{{ input }}`）、hidePanel/interactionToggle/forceInteractive(editing) 一致（`html.rs:93-105`）；**渲染为文本沙箱占位**（`plugin_content.rs:158-161`），编辑器无行号槽 | iframe 缺失（设计性） | 中 |
| 官方插件 · panorama（2:1 校验、上游取图、内置生图面板 prompt 前缀、resource=image） | `plugins/canvas/panorama/src/index.tsx` | 部分：全部逻辑一致（`official/panorama.rs:25-81`，系统提示词逐字相同），球面查看器为占位（`plugin_content.rs:162-170`） | 3D 缺失（设计性） | 中 |
| 插件节点缺失态（未注册类型） | `canvas-node.tsx` `MissingPluginContent` | 一致：`view.rs:16-19,99-117` + puzzle 图标 | 无 | 低 |

###### 3. AI 服务

| 项目 | Web（文件） | 原生现状 | 差距 | 严重度 |
|---|---|---|---|---|
| 文生图 `POST /images`（IMAGE_MODEL 强制、n clamp 1–10、quality/size/background、重试×3） | `services/api/image.ts:220-320,465-515` | 一致：`crates/ai/src/image.rs:15-33,182-262,351-363`、`retry.rs:15-19` | 无 | 低 |
| 图像编辑 / 参考图 img2img（n=1、`input_references`） | `image.ts:516-570` | 一致：`image.rs:238-260,364-377`（测试 `443-452`） | 无 | 低 |
| 流式文本 / 视觉（SSE、on_delta、工具调用解析） | `image.ts:389-463` | 一致：`response.rs:43-120,127-305` | 无 | 低 |
| 图像问答（视觉） | `image.ts:571-…` | 一致：`request_image_question`（`image.rs:378-394`）已接入 text 生成（`ai_service.rs:336-342`） | 无 | 低 |
| 视频 · OpenAI multipart / OpenRouter JSON | `video.ts:225-283,284-323` | 一致：`video.rs:176-321`、`video_request.rs`（表单/JSON 形状逐字段） | 无 | 低 |
| 视频 · 首/尾帧槽、参考图/视频/音频、mode 解析与上限校验（9/3/3/12、2–15s、50/15/64MB） | `video.ts:313-475`、`lib/video-generation.ts:32-35` | 部分：库层全实现（`video_request.rs:34-51,106-…`，测试 `crates/ai/tests/ai_http.rs`），**应用只走参考图路径**：`Job.references: Vec<String>`→`create_video_task`（`ai_service.rs:345-358`，`video.rs:212-224`）；`metadata.videoSlots` 有绑定（`drop_bindings.rs:248-306`）但**生成请求不读取**；videoMode 端到端缺失 | 帧槽/视频音频参考未接线 | 高 |
| 视频 · 轮询 120×2500ms、可取消、失败判定 | `video.ts:97-168,256-272` | 一致：`wait_for_video_task`（`video.rs:496-524`）、`assert_video_config:525` | 无 | 低 |
| 视频 · 任务查询/续跑/下载、帧捕获（悬浮工具条） | `canvas-node-hover-toolbar.tsx:129-152` | 缺失：悬浮条只有 info/duplicate/插件项/字体/layers/delete（`bars.rs:106-124`）；无 queryVideoTask/downloadVideo/帧捕获 | 无视频任务 UI | 中 |
| 视频 · 插件/用户脚本 provider | `video.ts:169-211`（`runModelPlugin` 沙箱执行 JS） | 偏差：改为**声明式 JSON 模板 provider**（`plugin_video.rs:1-…`，`render_template/render_json_template/PluginVideoPoll`），但**应用未接线**（app 无调用点） | 能力未接线 | 中 |
| 模型插件脚本（图/文/视频自定义脚本） | `services/api/model-plugin.ts:127-188` | 缺失 | 设计性（无 JS 引擎） | 中 |
| OCR（提取图像文字） | `lib/canvas/canvas-ocr.ts:21-33`（复用 `requestImageQuestion`） | 缺失：请求函数有，OCR 封装/入口无 | 未接线 | 中 |
| 多角度重生成 / AI 放大（API 类，非 ONNX） | `canvas-node-angle-dialog.tsx`、`project.tsx:1620` | 缺失：无对话框、无调用 | 未接线 | 中 |
| 背景移除（本地 onnx worker）/ 图像分割（MobileSAM） | `services/background-removal.ts`、`lib/image/mobile-sam.ts` | 缺失 | 设计性（无 ONNX） | 中 |
| 生成成本查询 `GET /generation` | `image.ts:277-304` | 一致：`generation.rs` `fetch_generation_cost` + 成本页签查询框（`config_tabs.rs:394-417`，`ai_service.rs:541-570`） | 无（web 无查询 UI，属 net-new） | 低 |
| 成本估算表（MODEL_PRICES / TOKEN_PRICES / formatUsd / 6 位舍入 / audioGenerationCharge） | `lib/canvas/generation-cost.ts:13-66` | 一致：`generation_cost.rs:60-192`（逐值相同，测试 `198-301`） | 无 | 低 |
| 成本记录（上限 200、来源 estimate/lookup/**api**、生成后落账） | `stores/use-generation-cost-store.ts:48`、`use-canvas-generation.ts:125,284,414,769` | 部分：`cost.rs:19-146` 仅 estimate/lookup 两个来源；生成结束**只记估算**（`ai_service.rs:576-595`），无响应 `usage.cost` 读取；`estimate_token_cost` 已实现但**应用未用** | 实际成本缺失 | 中 |
| 成本页签 UI（总额/记录数/未定价提示/按模型分组/最近 20/清空） | `config-generation-cost.tsx:10-89` | 部分：总额+记录数+未定价提示+清空+`~` 估算前缀（`config_tabs.rs:32-211`，`config-modal.slint:928-1016`）；**无按模型分组、无最近 20 截断**；多一个手动查询框（net-new） | 少两段视图 | 低 |
| OpenRouter 账号用量 `GET /key`（总额/日/周/月/剩余/刷新，画布顶栏气泡） | `openrouter-account.ts:30-56`、`canvas-account-usage.tsx:8-68` | 部分：`account.rs:85-102` 完整实现 + 集成测试（`tests/ai_http.rs:481-527`）；**应用无任何调用点/UI**（grep `crates/app` 零命中） | 服务在、未接线 | 中 |
| OpenRouter 额度 `GET /credits` | 无 | 偏差（net-new）：`account.rs:105-114` + 测试；同样未接线 | 未接线 | 低 |
| 音频/语音 `POST /audio/speech`（voice/format/speed/instructions、字节计费） | 无（web 无音频服务） | 偏差（net-new）：`audio.rs:53-…`、`AUDIO_SPEECH_PATH`、`audio_generation_charge`（`generation_cost.rs:174-181`）+ 测试；**应用无调用点** | 未接线 | 低 |
| 提供商预设 3 条（OpenRouter/DeepSeek/Matilda，含模型+能力表） | `lib/provider-presets.ts:14-50` | 一致：`config.rs:169-205`（逐条相同，含 `textOnly`） | 无 | 低 |
| 渠道/模型模型层（`channel::model` 编解码、text-only 归一、能力猜测关键词、按能力解析/可选列表、baseUrl 归一、API URL 拼接） | `stores/use-config-store.ts:70-406` | 一致：`config.rs:16-434`（函数与常量逐条对齐） | 无 | 低 |
| 模型选择器（品牌图标 SVG、搜索） | `components/model-picker.tsx:22-118` | 部分：`ModelListCard` 静态列表 + 标签（`panel-popovers.slint:462-…`，`node_panel.rs:83-138` 内置 5 文/1 图/2 视频模型），无图标、无搜索；配置页签用 OptionRow 列表（`config_ui.rs:118-145`） | 少图标/搜索 | 低 |
| 每节点覆盖（quality/size/background/seconds/vquality/generateAudio/watermark/count/model） | `canvas-generation-helpers.ts:71-87` | 部分：`ai_service.rs:218-256` 覆盖前 8 项；**`reasoningEffort` 面板写入但请求不读**（`panel_events.rs:63` vs `ai_service.rs:220-256`）；**videoMode 端到端缺失**；文本模式的 `metadata.count` 覆盖缺失；`isOpenRouterVideoModel(nodeModel)` 特例缺失 | 3 处覆盖断链 | 中 |
| 每节点设置弹层（图：质量/比例/分辨率/数量；视频：分辨率/秒数/音频/水印；文：推理/数量） | `canvas-image-settings-popover.tsx`、`canvas-video-settings-popover.tsx`、`canvas-text-settings-popover.tsx` | 一致：`panel-popovers.slint:177-460`、`node_panel.rs:374-403`、`panel_events.rs:50-63` | 无 | 低 |
| 重试 / 并发（2 / 3 次 / 2000ms） | `canvas-generation-helpers.ts:153-155,157-190` | 一致：`retry.rs:15-19` | 无 | 低 |
| 错误映射（401/403/429/404/502/503、HTML 错误页、多形状 body） | `services/api/api-error.ts:13-60` | 一致：`error.rs:50-128`（测试 `160-204`） | 无 | 低 |
| 运行时日志（上限、级别+分类筛选、清空/复制/下载、明细折叠） | `use-runtime-log-store.ts:7-45`（500 条、localforage 持久化）、`config-runtime-logs.tsx:9-110` | 部分：环形缓冲 200 条（`runtime_log.rs:15`）、内存不持久化；页签仅**级别筛选**+清空（`log_view.rs:29-48`、`config-modal.slint:1038-1140`），无分类筛选、无复制/下载、明细不折叠 | 少 3 项操作 + 上限减半 | 中 |
| 配置模态 10 页签 | `app-config-modal.tsx:37-173` | 一致：providers/models/appearance/generation/local-models/local-storage/cost/agent/logs/about 全在（`config-modal.slint:394-463`，`config_ui.rs:40-64`） | 尺寸 900×620 vs 980×560 | 低 |
| Agent 审计页签（操作记录表） | `config-agent-audit.tsx:1-…` | 缺失：诚实空态（`config_tabs.rs:11-12`、`config-modal.slint:1019-1036`） | Agent 运行时未移植（设计性） | 中 |
| Canvas Agent SDK 运行时（agent 客户端/工具调用/状态快照） | `services/api/canvas-agent.ts`、`lib/agent/*` | 缺失 | 设计性 | 中 |
| 本地模型页签（运行时可用性、目录、刷新/定位、模型安装状态） | `config-local-models.tsx:1-…` | 偏差：探针+目录+模型行齐全（`config-modal.slint:749-849`），但无 ONNX 推理 | 无推理（设计性） | 中 |
| 本地存储页签（用量、分类桶、刷新/定位/清空） | `config-local-storage.tsx:1-…` | 一致：真实扫描数据目录（`storage_scan.rs:1-110`、`config_tabs.rs:126-171`） | 实现媒介不同（磁盘 vs localforage） | 低 |

---

##### UI 视觉 (visual)

| 项目 | Web 的规格（token/数值/文件） | 原生实现 | 差异 | 严重度 |
|---|---|---|---|---|
| 主题 token 全表（canvas/node/toolbar/globals 明暗两套） | `lib/canvas-theme.ts:4-110`、`styles/globals.css:13-65` | 一致：`theme.slint:10-70` 逐值抄录（含 `--radius: 0.625rem` → radius-sm/md/lg/xl/2xl/3xl/4xl = 6/8/10/14/18/22/26px，`theme.slint:156-163`） | 无 | 低 |
| 面板/停靠布局：左 288 / 右 288 / 下 208，可拖 168–640，分栏 0.2–0.8，localStorage v4 | `dock/dock-layout.ts:31-120` | 部分：左右 288px 与默认值一致（`texture.slint:454,593`），但固定不可调、无底部停靠、无分栏/页签 | 见功能轴 | 中 |
| 纹理顶栏 | `texture-studio.tsx:664` `h-11`（44px）glass-surface + 底部 1px 边框；标题 `max-w-[220px]` text-sm/600 | 一致高度 44px（`texture.slint:172`）、标题 13px/600 但 `max-width:160px`；Edit/View/Export/**Dock** 四个菜单按钮在左（`texture.slint:208-231`），web 为 Edit/View + DockWindowMenu 图标按钮 | 宽度少 60px、Dock 按钮形态不同 | 低 |
| 缩放控件位置 | 画布右下角浮层（glass-surface，`texture-studio.tsx:730-743`） | 偏差：并入顶栏右侧（`texture.slint:238-263`），无画布浮层 | 位置 | 低 |
| 面板标题 | `Section`：`text-xs`(12px) `font-semibold` 前景色，`px-3 py-3` + border-b（`params-panel.tsx:44-54`） | 偏差：`PanelHeader` 26px 高、11px/700、`letter-spacing:0.8px`、**全大写**、`node-muted` 色（`pixel-panels.slint` PanelHeader；标题字面量 `LAYERS/PARAMS/OUTPUT/EFFECTS/GRADIENT`） | 字号/字重/大小写/颜色 | 低 |
| 图层行 | 高约 30px，`px-2 py-1.5` text-xs，激活 `bg-brand-soft text-brand`（`layer-list.tsx:50`） | 一致：30px、激活 `node-accent-soft`（`texture-panels.slint:18-19`）；图标按钮 5 个一致 | 无 | 低 |
| 渐变条几何 | 高 `1.5rem`(24px)、圆角、border；手柄 `w-2.5`(10px) `h-6`(24px) `top-0` 圆角 sm 双边框（`gradient-editor.tsx:175-177,350-370`） | 偏差：条高 26px（`texture-panels.slint:760`）；手柄 12×18px、3px 圆角、垂直居中（`texture-panels.slint:711-724`） | 条 +2px、手柄比例不同 | 低 |
| 渐变 ramp 呈现 | 4 列网格，每项含渐变预览条 + 名称（`gradient-editor.tsx:409-425`） | 偏差：文字 chips 列表，无渐变预览（`texture-panels.slint:898-911`） | 无预览 | 中 |
| 类型浏览器布局 | Modal **880×…**，网格 `repeat(auto-fill,minmax(96px,1fr))`，64px 缩略图 + 棋盘格 16px + 星标悬停显现 + 激活 `accentSoft+inset ring`（`type-browser.tsx:84-146`） | 偏差：ModalShell **固定 520px**（`modals.slint:91`），68px **列表行**（64px 缩略图 + 名称 13px + 分类 10px + 星标），无棋盘格衬底 | 宽 -360px、网格→列表 | 高 |
| 样本弹窗 | Modal 880，`minmax(148px,1fr)` 卡片网格 + 128px 缩略图 + Apply 实底按钮（`samples-modal.tsx:61-90`） | 偏差：520px、140px 列表行（`texture-dialogs.slint:144-203`），整行点击应用 | 布局 | 中 |
| 预设库弹窗 | Modal 560（`preset-library-modal.tsx:136`） | 偏差：520px | -40px | 低 |
| 精灵表/通道弹窗 | Modal 420（`sheet-export-dialog.tsx:42`、`channel-pack-dialog.tsx:60`） | 偏差：520px | +100px | 低 |
| 插件管理器弹窗 | Modal **640**，行卡 `rounded-xl border node.stroke bg node.fill`、36px 图标槽、版本胶囊、琥珀警告条、官方行 loading 按钮（`canvas-plugin-manager-modal.tsx:129-147,216-221`） | 偏差：520px（ModalShell）；行 62px `toolbar-panel` 底、版本 `vX` 文本、灰 badge（`plugin-manager.slint:143-241`）；无警告条 | 宽 -120px、少警示 | 中 |
| 配置模态 | Modal **980** 宽、正文高 560、左侧 `w-52`(208px) 页签栏、内容 `padding:20px 24px`（`app-config-modal.tsx:17,35-36,187-196`） | 偏差：900×620，页签栏 184px，内容 padding 18px（`config-modal.slint:320-321,389,484`） | 宽 -80px、高 +60px | 低 |
| 遮罩/弹窗外观 | AntD v6：`rgba(0,0,0,0.45)` 遮罩、圆角 8、阴影 `0 6px 16px rgba(0,0,0,.08)`（库默认；web 仅覆盖玻璃底） | 一致取向：`rgba(0,0,0,0.45)`、radius-lg(10px)、`drop-shadow 60px/18%` 下坠 20px（`modals.slint:77,96-99`） | 阴影更重 | 低 |
| 玻璃/卡片类 | `.glass-card/.glass-surface/.glass-raised`（`globals.css:196-215`，raised 阴影 `0 12px 34px color-mix(…12%)`；`toolbar.surfaceShadow` = `0 14px 34px rgba(28,25,23,.26), 0 2px 6px …`） | 部分：`Theme.glass/glass-strong/toolbar-panel` 色值一致（`theme.slint:44,69-70`），阴影以逐组件 `drop-shadow` 近似（如 `create-menu.slint:29-31` 34px/12% 上抛） | 无统一 raised 阴影 token | 低 |
| 字体/字号 | Tailwind 默认栈（Inter/system）；正文 12–14px、微型标签 10–11px | 一致量级：11/12/13/14px 字面量遍布面板；无字体族声明（Slint 默认） | 字体族未指定 | 低 |
| 亮/暗处理 | `canvasThemes.light/dark` + `.dark` 变量；顶栏月亮图标切换 | 一致：`Theme.dark` 单开关全量重算（`theme.slint:7`），顶栏切换（`top-bar.slint`） | 无 | 低 |
| 焦点态 | AntD/Tailwind `focus-visible` ring、`data-[state=open]:ring-2 ring-ring/20`（`model-picker.tsx:52-54`） | 部分：输入框聚焦改边框色（`texture-dialogs.slint:41` 等 `has-focus ? node-accent : border`），无 ring/外发光 | 焦点反馈弱 | 低 |
| 禁用态 | `disabled:opacity-30/40`、`has-[:disabled]:opacity-35` | 一致：`opacity:0.35`（`plugin-manager.slint:45`）、图层按钮禁用同款 | 无 | 低 |

---

##### UX 交互 (interaction)

基准动效目录：`docs/research/motion-catalog.md`（web 侧数值来源）。

| 项目 | Web 的行为/动效（时长·缓动·文件） | 原生实现 | 差异 | 严重度 |
|---|---|---|---|---|
| 基线 hover 过渡 | Tailwind 默认 **150ms cubic-bezier(0.4,0,0.2,1)**（catalog §0/§4） | 一致：`Theme.motion-fast=150ms` + 同曲线（`theme.slint:167`，全仓 ~20 处 `animate background/border-color`） | 无 | 低 |
| 弹层入场 | `canvas-popover-in` **160ms ease-out**，`translateY(4px)+scale(.98)`（`globals.css:735-740`） | 一致：`create-menu.slint:18-21`、`toolbar.slint:118-124`（ZoomPanel）、`floating.slint:14`、`node-list.slint:131` 均 160ms ease-out + 0.98→1 | 无 | 低 |
| 模态入场 | AntD v6 默认（zoom-big + fade，库内约 200ms） | 近似：`ModalShell` **160ms ease-out** + 0.98→1（`modals.slint:62-71`） | 时长略短（AntD 默认不可见源码） | 低 |
| 拖拽幽灵 chip | 150ms ease-out（`globals.css:807-821`） | 一致：`Theme.motion-ghost=150ms`（`theme.slint:168`、`drag-ghost.slint:12`） | 无 | 低 |
| 连接标签弹入 | 220ms cubic-bezier(.2,.85,.18,1)（`globals.css:842-858`） | 一致：`Theme.motion-label=220ms`（`theme.slint:169`） | 无 | 低 |
| 侧栏开合 | 500ms cubic-bezier(0.22,1,0.36,1)，拖拽中 0ms（`canvas-side-panel.tsx:11-12`） | 一致：`side-panel.slint:71`（`resizing ? 0ms : Theme.panel-motion` 500ms + 同曲线） | 无 | 低 |
| 节点入场/返回 | 420ms cubic-bezier(.2,.85,.18,1)（`.canvas-node-enter`） | 一致：`node-card.slint:512-517` 420ms 同曲线 | 无 | 低 |
| 纹理渐变手柄按下延迟 | 220ms 后才进入拖拽，先点击后拖动不会误移（`gradient-editor.tsx:179,277-305`） | 一致：`GRADIENT_DRAG_DELAY_MS=220` + `gradient_drag_arm`（`texture.rs:53-57,1423-1456,1540-1546`） | 无 | 低 |
| 纹理拖拽/平移/缩放 | 指针 1:1，无缓动（catalog §5） | 一致：`pan_to`/`zoom_at` 直接写属性（`texture.rs:389-449`），无 `animate` | 无 | 低 |
| 长操作进度：GIF 导出 | 进度条 0–100（AntD Progress）+ 不可关闭遮罩（`texture-studio.tsx:767-772`） | 一致：对话框内 6px 进度条 + 百分比 + 运行中禁用 Cancel/Export（`texture-dialogs.slint:765-810`，`texture.rs:62-63` 24ms/tick 分片） | 原生可取消感更强 | 低 |
| 长操作进度：类型/样本缩略图 | "Rendering thumbnails…" spinner + 逐项 "…" 占位（`type-browser.tsx:91-96,135-137`；`use-texture-thumbnails.ts:41-57`） | 缺失：打开即同步渲染全部（`texture_render.rs:229-241`、`texture.rs:346-363`） | 无进度、首开可能冻结 | 中 |
| 长操作进度：预览渲染 | rAF 合帧（`texture-studio.tsx:135-144`） | 一致：脏标记 + `ensure_preview` 同步渲染（`texture.rs:331-344`），60ms tick 轮询尺寸（`texture.rs:1878-1900`） | 无 | 低 |
| 长操作进度：生成 | 节点 loading spinner + 可停止（`canvas-node.tsx` LoadingContent；`canvas-node-prompt-panel` Running→stop） | 一致：节点状态 loading/success/error（`ai_service.rs:500-540`）、面板 Running→stop（`panel_generate`）、重试入口（`content_events.rs:24`） | 无 | 低 |
| 空态覆盖 | 类型"无匹配"、预设库"暂无预设"、成本/日志/Agent 空态、管理器各页签空态 | 部分：管理器（`plugin-manager.slint:130-141`）、成本/日志/Agent（`config-modal.slint:985-988,1084-1088,1024-1034`）有；**类型浏览器无"无匹配"文案**（`texture-dialogs.slint:84-139`）、**预设库无空态**（`texture-dialogs.slint:254-294`） | 少 2 处 | 低 |
| 错误态覆盖 | 导出失败/导入失败 toast、加载失败横幅（管理器）、节点错误+retry | 一致：导出/导入消息行（`texture-dialogs.slint:437-443,758-764`）、管理器反馈行（`plugin-manager.slint:283-290`）、节点 retry（`content_events.rs`） | 管理器缺错误横幅 | 低 |
| 加载态覆盖 | 管理器 Switch/Button `loading`、官方页刷新 spin | 部分：无 loading 反馈（记录式安装瞬时完成，`plugin-manager.slint:215-239` 直接切状态）；官方页无刷新按钮 | 少 spinner | 低 |
| 键盘支持：纹理 | Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y（`texture-studio.tsx:381-397`） | 一致：`shortcuts.rs:66-85`（含输入框避让语义：仅 Ctrl 组合） | View 菜单虚假 `+/-` 提示 | 低 |
| 键盘支持：类型浏览器 | `role=button tabIndex=0` + Enter/Space 选择（`type-browser.tsx:105-114`） | 缺失：TouchArea 无键盘焦点/回车选择（`texture-dialogs.slint:95-102`） | 不可键盘操作 | 低 |
| 键盘支持：画布快捷键参考 | 缩放面板 → Shortcuts 弹窗（`canvas-toolbar.tsx:356-378`） | 一致：ZoomPanel → `open-shortcuts` → `shortcuts-modal.slint`（`toolbar.slint:212-218`）；行数据 `shortcuts-list.slint` | 无 | 低 |
| 可发现性：停靠面板菜单 | DockWindowMenu（图标按钮 + 面板勾选项 + Reset）（`dock-window-menu.tsx:11-33`） | 一致：Dock 菜单 + 勾选 + Reset Layout（`texture.slint:420-439`） | 无 | 低 |
| 可发现性：Dock 工具提示 | `DockTip` 悬停标签（`canvas-toolbar.tsx:74,114`） | 缺失：`toolbar.slint` 无 hover tooltip | 图标无文字提示 | 低 |
| 可发现性：创建菜单分组 | 三组按钮各自弹独立面板，标题 + 图标槽 + 行 hover（`canvas-toolbar.tsx:267-295`） | 部分：组按钮过滤同一菜单（`create-menu.slint:12-99`），行有图标+标题+所属组小字；无标题行 | 形态差异 | 低 |
| 动效减弱（prefers-reduced-motion） | 4 段 `@media (prefers-reduced-motion)` + 侧栏 0ms 路径（`globals.css:769,789,823,860`） | 缺失：全仓 slint 无 reduced-motion 处理 | 无辅助功能开关 | 低 |
| 过渡态禁用玻璃模糊 | `[data-canvas-zooming/dragging]` 关 backdrop-filter（`globals.css:218-226`） | 不适用：Slint 无 backdrop-filter（玻璃为纯色 alpha） | — | — |

---

##### 已达成（本轮核实）

1. **纹理 117/117 类型全部渲染**：`TYPE_ID` 与 `TYPE_ORDER` 逐条等于 web，渲染分派 117 臂全覆盖，测试断言无空白缩略图（`renderer/types/mod.rs`、`registry_data.rs`、`texture.rs:2329-2344`）；分类计数 Basic 10 / Pattern 49 / FX 58 与 web 相同。
2. **GIF 导出 UI + seamless 无缝循环**：fps×duration 帧数、20% 尾段交叉淡化、进度条、可取消，与 web 数值一致（`texture_export.rs:149-180`、`gif_export.rs:22-38`、`texture-dialogs.slint:624-813`）。
3. **渐变编辑器交互补齐**：条上点击新增（含 0.02 最小间距）、220ms 按住拖拽、双击删除（≥2 个）、Reset、16 条 ramp、真实 HSV 方块+色相条（`texture.rs:1423-1476`、`texture-color.slint`）。
4. **参数/变换步进吸附 + 双 Reset**：`snap_to_step` 应用于参数与 7 个 transform 字段；Reset Transform/Parameters 在面板与 Edit 菜单双入口（`texture.rs:546,1208,1229,988-1005`）。
5. **收藏持久化**：类型收藏落盘并可重启恢复（`store.rs:361-374`、`texture.rs:817-854`）。
6. **法线图 Sobel strength=2 对齐 web**，并有专门测试（`texture_export.rs:18-20,677-696`）。
7. **输出面板开关与键盘撤销/重做路由修复**：Animate/Black Background/Checkerboard 三开关经共享 action 路径生效（`texture.rs:584-589,2112-2115` 测试）；纹理视图的 Ctrl+Z/Y 只驱动纹理历史、不污染画布历史（`shortcuts.rs:66-85`、`texture.rs:2140-2161`）。
8. **AI 成本闭环**：估算表逐值移植 + `/generation` 查询 + 成本页签（含 net-new 手动查询框）+ 生成后估算落账 + `~` 前缀（`generation_cost.rs`、`generation.rs`、`cost.rs`、`config_tabs.rs:32-211`）。
9. **视频请求全保真库层**：首/尾帧、参考图/视频/音频、mode 解析、9/3/3/12 与 2–15s、50/15/64MB 上限全部实现并有 HTTP 集成测试（`video_request.rs`、`crates/ai/tests/ai_http.rs`）；声明式插件视频 provider 亦在库层（`plugin_video.rs`）。
10. **账号/音频服务层**：`/key`、`/credits`、`/audio/speech` 实现并测试（`account.rs`、`audio.rs`）。
11. **运行时日志环形缓冲 + 成本/日志/Agent 页签**：配置模态 10/10 页签齐备（`runtime_log.rs`、`config-modal.slint:394-463`）。
12. **插件声明式清单 + 安装态往返 + render-kind + 升级/i18n 助手**：清单解析/校验/规范化/再输出、`InstalledRecord/InstalledRegistry` serde 往返、7 种 render-kind、`compare_semver/has_upgrade/upgrade_label`、`localized/name_for`（`plugins/src/manifest/*`、`manager/*`、`render_kind.rs`）。
13. **Dock 创建菜单分组按钮 + 谜题打开插件管理器**：input/prompt/generator 三组按钮按组过滤（组不可用时禁用），puzzle → 插件管理器（`toolbar.slint:313-339,379`、`plugin_manager.rs:19-49`、`plugin_views.rs:67-111`）。
14. **插件节点原生渲染**：sticky（配色/空态文案/墨色）、markdown（`to_text` + `to_html` 子集含转义与 `javascript:` 过滤）、svg（`resvg` 光栅 ≤720px + 上游采纳）、html（`{{input}}` 逐字符一致）、panorama（2:1 校验 + 上游取图 + 内置生图面板 prompt 前缀）全部有原生实现与测试（`plugin_content.rs`、`crates/plugins/src/official/*`）。

---

##### Top 10 差距（按用户可见影响排序）

1. **【功能·回归 vs web】插件安装/启停状态不持久化**：`InstalledRegistry` 可往返但应用层从不落盘（`store.rs` 无 plugins 读写），重启后官方插件一律回到 enabled、URL 安装记录丢失；web 用 zustand persist（`use-plugin-store.ts:26-45`）。
2. **【功能·回归 vs web】视频生成的帧槽/参考媒体/mode 未接线**：库层全保真（`video_request.rs`）但 `ai_service.rs:345-358` 只走参考图；`metadata.videoSlots` 已能由拖拽绑定（`drop_bindings.rs:248-306`）却没人读，video-prompt 的首/尾帧与 videoMode 在 UI→请求链路上断裂（web：`video.ts:313-347`、`generation-helpers.ts:84`）。
3. **【UI·回归 vs web】所有 ModalShell 弹窗固定 520px**：类型浏览器（web 880）、样本（880）、插件管理器（640）全部挤在 520px，且类型浏览器由网格退化为列表（`modals.slint:91` vs `type-browser.tsx:84,98`、`canvas-plugin-manager-modal.tsx:216`）——纹理浏览与管理器首屏观感明显不同。
4. **【功能·回归 vs web】纹理停靠面板不可拖拽/分栏/持久化**：固定 288px 双列（`texture.slint:453-687`）vs web 的 168–640 拖拽 + 0.2–0.8 分栏 + 面板换边/页签 + localStorage v4（`dock-layout.ts:31-120`、`dock-panel.tsx`）。
5. **【功能·回归 vs web】每节点覆盖断链 3 处**：`reasoningEffort` 面板可写但请求不读（`panel_events.rs:63` vs `ai_service.rs:220-256`）、`videoMode` 端到端缺失、文本模式 `metadata.count` 不生效；web 的 `buildGenerationConfig` 三者都应用（`canvas-generation-helpers.ts:71-87`）。
6. **【功能·回归 vs web】成本只记估算、无实际成本**：生成结束固定 `record_estimate`（`ai_service.rs:576-595`），无响应 `usage.cost` 读取（`cost.rs:17-18` 自述），`estimate_token_cost` 未接线；web 会用响应 usage/`/generation` 记为 api/lookup（`image.ts:294-303`、`use-canvas-generation.ts:125,284`）。
7. **【功能·回归 vs web】账号用量与语音服务在库不在应用**：`fetch_openrouter_key_usage/credits`、`request_speech` 全仓仅测试引用，画布顶栏无用量气泡（web `canvas-account-usage.tsx`），音频能力无入口。
8. **【UX·回归 vs web】纹理缩略图无渐进渲染**：打开浏览器/样本即同步渲染全部可见类型（最多 117×64px，`texture_render.rs:229-241`），web 是懒渲染 + spinner + "…" 占位（`use-texture-thumbnails.ts:41-57`）；另有 350ms 历史合并缺失导致滑杆拖动产生密集撤销点（`texture.rs:319-324` vs `texture-studio.tsx:71,244-251`）。
9. **【功能·回归 vs web】画布图像工具与视频任务 UI 缺失**：悬浮条仅 info/duplicate/插件项/字体/layers/delete（`bars.rs:106-124`）；OCR（`canvas-ocr.ts`）、多角度、AI 放大（皆 API 类，非设计性不可能）、抠图/分割（设计性）与视频任务查询/续跑/帧捕获（`canvas-node-hover-toolbar.tsx:129-152`）均无入口。
10. **【功能/UX·设计性偏差】无 JS 执行引擎带来的插件运行时面**：第三方插件安装=登记（`plugin_manager_model.rs:143-181`）、`setup`/CSS/`Content`/`Panel`/`toolbar`/`onDoubleClick` 无等价物（`abi.rs`、`plugin_views.rs:230-237`）、事件总线与插件存储已实现但零接线（`event_bus.rs`、`storage.rs`）、模型插件脚本与 Canvas Agent 运行时缺失（`model-plugin.ts`、`canvas-agent.ts`）——其中"事件总线/存储未接线"是可闭合的落差，其余为设计取舍。

---

**补充说明（未列入 Top 10 但需备案）**：`texture.rs:1614-1615` 的 GIF 时长上限 0.1–30 与 web 0.5–10 不一致（边界值改动，按 AGENTS.md 需先确认）；精灵表恒定写出 metadata JSON（web 为勾选）；预设导入语义（新增记录 vs 应用并关闭）与同名覆盖缺失；插件管理器缺警告横幅/刷新/升级圆点；运行时日志 200 条不持久化且无分类筛选/复制/下载；无 reduced-motion 处理。以上均已在上表逐行给出双端引用。

## 已达成（本轮核实 · 合并去重）

> 三份报告「已达成」清单的并集，重复项已合并并标注来源（A=画布、B=工作室、C=纹理/插件/AI）。

**画布与全局外壳（A）**

- 画布媒体渲染：图片位图（缩略图优先）、图集接触印相 + 计数片、视频海报帧 + 时长条、资产节点只读列表（`media.rs` / `canvas_content.rs` / `node-card.slint`）。
- 文本/标题就地编辑（双击、Enter/Esc/blur 提交语义、单条历史）（`inline_edit.rs` / `content_events.rs`）。
- 生成中 spinner、错误态 + Retry、媒体信息条（`title · W×H · bytes · ~$cost`）、锁徽标。
- 关联高亮（节点 + 聚焦变暗 160ms）、连线关系/编号标签、连线点选删除、Ctrl+右键裁线。
- 网格三模式 + dock 背景弹层 + 配置 Appearance 持久化。
- 复制/剪切/粘贴（内部寄存器 + 系统剪贴板 JSON 信封、连接重映射、单条历史）。
- 缩放面板（5–500% 滑杆、重置、适配）+ 小地图 + 标尺 + 快捷键弹窗（17 行）+ Ctrl+0/1（净新增）。
- 节点拖放绑定（assets 收集 / stack 吸收 / 视频槽位，含模型帧数限制）+ 拖拽幽灵（`drop_bindings.rs` / `drag-ghost.slint`）。
- 侧栏拖拽调宽 220–480 + `ui.json` 持久化（宽度/开合/主题/语言）。
- 节点列表：类型过滤、层数/选中表头、勾选、批量重命名（按列表序编号、单条历史）。
- 配置弹窗 10/10 页签（providers/models/appearance/generation/local-models/local-storage/cost/agent/logs/about）〔与 C 合并〕。
- 主题：画布/节点/工具栏 token 逐值镜像 + 持久化 + follow-system（2s 轮询注册表）+ 顶栏月亮键〔与 B/C 的明暗 token 行合并〕。
- i18n 外壳脚手架（en-US + zh-CN：顶栏/dock/配置弹窗/快捷键描述）+ 插件清单 `localized/name_for` 助手〔与 C 合并〕。
- 工作区页签路由（有项目回工作区、无项目落按 kind 过滤的库）+ 点击放置（右键/双击空白锚点创建）+ dock 三组创建菜单（按组过滤、组不可用禁用）+ 谜题→插件管理器〔与 C 的创建菜单/谜题行合并〕。

**工作室（B）**

- Image Studio：图层蒙版全套（新增/应用/删除/目标绘制/蒙版视图）；交互式变换（free 旋转手柄 + skew/distort/perspective/warp 网格）；完整文本引擎面板（字符/段落/路径/变形全部控件）；图层拖拽重排 + 组折叠 + 位图/蒙版缩略图 + fx 徽标；标尺拖拽建参考线/移动/越界删除；SV 拾色面板 + 色相条；位图文件导入；输出模态（下载 / 存为画布节点）；Ctrl+T / Ctrl+Shift+T 已绑定。
- Pixel Studio：图形拾色器（SV + 色相条 + 前后景 + 交换/重置）；洋葱皮（1–3 邻居、前后着色）；图层复制/向下合并/拼合/重命名/锁定；调色板 `.gpl`/`.pal` 导入导出；GIF 导出对话框；画布图像节点导入源；坞可见性持久化；agent/headless 域层（`crates/pixel/src/agent.rs`、`headless.rs`）。

**纹理 / 插件 / AI（C）**

- 纹理 **117/117 类型全部渲染**：`TYPE_ID`/`TYPE_ORDER` 与 web 逐条一致，117 臂渲染分派全覆盖，测试断言无空白缩略图；分类计数 Basic 10 / Pattern 49 / FX 58 与 web 相同。
- GIF 导出 UI + seamless 无缝循环（fps×duration、20% 尾段交叉淡化、进度条、可取消，数值与 web 一致）。
- 渐变编辑器交互补齐（条上点击新增含 0.02 最小间距、220ms 按住拖拽、双击删除、Reset、16 条 ramp、真实 HSV 方块+色相条）。
- 参数/变换步进吸附 + 双 Reset（面板与 Edit 菜单双入口）。
- 收藏持久化（重启可恢复）；法线图 Sobel strength=2（有专门测试）。
- 输出面板三开关（Animate/Black Background/Checkerboard）经共享 action 生效；纹理视图 Ctrl+Z/Y 只驱动纹理历史、不污染画布历史（有测试）。
- AI 成本闭环：估算表逐值移植 + `/generation` 查询 + 成本页签（含 net-new 手动查询框）+ 生成后估算落账 + `~` 前缀。
- 视频请求全保真库层：首/尾帧、参考图/视频/音频、mode 解析、9/3/3/12 与 2–15s、50/15/64MB 上限全部实现并有 HTTP 集成测试；声明式插件视频 provider 亦在库层。
- 账号/音频服务层：`/key`、`/credits`、`/audio/speech` 实现并测试（`account.rs`、`audio.rs`）。
- 运行时日志环形缓冲 + 成本/日志/Agent 页签（配置模态 10/10 页签齐备）。
- 插件声明式清单（解析/校验/规范化/再输出）+ 安装态 serde 往返 + 7 种 render-kind + 升级/i18n 助手（`compare_semver/has_upgrade/upgrade_label`）。
- 插件节点原生渲染：sticky（配色/空态/墨色）、markdown（`to_text` + `to_html` 子集含转义与 `javascript:` 过滤）、svg（`resvg` 光栅 ≤720px + 上游采纳）、html（`{{input}}` 逐字符一致）、panorama（2:1 校验 + 上游取图 + prompt 前缀）全部有原生实现与测试。

---

## 设计性偏差（合并）

> 平台性不可能项或明确的设计取舍，**不计入差距**；每条附一行理由。来源：A=画布、B=工作室、C=纹理/插件/AI。

| # | 设计性偏差 | 一行理由 |
|---|---|---|
| 1 | 无 webview / HTML 引擎（A、C） | html 插件 iframe 与 markdown 富渲染无法移植，原生以文本沙箱/子集渲染器替代（`plugin_content.rs`、`official/markdown`）。 |
| 2 | 无 JS 执行引擎（A、C） | web 插件是 React 组件（Blob `import()`），原生改为声明式清单 + 内置原生渲染；模型插件脚本、视频插件任务不移植。 |
| 3 | 无 3D 渲染器（A、C） | panorama（three.js）以占位呈现；2:1 校验、上游取图、prompt 前缀逻辑保留。 |
| 4 | 无 ONNX 运行时（A、B、C） | 背景移除、MobileSAM 分割、本地 AI 放大不移植；本地模型页签仅扫描/打开目录，无推理。 |
| 5 | 无 backdrop-filter（A、B、C） | Slint 能力限制，玻璃/模糊降级为纯 alpha 填充（`theme.slint:69-70`）。 |
| 6 | 无 localStorage / localforage（A、B、C） | 改用磁盘 JSON（`library.json` / `ui.json` / `config.json`）；像素项目为 JSON + base64 PNG。 |
| 7 | 无浏览器存储用量概念（A、C） | Local storage 页签改为真实扫描数据目录，统计对象不同（磁盘 vs 浏览器存储）。 |
| 8 | 无原生文件对话框（B、C） | 导入/导出/位图导入一律「输入路径」提示框代替浏览器下载/选择器。 |
| 9 | 无视频解码器（A） | 视频节点以海报帧 + 时长/状态条代替播放。 |
| 10 | 无浏览器扩展桥与 URL/分享链接面（C） | `browser-cache.ts`、`#tc=` 无入口；`#tc=` 编解码器仅保留在库层（`project.rs:89-101`）。 |
| 11 | 无 agent 运行时（A、B、C） | Canvas Agent SDK / image/pixel agent 命名空间无桥；Agent 审计页签保持诚实空态；pixel agent/headless 域层保留。 |
| 12 | 无 BroadcastChannel 多窗口同步（A） | 桌面单窗口形态，不做跨窗口状态广播。 |
| 13 | 无浏览器级系统剪贴板图片读取（A） | 剪贴板仅文本 JSON 信封；图片/纯文本→节点粘贴缺失。 |
| 14 | 无 CSS 引擎（C） | 插件 CSS 注入（`plugin.css` / `injectCSS`）无法移植。 |
| 15 | 无插件 `setup()`/cleanup 钩子与 `Content`/`Panel`/`toolbar`/`onDoubleClick` 运行时组件面（C） | 依赖 JS 运行时；`PanelKind::Plugin` → None，以声明式面板替代。 |
| 16 | 无 reduced-motion / 系统动效偏好适配（A、B、C） | Slint 侧无动效偏好处理（web 有 4 段 `prefers-reduced-motion` 守卫）。 |
| 17 | 无移动端抽屉 / 移动断点（A） | 桌面形态不移植移动交互；工具选项栏断点行为有意改为常显行。 |
| 18 | 无 WebGL2（C） | 纹理预览以 CPU 光栅替代（`PREVIEW_SIZE` + `ensure_preview`）。 |
| 19 | 无 gif.js worker 编码（C） | GIF 以 CPU 编码替代（workers=2/quality=10 不移植），导出数值对齐。 |
