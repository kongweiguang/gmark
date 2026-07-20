// @author kongweiguang

use super::{DiskPageCache, LinePage};
use std::sync::{Arc, OnceLock};

fn page(first_line: u64) -> Arc<LinePage> {
    Arc::new(LinePage {
        first_line,
        first_offset: first_line,
        line_count: 1,
        encoded_lengths: vec![1],
        decoded_ends: OnceLock::new(),
    })
}

#[test]
fn disk_page_lru_refreshes_recency_on_hit() {
    let mut cache = DiskPageCache::with_capacity(2);
    cache.insert(1, page(1));
    cache.insert(2, page(2));

    assert_eq!(cache.get(1).map(|page| page.first_line), Some(1));
    cache.insert(3, page(3));

    assert!(cache.get(2).is_none());
    assert_eq!(cache.get(1).map(|page| page.first_line), Some(1));
    assert_eq!(cache.get(3).map(|page| page.first_line), Some(3));
    assert_eq!(cache.len(), 2);
}
