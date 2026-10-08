# Markdown 缓存内存统计缺口与字节预算评估

**状态**：Open（已补诊断，原始会话缓存字节待采集）
**创建日期**：2026-10-08
**分支**：`feat/markdown-cache-memory-stats`
**GitHub Issue**：[#350](https://github.com/cc-claws/cc-code/issues/350)

## 已有数据

用户提供的 `/gc` 输出：

| 指标 | 观测值 |
|---|---:|
| RSS（回收后） | 223.9 MB |
| mimalloc allocated（明细） | 185.4 MB |
| origin_messages | 907 条，约 2.2 MB |
| pipeline.completed | 875 条，约 2.7 MB |
| view_messages | 875 条，约 3.0 MB |
| 已知合计 | 约 7.9 MB |
| allocated 内未识别 | 约 177.5 MB |
| Markdown 缓存 | 1024/1024 条，未提供字节数 |

这些数字只能证明原有估算覆盖不足，不能把 177.5 MB 全部归因于 Markdown 缓存，
也不能判断是否泄漏。`page_committed` 的高水位不作为当前物理占用依据。
原日志 RSS 减少却显示正号的问题已由 #345/#346 修复，不纳入本次改动。

## 已确认的代码行为

- `cc-widgets/src/markdown/cache.rs` 仅按 1024 条限制，没有字节预算。
- `cc-tui/src/app/message_pipeline/transform.rs::build_streaming_bubble` 每次对增长中的完整文本预解析。
- 缓存 key 包含内容 hash 与宽度；流式中间版本和不同宽度可能分别保留。
- 旧 `/gc` 未统计 Markdown 缓存解析产物字节，且无依据地直接打印“非泄漏”。

## 本次补充

- `MarkdownCache::stats()`：在单次加锁中读取条数、容量、数据堆估算总字节、最大条目、行数及 Span 数。
- 估算遍历已有容器，按 `capacity()` 计入 `Text.lines`、`Line.spans`、链接数组与自有字符串；不克隆、不改变 LRU 顺序。
- `/gc` 显示总量、平均及最大条目，并向 tracing 写入原始字节字段。
- 缓存数据估算纳入已知合计；未识别量改为待定位，不再直接断言非泄漏。

统计不包含 LRU 节点、哈希表与分配器开销，不等同于 RSS。
只增加显式诊断能力，本次未设置 16 MB 预算，也未修改渲染、淘汰或回收行为。

## 真实采集与后续决策

现有运行进程使用旧二进制，无法通过外部 RSS 读出其进程内 Markdown 缓存内容。
测试数据和新进程的空缓存不能替代用户原始会话的测量。

使用含本次改动的二进制，在相同使用场景执行 `/gc`，记录：

1. 空闲时：RSS、allocated、缓存总字节、最大条目、行数及 Span 数。
2. 长回答流式完成后：再次记录上述字段及变化量。
3. 后续如实施字节预算，以相同场景对比缓存字节与 RSS，保留显示正确性和解析性能检查。

候选 16 MB 数据预算的超出量为 `max(当前缓存数据估算 - 16 MiB, 0)`；
该值用于评估缓存淘汰规模，不能承诺等量 RSS 降幅。
未取得缓存字节数据前，不给出具体节省值。

## 验收

- `/gc` 显示单次缓存快照的字节、平均/最大条目及行/Span 数。
- 原始 tracing 字段可用于前后对比，不包含缓存文本内容。
- 预留容量、自有/借用字符串、替换、淘汰、清空及 LRU 顺序均有隔离测试。
- 编译检查与缓存测试完成后记录结果；原始会话实测独立标记为待采集。

## 本地验证

- `cargo test --offline -p cc-widgets --features markdown --lib markdown::cache_tests`：11 个测试全部通过，含 4 个新增统计测试。
- 三个改动 Rust 文件的 `rustfmt --edition 2021 --check`：通过。
- `git diff --check`：通过。
- `cargo check --offline -p cc-tui --bin cc-code`：通过。
- `cargo build --offline -p cc-tui --bin cc-code`：因本机磁盘空间不足中断，未生成新版可执行文件；已清理本次失败构建生成的临时对象和增量缓存。
- 独立代码审阅：当前未发现明显问题。
- 原始会话 Markdown 缓存字节与内存节省量：尚未采集，不用测试构造数据代替。
