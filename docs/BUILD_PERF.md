# OpenCanvas Desktop 构建性能分析

日期：2026-10-27 · 范围：`crates/app` 的 release 打包链路

## 结论

11 分钟的 release 打包**不是链接器慢**，而是：

> **整个 UI 被 Slint 生成成一个 35 MB / 341,160 行的 Rust 模块，和 65k 行应用逻辑一起放在 `open-canvas-app` 一个 crate 里编译。**

这台机器上实测：

| 项目 | 数值 |
|---|---:|
| Slint 源文件 | 22,633 行 / 40 个 `.slint` |
| 生成文件 `out/app.rs` | 35,251,545 B / 341,160 行 |
| 生成模块里的组件 | 156 个 |
| 生成模块里的 struct/enum | 53 个 |
| 生成模块里的 layout/input 函数 | 1,105 个 |
| `crates/app` Rust | 65,674 行 / 109 个 `.rs` |
| 其中 image 模块 | 29,221 行 |
| 默认 release 打包 | **11m 00s** |
| `opt-level=2 + codegen-units=256` | **9m 49s** |
| 无改动重打包 | 1.92s |
| 全量测试（workspace） | 6m 54s / 2,418 passed |
| 最终 exe | 57–59 MB；`.text` 42 MB，`.rdata` 17 MB，无 `.debug` |
| PDB | 23 MB；`strip=debuginfo` 已开启，lld-link 已在使用 |

## 时间花在哪里

用 `-Z time-passes` 抓到的一次 app crate 编译（默认 release，注意当时有 bilibili 的 release 构建并行）：

| pass | 时间 |
|---|---:|
| `type_check_crate` | 62.2s |
| `MIR_borrow_checking` | 43.2s |
| `monomorphization_collector_graph_walk` | 34.6s |
| `codegen_to_LLVM_IR` | 13.7s |
| `codegen_crate` | 64.1s |
| `LLVM_passes` | 114.9s |
| 峰值 RSS | 7.5 GB |

前端约 2 分钟，IR/LLVM 约 3 分钟；剩下的时间主要花在写 object、链接和写 PDB 上（16 个 CGU 的 object 合计 60 MB，最大一个 14 MB）。所以：

- 单纯加 `-j` 不会救；只有一个 crate，能并行的只有 codegen units。
- 单纯换链接器也不会救；lld-link 已经是较快的选择，且 exe 没有 debug section。
- 单纯把 Rust 模块拆成多个 crate，也**不会**把 341k 行的生成模块拆开——它仍然是 UI crate 里的一个 giant CGU。

## 为什么 `codegen-units=256` 没有想象中快

`cargo build -v` 确认 rustc 确实收到了 `-C codegen-units=256`，但最终仍然只产生 **16 个 `.rcgu.o`**：

```
open_canvas_app...-cgu.00.rcgu.o ... cgu.15.rcgu.o   # 16 个，合计 60 MB
```

原因：生成代码全部在 `mod slint_generatedAppWindow { ... }` 这一个模块里。rustc 的 CGU 划分以模块为基础，单个巨大模块很难再切碎；`codegen-units=256` 只是把应用自身的 65k 行分得更细，Slint 生成模块仍然是那个最大的单元。

实测 `opt-level=2 + codegen-units=256` 只从 11m00s 降到 9m49s（约 11%），且 object 仍是 16 个；这点收益主要来自 `opt-level=2`，不是 codegen units。

## 可选方案

### A. `experimental-module-builds` + `@library` —— 已做 PoC，结论是**不能拆组件代码**

Slint 1.18.1 的 `experimental-module-builds` 可以让一个 crate 用 `.as_library("poc")` 编译 Slint library，父 crate 用 `import { PocCard } from "@poc";` 引用。但 PoC 结果：

| 文件 | 大小 | `InnerPocCard` 出现次数 |
|---|---:|---:|
| 子 library 生成的 `poc.rs` | 41,227 B | 60 |
| 父 crate 生成的 `parent.rs` | 58,012 B | **39** |

父 crate 的 `parent.rs` 里仍然有完整的 `InnerPocCard_root_1`、布局和属性绑定；它只从子 crate 复用了 `PocTheme`（global）和类型。也就是说：

> **`@library` 只共享 global / struct / enum，不共享组件实现；父 crate 仍然把子组件 inline 生成。**

所以按 `@library` 把 Image Studio / Library / Config 拆出去，**不会减少 341k 行生成代码**，打包时间不会降到 4–6 分钟；反而会多一份重复生成。

`@library` 仍然有用的地方：

- `ui-theme`：`Theme`、`I18n`、`StudioTheme` 这些 global 只定义一次；
- `ui-types`：`overlay-types.slint` 的 struct/enum 只定义一次，父/子 crate 用同一套 Rust 类型；
- 避免父/子各自定义同名类型导致的类型不匹配。

### A2. 真正拆组件：`ComponentContainer` + `ComponentFactory`（推荐）

父 `.slint` 只留容器：

```slint
ComponentContainer {
    component-factory: root.studio-factory;
    width: 100%;
    height: 100%;
}
```

Rust 侧注入子组件：

```rust
let studio_handle: Rc<RefCell<Option<ImageStudio>>> = Rc::new(RefCell::new(None));
ui.set_studio_factory(ComponentFactory::new({
    let studio_handle = studio_handle.clone();
    move |_| {
        let studio = ImageStudio::new().ok()?;
        *studio_handle.borrow_mut() = Some(studio.clone_strong());
        Some(studio)
    }
}));
```

- 子组件在自己的 crate 里编译，父 crate 不会生成它的实现；
- 父/子通过 handle 设置属性、接回调；
- `Theme` / `I18n` 需要分别同步到每个子 handle（除非后续验证 `ComponentContainer` 能继承 parent globals）；
- 这是稳定 API，不依赖 experimental feature。

推荐拆分顺序（按风险从低到高）：

| 步骤 | 内容 | 预计全量 | 单视图改动 |
|---|---|---:|---:|
| 1 | `ConfigModal` + `PluginManager` + `Shortcuts` → `ui-config` | 8–9m | 1–2m |
| 2 | `LibraryScreen` + `ExportImport` → `ui-library` | 7–8m | 2–3m |
| 3 | `ImageStudio` + panels/layers/dialogs → `ui-studio` | **5–7m** | 2–4m |
| 4 | 可选：画布 shell 再拆 NodeCard/SidePanel | 4–6m | 2–3m |

注意：以上是**按生成代码量估算**，不是实测；每步都要用 `cargo build --release -p open-canvas-app -j 8` 计时，并用 `ls target-rel2/release/deps/open_canvas_app.*.rcgu.o | wc -l` 确认 CGU 是否真的变小。

### B. `ComponentContainer` 的接线细节（A2 的补充）

父 `.slint` 里放：

```slint
ComponentContainer {
    component-factory: root.studio-factory;
    width: 100%;
    height: 100%;
}
```

Rust 侧：

```rust
ui.set_studio_factory(ComponentFactory::new(move |_| {
    let studio = ImageStudio::new().ok()?;
    // 保存 handle，后续设置属性/回调
    Some(studio)
}));
```

- 子组件在自己的 crate 里编译，父 crate 不重新生成；
- 稳定 API，不依赖 experimental feature；
- 代价：所有属性/回调要跨 handle 手动接线，运行时多一层间接；`Theme` / `I18n` 也要分别同步。
- 适合作为 A 失败时的 fallback，或先拆弹窗这类低耦合组件。

### C. 机械拆出 `open-canvas-ui` crate（中间方案）

把 `build.rs` + `include_modules!` 移到 `crates/ui`，app crate 只保留 65k 行逻辑。

- 只改 Rust 逻辑时：app crate 单独重编，约 **2–3m**（UI crate 缓存）；
- 只改 `.slint` 时：UI crate 仍要重编 341k 行，约 **9–10m**，app crate 还会因依赖变化再编一次；
- 不拆 giant CGU，所以对 UI 改动帮助有限。
- 适合先做，成本低；如果 UI 改动频繁，最终还是需要 A 或 B。

### D. Profile / 链接优化（已做一部分）

- `opt-level=2`：实测 9m49s，保留；
- `codegen-units=256`：没有把 giant module 切开，收益不明确；保留是因为已缓存，后续可再评估；
- `strip="symbols"` / `-C link-arg=/DEBUG:NONE`：可能减少 PDB/链接时间，但会丢掉 crash backtrace 的函数名；当前 `strip=debuginfo` 已去掉 debug section，PDB 仍有 23 MB；
- `panic="abort"`：减小体积/链接，但改变 panic 行为；不建议在崩溃日志还在迭代时改；
- `incremental=true`：可能帮助纯 Rust 改动的二次编译，但 Slint 生成文件一变就失效；需要单独测；
- `lto`：会增加时间，不要开。

### E. Slint 解释器（激进）

用 `slint-interpreter` 在运行时加载 `.slint`，app crate 不再生成 341k 行。

- 打包可能降到 **2–3m**；
- 但所有 UI 接线要改成动态 `Value`/`ComponentInstance`，运行时性能也会下降；
- 对 120fps 画布交互风险太大，不建议作为第一步。

## 推荐顺序

1. **保留当前 `opt-level=2`**，先拿到 9m49s 的基线；新 exe 已同步到快捷方式（`20261027b`）。
2. **先拆 `ConfigModal`**（最小、低耦合）：用 `ComponentContainer` + `ComponentFactory` 把 `ui-config` 做成独立 crate，验证父 crate 不再生成子组件、Theme/I18n 同步和回调跨 handle。
3. Config 通过后拆 **LibraryScreen**，再拆 **Image Studio**（最大、收益最高）。
4. `@library` 只用于 `ui-theme` / `ui-types` 共享 global 和 struct/enum，不用于拆组件实现。
5. 每次拆分后跑：
   - `cargo build --release -p open-canvas-app -j 8` 计时；
   - `ls target-rel2/release/deps/open_canvas_app.*.rcgu.o | wc -l` 看 CGU 是否真的变小；
   - `cargo test --workspace`（约 6m54s / 2,418）；
   - 启动 smoke test + 关键交互（画布拖动、缩放、Image Studio 打开）。

## 测量命令

```bash
cd D:/Dev/open-canvas-desktop
export PATH="/c/Users/antar/.cargo/bin:$PATH"
export CARGO_TARGET_DIR=target-rel2

# 正常 release 打包
time cargo build --release -p open-canvas-app -j 8

# Cargo 的 HTML 时间线
cargo build --release -p open-canvas-app --timings

# rustc pass 级时间（会重编依赖，建议在分支/独立 target 跑）
RUSTC_BOOTSTRAP=1 cargo rustc -p open-canvas-app --release -j 8 -- -Z time-passes

# CGU 数量与体积
ls target-rel2/release/deps/open_canvas_app.*.rcgu.o | wc -l
ls -l target-rel2/release/deps/open_canvas_app.*.rcgu.o | awk '{s+=$5} END {print s/1048576 " MB"}'
```

## 当前改动

- `Cargo.toml` 新增：
  ```toml
  [profile.release.package.open-canvas-app]
  opt-level = 2
  codegen-units = 256
  ```
- 实测 9m49s（deps 已缓存）；profile 改动本身触发过一次 15m10s 的全量依赖重编，后续不会。
- 新 exe：`target/release/open-canvas-app-20261027b.exe`（57 MB），快捷方式已指向它；6 秒启动 smoke test 无崩溃。
