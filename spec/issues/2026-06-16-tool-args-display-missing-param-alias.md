# 工具调用参数摘要缺失：Read/Glob 等工具不显示参数

- **GitHub Issue**：#175 (https://github.com/cc-claws/cc-code/issues/175)

**日期:** 2026-06-16
**状态:** Open
**模块:** peri-tui (tool_display)
**严重程度:** 中（功能缺失，不影响核心流程）

## 问题描述

Read/Glob 等工具调用时，TUI 工具块头部不显示参数摘要。产品预期显示 `Read(D:\code\...\index.php)`，实际只显示 `Read`。

## 复现步骤

1. 启动 Peri Code TUI
2. 让 LLM 调用 Read 工具读取文件
3. 观察工具块头部显示

**预期:** `● ▾ Read(public/index.php)`
**实际:** `● ▾ Read`

## 根因分析

### 1. 参数名别名缺失（主因）

`tool_display.rs:44` 硬编码读取 `input["file_path"]`：
```rust
"Read" | "Write" | "Edit" => input["file_path"].as_str().map(|p| strip_cwd(p, cwd)),
```

LLM 有时使用 `path` 而非 `file_path`（项目已有 `spec/plans/2026-06-02-param-alias-path-to-file_path.md` 记录此问题），导致 `input["file_path"]` 返回 `None`，`format_tool_args` 返回 `None`。

`acp_bridge.rs:142` 用 `.unwrap_or_default()` 静默吞掉 `None`，参数不显示。

### 2. Glob 的 strip_cwd 语义错误（次因）

`tool_display.rs:45-47`：
```rust
"Glob" => input["pattern"]
    .as_str()
    .map(|p| truncate(&strip_cwd(p, cwd), 200)),
```

`strip_cwd` 是路径剥离函数，对 glob pattern（如 `app/admin/controller/**/*.php`）做路径剥离语义不正确。Pattern 不是文件路径，不应走 `strip_cwd`。

### 3. Grep 参数显示正确（对比）

`tool_display.rs:48`：`"Grep" => input["pattern"].as_str().map(|s| truncate(s, 200))` — 直接截断，不走 `strip_cwd`，显示正确。

## 影响范围

- Read 工具：不显示文件路径
- Write 工具：不显示文件路径
- Edit 工具：不显示文件路径
- Glob 工具：pattern 被错误地做路径剥离（可能显示异常）

## 修复方案

### Step 1: 添加参数名别名回退

在 `format_tool_args` 中，当主参数名不存在时，尝试别名：

```rust
// tool_display.rs
"Read" | "Write" | "Edit" => {
    let path = input["file_path"].as_str()
        .or_else(|| input["path"].as_str());
    path.map(|p| strip_cwd(p, cwd))
}
```

### Step 2: Glob 不走 strip_cwd

```rust
"Glob" => input["pattern"]
    .as_str()
    .map(|p| truncate(p, 200)),  // 直接截断，不走 strip_cwd
```

### Step 3: 添加单元测试

```rust
#[test]
fn test_format_tool_args_read_fallback_to_path() {
    let input = serde_json::json!({"path": "/home/user/project/src/main.rs"});
    let result = format_tool_args("Read", &input, Some("/home/user/project/"));
    assert_eq!(result.as_deref(), Some("src/main.rs"));
}

#[test]
fn test_format_tool_args_glob_no_strip_cwd() {
    let input = serde_json::json!({"pattern": "app/admin/**/*.php"});
    let result = format_tool_args("Glob", &input, Some("/home/user/project/"));
    assert_eq!(result.as_deref(), Some("app/admin/**/*.php"));
}
```

## 关联文件

- `peri-tui/src/app/tool_display.rs` — 主要修改
- `peri-tui/src/app/tool_display_test.rs` — 新增测试
- `spec/plans/2026-06-02-param-alias-path-to-file_path.md` — 相关计划（执行层别名，本 issue 是显示层别名）

## 关联 Issues

- 无直接关联 issue，但与 `2026-06-02-param-alias-path-to-file_path` 计划同源
