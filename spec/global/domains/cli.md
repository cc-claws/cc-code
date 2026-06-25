# CLI 领域

## 领域综述

CLI 相关工具链：update、版本管理和与远程脚本的协作。

## 核心流程

（后续填充）

## 技术方案总结

| 维度 | 选型 |
|------|------|
（后续填充）

---

## Issue 经验附录

### issue_2026-05-16-self-update-simplify-to-curl-pipe-bash

**摘要:** update.rs 应简化为 curl 远程脚本 | bash
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** 代码去重, curl-pipe-bash, 双份实现, 维护负担
**问题本质:** Rust 侧重新实现了 install.sh 的完整更新流程（GitHub API、下载、校验、解压），两份代码逻辑重复
**通用模式:** 已有可用的脚本时，Rust 侧应委托执行而非重新实现。减少重复逻辑是降低维护成本的首选策略
**技术决策:** Rust update.rs 精简为 spawn 子进程执行远程脚本 + 流式输出
**涉及文件:** peri-tui/src/self_update.rs → update.rs, scripts/install.sh
**CLAUDE.md 链接:** false

### issue_2026-05-23-continue-flag-not-implemented

**摘要:** -c/--continue 未实现：启动后显示空会话
**状态:** Fixed
**归档日期:** 2026-05-24
**关键词:** -c/--continue, 会话恢复, open_thread, ThreadStore
**问题本质:** main.rs 中的 continue/resume 处理是空壳（仅打日志），未调用已有的 open_thread() 恢复逻辑
**通用模式:** 基础设施已存在但未接线的功能，需要逐个验证入口→基础设施→出口的完整数据流
**涉及文件:** peri-tui/src/main.rs:485-488, peri-tui/src/app/thread_ops.rs:149
**CLAUDE.md 链接:** false
