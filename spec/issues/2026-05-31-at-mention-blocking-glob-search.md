# @ mention 文件搜索性能差 + 多目录搜不到

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-31
**修复日期**：2026-05-31

## 问题描述

输入框输入 `@` 后出现卡顿和 CPU 飙升。虽然已有 300ms debounce 和异步搜索（`spawn_blocking`），但 debounce 间隔不够、异步搜索启动后第一次 glob 依然阻塞。同时，输入 `@side` 搜不到 `side-projects/`，输入 `@issue` 搜不到 `spec/issues/` 下的文件——多个目录均出现搜索遗漏。

## 症状详情

| 维度 | 表现 |
|------|------|
| 触发时机 | 输入 `@` 后打第一个字符立刻卡顿 |
| CPU 表现 | CPU 极高 |
| 内存表现 | `spawn_blocking` 执行 glob 时内存暴涨，大项目（含 node_modules/target）可达数百 MB |
| 搜索结果 | `@side` 搜不到 `side-projects/`、`@issue` 搜不到 `spec/issues/` 下的文件 |
| 持续时间 | 卡顿持续数秒 |

### 症状 1：UI 线程阻塞

`update_at_mention_detection()` 在键盘事件处理中同步调用 `search_files()`，后者执行 `glob::glob()` + `SkimMatcherV2` 模糊匹配。perihelion 项目中 `side-projects/` 子目录包含大量 `node_modules`（daytona 约 6000+ 文件）和 `target`（git-graph 编译产物），glob 遍历需要 stat 数十万文件。

虽有 300ms debounce（`SEARCH_DEBOUNCE_MS`）和缓存机制，但 debounce 间隔不够，连续快速输入时仍有明显卡顿。异步搜索（`spawn_blocking`）已将 glob 移出 UI 线程，但第一次搜索的延迟依然影响体验。

### 症状 2：多个目录搜不到（side-projects、spec/issues 等）

用户输入 `@side`（query = `side`），glob pattern 为 `{cwd}/**/*side*`。`side-projects` 目录名包含 `side`，在文件系统遍历中应该是前几个匹配项之一。但 glob 遍历 `side-projects/` 内部时产生大量 `node_modules` 中的匹配，`MAX_GLOB_RESULTS = 200` 可能导致遍历停留在 `side-projects/` 子目录深处（遍历子目录是深度优先），导致主循环被 `.take(200)` 截断前只产生了子目录内的匹配，而 `side-projects` 本身虽然在早期被匹配到但 `should_ignore` 过滤后的有效结果数量很少，`side-projects` 可能被 fuzzy score 排名挤出前 15（`MAX_CANDIDATES`）。

同理，输入 `@issue` 时，glob 搜索 `{cwd}/**/*issue*`，`spec/issues/` 目录下的 `.md` 文件应被匹配，但可能被同一机制截断或挤出。

### 症状 3（2026-05-31 补充）：debounce 不够

300ms debounce 间隔在快速连续输入时不够，用户打字速度超过 debounce 频率时仍会频繁触发 glob 搜索。表现：连续输入 3-4 个字符后仍感到明显卡顿。

### 症状 4（2026-05-31 补充）：内存暴涨

`spawn_blocking` 线程执行 `glob::glob()` 时会在 tokio 线程池中分配大量内存缓存文件路径。大项目（如 perihelion 含 `side-projects/` 的 node_modules + target）中 glob 遍历数万文件，线程池线程不会被释放回操作系统，导致主进程内存持续偏高。

## 期望方案

用户提议**进程模型**：将文件搜索逻辑拆分为独立子进程，通过 IPC（stdin/stdout JSON）与 TUI 主进程通信：

- 搜索进程按需启动（首次输入 `@` + 字符时 spawn）
- 搜索进程收到 query → 执行 glob + 模糊匹配 → 返回候选列表
- 主进程 debounce（如 200ms）后发送 query 到搜索进程
- **搜索进程闲置 1s 无新 query 时自动退出**，释放所有内存
- 下次触发 `@` 时重新 spawn 搜索进程（冷启动延迟可通过进程预热缓解）

优势：
1. 内存隔离：搜索进程退出后内存完全归还 OS，不影响 TUI 主进程
2. CPU 隔离：glob 密集计算在独立进程中，不阻塞 tokio 线程池
3. 可控生命周期：闲置自动销毁 + 按需启动，不常驻

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 在输入框输入 `@`
  2. 继续输入任意字符（如 `s` 或 `side`）
  3. 观察到 UI 卡顿，CPU 飙升
  4. 候选列表中 `side-projects/` 和 `spec/issues/` 下的文件不可见

## 修复记录（2026-05-31）

**方案**：线程模型（非进程模型），复用 Glob 工具逻辑。

**核心改动**：
1. `file_search.rs`：`glob::glob()` → `walkdir::WalkDir` + `should_skip_dir`（对齐 GlobFilesTool），移除 `MAX_GLOB_RESULTS` 截断
2. `mod.rs`：`tokio::spawn` + `spawn_blocking` + `CancellationToken` → `std::thread::spawn` + `std::sync::mpsc` + `recv_timeout(1s)` idle 退出
3. `keyboard.rs`：`start_async_search(cwd, query)` → `ensure_cwd(cwd)` + `start_search(query)`
4. Debounce 300ms → 200ms

**效果**：
- 搜索遗漏：walkdir + should_skip_dir 在遍历时跳过 node_modules/target，不再被截断，side-projects/spec/issues 均可搜到
- 内存：专用线程（2MB stack），idle 1s 自动退出并 drop 所有数据；不再占用 tokio 线程池
- CPU：glob 密集计算在独立线程，排空队列只处理最新 query
- 性能：200ms debounce + 搜索线程排空，连续输入无卡顿

**涉及文件**：
- `peri-tui/src/app/at_mention/file_search.rs`
- `peri-tui/src/app/at_mention/mod.rs`
- `peri-tui/src/event/keyboard.rs`
- `peri-tui/Cargo.toml`（`glob` → `walkdir`）

## 涉及文件

- `peri-tui/src/app/at_mention/file_search.rs` — `search_files()` 同步 glob + 模糊匹配
- `peri-tui/src/event/keyboard.rs` — `update_at_mention_detection()` 在主线程调用搜索
- `peri-tui/src/app/at_mention/mod.rs` — `AtMentionState` 状态管理、缓存和节流逻辑
