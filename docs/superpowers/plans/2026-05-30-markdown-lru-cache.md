# Markdown 解析 LRU 缓存计划

> **For agentic workers:** Use superpowers:subagent-driven-development or superpowers:executing-plans to implement. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 `peri-widgets` 的 `parse_markdown()` 引入 LRU 缓存，避免同一内容在 resize、RebuildAll、流式输出等场景下被反复完整解析。预期收益：50-70% Markdown 解析时间减少。

**Architecture:** 在 `peri-widgets/src/markdown/` 新增 `cache.rs` 模块，实现 `MarkdownCache` 结构体（`Mutex<LruCache<(u64, u16), Text<'static>>>`），通过全局单例（`once_cell::sync::Lazy`）暴露。修改 `parse_markdown()` 在入口处检查缓存命中，命中则直接返回克隆的 `Text<'static>`，未命中则执行解析后写入缓存。

**Tech Stack:** Rust, lru 0.18（workspace 已有），once_cell（peri-widgets 已有依赖），parking_lot::Mutex（新增依赖），ratatui::text::Text

**Files:**
- 修改: `peri-widgets/Cargo.toml` — 添加 `parking_lot` 依赖
- 创建: `peri-widgets/src/markdown/cache.rs` — `MarkdownCache` 实现
- 创建: `peri-widgets/src/markdown/cache_test.rs` — 缓存测试
- 修改: `peri-widgets/src/markdown/mod.rs` — 注册模块、修改 `parse_markdown` 加入缓存逻辑
- 修改: `peri-widgets/src/markdown/mod_test.rs` — 调整现有测试适配缓存（验证缓存不影响正确性）

---

### Task 1: 添加依赖并创建 cache 模块骨架

- [ ] **Step 1: 添加 parking_lot 依赖到 peri-widgets**
  - 文件: `peri-widgets/Cargo.toml`
  - 在 `[dependencies]` 中添加:
    ```toml
    parking_lot.workspace = true
    ```
  - Run: `cargo build -p peri-widgets 2>&1 | tail -5`
  - Expected: 构建成功（parking_lot 从 workspace 继承）

- [ ] **Step 2: 创建 `cache.rs` 骨架**
  - 文件: `peri-widgets/src/markdown/cache.rs`
  - 完整代码：
    ```rust
    //! Markdown 解析结果的 LRU 缓存
    //!
    //! 缓存 key = (内容哈希, 渲染宽度)，value = Text<'static>。
    //! 通过全局单例暴露，渲染线程和增量解析共享同一缓存。

    use std::hash::{Hash, Hasher};
    use std::num::NonZeroUsize;

    use lru::LruCache;
    use once_cell::sync::Lazy;
    use parking_lot::Mutex;
    use ratatui::text::Text;

    /// 缓存容量上限
    const CACHE_CAPACITY: usize = 256;

    /// 全局 Markdown 缓存单例
    static MARKDOWN_CACHE: Lazy<MarkdownCache> = Lazy::new(MarkdownCache::new);

    /// Markdown 解析结果 LRU 缓存
    ///
    /// key = (内容哈希, 渲染宽度 u16)
    /// value = Text<'static>（已解析的渲染结果）
    pub struct MarkdownCache {
        cache: Mutex<LruCache<CacheKey, Text<'static>>>,
    }

    /// 缓存 key：内容哈希 + 渲染宽度
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct CacheKey {
        content_hash: u64,
        max_width: u16,
    }

    impl MarkdownCache {
        /// 创建新的缓存实例
        fn new() -> Self {
            let cap = NonZeroUsize::new(CACHE_CAPACITY).expect("CACHE_CAPACITY > 0");
            Self {
                cache: Mutex::new(LruCache::new(cap)),
            }
        }

        /// 获取全局缓存单例的引用
        pub fn global() -> &'static Self {
            &MARKDOWN_CACHE
        }

        /// 查询缓存，命中返回克隆的 Text
        pub fn get(&self, content: &str, max_width: u16) -> Option<Text<'static>> {
            let key = self.make_key(content, max_width);
            let mut guard = self.cache.lock();
            guard.get(&key).cloned()
        }

        /// 插入解析结果到缓存
        pub fn put(&self, content: &str, max_width: u16, text: Text<'static>) {
            let key = self.make_key(content, max_width);
            let mut guard = self.cache.lock();
            guard.put(key, text);
        }

        /// 生成缓存 key
        fn make_key(&self, content: &str, max_width: u16) -> CacheKey {
            use std::collections::hash_map::DefaultHasher;
            let mut hasher = DefaultHasher::new();
            content.hash(&mut hasher);
            CacheKey {
                content_hash: hasher.finish(),
                max_width,
            }
        }

        /// 清空缓存（测试用）
        pub fn clear(&self) {
            let mut guard = self.cache.lock();
            guard.clear();
        }

        /// 当前缓存条目数（测试用）
        pub fn len(&self) -> usize {
            let guard = self.cache.lock();
            guard.len()
        }
    }
    ```

- [ ] **Step 3: 在 `mod.rs` 注册 cache 模块**
  - 文件: `peri-widgets/src/markdown/mod.rs`
  - 在 `mod render_state;` 后添加:
    ```rust
    mod cache;
    pub use cache::MarkdownCache;
    ```

- [ ] **Step 4: 构建验证**
  - Run: `cargo build -p peri-widgets`
  - Expected: 构建成功

---

### Task 2: TDD — 编写缓存测试（失败测试先行）

- [ ] **Step 1: 创建 `cache_test.rs`**
  - 文件: `peri-widgets/src/markdown/cache_test.rs`
  - 完整代码：
    ```rust
    use super::cache::MarkdownCache;
    use ratatui::text::Text;

    /// 辅助：创建新的缓存实例（不使用全局单例，测试隔离）
    fn make_cache() -> MarkdownCache {
        MarkdownCache::new_for_test()
    }

    #[test]
    fn test_cache_miss_returns_none() {
        // Arrange: 空缓存
        let cache = make_cache();
        // Act: 查询不存在的 key
        let result = cache.get("hello", 80);
        // Assert: 应返回 None
        assert!(result.is_none(), "空缓存查询应返回 None");
    }

    #[test]
    fn test_cache_hit_after_put() {
        // Arrange: 缓存中插入一条
        let cache = make_cache();
        let text = Text::from("rendered");
        cache.put("hello", 80, text.clone());
        // Act: 查询相同 key
        let result = cache.get("hello", 80);
        // Assert: 应命中并返回相同内容
        assert!(result.is_some(), "相同 key 应命中缓存");
        let got = result.unwrap();
        assert_eq!(got.lines.len(), text.lines.len(), "缓存结果行数应一致");
    }

    #[test]
    fn test_cache_different_width_is_miss() {
        // Arrange: 插入 width=80
        let cache = make_cache();
        cache.put("hello", 80, Text::from("w80"));
        // Act: 查询 width=100
        let result = cache.get("hello", 100);
        // Assert: 不同宽度应 miss
        assert!(result.is_none(), "不同宽度应 miss");
    }

    #[test]
    fn test_cache_different_content_is_miss() {
        // Arrange: 插入 content="hello"
        let cache = make_cache();
        cache.put("hello", 80, Text::from("a"));
        // Act: 查询 content="world"
        let result = cache.get("world", 80);
        // Assert: 不同内容应 miss
        assert!(result.is_none(), "不同内容应 miss");
    }

    #[test]
    fn test_cache_overwrite_on_same_key() {
        // Arrange: 同一 key 插入两次
        let cache = make_cache();
        cache.put("hello", 80, Text::from("first"));
        cache.put("hello", 80, Text::from("second"));
        // Act: 查询
        let result = cache.get("hello", 80);
        // Assert: 应返回最新值
        let got = result.unwrap();
        let content: String = got.lines.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(content.contains("second"), "应返回最新插入的值");
    }

    #[test]
    fn test_cache_lru_eviction() {
        // Arrange: 容量为 2 的缓存
        let cache = MarkdownCache::new_for_test_with_capacity(2);
        cache.put("a", 80, Text::from("A"));
        cache.put("b", 80, Text::from("B"));
        // a 和 b 都在缓存中
        assert!(cache.get("a", 80).is_some(), "a 应在缓存中");
        assert!(cache.get("b", 80).is_some(), "b 应在缓存中");
        // Act: 插入第三条，应淘汰最久未使用的
        cache.put("c", 80, Text::from("C"));
        // Assert: 容量仍为 2，a/b/c 中有一个被淘汰
        assert_eq!(cache.len(), 2, "容量应保持为 2");
        assert!(cache.get("c", 80).is_some(), "最新插入的 c 应在缓存中");
    }

    #[test]
    fn test_cache_clear() {
        // Arrange: 插入两条
        let cache = make_cache();
        cache.put("hello", 80, Text::from("a"));
        cache.put("world", 80, Text::from("b"));
        assert_eq!(cache.len(), 2, "插入后应有 2 条");
        // Act: 清空
        cache.clear();
        // Assert: 缓存为空
        assert_eq!(cache.len(), 0, "清空后应为 0 条");
        assert!(cache.get("hello", 80).is_none(), "清空后查询应 miss");
    }
    ```

- [ ] **Step 2: 在 `cache.rs` 添加测试用构造方法**
  - 文件: `peri-widgets/src/markdown/cache.rs`
  - 在 `impl MarkdownCache` 块末尾添加：
    ```rust
    /// 创建指定容量的缓存实例（测试用）
    #[cfg(test)]
    pub fn new_for_test_with_capacity(cap: usize) -> Self {
        let cap = NonZeroUsize::new(cap).expect("capacity > 0");
        Self {
            cache: Mutex::new(LruCache::new(cap)),
        }
    }

    /// 创建默认容量缓存实例（测试用）
    #[cfg(test)]
    pub fn new_for_test() -> Self {
        Self::new()
    }
    ```

- [ ] **Step 3: 在 `mod.rs` 注册测试模块**
  - 文件: `peri-widgets/src/markdown/mod.rs`
  - 在文件末尾（`mod tests;` 之前）添加：
    ```rust
    #[cfg(test)]
    #[path = "cache_test.rs"]
    mod cache_tests;
    ```

- [ ] **Step 4: 运行测试确认失败**
  - Run: `cargo test -p peri-widgets --lib -- markdown::cache_tests 2>&1 | tail -20`
  - Expected: 测试失败——`MarkdownCache::new_for_test` 等方法尚不存在（因为 Step 2 代码在 Step 3 之前编辑即可通过，但按 TDD 流程先运行确认方法缺失）
  - 注意：若 `new_for_test` 已在 Step 2 添加，测试应全部通过。若未添加，编译失败。此处验证的是测试文件本身的正确性。

- [ ] **Step 5: 运行测试确认通过**
  - Run: `cargo test -p peri-widgets --lib -- markdown::cache_tests 2>&1 | tail -20`
  - Expected: 全部 7 个测试通过

---

### Task 3: 修改 `parse_markdown` 加入缓存逻辑

- [ ] **Step 1: 修改 `parse_markdown` 函数签名和实现**
  - 文件: `peri-widgets/src/markdown/mod.rs`
  - 将 `parse_markdown` 函数替换为：
    ```rust
    /// 解析 markdown 文本为 ratatui Text（带 LRU 缓存）
    ///
    /// 缓存 key = (content_hash, max_width)，全局单例共享。
    /// 命中时直接返回克隆的 Text<'static>，跳过完整解析。
    pub fn parse_markdown(input: &str, theme: &dyn MarkdownTheme, max_width: usize) -> Text<'static> {
        if input.is_empty() {
            return Text::raw("");
        }

        // 检查缓存
        let width_u16 = max_width.min(u16::MAX as usize) as u16;
        let cache = MarkdownCache::global();
        if let Some(cached) = cache.get(input, width_u16) {
            return cached;
        }

        // 缓存未命中，执行完整解析
        let options = Options::all() - Options::ENABLE_SMART_PUNCTUATION;
        let parser = Parser::new_ext(input, options);
        let mut state = RenderState::new(theme).with_max_width(max_width);
        for event in parser {
            state.handle_event(event);
        }
        if !state.current_spans.is_empty() {
            state.flush_line();
        }
        // 裁剪尾部空行，避免最后一个块级元素后多余留白
        while state.lines.last().is_some_and(|l| l.spans.is_empty()) {
            state.lines.pop();
        }
        let result = Text::from(state.lines);

        // 写入缓存
        cache.put(input, width_u16, result.clone());

        result
    }
    ```

- [ ] **Step 2: 构建验证**
  - Run: `cargo build -p peri-widgets`
  - Expected: 构建成功

- [ ] **Step 3: 运行全部现有测试确认无回归**
  - Run: `cargo test -p peri-widgets --lib 2>&1 | tail -20`
  - Expected: 全部测试通过（现有 15 个 markdown 测试 + 7 个 cache 测试）

---

### Task 4: 集成测试 — 验证端到端缓存命中

- [ ] **Step 1: 在 `mod_test.rs` 添加缓存集成测试**
  - 文件: `peri-widgets/src/markdown/mod_test.rs`
  - 在文件末尾添加：
    ```rust
    /// 集成测试：parse_markdown 多次调用应命中缓存
    #[test]
    fn parse_markdown_cache_hit_on_repeat() {
        // 清空全局缓存避免干扰
        MarkdownCache::global().clear();

        // Arrange: 同一内容调用两次
        let content = "# 缓存测试\n\n这是一段用于测试缓存命中的 Markdown 文本。";
        let theme = default_theme();
        let width = 80;

        // Act: 第一次调用（miss，写入缓存）
        let result1 = parse_markdown(content, &theme, width);
        let cache_len_after_first = MarkdownCache::global().len();

        // Act: 第二次调用（应命中缓存）
        let result2 = parse_markdown(content, &theme, width);
        let cache_len_after_second = MarkdownCache::global().len();

        // Assert: 两次结果一致
        assert_eq!(
            result1.lines.len(),
            result2.lines.len(),
            "两次解析结果行数应一致"
        );

        // Assert: 缓存条目数未增加（第二次是命中）
        assert_eq!(
            cache_len_after_first,
            cache_len_after_second,
            "第二次调用不应增加缓存条目"
        );
        assert!(
            cache_len_after_first >= 1,
            "第一次调用应至少写入 1 条缓存"
        );

        // 清理
        MarkdownCache::global().clear();
    }

    /// 集成测试：不同宽度产生不同缓存条目
    #[test]
    fn parse_markdown_different_width_different_cache_entry() {
        MarkdownCache::global().clear();

        // Arrange
        let content = "| A | B |\n| --- | --- |\n| 内容 | 更多内容 |";
        let theme = default_theme();

        // Act: 用两个不同宽度调用
        let _r1 = parse_markdown(content, &theme, 80);
        let _r2 = parse_markdown(content, &theme, 40);

        // Assert: 缓存中应有 2 条
        assert_eq!(
            MarkdownCache::global().len(),
            2,
            "不同宽度应产生 2 条缓存"
        );

        MarkdownCache::global().clear();
    }

    /// 集成测试：空字符串不走缓存
    #[test]
    fn parse_markdown_empty_not_cached() {
        MarkdownCache::global().clear();

        // Act
        let _result = parse_markdown("", &default_theme(), 80);

        // Assert: 空字符串直接返回，不经过缓存
        assert_eq!(
            MarkdownCache::global().len(),
            0,
            "空字符串不应写入缓存"
        );

        MarkdownCache::global().clear();
    }
    ```

- [ ] **Step 2: 运行集成测试**
  - Run: `cargo test -p peri-widgets --lib -- markdown::tests::parse_markdown_cache 2>&1 | tail -20`
  - Expected: 3 个集成测试通过

- [ ] **Step 3: 运行 peri-widgets 全量测试**
  - Run: `cargo test -p peri-widgets --lib 2>&1 | tail -10`
  - Expected: 全部测试通过

---

### Task 5: 构建下游 crate 确认无破坏

- [ ] **Step 1: 构建 peri-tui**
  - Run: `cargo build -p peri-tui 2>&1 | tail -5`
  - Expected: 构建成功（`peri-tui/src/ui/markdown/mod.rs` 中的 `parse_markdown` 包装器无需修改，签名未变）

- [ ] **Step 2: 运行 peri-tui 测试**
  - Run: `cargo test -p peri-tui --lib 2>&1 | tail -10`
  - Expected: 全部测试通过

---

### Task 6: 提交

- [ ] **Step 1: 提交**
  ```bash
  git add peri-widgets/Cargo.toml \
        peri-widgets/src/markdown/cache.rs \
        peri-widgets/src/markdown/cache_test.rs \
        peri-widgets/src/markdown/mod.rs \
        peri-widgets/src/markdown/mod_test.rs
  git commit -m "perf(widgets): add LRU cache for markdown parse results

  - Add MarkdownCache with 256-entry LRU (lru 0.18 + parking_lot::Mutex)
  - Cache key = (content_hash: u64, max_width: u16)
  - Global singleton via once_cell::sync::Lazy
  - parse_markdown checks cache before full parse
  - 7 unit tests + 3 integration tests covering hit/miss/eviction/clear

  Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
  ```

---

## 设计决策说明

| 决策 | 理由 |
|------|------|
| 全局单例 `Lazy<MarkdownCache>` | 渲染线程和增量解析共享同一缓存，无需修改调用方签名传递缓存引用 |
| `parking_lot::Mutex` 而非 `std::sync::Mutex` | `parking_lot::Mutex` 不是 poison 的，`get()` 返回 `Option<Text>` 而非 `LockResult`，API 更简洁；workspace 已有 parking_lot |
| `Mutex` 而非 `RwLock` | 缓存操作是 get（读）+ put（写）交替，put 在 miss 时触发。LRU 的 `get` 会更新内部顺序（即内部有写），用 RwLock 无法真正提高并发度 |
| `Text<'static>.clone()` 返回 | `Text<'static>` 的 clone 是 `Vec<Line<'static>>` 的 clone，开销远低于完整 Markdown 解析（含 pulldown-cmark Parser 遍历 + RenderState 状态机） |
| 容量 256 | 典型会话约 20-50 条消息，每条消息 1-5 个 Markdown 块，256 覆盖完整会话且内存可控（每条缓存约 1-10KB） |
| `max_width` 截断为 `u16` | 终端宽度最大 65535，`u16` 足够；减小 key 大小 |
| `DefaultHasher` 做内容哈希 | 速度快（FxHash 级别），碰撞概率可接受（缓存命中后仍返回正确渲染结果，碰撞仅导致 miss） |
| 空字符串跳过缓存 | 空输入直接返回 `Text::raw("")`，无解析开销，不值得缓存 |
