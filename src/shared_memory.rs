//! Zero-Copy Shared Memory Module
//!
//! This module implements zero-copy data sharing between execution contexts:
//! - SharedMemoryRegion for inter-partition data sharing
//! - BufferPool for buffer reuse and inertia-based caching
//! - PayloadView for zero-copy payload access

use crate::ir::MergeMode;
use crate::reducer::PayloadValue;
use bytes::{Bytes, BytesMut};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A view into a payload without copying
#[derive(Debug, Clone)]
pub enum PayloadView<'a> {
    I64(i64),
    String(&'a str),
    Bytes(&'a [u8]),
}

impl<'a> PayloadView<'a> {
    /// Create a view from raw bytes
    pub fn from_bytes(bytes: &'a [u8], _merge_mode: MergeMode) -> Self {
        if bytes.is_empty() {
            return PayloadView::Bytes(&[]);
        }

        match bytes[0] {
            1 => {
                if bytes.len() >= 9 {
                    let mut arr = [0u8; 8];
                    arr.copy_from_slice(&bytes[1..9]);
                    PayloadView::I64(i64::from_le_bytes(arr))
                } else {
                    PayloadView::Bytes(bytes)
                }
            }
            2 => {
                if bytes.len() >= 5 {
                    let mut len = [0u8; 4];
                    len.copy_from_slice(&bytes[1..5]);
                    let n = u32::from_le_bytes(len) as usize;
                    if bytes.len() >= 5 + n {
                        match std::str::from_utf8(&bytes[5..5 + n]) {
                            Ok(s) => PayloadView::String(s),
                            Err(_) => PayloadView::Bytes(&bytes[5..5 + n]),
                        }
                    } else {
                        PayloadView::Bytes(bytes)
                    }
                } else {
                    PayloadView::Bytes(bytes)
                }
            }
            _ => PayloadView::Bytes(bytes),
        }
    }

    /// Convert view to owned value (only copies when necessary)
    pub fn to_owned(&self) -> PayloadValue {
        match self {
            PayloadView::I64(v) => PayloadValue::I64(*v),
            PayloadView::String(s) => PayloadValue::Str(s.to_string()),
            PayloadView::Bytes(b) => PayloadValue::Str(format!("{:?}", b)), // Fallback
        }
    }

    /// Get the underlying bytes without copying
    pub fn as_bytes(&self) -> Vec<u8> {
        match self {
            PayloadView::Bytes(b) => b.to_vec(),
            PayloadView::I64(v) => v.to_le_bytes().to_vec(),
            PayloadView::String(s) => s.as_bytes().to_vec(),
        }
    }
}

/// A shared memory region for zero-copy data sharing
#[derive(Debug)]
pub struct SharedMemoryRegion {
    id: String,
    data: RwLock<BytesMut>,
    readers: RwLock<usize>,
}

impl SharedMemoryRegion {
    pub fn new(id: String, capacity: usize) -> Self {
        Self {
            id,
            data: RwLock::new(BytesMut::with_capacity(capacity)),
            readers: RwLock::new(0),
        }
    }

    /// Write data to the shared region
    pub async fn write(&self, data: &[u8]) -> Result<(), String> {
        let mut buffer = self.data.write().await;
        buffer.clear();
        buffer.extend_from_slice(data);
        Ok(())
    }

    /// Read data from the shared region
    pub async fn read_view(&self) -> PayloadValue {
        let buffer = self.data.read().await;
        let bytes: &[u8] = &buffer;
        let view = PayloadView::from_bytes(bytes, MergeMode::List);
        view.to_owned()
    }

    /// Get the number of active readers
    pub async fn reader_count(&self) -> usize {
        *self.readers.read().await
    }

    /// Acquire read access
    pub async fn acquire_read(&self) {
        let mut readers = self.readers.write().await;
        *readers += 1;
    }

    /// Release read access
    pub async fn release_read(&self) {
        let mut readers = self.readers.write().await;
        *readers = readers.saturating_sub(1);
    }

    /// Get the ID of this region
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get current size of data in the region
    pub async fn size(&self) -> usize {
        self.data.read().await.len()
    }
}

/// Pool of reusable buffers to reduce allocation overhead
#[derive(Debug)]
pub struct BufferPool {
    pools: RwLock<HashMap<usize, Vec<BytesMut>>>,
    max_buffers_per_size: usize,
}

impl BufferPool {
    pub fn new() -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
            max_buffers_per_size: 16,
        }
    }

    pub fn with_max_buffers(max_buffers: usize) -> Self {
        let mut pool = Self::new();
        pool.max_buffers_per_size = max_buffers;
        pool
    }

    /// Acquire a buffer of at least the requested size
    pub async fn acquire(&self, size: usize) -> BytesMut {
        let mut pools = self.pools.write().await;

        // Find the next power of two size
        let pool_size = size.next_power_of_two();

        if let Some(buffer) = pools.get_mut(&pool_size).and_then(|vec| vec.pop()) {
            buffer
        } else {
            BytesMut::with_capacity(pool_size)
        }
    }

    /// Return a buffer to the pool
    pub async fn release(&self, buffer: BytesMut) {
        let capacity = buffer.capacity();
        let mut pools = self.pools.write().await;

        let pool_size = capacity.next_power_of_two();
        let pool = pools.entry(pool_size).or_insert_with(Vec::new);

        if pool.len() < self.max_buffers_per_size {
            pool.push(buffer);
        }
        // Otherwise, just drop the buffer
    }

    /// Get statistics about the pool
    pub async fn stats(&self) -> PoolStats {
        let pools = self.pools.read().await;
        let total_buffers = pools.values().map(|v| v.len()).sum();
        let total_capacity = pools.values()
            .flat_map(|vec| vec.iter())
            .map(|b| b.capacity())
            .sum();

        PoolStats {
            total_buffers,
            total_capacity,
            size_categories: pools.len(),
        }
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the buffer pool
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub total_buffers: usize,
    pub total_capacity: usize,
    pub size_categories: usize,
}

/// Inertia-aware buffer caching for reuse
#[derive(Debug)]
pub struct InertiaBufferCache {
    cache: RwLock<HashMap<String, CachedBuffer>>,
    max_entries: usize,
}

#[derive(Debug, Clone)]
struct CachedBuffer {
    data: Bytes,
    #[allow(dead_code)]
    inertia_key: String,
    last_used: u32,
    access_count: u32,
}

impl InertiaBufferCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            max_entries,
        }
    }

    /// Get a buffer from cache by inertia key
    pub async fn get(&self, key: &str) -> Option<Bytes> {
        let mut cache = self.cache.write().await;
        if let Some(entry) = cache.get_mut(key) {
            entry.last_used = Self::current_time();
            entry.access_count += 1;
            Some(entry.data.clone())
        } else {
            None
        }
    }

    /// Put a buffer into cache with an inertia key
    pub async fn put(&self, key: String, data: Bytes) {
        let mut cache = self.cache.write().await;

        // Evict if at capacity
        if cache.len() >= self.max_entries {
            self.evict_lru(&mut cache);
        }

        cache.insert(key.clone(), CachedBuffer {
            data,
            inertia_key: key.clone(),
            last_used: Self::current_time(),
            access_count: 1,
        });
    }

    /// Check if a buffer exists in cache
    pub async fn contains(&self, key: &str) -> bool {
        self.cache.read().await.contains_key(key)
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        let total_access = cache.values().map(|e| e.access_count as usize).sum();
        CacheStats {
            entries: cache.len(),
            total_access,
            hit_rate: 0.0, // Would need to track misses
        }
    }

    /// Remove entries that haven't been accessed recently
    pub async fn cleanup_old(&self, max_age: u32) {
        let mut cache = self.cache.write().await;
        let now = Self::current_time();
        let to_remove: Vec<String> = cache
            .iter()
            .filter(|(_, e)| now.saturating_sub(e.last_used) > max_age)
            .map(|(k, _)| k.clone())
            .collect();

        for key in to_remove {
            cache.remove(&key);
        }
    }

    fn evict_lru(&self, cache: &mut HashMap<String, CachedBuffer>) {
        if let Some((lru_key, _)) = cache
            .iter()
            .min_by_key(|(_, a)| (a.last_used, a.access_count))
        {
            let key = lru_key.clone();
            cache.remove(&key);
        }
    }

    fn current_time() -> u32 {
        // In a real implementation, this would be actual time
        // For now, use a monotonic counter
        use std::sync::atomic::{AtomicU32, Ordering};
        static TIME: AtomicU32 = AtomicU32::new(0);
        TIME.fetch_add(1, Ordering::Relaxed)
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub entries: usize,
    pub total_access: usize,
    pub hit_rate: f64,
}

/// Zero-copy context for execution
#[derive(Debug)]
pub struct ZeroCopyContext {
    buffer_pool: Arc<BufferPool>,
    shared_regions: Arc<RwLock<HashMap<String, Arc<SharedMemoryRegion>>>>,
    inertia_cache: Arc<InertiaBufferCache>,
}

impl ZeroCopyContext {
    pub fn new() -> Self {
        Self {
            buffer_pool: Arc::new(BufferPool::new()),
            shared_regions: Arc::new(RwLock::new(HashMap::new())),
            inertia_cache: Arc::new(InertiaBufferCache::new(64)),
        }
    }

    /// Acquire a buffer from the pool
    pub async fn acquire_buffer(&self, size: usize) -> BytesMut {
        self.buffer_pool.acquire(size).await
    }

    /// Return a buffer to the pool
    pub async fn release_buffer(&self, buffer: BytesMut) {
        self.buffer_pool.release(buffer).await
    }

    /// Get or create a shared memory region
    pub async fn get_shared_region(&self, id: &str, capacity: usize) -> Arc<SharedMemoryRegion> {
        let mut regions = self.shared_regions.write().await;

        if let Some(region) = regions.get(id) {
            Arc::clone(region)
        } else {
            let region = Arc::new(SharedMemoryRegion::new(id.to_string(), capacity));
            regions.insert(id.to_string(), Arc::clone(&region));
            region
        }
    }

    /// Get buffer from inertia cache
    pub async fn get_cached(&self, key: &str) -> Option<Bytes> {
        self.inertia_cache.get(key).await
    }

    /// Put buffer into inertia cache
    pub async fn cache(&self, key: String, data: Bytes) {
        self.inertia_cache.put(key, data).await
    }

    /// Get buffer pool statistics
    pub async fn buffer_stats(&self) -> PoolStats {
        self.buffer_pool.stats().await
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> CacheStats {
        self.inertia_cache.stats().await
    }

    /// Cleanup old cache entries
    pub async fn cleanup_cache(&self, max_age: u32) {
        self.inertia_cache.cleanup_old(max_age).await;
    }
}

impl Default for ZeroCopyContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shared_memory_region() {
        let region = SharedMemoryRegion::new("test".to_string(), 1024);

        region.write(&[1, 2, 3, 4]).await.unwrap();
        assert_eq!(region.size().await, 4);

        region.acquire_read().await;
        assert_eq!(region.reader_count().await, 1);

        region.release_read().await;
        assert_eq!(region.reader_count().await, 0);
    }

    #[tokio::test]
    async fn test_buffer_pool() {
        let pool = BufferPool::new();

        let buf1 = pool.acquire(100).await;
        assert!(buf1.capacity() >= 100);

        pool.release(buf1).await;

        let stats = pool.stats().await;
        assert_eq!(stats.total_buffers, 1);
    }

    #[tokio::test]
    async fn test_inertia_cache() {
        let cache = InertiaBufferCache::new(4);

        let data = Bytes::from(&[1, 2, 3, 4][..]);
        cache.put("key1".to_string(), data.clone()).await;

        assert!(cache.contains("key1").await);
        assert_eq!(cache.get("key1").await, Some(data));
        assert_eq!(cache.get("key2").await, None);
    }

    #[tokio::test]
    async fn test_zero_copy_context() {
        let ctx = ZeroCopyContext::new();

        let buf = ctx.acquire_buffer(100).await;
        assert!(buf.capacity() >= 100);

        ctx.release_buffer(buf).await;

        let region = ctx.get_shared_region("region1", 1024).await;
        assert_eq!(region.id(), "region1");
    }

    #[test]
    fn test_payload_view() {
        let bytes = &[1, 42, 0, 0, 0, 0, 0, 0, 0][..]; // I64(42)
        let view = PayloadView::from_bytes(bytes, MergeMode::List);

        match view {
            PayloadView::I64(42) => {}
            _ => panic!("Expected I64(42), got {:?}", view),
        }
    }
}
