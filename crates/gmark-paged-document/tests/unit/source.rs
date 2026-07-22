// @author kongweiguang

use super::PageCache;
use std::sync::Arc;

fn page(value: u8) -> Arc<[u8]> {
    Arc::from([value])
}

#[test]
fn mature_lru_keeps_recent_hits_and_evicts_the_oldest_page() {
    let mut cache = PageCache::with_capacity(2);
    cache.insert(1, page(1));
    cache.insert(2, page(2));

    assert_eq!(cache.get(1).as_deref(), Some([1].as_slice()));
    cache.insert(3, page(3));

    assert!(cache.get(2).is_none());
    assert_eq!(cache.get(1).as_deref(), Some([1].as_slice()));
    assert_eq!(cache.get(3).as_deref(), Some([3].as_slice()));
    assert_eq!(cache.len(), 2);
}
