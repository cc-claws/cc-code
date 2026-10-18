//! 带**字节预算**的 LRU 缓存。
//!
//! 背景：diff 与 markdown 缓存原本只按**条数**限流，但单条可携带数十 KB 到
//! 接近 `MAX_DIFF_SIZE_BYTES`（1 MB）的解析产物，导致实际上限不可控，且这些
//! 常驻内存不受 `/gc` 统计。本模块在条数上限之外追加字节预算：每次写入后按
//! LRU 顺序淘汰，直到总估算字节回到预算内。
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
    /// 单个条目若自身超过预算，会被立刻淘汰（不长期驻留超大条目）。
    pub fn put(&mut self, key: K, value: V) {
        // 条数上限触发的淘汰同样要扣减字节，但 `LruCache::put` 静默丢弃最旧
        // 条目、不返回被淘汰的值，因此在插入前先自行弹出。
        if self.lru.len() == self.lru.cap().get() && self.lru.peek(&key).is_none() {
            if let Some((_, evicted)) = self.lru.pop_lru() {
                self.bytes = self.bytes.saturating_sub((self.size_of)(&evicted));
            }
        }
        let incoming = (self.size_of)(&value);
        if let Some(old) = self.lru.put(key, value) {
            self.bytes = self.bytes.saturating_sub((self.size_of)(&old));
        }
        self.bytes = self.bytes.saturating_add(incoming);
        self.evict_to_budget();
    }

    /// 按 LRU→MRU 顺序遍历存活条目（不改变 LRU 顺序）。
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.lru.iter()
    }

    /// 清空全部条目并把字节计数归零。
    pub fn clear(&mut self) {
        self.lru.clear();
        self.bytes = 0;
    }

    /// 当前条目数。
    pub fn len(&self) -> usize {
        self.lru.len()
    }

    /// 条数上限。
    pub fn capacity(&self) -> usize {
        self.lru.cap().get()
    }

    /// 字节预算。
    pub fn budget(&self) -> usize {
        self.budget
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
