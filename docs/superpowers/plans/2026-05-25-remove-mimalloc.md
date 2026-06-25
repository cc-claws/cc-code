# Plan: 移除 mimalloc，回退到系统默认分配器

**关联 Issue**: `spec/issues/2026-05-25-mimalloc-worse-than-jemalloc.md`

## 目标

mimalloc 全局分配器在本项目工作负载下内存峰值表现反而比 jemalloc 更差（普通对话即 100MB+ RSS）。决定完全移除第三方全局分配器，回退到系统默认分配器。

## 步骤

### Step 1: 移除 workspace 依赖声明
- **文件**: `Cargo.toml`
- **操作**: 删除 `[workspace.dependencies]` 中 `mimalloc` 和 `libmimalloc-sys` 两行
- **验证**: `cargo check -p peri-tui` 不再引用 `mimalloc`/`libmimalloc-sys`

### Step 2: 移除 peri-tui crate 依赖
- **文件**: `peri-tui/Cargo.toml`
- **操作**: 删除 `[target.'cfg(not(target_os = "windows"))'.dependencies]` 节中的 `mimalloc` 和 `libmimalloc-sys`
- **验证**: 同上

### Step 3: 移除全局分配器声明
- **文件**: `peri-tui/src/main.rs`
- **操作**: 删除第 3-5 行的 `#[global_allocator]` 声明块，以及第 252-255 行的 mimalloc 注释
- **验证**: `cargo build -p peri-tui` 成功（自动使用系统默认分配器）

### Step 4: 替换 `alloc_collect()` 为空操作
- **文件**: `peri-tui/src/app/thread_ops.rs`
- **操作**: 将 `#[cfg(not(target_os = "windows"))]` 分支的 `alloc_collect()` 替换为空函数体，删除 `libmimalloc_sys::mi_collect` 调用和注释
- **验证**: 编译通过，`open_thread`/`new_thread` 中 `alloc_collect()` 调用仍编译

### Step 5: 删除 heapdump 命令
- **文件**:
  - `peri-tui/src/command/core/heapdump.rs` → **删除整个文件**
  - `peri-tui/src/command/core/mod.rs` → 删除 `pub mod heapdump;` 和 `pub use heapdump::HeapdumpCommand;`
  - `peri-tui/src/command/mod.rs` → 删除 `r.register(Box::new(core::heapdump::HeapdumpCommand));`
- **验证**: `cargo build -p peri-tui` 成功，`/heapdump` 命令不可用

### Step 6: 删除旧的 plan 文件
- **文件**: `docs/superpowers/plans/2026-05-25-replace-jemalloc-with-mimalloc.md`
- **操作**: 删除文件
- **验证**: 文件不存在

### Step 7: 全量编译验证
- **命令**: `cargo build` (workspace 全量)
- **预期**: 编译通过，无 mimalloc/libmimalloc-sys 引用，无 /heapdump 命令
