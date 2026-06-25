### Acceptance Task: 构建验证 + 端到端检查

#### 前置条件

- Task 1-9 全部完成
- `cargo build` 全量通过

#### 验收步骤

- [ ] **A.1**: 全量构建

```bash
cd /Users/konghayao/code/ai/perihelion
cargo build -p peri-acp -p peri-tui 2>&1
```

预期：编译通过，无编译错误和编译警告。

- [ ] **A.2**: 全量测试

```bash
cargo test -p peri-acp --lib 2>&1
cargo test -p peri-tui --lib 2>&1
```

预期：所有测试通过（新增的 dispatch 单元测试 + 已有测试）。

- [ ] **A.3**: Clippy 检查

```bash
cargo clippy -p peri-acp -p peri-tui -- -D warnings 2>&1
```

预期：无 clippy 警告。

- [ ] **A.4**: 检查 dispatch 模块完整性

```bash
grep "^pub mod\|^pub use" /Users/konghayao/code/ai/perihelion/peri-acp/src/dispatch/mod.rs
```

预期输出应包含：
- `pub mod init;`
- `pub mod list_sessions;`
- `pub mod commands;`
- `pub mod session_load;`
- `pub mod session_fork;`
- 对应的 `pub use` 重导出

- [ ] **A.5**: 检查 stdio handler 注册完整性

```bash
grep -c "on_receive_request\|on_receive_notification" /Users/konghayao/code/ai/perihelion/peri-tui/src/acp_stdio.rs
```

预期：至少 14 个 handler 注册（原有 8 个 + 新增 6-8 个）。

- [ ] **A.6**: 检查重复代码消除

```bash
grep -n "fn build_available_commands\|fn build_stdio_available_commands" /Users/konghayao/code/ai/perihelion/peri-tui/src/acp_server/notify.rs /Users/konghayao/code/ai/perihelion/peri-tui/src/acp_stdio.rs
```

预期：无匹配（原有本地定义已删除，统一使用 dispatch 版本）。

- [ ] **A.7**: 检查 TUI 路径仍使用 dispatch 函数

```bash
grep -n "dispatch::load_session_messages\|dispatch::fork_session\|dispatch::build_available_commands" /Users/konghayao/code/ai/perihelion/peri-tui/src/acp_server/requests.rs
```

预期：有匹配结果（session/load 和 session/fork handler 使用 dispatch 函数）。

- [ ] **A.8**: 检查 ACP_COMPATIBLE.csv 更新

```bash
grep -c "✅" /Users/konghayao/code/ai/perihelion/docs/ACP_COMPATIBLE.csv
```

预期：stdio_transport 列 ✅ 数量从 6 增加到 12-14（取决于 set_thinking 和 cancel_request 是否成功实现）。

- [ ] **A.9**: 验证 TUI 路径不退化

```bash
# 检查 session/load handler 结构完整
rg -A30 '"session/load"' /Users/konghayao/code/ai/perihelion/peri-tui/src/acp_server/requests.rs | head -40
```

预期：handler 使用 `dispatch::load_session_messages()` 而非直接调用 `thread_store.load_messages()`。

#### 失败排查指引

| 失败 | 查看 Task |
|------|----------|
| dispatch 编译错误 | Task 1, 2, 3 |
| acp_stdio.rs 编译错误 | Task 5, 6, 7, 8 |
| 重复代码残留 | Task 4 |
| TUI handler 退化 | Task 4 |
| CSV 不匹配 | Task 9 |
| 测试失败 | Task 2, 3 |

---
