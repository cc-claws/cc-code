//! 带**字节预算**的 LRU 缓存。
//!
//! 背景：diff 相关缓存原本只按**条数**限流（64 条），但单条可携带接近
//! `MAX_DIFF_SIZE_BYTES`（1 MB）的文本，导致实际上限不可控（理论 ~64 MB），
//! 且这些常驻内存不受 `/gc` 统计。本模块在条数上限之外追加字节预算：每次写入
//! 后按 LRU 顺序淘汰，直到总估算字节回到预算内。
//!
//! 注意：估算是近似值（容器 capacity + 自有字符串 capacity），不含分配器元数据，
//! 仅用于容量控制与诊断，不能等同于 RSS。

use std::hash::Hash;
use std::num::NonZeroUsize;

use lru::LruCache;
use ratatui::text::Line;

/// 缓存统计快照（供 `/gc` 诊断使用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    /// 当前条目数
    pub entries: usize,
    /// 条数上限
    pub capacity: usize,
    /// 当前总估算字节
    pub bytes: usize,
    /// 字节预算
    pub budget: usize,
}

/// 同时受「条数上限」与「字节预算」约束的 LRU 缓存。
pub struct ByteBudgetCache<K: Hash + Eq, V> {
    lru: LruCache<K, V>,
    bytes: usize,
    budget: usize,
    size_of: fn(&V) -> usize,
}

impl<K: Hash + Eq, V> ByteBudgetCache<K, V> {
    /// `max_entries` 为条数上限（至少 1），`budget` 为字节预算。
    pub fn new(max_entries: usize, budget: usize, size_of: fn(&V) -> usize) -> Self {
        Self {
            lru: LruCache::new(NonZeroUsize::new(max_entries.max(1)).unwrap()),
            bytes: 0,
            budget,
            size_of,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.lru.get(key)
    }

    /// 写入并维持字节预算。同 key 覆盖时先扣回旧值字节。
    /// 单个条目若自身超过预算，会被立刻淘汰（不长期驻留超大 diff）。
    pub fn put(&mut self, key: K, value: V) {
        let incoming = (self.size_of)(&value);
        if let Some(old) = self.lru.put(key, value) {
            self.bytes = self.bytes.saturating_sub((self.size_of)(&old));
        }
        self.bytes = self.bytes.saturating_add(incoming);
        self.evict_to_budget();
    }

    fn evict_to_budget(&mut self) {
        while self.bytes > self.budget {
            match self.lru.pop_lru() {
                Some((_, evicted)) => {
                    self.bytes = self.bytes.saturating_sub((self.size_of)(&evicted));
                }
                None => {
                    self.bytes = 0;
                    break;
                }
            }
        }
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.lru.len(),
            capacity: self.lru.cap().get(),
            bytes: self.bytes,
            budget: self.budget,
        }
    }
}

/// 估算渲染后 `Line<'static>` 切片的近似堆占用（字节）。
///
/// 计 `Line`/`Span` 容器与 `Cow::Owned` 字符串；`Cow::Borrowed` 内容不计
/// （本就内联在 `Text` 中）。不含分配器元数据。
pub fn estimate_lines_bytes(lines: &[Line<'static>]) -> usize {
    use std::borrow::Cow;
    let mut total = std::mem::size_of_val(lines);
    for line in lines {
        total += line.spans.capacity() * std::mem::size_of::<ratatui::text::Span<'static>>();
        for span in &line.spans {
            if let Cow::Owned(s) = &span.content {
                total += s.capacity();
            }
        }
    }
    total
}

#[cfg(test)]
#[path = "byte_cache_test.rs"]
mod tests;
