# 贡献指南

感谢你对 cc-code 项目的关注！本文档将帮助你了解如何参与贡献。

## 开发环境

### 依赖

- Rust 2021 edition
- Tokio 异步运行时
- crossterm（终端控制）

### 构建

```bash
# 构建所有 crate
cargo build

# 构建指定 crate
cargo build -p peri-tui

# 运行 TUI
cargo run -p peri-tui

# HITL 审批模式
cargo run -p peri-tui -- -a
```

### 测试

```bash
# 全量测试
cargo test

# 单个 crate 测试
cargo test -p peri-agent

# 单个测试
cargo test -p peri-agent --lib -- test_name
```

### 代码检查

```bash
# 安装 git hooks
lefthook install

# pre-commit 检查（fmt/check/clippy）
lefthook run pre-commit
```

## 项目结构

```
cc-code/
├── peri-agent/        # 核心 Agent 框架
├── peri-middlewares/   # 中间件实现
├── peri-tui/          # TUI 应用
├── peri-acp/          # ACP 服务层
├── peri-widgets/      # Widget 组件库
├── peri-lsp/          # LSP 客户端
├── langfuse-client/   # 遥测客户端
└── spec/              # 设计文档
```

详细说明请参考各 crate 的 README.md。

## 编码规范

### Rust 风格

- Rust 2021 edition
- tokio async/await + async-trait
- 库用 `thiserror`，应用层用 `anyhow::Result`
- 日志用 `tracing`，禁止 `println!`/`eprintln!`

### 文件组织

- 测试与源码分离为同目录 `_test.rs` 文件（≥30 行必须分离）
- 每模块一目录，`mod.rs` 入口
- Workspace resolver = "2"，禁止下层依赖上层

### 命名规范

- 测试命名：`test_<被测对象>_<场景>`
- Mock 命名：`make_` 前缀（函数），`Mock` 前缀（结构体）
- 注释、断言消息用中文

### 字符串处理

- 字符串截断必须用字符级操作：`s.chars().take(N).collect()`
- 终端列宽用 `unicode-width` crate（CJK 占 2 列）

## 提交流程

### 分支命名

格式：`<type>/<name>` 或 `<type>/<scope>/<name>`

| type | 含义 |
|------|------|
| `feat` / `feature` | 新功能 |
| `fix` | 缺陷修复 |
| `perf` | 性能优化 |
| `refactor` | 重构（无行为变化） |
| `docs` | 文档 |
| `chore` | 杂项构建/工具 |
| `test` | 测试 |

示例：`feature/acp-improve`、`fix/windows-pty-and-crlf`

**禁用项**：
- 禁用 `#` 字符（会让 GitHub Actions 静默失效）
- 禁用中文字符（分支名只用英文）
- 禁用 `master`/`main`/`test` 作为开发分支

### Commit 信息

格式：
```
<type>(<scope>): <subject>

<详细描述>

修改内容：
- <文件> <改动>

特性/影响：
- <说明>

Co-Authored-By: Claude <noreply@anthropic.com>
```

示例：
```
feat(acp): add /commit command for one-click git commit

实现一键 git commit 功能，自动生成 commit message。

修改内容：
- peri-acp/src/dispatch/commit.rs 新增 CommitCommand
- peri-tui/src/app/commands.rs 注册 /commit 命令

特性/影响：
- 支持 conventional commits 格式
- 自动检测变更文件并生成描述

Co-Authored-By: Claude claude-sonnet-4.6 <noreply@anthropic.com>
```

### PR 流程

1. 在 [GitHub](https://github.com/cc-claws/cc-code) 创建 Issue
2. 创建 feature/hotfix 分支
3. 开发 + commit
4. 创建 PR，用 `Fixes #N` 关联 Issue
5. 等待 CI 通过
6. 请求 review

## 文档规范

### README.md

每个 crate 都应该有 README.md，包含：

- 一句话概述
- 核心功能列表
- 使用示例
- 依赖关系
- 相关文档链接

### CLAUDE.md

开发指南，包含：

- 模块职责说明
- 执行顺序/数据流
- 陷阱记录（[TRAP] 标记）
- 测试注意事项

### 代码注释

- 默认不写注释
- 只在 WHY 非显而易见时添加：隐藏约束、微妙不变量、特定 bug 的 workaround
- 禁止写多行 docstring

## 测试规范

### 测试风格

- Arrange-Act-Assert，无空行分隔
- 断言优先 `assert_eq!`/`assert!`
- `.unwrap()` 仅用于构造测试数据
- 最小依赖：`assert!`/`assert_eq!`/`matches!` + `tempfile` + `tokio-test`

### 测试隔离

- 禁止写入全局配置
- 用 `App::save_config(cfg, self.config_path_override.as_deref())`
- 使用 tempfile 临时目录

### 示例测试

```rust
#[tokio::test]
async fn test_agent_execute_simple_task() {
    // Arrange
    let agent = ReActAgent::new(MockLLM::always_answer("完成"))
        .max_iterations(1);
    let mut state = AgentState::new("/tmp");

    // Act
    let output = agent.execute(AgentInput::text("测试任务"), &mut state).await.unwrap();

    // Assert
    assert_eq!(output.text, "完成");
    assert_eq!(output.steps, 1);
}
```

## 问题反馈

- [GitHub Issues](https://github.com/cc-claws/cc-code/issues)
- [Discussions](https://github.com/cc-claws/cc-code/discussions)

## 许可证

贡献的代码将按照 [Apache License 2.0](LICENSE) 发布。
