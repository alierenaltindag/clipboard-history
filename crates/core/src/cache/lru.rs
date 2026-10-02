use lru::LruCache;
use std::hash::Hash;
use std::num::NonZeroUsize;
use std::sync::Mutex;

pub struct BoundedCache<K, V> {
    inner: Mutex<LruCache<K, V>>,
}

impl<K: Hash + Eq + Clone, V: Clone> BoundedCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        let non_zero_cap = NonZeroUsize::new(capacity.max(1)).unwrap();
        Self {
            inner: Mutex::new(LruCache::new(non_zero_cap)),
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        let mut guard = self.inner.lock().unwrap();
        guard.get(key).cloned()
    }

    pub fn put(&self, key: K, value: V) {
        let mut guard = self.inner.lock().unwrap();
        guard.put(key, value);
    }

    pub fn remove(&self, key: &K) -> Option<V> {
        let mut guard = self.inner.lock().unwrap();
        guard.pop(key)
    }

    pub fn clear(&self) {
        let mut guard = self.inner.lock().unwrap();
        guard.clear();
    }

    pub fn len(&self) -> usize {
        let guard = self.inner.lock().unwrap();
        guard.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
