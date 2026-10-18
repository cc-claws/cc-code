use super::*;

fn bytes_of(v: &Vec<u8>) -> usize {
    v.capacity()
}

fn make_cache(max_entries: usize, budget: usize) -> ByteBudgetCache<u32, Vec<u8>> {
    ByteBudgetCache::new(max_entries, budget, bytes_of)
}

#[test]
fn test_byte_budget_evicts_lru_when_exceeded() {
    // Arrange：预算 100 字节，每条约 10 字节
    let mut cache = make_cache(64, 100);

    // Act：写入 8 条（约 80 字节，未超预算）
    for k in 0..8u32 {
        cache.put(k, vec![0u8; 10]);
    }
    let before = cache.stats();

    // 再写入，越过预算
    for k in 8..20u32 {
        cache.put(k, vec![0u8; 10]);
    }
    let after = cache.stats();

    // Assert：字节不超预算，且最旧的条目被淘汰
    assert!(before.bytes <= before.budget, "未超预算时不应淘汰");
    assert!(after.bytes <= after.budget, "超预算后字节应回落到预算内");
    assert!(after.entries < 20, "应有条目被淘汰");
    assert!(cache.get(&0).is_none(), "最旧条目 0 应被淘汰");
    assert!(cache.get(&19).is_some(), "最新条目应存活");
}

#[test]
fn test_put_same_key_does_not_double_count_bytes() {
    // Arrange：预算充足
    let mut cache = make_cache(64, 10_000);

    // Act：同一 key 写入两次
    cache.put(1, vec![0u8; 10]);
    let first = cache.stats();
    cache.put(1, vec![0u8; 10]);
    let second = cache.stats();

    // Assert：条数仍为 1，字节不翻倍
    assert_eq!(first.entries, 1);
    assert_eq!(second.entries, 1);
    assert_eq!(first.bytes, second.bytes, "同 key 覆盖不应累加字节");
}

#[test]
fn test_entry_larger_than_budget_is_evicted() {
    // Arrange：预算 100 字节
    let mut cache = make_cache(64, 100);

    // Act：写入一条远超预算的条目
    cache.put(1, vec![0u8; 5000]);
    let stats = cache.stats();

    // Assert：超大条目被立刻淘汰，不长期驻留
    assert_eq!(stats.entries, 0, "超预算条目应被淘汰");
    assert_eq!(stats.bytes, 0);
    assert!(cache.get(&1).is_none());
}

#[test]
fn test_entry_count_cap_still_enforced() {
    // Arrange：条数上限 2，预算足够大
    let mut cache = make_cache(2, 1_000_000);

    // Act
    cache.put(1, vec![0u8; 4]);
    cache.put(2, vec![0u8; 4]);
    cache.put(3, vec![0u8; 4]);
    let stats = cache.stats();

    // Assert：条数不超上限，最旧被淘汰
    assert_eq!(stats.entries, 2);
    assert!(cache.get(&1).is_none());
    assert!(cache.get(&3).is_some());
}

#[test]
fn test_entry_count_cap_eviction_subtracts_bytes() {
    // Arrange：条数上限 2，预算足够大
    let mut cache = make_cache(2, 1_000_000);

    // Act：写入 3 条，第三条触发条数上限淘汰
    cache.put(1, vec![0u8; 10]);
    cache.put(2, vec![0u8; 10]);
    cache.put(3, vec![0u8; 10]);
    let stats = cache.stats();

    // Assert：被条数上限淘汰的条目也要扣减字节，否则字节统计会持续虚高
    assert_eq!(stats.entries, 2);
    assert_eq!(stats.bytes, 20, "条数上限淘汰应同步扣减字节");
}
