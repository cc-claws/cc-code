//! Markdown 解析结果的 LRU 缓存
//!
//! 缓存 key = (内容哈希, 渲染宽度)，value = Text<'static>。
//! 通过全局单例暴露，渲染线程和增量解析共享同一缓存。

use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;

use super::{LinkHit, MarkdownDoc};
use lru::LruCache;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use ratatui::text::{Line, Span};

/// 缓存容量上限
const CACHE_CAPACITY: usize = 1024;

/// 全局 Markdown 缓存单例
static MARKDOWN_CACHE: Lazy<MarkdownCache> = Lazy::new(MarkdownCache::new);

/// Markdown 解析结果 LRU 缓存
///
/// key = (内容哈希, 渲染宽度 u16)
/// value = MarkdownDoc（已解析的渲染结果 + 链接命中区）
pub struct MarkdownCache {
    cache: Mutex<LruCache<CacheKey, MarkdownDoc>>,
}

/// 同一次加锁读取的缓存快照；字节数按容器和字符串 capacity 估算。
///
/// 只统计解析产物拥有的堆分配，不含 LRU 节点、哈希表和分配器开销；
/// 借用的静态字符串不计入，不能将该估算等同于 RSS 或实际可回收量。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarkdownCacheStats {
    pub entries: usize,
    pub capacity: usize,
    pub estimated_heap_bytes: usize,
    pub largest_entry_heap_bytes: usize,
    pub rendered_lines: usize,
    pub rendered_spans: usize,
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

    /// 查询缓存，命中返回克隆的 MarkdownDoc
    pub fn get(&self, content: &str, max_width: u16) -> Option<MarkdownDoc> {
        let key = self.make_key(content, max_width);
        let mut guard = self.cache.lock();
        guard.get(&key).cloned()
    }

    /// 插入解析结果到缓存
    pub fn put(&self, content: &str, max_width: u16, doc: MarkdownDoc) {
        let key = self.make_key(content, max_width);
        let mut guard = self.cache.lock();
        guard.put(key, doc);
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
    #[allow(dead_code)]
    pub fn clear(&self) {
        let mut guard = self.cache.lock();
        guard.clear();
    }

    /// 当前缓存条目数（测试用）
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        let guard = self.cache.lock();
        guard.len()
    }

    /// 当前缓存是否为空
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 缓存容量上限
    pub fn capacity(&self) -> usize {
        let guard = self.cache.lock();
        guard.cap().get()
    }

    /// 按需统计，不复制解析产物、不改变 LRU 顺序，也不增加渲染热路径开销。
    pub fn stats(&self) -> MarkdownCacheStats {
        use std::borrow::Cow;
        let guard = self.cache.lock();
        let mut stats = MarkdownCacheStats {
            entries: guard.len(),
            capacity: guard.cap().get(),
            ..Default::default()
        };
        for (_, doc) in guard.iter() {
            let mut bytes = doc.text.lines.capacity() * std::mem::size_of::<Line<'static>>()
                + doc.links.capacity() * std::mem::size_of::<LinkHit>();
            stats.rendered_lines += doc.text.lines.len();
            for line in &doc.text.lines {
                bytes += line.spans.capacity() * std::mem::size_of::<Span<'static>>();
                stats.rendered_spans += line.spans.len();
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
            stats.estimated_heap_bytes += bytes;
            stats.largest_entry_heap_bytes = stats.largest_entry_heap_bytes.max(bytes);
        }
        stats
    }

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
}
