//! Markdown 解析结果的 LRU 缓存
//!
//! 缓存 key = (内容哈希, 渲染宽度, 主题指纹)，value = MarkdownDoc。
//! 通过全局单例暴露，渲染线程和增量解析共享同一缓存。

use std::hash::{Hash, Hasher};

use super::{LinkHit, MarkdownDoc};
use crate::byte_cache::ByteBudgetCache;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use ratatui::text::{Line, Span};

/// 条数上限（兜底；真正的约束是字节预算）
const CACHE_CAPACITY: usize = 1024;

/// 字节预算：8 MB，与 diff / diff 渲染缓存对齐。
///
/// 单条解析产物约 50 KB 量级，条数上限防不住「宽度变体」把 1024 条填满
/// （实测满容量时数据堆约 52.8 MB）。超预算时按 LRU 淘汰最旧条目。
const CACHE_BUDGET_BYTES: usize = 8 * 1024 * 1024;

/// 全局 Markdown 缓存单例
static MARKDOWN_CACHE: Lazy<MarkdownCache> = Lazy::new(MarkdownCache::new);

/// Markdown 解析结果 LRU 缓存
///
/// key = (内容哈希, 渲染宽度 u16, 主题指纹 u64)
/// value = MarkdownDoc（已解析的渲染结果 + 链接命中区）
///
/// 淘汰同时受字节预算与条数上限约束，优先命中字节预算——条数上限只是兜底，
/// 防止极小的解析产物让条目数无限增长。
pub struct MarkdownCache {
    cache: Mutex<ByteBudgetCache<CacheKey, MarkdownDoc>>,
}

/// 同一次加锁读取的缓存快照；字节数按容器和字符串 capacity 估算。
///
/// 只统计解析产物拥有的堆分配，不含 LRU 节点、哈希表和分配器开销；
/// 借用的静态字符串不计入，不能将该估算等同于 RSS 或实际可回收量。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarkdownCacheStats {
    pub entries: usize,
    pub capacity: usize,
    /// 字节预算
    pub budget: usize,
    pub estimated_heap_bytes: usize,
    pub largest_entry_heap_bytes: usize,
    pub rendered_lines: usize,
    pub rendered_spans: usize,
}

/// 缓存 key：内容哈希 + 渲染宽度 + 主题指纹
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CacheKey {
    content_hash: u64,
    max_width: u16,
    theme_fingerprint: u64,
}

/// 估算单份解析产物的堆占用（字节）：容器 capacity + 自有字符串 capacity。
///
/// 与 `/gc` 的 `estimated_heap_bytes` 同一口径，也用作缓存的字节预算计量。
fn estimate_doc_bytes(doc: &MarkdownDoc) -> usize {
    use std::borrow::Cow;
    let mut bytes = doc.text.lines.capacity() * std::mem::size_of::<Line<'static>>()
        + doc.links.capacity() * std::mem::size_of::<LinkHit>();
    for line in &doc.text.lines {
        bytes += line.spans.capacity() * std::mem::size_of::<Span<'static>>();
        for span in &line.spans {
            if let Cow::Owned(text) = &span.content {
                bytes += text.capacity();
            }
        }
    }
    bytes += doc
        .links
        .iter()
        .map(|link| link.url.capacity())
        .sum::<usize>();
    bytes
}

impl MarkdownCache {
    /// 创建新的缓存实例
    fn new() -> Self {
        Self::with_limits(CACHE_CAPACITY, CACHE_BUDGET_BYTES)
    }

    fn with_limits(capacity: usize, budget: usize) -> Self {
        Self {
            cache: Mutex::new(ByteBudgetCache::new(capacity, budget, estimate_doc_bytes)),
        }
    }

    /// 获取全局缓存单例的引用
    pub fn global() -> &'static Self {
        &MARKDOWN_CACHE
    }

    /// 查询缓存，命中返回克隆的 MarkdownDoc
    pub fn get(
        &self,
        content: &str,
        max_width: u16,
        theme_fingerprint: u64,
    ) -> Option<MarkdownDoc> {
        let key = self.make_key(content, max_width, theme_fingerprint);
        let mut guard = self.cache.lock();
        guard.get(&key).cloned()
    }

    /// 插入解析结果到缓存
    pub fn put(&self, content: &str, max_width: u16, theme_fingerprint: u64, doc: MarkdownDoc) {
        let key = self.make_key(content, max_width, theme_fingerprint);
        self.cache.lock().put(key, doc);
    }

    /// 生成缓存 key
    fn make_key(&self, content: &str, max_width: u16, theme_fingerprint: u64) -> CacheKey {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        CacheKey {
            content_hash: hasher.finish(),
            max_width,
            theme_fingerprint,
        }
    }

    /// 清空缓存（测试用）
    #[allow(dead_code)]
    pub fn clear(&self) {
        self.cache.lock().clear();
    }

    /// 当前缓存条目数（测试用）
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.cache.lock().len()
    }

    /// 当前缓存是否为空
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 缓存条数上限
    pub fn capacity(&self) -> usize {
        self.cache.lock().capacity()
    }

    /// 缓存字节预算
    pub fn budget(&self) -> usize {
        self.cache.lock().budget()
    }

    /// 按需统计，不复制解析产物、不改变 LRU 顺序，也不增加渲染热路径开销。
    pub fn stats(&self) -> MarkdownCacheStats {
        let guard = self.cache.lock();
        let base = guard.stats();
        let mut stats = MarkdownCacheStats {
            entries: base.entries,
            capacity: base.capacity,
            budget: base.budget,
            estimated_heap_bytes: base.bytes,
            ..Default::default()
        };
        for (_, doc) in guard.iter() {
            stats.largest_entry_heap_bytes =
                stats.largest_entry_heap_bytes.max(estimate_doc_bytes(doc));
            stats.rendered_lines += doc.text.lines.len();
            for line in &doc.text.lines {
                stats.rendered_spans += line.spans.len();
            }
        }
        stats
    }

    /// 创建指定容量的缓存实例（测试用）
    #[cfg(test)]
    pub fn new_for_test_with_capacity(cap: usize) -> Self {
        Self::with_limits(cap, CACHE_BUDGET_BYTES)
    }

    /// 创建指定条数上限与字节预算的缓存实例（测试用）
    #[cfg(test)]
    pub fn new_for_test_with_limits(cap: usize, budget: usize) -> Self {
        Self::with_limits(cap, budget)
    }

    /// 创建默认容量缓存实例（测试用）
    #[cfg(test)]
    pub fn new_for_test() -> Self {
        Self::new()
    }
}
