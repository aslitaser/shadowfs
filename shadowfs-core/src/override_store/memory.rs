//! Memory tracking and allocation management.

use crate::error::ShadowError;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Tracks memory usage with atomic operations for thread-safe allocation.
#[derive(Debug)]
pub struct MemoryTracker {
    /// Current memory usage in bytes
    current_usage: AtomicUsize,
    
    /// Maximum allowed memory in bytes
    max_allowed: usize,
    
    /// Total number of allocations made
    allocation_count: AtomicU64,
}

impl MemoryTracker {
    /// Creates a new memory tracker with the specified limit.
    pub fn new(max_allowed: usize) -> Self {
        Self {
            current_usage: AtomicUsize::new(0),
            max_allowed,
            allocation_count: AtomicU64::new(0),
        }
    }
    
    /// Attempts to allocate memory, returning a guard if successful.
    ///
    /// # Arguments
    /// * `size` - Number of bytes to allocate
    ///
    /// # Returns
    /// * `Ok(MemoryGuard)` - Guard that will release memory when dropped
    /// * `Err(ShadowError)` - If allocation would exceed limits
    pub fn try_allocate(&self, size: usize) -> Result<MemoryGuard, ShadowError> {
        // Try to allocate atomically
        let mut current = self.current_usage.load(Ordering::Relaxed);
        
        loop {
            let new_usage = current.saturating_add(size);
            
            // Check if allocation would exceed limit
            if new_usage > self.max_allowed {
                let _available = self.max_allowed.saturating_sub(current);
                return Err(ShadowError::OverrideStoreFull {
                    current_size: current,
                    max_size: self.max_allowed,
                });
            }
            
            // Try to update atomically
            match self.current_usage.compare_exchange_weak(
                current,
                new_usage,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // Successfully allocated
                    self.allocation_count.fetch_add(1, Ordering::Relaxed);
                    return Ok(MemoryGuard {
                        size,
                        tracker: self,
                    });
                }
                Err(actual) => {
                    // Another thread changed the value, retry
                    current = actual;
                }
            }
        }
    }
    
    /// Returns the current memory usage in bytes.
    pub fn current_usage(&self) -> usize {
        self.current_usage.load(Ordering::Relaxed)
    }
    
    /// Returns the available space in bytes.
    pub fn available_space(&self) -> usize {
        let current = self.current_usage.load(Ordering::Relaxed);
        self.max_allowed.saturating_sub(current)
    }
    
    /// Returns true if memory usage is above 90%.
    pub fn is_under_pressure(&self) -> bool {
        self.get_pressure_ratio() > 0.9
    }
    
    /// Returns the memory pressure ratio (0.0 to 1.0).
    pub fn get_pressure_ratio(&self) -> f64 {
        let current = self.current_usage.load(Ordering::Relaxed) as f64;
        let max = self.max_allowed as f64;
        if max > 0.0 {
            current / max
        } else {
            1.0
        }
    }
    
    /// Internal method to release memory (called by MemoryGuard).
    fn release(&self, size: usize) {
        self.current_usage.fetch_sub(size, Ordering::AcqRel);
    }
}

/// RAII guard for allocated memory that releases it when dropped.
#[derive(Debug)]
pub struct MemoryGuard<'a> {
    /// Size of the allocation
    size: usize,
    
    /// Reference to the tracker
    #[allow(dead_code)]
    tracker: &'a MemoryTracker,
}

impl<'a> MemoryGuard<'a> {
    /// Returns the size of this allocation.
    pub fn size(&self) -> usize {
        self.size
    }
}

impl<'a> Drop for MemoryGuard<'a> {
    fn drop(&mut self) {
        self.tracker.release(self.size);
    }
}

// MemoryGuard is neither Clone nor Copy by design
// This ensures memory is only released once when the guard is dropped

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_tracker_allocation() {
        let tracker = MemoryTracker::new(1024);
        
        // Initial state
        assert_eq!(tracker.current_usage(), 0);
        assert_eq!(tracker.available_space(), 1024);
        assert!(!tracker.is_under_pressure());
        assert_eq!(tracker.get_pressure_ratio(), 0.0);
        
        // Allocate some memory
        let guard1 = tracker.try_allocate(256).unwrap();
        assert_eq!(tracker.current_usage(), 256);
        assert_eq!(tracker.available_space(), 768);
        assert_eq!(guard1.size(), 256);
        
        // Allocate more
        let guard2 = tracker.try_allocate(512).unwrap();
        assert_eq!(tracker.current_usage(), 768);
        assert_eq!(tracker.available_space(), 256);
        
        // Drop first guard
        drop(guard1);
        assert_eq!(tracker.current_usage(), 512);
        assert_eq!(tracker.available_space(), 512);
        
        // Drop second guard
        drop(guard2);
        assert_eq!(tracker.current_usage(), 0);
        assert_eq!(tracker.available_space(), 1024);
    }
    
    #[test]
    fn test_memory_tracker_allocation_failure() {
        let tracker = MemoryTracker::new(1024);
        
        // Allocate most of the memory
        let _guard = tracker.try_allocate(1000).unwrap();
        
        // Try to allocate more than available
        let result = tracker.try_allocate(100);
        assert!(result.is_err());
        
        match result.err().unwrap() {
            ShadowError::OverrideStoreFull { current_size, max_size } => {
                assert_eq!(current_size, 1000);
                assert_eq!(max_size, 1024);
            }
            _ => panic!("Expected OverrideStoreFull error"),
        }
    }
    
    #[test]
    fn test_memory_pressure_detection() {
        let tracker = MemoryTracker::new(1000);
        
        // No pressure initially
        assert!(!tracker.is_under_pressure());
        assert!(tracker.get_pressure_ratio() < 0.1);
        
        // Allocate 80% - still no pressure
        let _guard1 = tracker.try_allocate(800).unwrap();
        assert!(!tracker.is_under_pressure());
        assert!((tracker.get_pressure_ratio() - 0.8).abs() < 0.01);
        
        // Allocate to 95% - now under pressure
        let _guard2 = tracker.try_allocate(150).unwrap();
        assert!(tracker.is_under_pressure());
        assert!((tracker.get_pressure_ratio() - 0.95).abs() < 0.01);
    }
    
    #[test]
    fn test_concurrent_allocation() {
        use std::sync::Arc;
        use std::thread;
        
        let tracker = Arc::new(MemoryTracker::new(10000));
        let mut handles = vec![];
        
        // Spawn multiple threads that allocate memory
        for i in 0..10 {
            let tracker_clone = Arc::clone(&tracker);
            let handle = thread::spawn(move || {
                let mut total_allocated = 0;
                for j in 0..10 {
                    if let Ok(_guard) = tracker_clone.try_allocate(10 + i * 10 + j) {
                        total_allocated += 10 + i * 10 + j;
                        // Guard is dropped here, freeing memory immediately
                    }
                }
                total_allocated
            });
            handles.push(handle);
        }
        
        // Wait for all threads and sum allocations
        let total_attempted: usize = handles.into_iter()
            .map(|h| h.join().unwrap())
            .sum();
        
        // All memory should be freed by now
        assert_eq!(tracker.current_usage(), 0);
        
        // Check that we attempted reasonable allocations
        assert!(total_attempted > 0);
        assert!(total_attempted <= 10000);
    }
}