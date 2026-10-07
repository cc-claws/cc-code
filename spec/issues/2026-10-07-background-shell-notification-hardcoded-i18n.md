# 后台 shell 通知展示文案硬编码中文（未走 i18n）

**状态**：Fixed
**优先级**：低（界面语言一致性；英文环境下每轮后台 shell 通知都显示中文）
**创建日期**：2026-10-07
**分支**：`fix/i18n-background-shell-display`

---

## 一、问题描述

用户语言设置为 **英文** 时，后台 shell 的完成 / 超时 / 取消 / 终止 / 等待输入
通知在聊天区展示的前缀仍是**中文**：

```text
后台 shell 已超时终止: <command> (timed out (execution deadline exceeded))
```

期望（英文环境）：

```text
Background shell timed out: <command> (timed out (execution deadline exceeded))
```

> 注：括注里的 `status` 文本来自 agent 侧，本就是英文——真正被硬编码成中文的
> 只有「前缀动词」。

## 二、根因

`cc-tui/src/app/background_shell.rs::shell_notification_display_text()` 内直接
用中文字面量判断并组装前缀：

```rust
let verb = if status.starts_with("failed") {
    "后台 shell 失败"
} else if status.starts_with("timed out") {
    "后台 shell 已超时终止"
} else if status.starts_with("cancelled") {
    "后台 shell 已取消"
} else if status == "terminated" {
    "后台 shell 已终止"
} else {
    "后台 shell 已完成"
};
```

该函数**处于静态构造路径**：`MessageViewModel::user` / `system` /
`user_with_expanded` / `from_base_message*` 内部都会调用它，把带 XML 的后台通知
转成可读提示。这些构造器是**关联函数（无 `&self`）**，拿不到
`App`/`ServiceRegistry`，因而**也没有 `LcRegistry`**——只能硬编码，绕过了
`tr()`。这是与 2026-05-16（setup 向导）、2026-05-26（login 面板）同族的
「新增文本未走 i18n」问题，但比它们更难，因为调用点没有语言上下文。

## 三、改动

核心难点是**把语言送到没有 `App` 上下文的静态构造路径**。方案：引入
**进程级语言注册表**，静态路径读全局，启动与 `/lang` 切换时同步。

| 文件 | 改动 |
|------|------|
| `cc-tui/src/i18n/mod.rs` | 新增 `init_global(lc)` / `global() -> Arc<LcRegistry>`（`static RwLock<Option<Arc<LcRegistry>>>`，未初始化回退 `en`）；`FluentBundle` 由 `new` 改为 `new_concurrent`（基于 `Mutex` 的 `IntlLangMemoizer`），使 `LcRegistry: Sync`，可置于全局并跨线程（渲染线程）读取；`LcRegistry` 手写 `Clone`（bundle 不支持 `Clone`，改为按当前语言重建） |
| `cc-tui/src/app/background_shell.rs` | 新增 `shell_notification_display_text_with(raw, lc)`；原 `shell_notification_display_text` 委托它并读 `i18n::global()`；5 处动词 + 等待输入前缀改走 `lc.tr("shell-notify-*")` |
| `cc-tui/src/app/mod.rs` | `App::new` 用真实配置 `init_global(lc.clone())` |
| `cc-tui/src/command/session/lang.rs` | `/lang` 切换成功后 `init_global(app.services.lc.clone())` |
| `cc-tui/locales/en/main.ftl` / `zh-CN/main.ftl` | 新增 6 个 key：`shell-notify-failed` / `-timed-out` / `-cancelled` / `-terminated` / `-completed` / `-waiting-input` |
| `cc-tui/src/app/background_shell_test.rs` | 断言中文文案者改走 `*_with(&lc_zh())`；新增英文分支用例 |
| `cc-tui/src/ui/message_view/message_view_test.rs` | 静态构造用例改为**语言无关断言**（只验证保留命令/状态、不泄露 XML），避免依赖进程全局语言 |

**未采用的做法**：给 `MessageViewModel::user/system/from_base_message*` 加
`lc` 参数——有 50+ 处 `system(...)`、40+ 处 `push_system_note(...)` 调用点，
签名改动会波及全仓；进程级注册表可零改动调用点。

## 四、验证

- `cargo build -p cc-tui`：通过，无 warning。
- `cargo clippy -p cc-tui`：无 warning/error。
- 相关套件：`i18n`（11 passed）、`background_shell`（15 passed / 2 ignored）、
  `message_view`（28 passed）、`shell_command`（38 passed）、
  `message_pipeline`（102 passed）全绿。
- CI（PR #338）：macOS / Ubuntu / Windows 三平台 Build 全部 **pass**。
- 已知与本改动无关：本机 `shell_exec::lifecycle_tests` 的 5 个进程清理用例
  （cmd / git-bash / powershell 子进程回收）在本机失败，疑为 Windows 本机环境问题。

## 五、人工验证要点

1. 语言设为 `en`，触发一次后台 shell 完成：前缀应显示
   `Background shell completed`（而非中文）。
2. `/lang zh-CN` 后再触发：前缀应回到 `后台 shell 已完成`。
3. 超时（`Background shell timed out`）、stall 等待输入
   （`Background shell waiting for input`）同理。

## 六、关联

- 同族历史 issue：`spec/archive-issues/2026-05-16-setup-language-step-hardcoded-no-i18n.md`、
  `spec/archive-issues/2026-05-26-login-panel-hardcoded-chinese-no-i18n.md`
- PR：[#338](https://github.com/cc-claws/cc-code/pull/338)
- 触发复盘：报告者以中文提问，助手却用英文回答 → 顺带发现「后台 shell 通知
  语言」与用户语言不一致的硬编码缺陷。
