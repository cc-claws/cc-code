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
    let content: String = got
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
