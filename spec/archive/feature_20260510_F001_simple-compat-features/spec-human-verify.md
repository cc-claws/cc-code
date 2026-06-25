# 简单兼容特性批次 人类验收清单

**Feature:** 20260510_F001 - 简单兼容特性批次
**Plan:** spec-plan.md | **Design:** spec-design.md
**生成时间:** 2026-05-10

---

## 场景 1: 配置文件兼容性（C2 — $schema passthrough）

**用户目标:** settings.json 中包含 `$schema` 字段时，Perihelion 能正常读写，不报错

**触发路径:**
1. 用户在 `~/.peri/settings.json` 中添加 `"$schema": "https://..."` 字段
2. 启动 TUI
3. TUI 正常加载，不抛反序列化错误

**自动验证点:**
- [x] [A] $schema 字段序列化/反序列化 roundtrip: `cargo test -p peri-tui -- config::types 2>&1` → 期望包含: `test result: ok`
  - **来源:** spec-plan.md Task 1 检查步骤
  - **目的:** 验证 passthrough 不影响配置读写

---

## 场景 2: CLAUDE.md 排除 glob（C6 — claudeMdExcludes）

**用户目标:** 配置排除模式后，匹配路径的 CLAUDE.md 不被加载

**触发路径:**
1. 用户在 settings.json 的 `config` 中设置 `"claudeMdExcludes": ["node_modules/**"]`
2. 项目中 `node_modules/some-pkg/CLAUDE.md` 存在
3. 启动 agent，该文件内容不出现于 system prompt

**自动验证点:**
- [x] [A] excludes 匹配时 CLAUDE.md 被跳过 + 不匹配时正常加载: `cargo test -p peri-middlewares -- agents_md 2>&1` → 期望包含: `test result: ok`
  - **来源:** spec-plan.md Task 1 检查步骤
  - **目的:** 验证 glob 排除逻辑正确性

---

## 场景 3: 本地项目配置加载（C1 — CLAUDE.local.md）

**用户目标:** CLAUDE.local.md 内容被追加到 CLAUDE.md 末尾，不入库的个人配置生效

**触发路径:**
1. 项目中同时存在 `CLAUDE.md` 和 `CLAUDE.local.md`
2. 启动 agent
3. 两份内容合并注入 system prompt

**自动验证点:**
- [x] [A] CLAUDE.local.md 合并逻辑全场景: `cargo test -p peri-middlewares -- agents_md 2>&1` → 期望包含: `test result: ok`
  - **来源:** spec-plan.md Task 2 检查步骤
  - **目的:** 验证 local 文件追加、无主文件、空内容等场景

---

## 场景 4: 外部文件引用解析（C4 — @import）

**用户目标:** CLAUDE.md 中 `<!-- @import path -->` 被替换为引用文件内容

**触发路径:**
1. CLAUDE.md 中写入 `<!-- @import docs/guide.md -->`
2. `docs/guide.md` 存在且含内容
3. agent 启动后 system prompt 中出现 guide.md 的内容而非占位符

**自动验证点:**
- [x] [A] @import 解析全场景（简单/嵌套/超深/循环/不存在/非法格式）: `cargo test -p peri-middlewares -- agents_md 2>&1` → 期望包含: `test result: ok`
  - **来源:** spec-plan.md Task 3 检查步骤
  - **目的:** 验证递归解析、深度限制、循环检测

---

## 场景 5: 推理力度调整（T1 — /effort）

**用户目标:** 通过 /effort 命令查看和切换推理力度级别

**触发路径:**
1. TUI 输入 `/effort` → 显示当前级别
2. 输入 `/effort low` → 切换为 low，输出确认
3. 下轮 LLM 调用使用新 effort 级别

**自动验证点:**
- [x] [A] /effort 命令编译与测试: `cargo test -p peri-tui -- effort 2>&1` → 期望包含: `test result: ok`
  - **来源:** spec-plan.md Task 4 检查步骤
  - **目的:** 验证命令注册和基本逻辑

**人工验证点:**
- [x] [H] `/effort` 无参数输出 → TUI 消息区显示 "当前推理力度: high\n用法: /effort low|medium|high" → 是/否
  - **来源:** spec-design.md T1 行为描述
  - **目的:** 验证消息可读性和格式

---

## 场景 6: 会话标题管理（T2 — /rename）

**用户目标:** 通过 /rename 命令修改当前会话标题

**触发路径:**
1. TUI 输入 `/rename` → 显示当前标题
2. 输入 `/rename 我的调试会话` → 更新标题并持久化
3. `/history` 面板显示新标题

**自动验证点:**
- [x] [A] update_title 持久化与 updated_at 更新: `cargo test -p peri-agent -- sqlite_store 2>&1` → 期望包含: `test result: ok`
  - **来源:** spec-plan.md Task 4 检查步骤
  - **目的:** 验证 SQL 更新和时间戳

**人工验证点:**
- [x] [H] `/rename 测试标题` 后 `/history` 面板显示新标题 → 是/否
  - **来源:** spec-design.md T2 验收标准
  - **目的:** 验证端到端标题持久化与显示

---

## 场景 7: 配置健康检查（T5 — /doctor）

**用户目标:** 通过 /doctor 命令快速诊断配置问题

**触发路径:**
1. TUI 输入 `/doctor`
2. 消息区显示 5 项检查结果表格
3. 各项状态正确反映实际环境

**自动验证点:**
- [x] [A] /doctor 命令编译与测试: `cargo test -p peri-tui -- doctor 2>&1` → 期望包含: `test result: ok`
  - **来源:** spec-plan.md Task 5 检查步骤
  - **目的:** 验证命令注册和基本输出

**人工验证点:**
- [x] [H] `/doctor` 输出为 Markdown 表格，包含 Settings/API Key/Provider/MCP/Model Alias 五行 → 是/否（反馈：建议不检测 API Key 环境变量）
  - **来源:** spec-design.md T5 输出格式
  - **目的:** 验证表格格式和完整性

---

## 场景 8: 构建与测试完整性

**用户目标:** 所有改动不引入回归，构建无 warning

**自动验证点:**
- [x] [A] 全量测试通过: `cargo test 2>&1 | tail -10` → 期望包含: `test result: ok`
  - **来源:** spec-plan.md Task 6
  - **目的:** 验证无回归
- [x] [A] 全量构建无 warning: `cargo build 2>&1 | grep -c warning` → 期望精确: `0`
  - **来源:** spec-plan.md Task 6
  - **目的:** 验证代码质量
- [x] [A] TUI crate 编译通过: `cargo build -p peri-tui 2>&1 | tail -3` → 期望包含: `Finished`
  - **来源:** spec-plan.md Task 4/5 检查步骤
  - **目的:** 验证命令集成

---

## 验收后清理

无需额外清理（本批次无新增服务/进程/临时文件）。
