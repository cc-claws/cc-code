use super::cache::MarkdownCache;
use super::MarkdownDoc;
use ratatui::text::{Line, Span, Text};

/// 辅助：创建新的缓存实例（不使用全局单例，测试隔离）
fn make_cache() -> MarkdownCache {
    MarkdownCache::new_for_test()
}

fn make_doc(s: &'static str) -> MarkdownDoc {
    MarkdownDoc {
        text: Text::from(s),
        links: Vec::new(),
    }
}

/// 辅助：构造单条自有字符串约 `bytes` 字节的解析产物
fn make_sized_doc(bytes: usize) -> MarkdownDoc {
    MarkdownDoc::from(Text::from("x".repeat(bytes)))
}

#[test]
fn test_cache_miss_returns_none() {
    // Arrange: 空缓存
    let cache = make_cache();
    // Act: 查询不存在的 key
    let result = cache.get("hello", 80, 0);
    // Assert: 应返回 None
    assert!(result.is_none(), "空缓存查询应返回 None");
}

#[test]
fn test_cache_hit_after_put() {
    // Arrange: 缓存中插入一条
    let cache = make_cache();
    let doc = make_doc("rendered");
    cache.put("hello", 80, 0, doc.clone());
    // Act: 查询相同 key
    let result = cache.get("hello", 80, 0);
    // Assert: 应命中并返回相同内容
    assert!(result.is_some(), "相同 key 应命中缓存");
    let got = result.unwrap();
    assert_eq!(
        got.text.lines.len(),
        doc.text.lines.len(),
        "缓存结果行数应一致"
    );
}

#[test]
fn test_cache_different_width_is_miss() {
    // Arrange: 插入 width=80
    let cache = make_cache();
    cache.put("hello", 80, 0, make_doc("w80"));
    // Act: 查询 width=100
    let result = cache.get("hello", 100, 0);
    // Assert: 不同宽度应 miss
    assert!(result.is_none(), "不同宽度应 miss");
}

#[test]
fn test_cache_different_content_is_miss() {
    // Arrange: 插入 content="hello"
    let cache = make_cache();
    cache.put("hello", 80, 0, make_doc("a"));
    // Act: 查询 content="world"
    let result = cache.get("world", 80, 0);
    // Assert: 不同内容应 miss
    assert!(result.is_none(), "不同内容应 miss");
}

#[test]
fn test_cache_overwrite_on_same_key() {
    // Arrange: 同一 key 插入两次
    let cache = make_cache();
    cache.put("hello", 80, 0, make_doc("first"));
    cache.put("hello", 80, 0, make_doc("second"));
    // Act: 查询
    let result = cache.get("hello", 80, 0);
    // Assert: 应返回最新值
    let got = result.unwrap();
    let content: String = got
        .text
        .lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect();
    assert!(content.contains("second"), "应返回最新插入的值");
}

#[test]
fn test_cache_lru_eviction() {
    // Arrange: 容量为 2 的缓存
    let cache = MarkdownCache::new_for_test_with_capacity(2);
    cache.put("a", 80, 0, make_doc("A"));
    cache.put("b", 80, 0, make_doc("B"));
    // a 和 b 都在缓存中
    assert!(cache.get("a", 80, 0).is_some(), "a 应在缓存中");
    assert!(cache.get("b", 80, 0).is_some(), "b 应在缓存中");
    // Act: 插入第三条，应淘汰最久未使用的
    cache.put("c", 80, 0, make_doc("C"));
    // Assert: 容量仍为 2，a/b/c 中有一个被淘汰
    assert_eq!(cache.len(), 2, "容量应保持为 2");
    assert!(cache.get("c", 80, 0).is_some(), "最新插入的 c 应在缓存中");
}

#[test]
fn test_cache_clear() {
    // Arrange: 插入两条
    let cache = make_cache();
    cache.put("hello", 80, 0, make_doc("a"));
    cache.put("world", 80, 0, make_doc("b"));
    assert_eq!(cache.len(), 2, "插入后应有 2 条");
    // Act: 清空
    cache.clear();
    // Assert: 缓存为空
    assert_eq!(cache.len(), 0, "清空后应为 0 条");
    assert!(cache.get("hello", 80, 0).is_none(), "清空后查询应 miss");
}

#[test]
fn test_cache_stats_empty_cache() {
    let cache = make_cache();
    let stats = cache.stats();
    assert_eq!(stats.entries, 0, "空缓存不应有条目");
    assert_eq!(stats.capacity, 1024, "条数上限应保持不变");
    assert_eq!(stats.budget, 8 * 1024 * 1024, "字节预算应为 8 MB");
    assert_eq!(
        stats.estimated_heap_bytes, 0,
        "空缓存的解析产物堆估算应为零"
    );
    assert_eq!(stats.largest_entry_heap_bytes, 0);
    assert_eq!(stats.rendered_lines, 0);
    assert_eq!(stats.rendered_spans, 0);
}

#[test]
fn test_cache_byte_budget_evicts_lru_when_exceeded() {
    // Arrange：预算 10 KB，每条约 4 KB，条数上限不构成约束
    let cache = MarkdownCache::new_for_test_with_limits(1024, 10_000);
    for i in 0..5 {
        cache.put(&format!("doc-{i}"), 80, 0, make_sized_doc(4096));
    }
    // Act：写入后的快照
    let stats = cache.stats();
    // Assert：字节不超预算，最旧条目被淘汰，最新条目存活
    assert!(
        stats.estimated_heap_bytes <= stats.budget,
        "超预算后字节应回落到预算内"
    );
    assert!(stats.entries < 5, "应有条目被字节预算淘汰");
    assert!(stats.entries >= 1, "预算淘汰后应至少保留最新条目");
    assert!(cache.get("doc-0", 80, 0).is_none(), "最旧条目应被淘汰");
    assert!(cache.get("doc-4", 80, 0).is_some(), "最新条目应存活");
}

#[test]
fn test_cache_entry_larger_than_budget_is_evicted() {
    // Arrange：预算 100 字节，单条解析产物远超预算
    let cache = MarkdownCache::new_for_test_with_limits(1024, 100);
    // Act：写入超大条目
    cache.put("huge", 80, 0, make_sized_doc(4096));
    let stats = cache.stats();
    // Assert：超大条目不应长期驻留
    assert_eq!(stats.entries, 0, "超过整体预算的条目应被立刻淘汰");
    assert_eq!(stats.estimated_heap_bytes, 0);
    assert!(cache.get("huge", 80, 0).is_none());
}

#[test]
fn test_cache_budget_accessor() {
    let cache = MarkdownCache::new_for_test_with_limits(16, 1234);
    assert_eq!(cache.budget(), 1234, "应暴露字节预算");
    assert_eq!(cache.capacity(), 16, "应暴露条数上限");
}

#[test]
fn test_cache_stats_counts_reserved_capacity_and_owned_content() {
    let cache = make_cache();
    let mut owned = String::with_capacity(4096);
    owned.push_str("中文");
    let mut url = String::with_capacity(512);
    url.push_str("https://example.com");
    let string_bytes = owned.capacity() + url.capacity();
    let mut spans = Vec::with_capacity(4);
    spans.push(Span::raw(owned));
    spans.push(Span::raw("借用的静态文本"));
    let span_bytes = spans.capacity() * std::mem::size_of::<Span<'static>>();
    let mut lines = Vec::with_capacity(3);
    lines.push(Line::from(spans));
    let mut links = Vec::with_capacity(2);
    links.push(super::LinkHit {
        line: 0,
        g_start: 0,
        g_end: 2,
        url,
    });
    let expected_bytes = string_bytes
        + span_bytes
        + lines.capacity() * std::mem::size_of::<Line<'static>>()
        + links.capacity() * std::mem::size_of::<super::LinkHit>();
    let doc = MarkdownDoc {
        text: Text {
            lines,
            ..Default::default()
        },
        links,
    };
    cache.put("capacity", 80, 0, doc);
    let stats = cache.stats();
    assert_eq!(stats.entries, 1);
    assert_eq!(
        stats.estimated_heap_bytes, expected_bytes,
        "应统计预留容量且不计静态借用文本"
    );
    assert_eq!(stats.largest_entry_heap_bytes, expected_bytes);
    assert_eq!(stats.rendered_lines, 1, "行数应按长度而不是容量统计");
    assert_eq!(stats.rendered_spans, 2, "Span 数应按长度而不是容量统计");
}

#[test]
fn test_cache_stats_tracks_replacement_eviction_and_clear() {
    let cache = MarkdownCache::new_for_test_with_capacity(1);
    cache.put(
        "large",
        80,
        0,
        MarkdownDoc::from(Text::from("x".repeat(4096))),
    );
    let large = cache.stats();
    cache.put("large", 80, 0, make_doc("短文本"));
    let replaced = cache.stats();
    cache.put("small", 80, 0, make_doc("其他短文本"));
    let evicted = cache.stats();
    cache.clear();
    let cleared = cache.stats();
    assert!(
        large.estimated_heap_bytes >= 4096,
        "大条目应计入其自有字符串"
    );
    assert!(
        replaced.estimated_heap_bytes < large.estimated_heap_bytes,
        "覆盖后不应残留旧条目统计"
    );
    assert_eq!(evicted.entries, 1, "淘汰后只能统计仍存活的条目");
    assert_eq!(evicted.estimated_heap_bytes, replaced.estimated_heap_bytes);
    assert_eq!(
        evicted.largest_entry_heap_bytes,
        evicted.estimated_heap_bytes
    );
    assert_eq!(cleared.entries, 0);
    assert_eq!(cleared.estimated_heap_bytes, 0);
    assert_eq!(cleared.largest_entry_heap_bytes, 0);
}

#[test]
fn test_cache_stats_preserves_lru_order() {
    let cache = MarkdownCache::new_for_test_with_capacity(2);
    cache.put("old", 80, 0, make_doc("旧条目"));
    cache.put("new", 80, 0, make_doc("新条目"));
    let stats = cache.stats();
    cache.put("latest", 80, 0, make_doc("最新条目"));
    assert_eq!(stats.entries, 2);
    assert!(
        cache.get("old", 80, 0).is_none(),
        "统计不应延长旧条目的缓存寿命"
    );
    assert!(cache.get("new", 80, 0).is_some());
    assert!(cache.get("latest", 80, 0).is_some());
}
