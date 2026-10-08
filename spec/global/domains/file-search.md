# 文件搜索 领域

## 领域综述

文件搜索领域负责代码库中的文件内容搜索功能，采用 **rg CLI 双引擎** 架构：优先调用外部 ripgrep 二进制，探测失败时回退到进程内纯 Rust 引擎，兼顾功能完整性与开箱即用。

核心职责：
- 双引擎解析：优先外部 rg CLI，回退纯 Rust grep/grep-regex crate 进程内搜索
- rg 路径探测：`PERI_RG_PATH` → exe 同级 `bin/rg(.exe)` → 系统 `PATH`，`OnceLock` 单次缓存
- Rust 引擎：grep + grep-regex crate 正则匹配，复用 ignore crate 的 WalkBuilder 目录遍历
- WalkParallel + crossbeam channel + num_cpus 实现多线程并行，15s 超时
- tokio::spawn_blocking 避免阻塞 async runtime

## 核心流程

### 引擎解析（优先级链）

```
resolve_rg()  ← OnceLock 缓存，进程生命周期内只探测一次
  1. $PERI_RG_PATH 环境变量
  2. exe 同级 bin/rg(.exe)（npm install.js 自动下载）
  3. 系统 PATH 探测 `rg --version`
  → 命中: execute_rg_grep / execute_rg_glob（外部 CLI）
  → 未命中: 回退纯 Rust 引擎（execute_rust_grep / ...）
```

### 搜索流程

```
Grep(pattern, path, glob, type, case_insensitive, whole_word, context, head_limit)
  → resolve_rg() 选择引擎
  → [外部 rg] 组装 --json/参数 → 子进程执行 → 解析输出
  → [Rust 引擎] RegexMatcherBuilder 构建 matcher
      → WalkBuilder 配置目录遍历（自动尊重 .gitignore）
      → WalkParallel + num_cpus 线程并行搜索
      → SearchSink 收集结果（content/files_with_matches/count 三种模式）
  → 15 秒超时 + 500 行上限
  → 输出格式与原 rg 工具保持一致
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 引擎架构 | rg CLI 双引擎：外部 ripgrep 优先，回退纯 Rust 引擎 |
| rg 路径探测 | `resolve_rg()`：PERI_RG_PATH → bin/rg(.exe) → PATH，OnceLock 缓存 |
| 二进制分发 | npm install.js 按平台下载 ripgrep 预编译二进制至 bin/ |
| 回退搜索库 | grep 0.4 + grep-regex（ripgrep 底层子 crate） |
| 目录遍历 | ignore crate WalkBuilder + WalkParallel |
| 并行模型 | crossbeam channel + num_cpus 线程 |
| 异步桥接 | tokio::task::spawn_blocking + 15s timeout |
| 接口兼容 | 工具名、参数 schema、description、输出格式保持不变 |
| 关键源码 | `cc-middlewares/src/tools/filesystem/rg_engine.rs`、`grep_args.rs` |

## Feature 附录

### feature_20260430_F003_replace-grep-with-ripgrep
**摘要:** 用 grep+grep-regex crate 替换外部 rg 进程调用实现进程内搜索
**关键决策:**
- 使用 grep + grep-regex crate（ripgrep 底层子 crate）替代 tokio::process::Command 调用 rg
- 复用已有的 ignore crate 的 WalkBuilder 做目录遍历
- 使用 WalkParallel + crossbeam channel 实现多线程并行搜索
- 通过 tokio::task::spawn_blocking 避免阻塞 async runtime，15 秒超时控制
- 工具名、参数 schema、description、输出格式保持不变，LLM 侧无感知
**归档:** [链接](../../archive/feature_20260430_F003_replace-grep-with-ripgrep/)
**归档日期:** 2026-04-30

### feature_20260924_F002_rg-dual-engine

**摘要:** 引入 rg CLI 双引擎——外部 ripgrep 优先，回退纯 Rust 引擎
**关键决策:**

- `resolve_rg()` 按 PERI_RG_PATH → exe 同级 bin/rg(.exe) → 系统 PATH 优先级探测，OnceLock 缓存
- 探测成功走外部 rg CLI（`execute_rg_grep` / `execute_rg_glob`），保证与 ripgrep 行为一致
- 探测失败回退纯 Rust 引擎（grep/grep-regex + WalkParallel 多线程，15s 超时）
- npm install.js 按平台自动下载 ripgrep 预编译二进制至 bin/，实现开箱即用
- 对 LLM 完全透明：工具名、参数 schema、输出格式保持不变

**归档日期:** 2026-09-28

---

## 相关 Feature
- → [agent.md](./agent.md) — FilesystemMiddleware 工具注册
