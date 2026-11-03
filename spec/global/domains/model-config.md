# 模型配置 领域

## 领域综述

模型配置领域负责 Provider 和模型的管理，包括 Provider 的 CRUD 操作、四档模型名（opus/sonnet/haiku/fable）内聚配置、/login 与 /model 面板的职责分离。

核心职责：

- ProviderConfig 自包含 ProviderModels 字段，每个 Provider 独立管理四个模型名
- active_provider_id + active_alias 直接解析，移除 ModelAliasMap 间接映射
- /login 面板负责 Provider CRUD（新建/编辑/删除），/model 面板仅负责选择 + Thinking 配置
- Type 切换时自动填充对应 provider_type 的默认模型名

## 核心流程

### Provider 配置管理

```
/login 面板（Browse 模式）
  → 列出所有 Provider
  → Enter 激活 / Tab 进入编辑
  → 编辑模式: 8 个字段（Name/Type/BaseUrl/ApiKey/OpusModel/SonnetModel/HaikuModel/FableModel）
  → Type 切换 → 自动填充默认模型名
  → Enter 保存 → settings.json 原子写回
```

### 模型选择流程

```
/model 面板（不选 Provider）
  → 选择 Alias（opus/sonnet/haiku/fable）标签页
  → 可调: alias / effort / max_tokens / 1M 上下文开关
  → 解析: active_provider_id + active_alias → ProviderConfig.models.{alias}
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 配置存储 | ~/.cc-code/settings.json，AppConfig 统一读写 |
| Provider 数据结构 | ProviderConfig { id, type, baseUrl, apiKey, models: ProviderModels } |
| baseUrl 规范化 | `cc_agent::llm::normalize_base_url` 自动容错：自动识别 localhost/127.0.0.1/内网IP补齐 http://，公网补齐 https://；Anthropic 剥离 /v1 与 /messages，OpenAI 补齐缺省 /v1 并剥离 /chat/completions |
| 别名解析 | active_provider_id + active_alias 直接定位，无间接映射 |
| 别名档位 | 四档 opus/sonnet/haiku/fable，`ProviderModels::ALL_ALIASES: [&str; 4]` |
| 默认模型名 | DEFAULT_MODELS 常量表（每个 type 一组四档元组 (opus, sonnet, haiku, fable)，Type 切换时自动填充） |
| 面板设计 | LoginPanel（Browse/Edit/New/ConfirmDelete）与 ModelPanel 互斥 |
| 向后兼容 | model_aliases 被 serde 安全忽略 |

## Feature 附录

### feature_20261103_F001_base-url-normalization

**摘要:** LLM baseUrl 自动容错清洗与友好错误提示
**关键决策:**

- 新增 `normalize_base_url` 工具：
  - 协议头防呆补齐：无 scheme 时，针对 `localhost`、`127.0.0.1`、`0.0.0.0`、`::1` 及私有内网 IP（`192.168.`、`10.`、`172.16-31.`）自动前插 `http://`；公网域名自动前插 `https://`
  - Anthropic 路径清洗：适配器内部固定拼接 `{base_url}/v1/messages`，因此将用户填写的尾部 `/v1/messages`、`/messages` 或 `/v1` 剥除，彻底避免 `.../v1/v1/messages` 404
  - OpenAI 路径清洗：适配器内部固定拼接 `{base_url}/chat/completions`，剥除尾部 `/chat/completions`，若 URL path 为空则补齐缺省的 `/v1`
- 引入 `format_reqwest_error`：转译底层 reqwest 错误，当捕获到 `is_builder()` 时输出明确的中文提示（包含请求 URL 与修复建议），杜绝冷冰冰的 `builder error: relative URL without a base`
- 全链路防呆：在适配器构造、`cc-acp` 配置解析、`cc-tui` 的 `/login` 面板与 Setup 向导保存入口处统一自动清洗规范化

**归档日期:** 2026-11-03

### feature_20260427_F003_model-config-refactor

**摘要:** Provider 自包含三级别模型名，/login 与 /model 职责分离
**关键决策:**

- ProviderConfig 新增 ProviderModels 字段，opus/sonnet/haiku/fable 模型名内聚到 Provider
- 移除 ModelAliasMap 间接映射，改为 active_provider_id + active_alias 直接解析
- /login 负责 Provider CRUD，/model 仅负责选择 + Thinking
- Type 切换时自动填充对应 provider_type 的默认模型名
- 旧配置格式不兼容，model_aliases 被 serde 安全忽略
- LoginPanel 与 ModelPanel 互斥，同一时间只能打开一个配置面板
**归档:** [链接](../../archive/feature_20260427_F003_model-config-refactor/)
**归档日期:** 2026-04-30

### feature_20260924_F001_fable-fourth-alias

**摘要:** 新增 fable 第四档模型别名，Provider 模型档位由三档扩展为四档
**关键决策:**

- ProviderModels 扩展 fable 字段，`ALL_ALIASES: [&str; 4] = ["opus", "sonnet", "haiku", "fable"]`
- /login 编辑面板新增 FableModel 字段（8 字段），/model 别名选择纳入 fable
- v0.6.76 引入；v0.6.79 修复：当存在既有 cc-code 配置时回填缺失的模型别名，避免老配置缺 fable 字段

**归档日期:** 2026-09-28

---

## 相关 Feature

---
*最后更新: 2026-11-03 — LLM baseUrl 自动容错清洗与友好错误提示（feature_20261103_F001_base-url-normalization）；此前: 2026-09-28 — fable 第四档模型别名（feature_20260924_F001_fable-fourth-alias）*

