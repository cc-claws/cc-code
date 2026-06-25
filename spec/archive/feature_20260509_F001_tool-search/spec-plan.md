# Tool Search 延迟加载 执行计划

**目标:** 将非核心工具（MCP 工具、Cron 工具等）从 LLM API 调用中移除，通过 SearchExtraTools + ExecuteExtraTool 两个元工具实现按需发现和代理执行，减少 token 开销并保持 prompt cache 稳定。

**技术栈:** Rust 2021, tokio, serde_json, std collections (HashMap/HashSet/BTreeMap), parking_lot RwLock

**设计文档:** ./spec-design.md

## 改动总览

本次改动涉及 2 个 crate：`peri-agent`（executor 工具收集过滤）和 `peri-middlewares`（tool_search 模块：core_tools / search index / 两个元工具 / middleware 集成）。Task 1-4 为纯新增模块（零修改现有文件），Task 5 修改 executor.rs 的工具收集逻辑，Task 6 完成 middleware 集成和 TUI agent 组装。依赖链：Task 1 → Task 2 → Task 3/4（并行）→ Task 5 → Task 6。

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证 cargo workspace 构建可用
  - `cargo build 2>&1 | tail -5`
  - 预期: 输出 "Finished" 且无编译错误
- [x] 验证测试框架可用
  - `cargo test -p peri-agent --lib --no-run 2>&1 | tail -3`
  - 预期: 编译成功，可运行测试
- [x] 验证 peri-middlewares 构建可用
  - `cargo build -p peri-middlewares 2>&1 | tail -3`
  - 预期: 输出 "Finished" 且无编译错误

**检查步骤:**
- [x] workspace 构建通过
  - `cargo build 2>&1 | grep -E "(Finished|error)"`
  - 预期: 包含 "Finished"，无 error
- [x] 测试框架可运行
  - `cargo test -p peri-agent --lib -- --list 2>&1 | head -3`
  - 预期: 列出测试名称，无配置错误


### Task 1: Core Tools 定义与延迟判定

**背景:**
业务语境 — 本 Task 定义工具分类的核心白名单和判定逻辑，为后续延迟加载提供基础。修改原因 — 当前代码中所有工具都直接发送给 LLM，需要区分 Core（始终加载）和 Deferred（按需发现）两类工具。上下游影响 — 本 Task 的 `CORE_TOOLS` 常量和 `is_deferred_tool()` 函数被 Task 2（搜索索引）和 Task 5（executor 过滤）依赖。

**涉及文件:**
- 新建: `peri-middlewares/src/tool_search/mod.rs`
- 新建: `peri-middlewares/src/tool_search/core_tools.rs`
- 修改: `peri-middlewares/src/lib.rs` (添加 `pub mod tool_search;` 声明和 re-export)

**执行步骤:**
- [x] 创建 tool_search 模块目录结构和 mod.rs 入口文件
  - 位置: `peri-middlewares/src/tool_search/mod.rs` (新建)
  - 在文件顶部添加模块文档注释，说明本模块负责工具分类、搜索索引和元工具实现
  - 添加 `pub mod core_tools;` 声明，导出 `core_tools` 子模块
  - 导出公共函数: `pub use core_tools::{is_deferred_tool, CORE_TOOLS, META_TOOLS};`
  - 原因: mod.rs 作为模块入口，统一管理子模块和公共接口

- [x] 实现 CORE_TOOLS 白名单和 META_TOOLS 集合
  - 位置: `peri-middlewares/src/tool_search/core_tools.rs` (新建)
  - 引入依赖: `use std::collections::HashSet;` 和 `use std::sync::LazyLock;`
  - 定义 `CORE_TOOLS` 为 `LazyLock<HashSet<&'static str>>`，包含 12 个工具名:
    - 文件操作 (6 个): "Read", "Write", "Edit", "Glob", "Grep", "folder_operations"
    - 执行 (1 个): "Bash"
    - Web (2 个): "WebFetch", "WebSearch"
    - 交互 (2 个): "Agent", "AskUserQuestion"
    - 管理 (1 个): "TodoWrite"
  - 定义 `META_TOOLS` 为 `Lazy<HashSet<&'static str>>`，包含 2 个元工具名: "SearchExtraTools", "ExecuteExtraTools"
  - 使用 `Lazy` 确保集合只初始化一次，避免运行时重复构建
  - 原因: 白名单需要在编译时确定，Lazy 提供线程安全的惰性初始化

- [x] 实现 is_deferred_tool() 判定函数
  - 位置: `peri-middlewares/src/tool_search/core_tools.rs` (在 CORE_TOOLS 和 META_TOOLS 定义之后)
  - 函数签名: `pub fn is_deferred_tool(tool_name: &str) -> bool`
  - 函数体实现: 返回 `!CORE_TOOLS.contains(tool_name) && !META_TOOLS.contains(tool_name)`
  - 添加单元测试文档注释示例，展示 Core Tool、Meta Tool、Deferred Tool 三种情况的判定结果
  - 原因: 判定逻辑需要覆盖所有工具类型，排除 Core 和 Meta 后剩余即为 Deferred

- [x] 在 lib.rs 中声明 tool_search 模块并导出公共接口
  - 位置: `peri-middlewares/src/lib.rs` (在 `pub mod skills;` 之后，`pub mod tools;` 之前)
  - 添加模块声明: `pub mod tool_search;`
  - 在 re-export 区域（文件中部 `pub use` 语句块）添加导出:
    - `pub use tool_search::{is_deferred_tool, CORE_TOOLS, META_TOOLS};`
  - 原因: 遵循现有 lib.rs 的 `pub mod + pub use` 模式，确保外部可访问公共接口

- [x] 为 core_tools.rs 编写单元测试
  - 测试文件: `peri-middlewares/src/tool_search/core_tools.rs` (在文件末尾添加 `#[cfg(test)] mod tests` 模块)
  - 测试场景:
    - [Core Tool 判定]: 输入 "Read" → 返回 false（is_deferred_tool 返回 false 表示非延迟加载）
    - [Meta Tool 判定]: 输入 "SearchExtraTools" → 返回 false
    - [Deferred Tool 判定]: 输入 "CronRegister" → 返回 true
    - [MCP Tool 判定]: 输入 "mcp__slack__send_message" → 返回 true
    - [未知工具判定]: 输入 "UnknownTool" → 返回 true（未知工具默认为 Deferred）
  - 运行命令: `cargo test -p peri-middlewares --lib tool_search::core_tools::tests`
  - 预期: 所有测试通过，覆盖 Core/Meta/Deferred/未知四种情况

**检查步骤:**
- [x] 验证模块编译通过
  - `cargo build -p peri-middlewares 2>&1 | grep -E "(Compiling|Finished|error)" | head -10`
  - 预期: 输出包含 "Compiling peri-middlewares" 和 "Finished"，无 error

- [x] 验证 CORE_TOOLS 包含全部 12 个核心工具
  - `grep -A 20 'pub static CORE_TOOLS' peri-middlewares/src/tool_search/core_tools.rs | grep -o '"[^"]*"' | wc -l`
  - 预期: 输出为 12（12 个核心工具名）

- [x] 验证 is_deferred_tool 函数可被外部调用
  - `grep -r 'pub use tool_search::is_deferred_tool' peri-middlewares/src/`
  - 预期: 在 lib.rs 中找到一行 re-export 声明

- [x] 验证单元测试覆盖所有分支
  - `cargo test -p peri-middlewares --lib tool_search::core_tools::tests -- --nocapture 2>&1 | grep -E "(test core_tool|test meta_tool|test deferred_tool|test unknown_tool|passed)"`
  - 预期: 输出包含 4 个测试名称，全部 passed


### Task 3: SearchExtraTools 元工具

**背景:**
实现 SearchExtraTools 元工具，提供 LLM 搜索延迟加载工具的能力。当前 LLM 无法感知 deferred tools 的存在，SearchExtraTools 通过查询参数返回匹配的工具列表（含完整 schema），使 LLM 能够发现并选择合适的工具，然后通过 ExecuteExtraTool（Task 4）调用。本 Task 依赖 Task 2 的 ToolSearchIndex.search() 方法，被 Task 5 的 executor 工具收集逻辑使用。

**涉及文件:**
- 新建: `peri-middlewares/src/tool_search/search_tool.rs`

**执行步骤:**
- [x] 创建 SearchExtraTools 结构体实现 BaseTool trait
  - 位置: 新建 `peri-middlewares/src/tool_search/search_tool.rs`
  - 添加依赖引入: `use std::sync::Arc; use async_trait::async_trait; use peri_agent::tools::BaseTool; use serde_json::Value; use crate::tool_search::tool_index::ToolSearchIndex;`
  - 定义结构体: `pub struct SearchExtraTools { index: Arc<ToolSearchIndex> }`
  - 实现构造函数: `pub fn new(index: Arc<ToolSearchIndex>) -> Self { Self { index } }`
  - 原因: 持有 ToolSearchIndex 引用用于搜索操作

- [x] 实现 BaseTool trait 的 name() 和 description() 方法
  - 位置: `search_tool.rs` 中 impl BaseTool 块
  - `name()` 返回: `"SearchExtraTools"`
  - `description()` 返回静态字符串: `"搜索并发现延迟加载的工具。输入关键词，返回匹配的工具列表（含完整 schema）。使用 ExecuteExtraTool 调用发现的工具。"`
  - 原因: 工具名称和描述需与 spec-design.md 第 205-207 行定义一致

- [x] 实现 parameters() 方法返回 JSON Schema
  - 位置: `search_tool.rs` 中 impl BaseTool 块
  - 返回值: `serde_json::json!({ "type": "object", "properties": { "query": { "type": "string", "description": "搜索关键词或自然语言描述" } }, "required": ["query"] })`
  - 原因: 定义输入参数 schema，与 spec-design.md 第 208-211 行一致

- [x] 实现 invoke() 方法执行搜索并返回 JSON 结果
  - 位置: `search_tool.rs` 中 impl BaseTool 块
  - 方法签名: `async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>>`
  - 解析 input: `let query = input.get("query").and_then(|v| v.as_str()).ok_or("SearchExtraTools: 缺少 query 参数")?;`
  - 调用搜索: `let results = self.index.search(query, 10).await;`
  - 构建 total_available: `let total = self.index.total_count();`
  - 序列化结果: `let output = serde_json::json!({ "results": results, "total_available": total });`
  - 返回: `Ok(serde_json::to_string(&output)?)`
  - 原因: 调用 ToolSearchIndex 搜索并返回 JSON 格式结果

- [x] 在 mod.rs 中导出 SearchExtraTools
  - 位置: `peri-middlewares/src/tool_search/mod.rs`
  - 添加: `pub mod search_tool; pub use search_tool::SearchExtraTools;`
  - 原因: 使 ToolSearchMiddleware 可以使用此工具

- [x] 为 SearchExtraTools 编写单元测试
  - 测试文件: `peri-middlewares/src/tool_search/search_tool.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - 正常搜索: 输入 "slack" → 返回包含 mcp__slack__* 工具的 results 数组
    - 空结果: 输入 "nonexistent" → 返回空 results 数组，total_available > 0
    - 缺少 query 参数: 输入 `{}` → 返回 Err
    - 无效 JSON: 输入 `null` → 返回 Err
  - 运行命令: `cargo test -p peri-middlewares --lib tool_search::search_tool`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证文件编译通过
  - `cargo build -p peri-middlewares 2>&1 | grep -E "(error|warning.*search_tool)"`
  - 预期: 无错误或警告

- [x] 验证工具名称和描述正确
  - `cargo test -p peri-middlewares --lib tool_search::search_tool::tests::test_tool_name_is_SearchExtraTools`
  - 预期: 测试通过，工具名称为 "SearchExtraTools"

- [x] 验证 parameters schema 格式正确
  - `cargo test -p peri-middlewares --lib tool_search::search_tool::tests::test_parameters_schema`
  - 预期: 测试通过，schema 包含 query 属性且 required

- [x] 验证搜索功能正常工作
  - `cargo test -p peri-middlewares --lib tool_search::search_tool::tests::test_invoke_search_returns_results`
  - 预期: 测试通过，返回 JSON 字符串包含 results 和 total_available 字段

---

### Task 2: TF-IDF 搜索引擎与关键词搜索

**背景:**
实现工具搜索的核心引擎，支持 TF-IDF 全文检索和关键词精确匹配混合排序。当前系统无工具搜索能力，LLM 无法从大量 deferred tools 中发现目标工具。本 Task 为 SearchExtraTools 元工具提供搜索能力，为 ExecuteExtraTool 提供工具查找能力。本 Task 输出被 Task 3（SearchExtraTools）和 Task 4（ExecuteExtraTool）依赖，本 Task 依赖 Task 1（core_tools 定义）的 CORE_TOOLS 常量。

**涉及文件:**
- 新建: `peri-middlewares/src/tool_search/mod.rs`
- 新建: `peri-middlewares/src/tool_search/keyword_search.rs`
- 新建: `peri-middlewares/src/tool_search/tool_index.rs`

**执行步骤:**
- [x] 创建 `tool_search/mod.rs` 模块入口
  - 位置: 新建文件
  - 定义 `pub mod core_tools; pub mod keyword_search; pub mod tool_index;`（core_tools 由 Task 1 创建，本 Task 创建 keyword_search 和 tool_index）
  - 导出公共类型: `pub use tool_index::{ToolSearchIndex, SearchResult};`
  - 原因: 模块化组织，便于后续元工具和 middleware 引用

- [x] 实现 `keyword_search.rs` 关键词搜索逻辑
  - 位置: 新建文件
  - 实现 `pub fn split_camel_case(name: &str) -> Vec<String>` — CamelCase 分词（`CronCreate` → `["cron", "create"]`），使用正则或手动遍历字符边界
  - 实现 `pub fn split_mcp_prefix(name: &str) -> Vec<String>` — MCP 前缀拆解（`mcp__slack__send_message` → `["slack", "send", "message"]`），按 `__` 分割后取第 2 个及之后部分（跳过 `mcp` 和 server_name）
  - 实现 `pub fn parse_query(query: &str) -> (Vec<String>, Vec<String>)` — 解析查询词，返回 `(required_words, optional_words)`，`+` 前缀词归入 required，其余归入 optional
  - 实现 `pub fn keyword_score(tool_name: &str, tool_desc: &str, required: &[String], optional: &[String]) -> f64` — 计算关键词分数，规则：
    - 必选词缺失 → 0.0 分
    - 必选词全部匹配 → 基础分 1.0
    - 可选词匹配 → 每个加 0.3，工具名精确匹配加 0.5，描述精确匹配加 0.2
  - 原因: 关键词搜索提供精确匹配能力，补充 TF-IDF 语义检索

- [x] 实现 `tool_index.rs` TF-IDF 索引和 ToolSearchIndex 结构体
  - 位置: 新建文件
  - 定义 `pub struct SearchResult { pub name: String, pub description: String, pub parameters: serde_json::Value, pub score: f64 }`
  - 定义内部结构 `struct TfIdfIndex { doc_freqs: HashMap<String, usize>, doc_vectors: HashMap<String, HashMap<String, f64>> }`
  - 实现 `fn tokenize(text: &str) -> Vec<String>` — CJK 字符级分割（每个字符一个 token）+ ASCII 按空格/下划线/连字符分割，转小写
  - 实现 `fn build_tfidf_index(tools: &[Arc<dyn BaseTool>]) -> TfIdfIndex`：
    - 对每个工具的字段（name 权重 3.0、description 权重 2.5）分别 tokenize
    - 计算词频 TF（该词在文档中的加权出现次数）
    - 计算逆文档频率 IDF（log(总文档数 / 包含该词的文档数 + 1)）
    - 存储每个文档的词向量（TF × IDF）
  - 实现 `fn cosine_similarity(vec1: &HashMap<String, f64>, vec2: &HashMap<String, f64>) -> f64` — 余弦相似度计算
  - 定义 `pub struct ToolSearchIndex { tools: RwLock<HashMap<String, Arc<dyn BaseTool>>>, tfidf_index: RwLock<TfIdfIndex> }`
  - 实现 `impl ToolSearchIndex`：
    - `pub fn new() -> Self` — 构造空索引
    - `pub fn build(&self, deferred_tools: Vec<Arc<dyn BaseTool>>)` — 构建 TF-IDF 索引，将工具存入 `tools` HashMap
    - `pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult>` — 混合搜索：
      - 调用 `keyword_search::parse_query(query)` 解析查询词
      - 对每个工具调用 `keyword_score()` 计算关键词分数
      - 对查询词 tokenize 得到查询向量，调用 `cosine_similarity()` 计算 TF-IDF 分数
      - 混合分数 = `关键词分数 × 0.4 + TF-IDF 分数 × 0.6`
      - 按分数降序排序，返回前 `limit` 个结果
    - `pub fn get_tool(&self, name: &str) -> Option<Arc<dyn BaseTool>>` — 从 `tools` HashMap 查找
    - `pub fn list_names(&self) -> Vec<(String, String)>` — 返回 `(name, description)` 元组列表
    - `pub fn format_deferred_list(&self) -> String` — 返回 Markdown 格式列表，每行 `- tool_name: description`
  - 原因: 提供完整的索引构建、搜索、查找能力，使用 RwLock 支持并发读

- [x] 更新 `peri-middlewares/src/lib.rs` 导出 tool_search 模块
  - 位置: 文件末尾添加 `pub mod tool_search;`
  - 原因: 使 tool_search 模块对 crate 外部可见（Task 3/4 需要引用）

- [x] 为 ToolSearchIndex 编写单元测试
  - 测试文件: `peri-middlewares/src/tool_search/tool_index_tests.rs`（或新建 `tests/tool_index_test.rs`）
  - 测试场景:
    - `test_build_index()`: 构建 3 个 mock 工具，验证 `list_names()` 返回正确数量
    - `test_keyword_search()`: 查询 "cron create"，验证 `CronRegister` 工具排在前列
    - `test_tfidf_search()`: 查询 "schedule task"，验证 TF-IDF 分数计算正确
    - `test_hybrid_search()`: 查询 "+slack message"，验证必选词过滤和混合排序
    - `test_get_tool()`: 验证按名称查找返回正确工具
    - `test_format_deferred_list()`: 验证返回 Markdown 格式正确
  - 运行命令: `cargo test -p peri-middlewares --lib tool_search`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证编译通过
  - `cargo build -p peri-middlewares 2>&1 | grep -E "(error|warning:.*tool_search)"`
  - 预期: 无错误，无 tool_search 相关警告
- [x] 验证模块导出正确
  - `grep -n "pub mod tool_search" peri-middlewares/src/lib.rs`
  - 预期: 找到一行声明
- [x] 验证公共 API 可用
  - `grep -E "pub (fn|struct|enum)" peri-middlewares/src/tool_search/*.rs | head -20`
  - 预期: 包含 ToolSearchIndex、search、build、get_tool 等公共接口

---

### Task 5: ReActAgent 工具收集过滤

**背景:**
业务语境 — 本 Task 修改 ReActAgent 的工具收集逻辑，将 deferred tools 过滤出 LLM 可见工具列表，减少 token 消耗并保护 prompt cache 稳定性。修改原因 — 当前 executor.rs L152-168 收集所有工具传给 LLM，随着 MCP 工具增多会导致上下文窗口浪费和缓存失效。上下游影响 — 本 Task 修改 executor.rs 的工具收集逻辑，依赖 Task 1 的 `is_deferred_tool()` 函数定义，本 Task 输出被 Task 6（TUI agent 组装）依赖。

**涉及文件:**
- 修改: `peri-agent/src/agent/executor.rs`

**执行步骤:**
- [x] 在 ReActAgent 结构体新增 tool_filter 字段
  - 位置: `peri-agent/src/agent/executor.rs` 中 ReActAgent 结构体定义（约 L30-50 区域，在现有字段之后）
  - 添加字段: `tool_filter: Option<fn(&str) -> bool>,`
  - 字段类型: `Option<fn(&str) -> bool>`，None 表示不过滤（向后兼容），Some(filter) 表示按 filter 函数过滤工具
  - 原因: 过滤函数由上层（TUI agent 组装时）注入，避免 peri-agent 依赖 peri-middlewares

- [x] 实现 with_deferred_callback() builder 方法
  - 位置: `peri-agent/src/agent/executor.rs` 中 ReActAgent 的 builder 方法区域（约 L80-120，与现有 with_xxx 方法并列）
  - 在结构体中新增字段: `deferred_callback: Option<Arc<dyn Fn(Vec<(String, Arc<dyn BaseTool>)>) + Send + Sync>>`
  - 在 `new()` 方法中初始化为 `None`
  - 方法签名: `pub fn with_deferred_callback(mut self, callback: Arc<dyn Fn(Vec<(String, Arc<dyn BaseTool>)>) + Send + Sync>) -> Self`
  - 方法体: `self.deferred_callback = Some(callback); self`
  - 原因: 提供 builder 接口，使上层（TUI agent 组装时）能接收 deferred tools 并构建 ToolSearchIndex

- [x] 在工具收集过滤逻辑后调用 deferred_callback
  - 位置: `peri-agent/src/agent/executor.rs` 的 execute() 方法中，在 tool_refs 构建之后、`self.chain.run_before_agent(state)` 之前
  - 添加逻辑: 如果 `self.deferred_callback` 为 Some，收集被过滤掉的 deferred tools，将其包装为 `Arc<dyn BaseTool>` 后调用回调
  - 关键伪代码:
    ```rust
    if let Some(ref cb) = self.deferred_callback {
        let deferred: Vec<(String, Arc<dyn BaseTool>)> = all_tools.values()
            .copied()
            .filter(|t| self.tool_filter.map_or(false, |f| f(t.name())))
            .map(|t| (t.name().to_string(), Arc::from(t)))
            .collect();
        cb(deferred);
    }
    ```
  - 注意: `Arc::from(&dyn BaseTool)` 需要 BaseTool 实现对象安全，需使用 `BoxToolWrapper` 包装后转 Arc，或使用 `ArcToolWrapper`。参考 `peri-middlewares/src/tools/mod.rs:25` 的 `BoxToolWrapper` 模式
  - 原因: 将 deferred tools 传递给外部 ToolSearchIndex，使 SearchExtraTools 能搜索到它们

- [x] 修改工具收集逻辑应用过滤
  - 位置: `peri-agent/src/agent/executor.rs` 的 execute() 方法中工具收集部分（约 L168，将 `let tool_refs: Vec<&dyn BaseTool> = all_tools.values().copied().collect();` 替换为过滤逻辑）
  - 替换为以下逻辑:
    ```rust
    let tool_refs: Vec<&dyn BaseTool> = if let Some(filter) = self.tool_filter {
        all_tools.values()
            .copied()
            .filter(|t| !filter(t.name()))
            .collect()
    } else {
        all_tools.values().copied().collect()
    };
    ```
  - 关键逻辑说明: 当 tool_filter 为 Some 时，只保留 filter 返回 false 的工具（即不过滤的工具）；当为 None 时，保留所有工具（向后兼容）
  - all_tools HashMap 保持完整，不受过滤影响，确保 ExecuteExtraTool 可以查找所有工具
  - 原因: 过滤 LLM 可见工具列表，同时保持完整工具集供 ExecuteExtraTool 使用

- [x] 为工具过滤逻辑编写单元测试
  - 测试文件: `peri-agent/src/agent/executor.rs` 的 `#[cfg(test)] mod tests` 模块（在现有测试之后追加）
  - 测试场景:
    - [无过滤行为]: 不设置 tool_filter，验证所有工具都包含在 tool_refs 中（向后兼容）
    - [过滤 Cron 工具]: 设置 filter 过滤 "Cron" 前缀工具，验证 tool_refs 不包含 CronRegister/CronList/CronRemove
    - [过滤 MCP 工具]: 设置 filter 过滤 "mcp__" 前缀工具，验证 tool_refs 不包含 mcp__read_resource
    - [元工具不过滤]: 设置 filter，验证 SearchExtraTools 和 ExecuteExtraTool 仍在 tool_refs 中（假设 filter 对它们返回 false）
    - [all_tools 完整性]: 设置 filter，验证 all_tools HashMap 仍包含所有工具（包括被过滤的）
  - Mock 工具构建: 使用 `MockTool` 结构体或简单实现 `BaseTool` trait 的测试工具
  - 运行命令: `cargo test -p peri-agent --lib agent::executor::tests::test_tool_filter`
  - 预期: 所有测试通过，验证过滤逻辑正确且向后兼容

**检查步骤:**
- [x] 验证编译通过
  - `cargo build -p peri-agent 2>&1 | grep -E "(Compiling peri-agent|Finished|error)"`
  - 预期: 输出包含 "Compiling peri-agent" 和 "Finished"，无 error

- [x] 验证 tool_filter 字段存在
  - `grep -n "tool_filter: Option<fn(&str) -> bool>" peri-agent/src/agent/executor.rs`
  - 预期: 找到一行字段声明

- [x] 验证 with_tool_filter 方法存在
  - `grep -n "pub fn with_tool_filter" peri-agent/src/agent/executor.rs`
  - 预期: 找到一行方法声明

- [x] 验证过滤逻辑正确应用
  - `grep -A 5 "let tool_refs: Vec<&dyn BaseTool>" peri-agent/src/agent/executor.rs | grep -E "(filter|collect)"`
  - 预期: 找到包含 filter 逻辑的代码段

- [x] 验证单元测试通过
  - `cargo test -p peri-agent --lib agent::executor::tests::test_tool_filter -- --nocapture 2>&1 | grep -E "(test |passed|FAILED)"`
  - 预期: 输出包含所有测试名称，全部 passed，无 FAILED

---

### Task 4: ExecuteExtraTool 元工具

**背景:**
业务语境 — 本 Task 实现延迟加载工具的代理执行机制，使 LLM 能够通过 ExecuteExtraTool 调用任何已发现的 deferred tool。修改原因 — 当前 deferred tools 不在 LLM 工具列表中，无法直接调用，需要通过元工具代理执行。上下游影响 — 本 Task 依赖 Task 2（ToolSearchIndex.get_tool()），被 Task 5 的 executor 工具收集逻辑使用，与 Task 3（SearchExtraTools）形成完整的延迟工具调用链：搜索发现 → 代理执行。

**涉及文件:**
- 新建: `peri-middlewares/src/tool_search/execute_tool.rs`

**执行步骤:**
- [x] 创建 ExecuteExtraTool 结构体实现 BaseTool trait
  - 位置: 新建 `peri-middlewares/src/tool_search/execute_tool.rs`
  - 添加依赖引入: `use std::sync::Arc; use async_trait::async_trait; use peri_agent::tools::BaseTool; use serde_json::Value; use crate::tool_search::tool_index::ToolSearchIndex;`
  - 定义结构体: `pub struct ExecuteExtraTool { index: Arc<ToolSearchIndex> }`
  - 实现构造函数: `pub fn new(index: Arc<ToolSearchIndex>) -> Self { Self { index } }`
  - 原因: 持有 ToolSearchIndex 引用用于查找目标 deferred tool

- [x] 实现 BaseTool trait 的 name() 和 description() 方法
  - 位置: `execute_tool.rs` 中 impl BaseTool 块
  - `name()` 返回: `"ExecuteExtraTool"`
  - `description()` 返回静态字符串: `"代理执行延迟加载的工具。输入目标工具名称和参数，返回执行结果。使用 SearchExtraTools 发现可用工具。"`
  - 原因: 工具名称和描述需与 spec-design.md 第 230 行定义一致

- [x] 实现 parameters() 方法返回 JSON Schema
  - 位置: `execute_tool.rs` 中 impl BaseTool 块
  - 返回值: `serde_json::json!({ "type": "object", "properties": { "tool_name": { "type": "string", "description": "延迟加载工具的名称" }, "params": { "type": "object", "description": "目标工具的参数" } }, "required": ["tool_name", "params"] })`
  - 原因: 定义输入参数 schema，与 spec-design.md 第 232-237 行一致

- [x] 实现 invoke() 方法执行代理调用
  - 位置: `execute_tool.rs` 中 impl BaseTool 块
  - 方法签名: `async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>>`
  - 解析 tool_name: `let tool_name = input.get("tool_name").and_then(|v| v.as_str()).ok_or("ExecuteExtraTool: 缺少 tool_name 参数")?;`
  - 解析 params: `let params = input.get("params").ok_or("ExecuteExtraTool: 缺少 params 参数")?.clone();`
  - 查找目标工具: `let tool = self.index.get_tool(tool_name).ok_or(format!("ExecuteExtraTool: 工具 '{}' 不存在或未注册为延迟工具", tool_name))?;`
  - 代理执行: `let result = tool.invoke(params).await?;`
  - 返回: `Ok(result)`
  - 原因: 从 ToolSearchIndex 查找目标工具并代理执行，透传原始错误信息

- [x] 在 mod.rs 中导出 ExecuteExtraTool
  - 位置: `peri-middlewares/src/tool_search/mod.rs`
  - 添加: `pub mod execute_tool; pub use execute_tool::ExecuteExtraTool;`
  - 原因: 使 ToolSearchMiddleware 可以使用此工具

- [x] 为 ExecuteExtraTool 编写单元测试
  - 测试文件: `peri-middlewares/src/tool_search/execute_tool.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - 正常代理执行: 输入 tool_name="CronRegister", params={...} → 返回 CronRegister.invoke() 的结果
    - 工具未找到: 输入 tool_name="UnknownTool" → 返回 Err 包含 "不存在或未注册为延迟工具"
    - 缺少 tool_name 参数: 输入 `{"params": {}}` → 返回 Err 包含 "缺少 tool_name 参数"
    - 缺少 params 参数: 输入 `{"tool_name": "CronRegister"}` → 返回 Err 包含 "缺少 params 参数"
    - 目标工具执行失败: 目标工具 invoke 返回 Err → ExecuteExtraTool 透传原始错误
  - 运行命令: `cargo test -p peri-middlewares --lib tool_search::execute_tool`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证文件编译通过
  - `cargo build -p peri-middlewares 2>&1 | grep -E "(error|warning.*execute_tool)"`
  - 预期: 无错误或警告

- [x] 验证工具名称和描述正确
  - `cargo test -p peri-middlewares --lib tool_search::execute_tool::tests::test_tool_name_is_ExecuteExtraTool`
  - 预期: 测试通过，工具名称为 "ExecuteExtraTool"

- [x] 验证 parameters schema 格式正确
  - `cargo test -p peri-middlewares --lib tool_search::execute_tool::tests::test_parameters_schema`
  - 预期: 测试通过，schema 包含 tool_name 和 params 两个属性且 required

- [x] 验证代理执行逻辑正常工作
  - `cargo test -p peri-middlewares --lib tool_search::execute_tool::tests::test_invoke_executes_deferred_tool`
  - 预期: 测试通过，成功查找并执行目标工具

- [x] 验证错误处理逻辑正确
  - `cargo test -p peri-middlewares --lib tool_search::execute_tool::tests::test_tool_not_found_returns_error`
  - 预期: 测试通过，未找到工具时返回结构化错误消息

---

### Task 6: ToolSearchMiddleware 集成与 TUI 组装

**背景:**
业务语境 — 本 Task 完成 ToolSearch 功能的集成，将 ToolSearchMiddleware 注入中间件链，在 TUI 的 agent.rs 中完成组装，确保延迟工具列表正确注入 system prompt，HITL 正确处理 ExecuteExtraTool 的权限透传。修改原因 — 当前代码中没有 ToolSearchMiddleware，需要新建并集成到中间件链；HITL middleware 不支持 ExecuteExtraTool 的权限透传，需要特殊处理。上下游影响 — 本 Task 依赖 Task 1-5 的所有输出（ToolSearchIndex、SearchExtraTools、ExecuteExtraTool、core_tools 定义），是整个功能的最后集成步骤，完成后即可在 TUI 中使用延迟工具加载。

**涉及文件:**
- 新建: `peri-middlewares/src/tool_search/middleware.rs`
- 修改: `peri-middlewares/src/tool_search/mod.rs`（导出 ToolSearchMiddleware）
- 修改: `peri-middlewares/src/lib.rs`（导出 ToolSearchMiddleware）
- 修改: `peri-tui/src/app/agent.rs`（组装 ToolSearchMiddleware + 注入 tool_filter）
- 修改: `peri-middlewares/src/hitl/mod.rs`（HITL 权限透传）

**执行步骤:**
- [x] 创建 ToolSearchMiddleware 实现 Middleware trait
  - 位置: 新建 `peri-middlewares/src/tool_search/middleware.rs`
  - 添加依赖引入: `use std::sync::Arc; use async_trait::async_trait; use peri_agent::middleware::r#trait::Middleware; use peri_agent::agent::state::State; use peri_agent::error::AgentResult; use peri_agent::tools::BaseTool; use crate::tool_search::tool_index::ToolSearchIndex; use crate::tool_search::search_tool::SearchExtraTools; use crate::tool_search::execute_tool::ExecuteExtraTool;`
  - 定义结构体: `pub struct ToolSearchMiddleware { tool_search_index: Arc<ToolSearchIndex> }`
  - 实现构造函数: `pub fn new(tool_search_index: Arc<ToolSearchIndex>) -> Self { Self { tool_search_index } }`
  - 实现 `Middleware<S>` trait 的 `name()` 方法: 返回 `"ToolSearch"`
  - 实现 `collect_tools()` 方法: 返回 `vec![Box::new(SearchExtraTools::new(Arc::clone(&self.tool_search_index))) as Box<dyn BaseTool>, Box::new(ExecuteExtraTool::new(Arc::clone(&self.tool_search_index)))]`
  - 实现 `before_agent()` 方法: 调用 `self.tool_search_index.format_deferred_list()` 生成延迟工具列表，调用 `state.prepend_message(BaseMessage::system(prompt))` 注入 system prompt，其中 prompt 为格式化后的延迟工具列表
  - 原因: ToolSearchMiddleware 负责注册元工具和注入延迟工具列表到 system prompt

- [x] 在 tool_search/mod.rs 中导出 ToolSearchMiddleware
  - 位置: `peri-middlewares/src/tool_search/mod.rs`
  - 添加: `pub mod middleware; pub use middleware::ToolSearchMiddleware;`
  - 原因: 使外部可以使用 ToolSearchMiddleware

- [x] 在 lib.rs 中导出 ToolSearchMiddleware
  - 位置: `peri-middlewares/src/lib.rs`（在现有 tool_search 导出区域）
  - 添加: `pub use tool_search::ToolSearchMiddleware;`
  - 原因: 使 peri-tui 可以导入 ToolSearchMiddleware

- [x] 在 agent.rs 中创建 ToolSearchIndex 并组装 ToolSearchMiddleware
  - 位置: `peri-tui/src/app/agent.rs` 的 `run_universal_agent()` 函数中（约 L269-339 区域）
  - 在 L269 `ReActAgent::new(model)` 之前添加: `let tool_search_index = Arc::new(peri_middlewares::tool_search::ToolSearchIndex::new());`
  - 在 L270 `.max_iterations(500)` 之后添加: `.with_deferred_callback(Arc::new({ let idx = Arc::clone(&tool_search_index); move |deferred: Vec<(String, Arc<dyn BaseTool>)>| { let tools: Vec<Arc<dyn BaseTool>> = deferred.into_iter().map(|(_, t)| t).collect(); idx.build(tools); }))`
  - 在 L335 MCP 中间件注册之后添加: `let executor = executor.add_middleware(Box::new(peri_middlewares::tool_search::ToolSearchMiddleware::new(Arc::clone(&tool_search_index))));`
  - 原因: deferred_callback 在 execute() 内部工具收集后调用，将 deferred tools 传入 ToolSearchIndex；ToolSearchMiddleware 必须在 MCP 之后注册，确保 MCP 工具已在 all_tools 中

- [x] 在 agent.rs 中注入 tool_filter 到 ReActAgent
  - 位置: `peri-tui/src/app/agent.rs` 的 `run_universal_agent()` 函数中（在 L269 `ReActAgent::new(model)` 链式调用中添加）
  - 在 L270 `.max_iterations(500)` 之后添加: `.with_tool_filter(peri_middlewares::tool_search::is_deferred_tool)`
  - 原因: 过滤 deferred tools 不传给 LLM，减少 token 消耗

- [x] 在 HITL middleware 中实现 ExecuteExtraTool 权限透传
  - 位置: `peri-middlewares/src/hitl/mod.rs`
  - 在 `default_requires_approval()` 函数之后（约 L50 之后）添加新函数 `pub fn effective_tool_name(tool_name: &str, input: &serde_json::Value) -> String`，实现逻辑: 当 tool_name == "ExecuteExtraTool" 时，从 input["tool_name"] 提取目标工具名并返回，否则返回 tool_name
  - 修改 `before_tool()` 方法（约 L346-364），将 `if !(self.requires_approval)(&tool_call.name)` 改为 `if !(self.requires_approval)(&effective_tool_name(&tool_call.name, &tool_call.input))`
  - 修改 `process_batch()` 方法中的权限判断逻辑（约 L152），将 `requires_approval(&tool.name)` 改为 `requires_approval(&effective_tool_name(&tool.name, &tool.input))`
  - 原因: HITL 需要基于目标工具名判断是否需要审批，而非 ExecuteExtraTool 本身

- [x] 为 ToolSearchMiddleware 编写单元测试
  - 测试文件: `peri-middlewares/src/tool_search/middleware.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - [collect_tools 返回元工具]: 调用 `collect_tools()`，验证返回 2 个工具，名称为 SearchExtraTools 和 ExecuteExtraTool
    - [before_agent 注入 system prompt]: Mock ToolSearchIndex 返回延迟工具列表，调用 `before_agent()`，验证 state.messages() 第一条为 system 消息且包含延迟工具列表
    - [format_deferred_list 调用]: 验证 `before_agent()` 调用了 `tool_search_index.format_deferred_list()`
  - 运行命令: `cargo test -p peri-middlewares --lib tool_search::middleware`
  - 预期: 所有测试通过

- [x] 为 HITL 权限透传编写单元测试
  - 测试文件: `peri-middlewares/src/hitl/mod.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - [effective_tool_name 直接工具]: 输入 tool_name="Bash"，返回 "Bash"
    - [effective_tool_name ExecuteExtraTool]: 输入 tool_name="ExecuteExtraTool"，input={"tool_name": "mcp__slack__send_message"}，返回 "mcp__slack__send_message"
    - [effective_tool_name 缺少 tool_name]: 输入 tool_name="ExecuteExtraTool"，input={}，返回 "ExecuteExtraTool"（降级为元工具本身）
    - [before_tool 透传权限]: Mock ExecuteExtraTool 调用，目标工具为 mcp__slack__send_message，验证 `default_requires_approval` 对目标工具名执行判断
  - 运行命令: `cargo test -p peri-middlewares --lib hitl::tests::test_execute_extra_tool_permission`
  - 预期: 所有测试通过

- [x] 为 agent.rs 组装编写集成测试
  - 测试文件: `peri-tui/src/app/agent.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - [tool_search_index 创建]: 验证 `run_universal_agent()` 中创建了 ToolSearchIndex
    - [middleware 注册顺序]: 验证 ToolSearchMiddleware 在 MCP 中间件之后注册
    - [tool_filter 注入]: 验证 ReActAgent 调用了 `with_tool_filter(is_deferred_tool)`
    - [MCP 成功时 callback]: Mock MCP pool 初始化成功，验证 deferred_callback 被调用，ToolSearchIndex.build 被执行
    - [MCP 失败时 callback]: Mock MCP pool 初始化失败，验证 callback 不被调用，ToolSearchIndex 保持空
  - 运行命令: `cargo test -p peri-tui --lib app::agent::tests::test_tool_search_integration`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 ToolSearchMiddleware 编译通过
  - `cargo build -p peri-middlewares 2>&1 | grep -E "(Compiling peri-middlewares|Finished|error)"`
  - 预期: 输出包含 "Compiling peri-middlewares" 和 "Finished"，无 error

- [x] 验证 ToolSearchMiddleware 导出正确
  - `grep -n "pub use tool_search::ToolSearchMiddleware" peri-middlewares/src/lib.rs`
  - 预期: 找到一行 re-export 声明

- [x] 验证 agent.rs 中间件注册顺序
  - `grep -A 3 "McpMiddleware::new" peri-tui/src/app/agent.rs | grep -c "ToolSearchMiddleware"`
  - 预期: 找到 ToolSearchMiddleware 在 MCP 之后注册

- [x] 验证 agent.rs tool_filter 注入
  - `grep -n "with_tool_filter" peri-tui/src/app/agent.rs`
  - 预期: 找到一行 `with_tool_filter(peri_middlewares::tool_search::is_deferred_tool)`

- [x] 验证 HITL effective_tool_name 函数存在
  - `grep -n "pub fn effective_tool_name" peri-middlewares/src/hitl/mod.rs`
  - 预期: 找到一行函数声明

- [x] 验证 HITL before_tool 使用 effective_tool_name
  - `grep -A 2 "if !(self.requires_approval)" peri-middlewares/src/hitl/mod.rs | grep "effective_tool_name"`
  - 预期: 找到 effective_tool_name 调用

- [x] 验证 ToolSearchMiddleware 单元测试通过
  - `cargo test -p peri-middlewares --lib tool_search::middleware -- --nocapture 2>&1 | grep -E "(test |passed|FAILED)"`
  - 预期: 输出包含所有测试名称，全部 passed，无 FAILED

- [x] 验证 HITL 权限透传单元测试通过
  - `cargo test -p peri-middlewares --lib hitl::tests::test_execute_extra_tool_permission -- --nocapture 2>&1 | grep -E "(test |passed|FAILED)"`
  - 预期: 输出包含所有测试名称，全部 passed，无 FAILED

- [x] 验证 agent.rs 集成测试通过
  - `cargo test -p peri-tui --lib app::agent::tests::test_tool_search_integration -- --nocapture 2>&1 | grep -E "(test |passed|FAILED)"`
  - 预期: 输出包含所有测试名称，全部 passed，无 FAILED

- [x] 验证 TUI 构建成功
  - `cargo build -p peri-tui 2>&1 | grep -E "(Compiling peri-tui|Finished|error)"`
  - 预期: 输出包含 "Compiling peri-tui" 和 "Finished"，无 error

---

### Task 7: Tool Search 延迟加载 验收

**前置条件:**
- 启动命令: `cargo build -p peri-tui`
- 所有 Task 0-6 已完成

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test 2>&1 | tail -20`
   - 预期: 全部测试通过，无 FAILED
   - 失败排查: 检查各 Task 的单元测试步骤

2. 验证 CORE_TOOLS 白名单完整（12 个核心工具）
   - `cargo test -p peri-middlewares --lib tool_search::core_tools::tests -- --nocapture 2>&1 | grep -E "(passed|FAILED)"`
   - 预期: 所有 Core/Meta/Deferred 判定测试通过
   - 失败排查: 检查 Task 1 `core_tools.rs`

3. 验证搜索引擎正确工作
   - `cargo test -p peri-middlewares --lib tool_search::tool_index -- --nocapture 2>&1 | grep -E "(passed|FAILED)"`
   - 预期: 索引构建、搜索排序、工具查找测试通过
   - 失败排查: 检查 Task 2 `tool_index.rs` 和 `keyword_search.rs`

4. 验证元工具正确实现
   - `cargo test -p peri-middlewares --lib tool_search::search_tool -- --nocapture 2>&1 | grep -E "(passed|FAILED)"`
   - `cargo test -p peri-middlewares --lib tool_search::execute_tool -- --nocapture 2>&1 | grep -E "(passed|FAILED)"`
   - 预期: SearchExtraTools 和 ExecuteExtraTool 测试通过
   - 失败排查: 检查 Task 3 `search_tool.rs` 和 Task 4 `execute_tool.rs`

5. 验证 executor 工具过滤逻辑
   - `cargo test -p peri-agent --lib agent::executor::tests::test_tool_filter -- --nocapture 2>&1 | grep -E "(passed|FAILED)"`
   - 预期: 过滤逻辑测试通过，向后兼容
   - 失败排查: 检查 Task 5 `executor.rs`

6. 验证中间件集成和 HITL 权限透传
   - `cargo test -p peri-middlewares --lib tool_search::middleware -- --nocapture 2>&1 | grep -E "(passed|FAILED)"`
   - `cargo test -p peri-middlewares --lib hitl::tests::test_execute_extra_tool_permission -- --nocapture 2>&1 | grep -E "(passed|FAILED)"`
   - 预期: ToolSearchMiddleware 和 HITL 权限透传测试通过
   - 失败排查: 检查 Task 6 `middleware.rs` 和 `hitl/mod.rs`

7. 验证 TUI 完整构建成功
   - `cargo build -p peri-tui 2>&1 | grep -E "(Compiling peri-tui|Finished|error)"`
   - 预期: 输出 "Finished"，无 error
   - 失败排查: 检查 Task 6 `agent.rs` 中间件注册和 tool_filter 注入
