# peri-lsp

LSP (Language Server Protocol) 客户端库，为 Agent 提供代码智能能力。

## 概述

`peri-lsp` 实现了 LSP 客户端协议，支持：

- **多语言服务器管理**：连接池管理多个 LSP 服务器
- **底层 JSON-RPC 通信**：通过 `request()` 和 `notify()` 方法发送 LSP 请求
- **诊断信息聚合**：收集和展示代码诊断
- **配置管理**：全局和项目级 LSP 配置
- **自动文件同步**：`did_open`/`did_change`/`did_save` 方法

## 核心模块

| 模块 | 职责 |
|------|------|
| `client.rs` | LSP 客户端实现，JSON-RPC 通信 |
| `pool.rs` | LSP 服务器连接池管理 |
| `config.rs` | LSP 配置加载和解析 |
| `diagnostics.rs` | 诊断信息收集和展示 |
| `jsonrpc/` | JSON-RPC 2.0 协议实现 |
| `protocol/` | LSP 协议类型定义 |
| `error.rs` | 错误类型定义 |

## 使用示例

### 基础用法

```rust
use peri_lsp::{LspClient, DiagnosticsRegistry};
use std::collections::HashMap;

// 创建 LSP 客户端
let diagnostics = DiagnosticsRegistry::new();
let client = LspClient::new(
    "rust-analyzer".to_string(),
    "rust-analyzer".to_string(),
    vec![],
    HashMap::new(),
    None,
    3, // max_restarts
    diagnostics,
);

// 启动服务器
client.start("file:///path/to/workspace").await?;

// 打开文件
client.did_open("file:///path/to/file.rs", "rust", "fn main() {}").await?;

// 发送自定义请求
let result = client.request(
    "textDocument/hover",
    Some(serde_json::json!({
        "textDocument": { "uri": "file:///path/to/file.rs" },
        "position": { "line": 0, "character": 5 }
    })),
    5000, // timeout_ms
).await?;

// 关闭服务器
client.shutdown().await;
```

### 文件同步

```rust
// 打开文件
client.did_open(uri, "rust", content).await?;

// 文件变更
client.did_change(uri, new_content).await?;

// 保存文件
client.did_save(uri).await?;
```

### 诊断信息

```rust
use peri_lsp::{DiagnosticsRegistry, DiagnosticSeverity};

let diagnostics = DiagnosticsRegistry::new();

// 获取文件诊断
let entries = diagnostics.get_file_diagnostics("file:///src/main.rs");

for entry in entries {
    match entry.severity {
        DiagnosticSeverity::Error => println!("错误: {}", entry.message),
        DiagnosticSeverity::Warning => println!("警告: {}", entry.message),
        _ => {}
    }
}
```

### 配置管理

```rust
use peri_lsp::{LspConfigFile, LspServerConfig, load_global_lsp_config};

// 全局配置
let global_config = load_global_lsp_config()?;

// 从配置创建服务器
let config = LspConfigFile {
    lsp_servers: HashMap::new(),
};
```

## 错误处理

```rust
use peri_lsp::LspError;

match client.request("textDocument/hover", params, 5000).await {
    Ok(result) => { /* 处理结果 */ }
    Err(LspError::Timeout) => {
        eprintln!("LSP 请求超时");
    }
    Err(e) => {
        eprintln!("LSP 错误: {}", e);
    }
}
```

## 与 Agent 集成

`peri-lsp` 被 `peri-middlewares` 的 `LspMiddleware` 使用，为 Agent 提供代码智能：

```
Agent 请求 → LspTool → LspMiddleware → peri-lsp → LSP Server
                ↓
        代码智能结果 → Agent 响应
```

`LspMiddleware` 的 `after_tool` 钩子会自动同步文件变更：

```rust
// 文件被 Write/Edit 工具修改后
after_tool → did_change → did_save → LSP Server 更新索引
```

## 依赖关系

```
peri-lsp
  ├── tokio           # 异步运行时
  ├── serde_json      # JSON-RPC
  ├── lsp-types       # LSP 协议类型
  └── lru             # 诊断缓存
```

## 测试

```bash
cargo test -p peri-lsp
```
