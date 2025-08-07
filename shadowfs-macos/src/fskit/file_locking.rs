use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock, Mutex, Condvar};
use std::time::{Duration, Instant};
use std::thread;

/// File locking subsystem with advisory locks and deadlock prevention
pub struct FileLockManager {
    /// Active locks indexed by file path
    locks: Arc<RwLock<HashMap<PathBuf, Vec<FileLock>>>>,
    /// Lock wait queue for each file
    wait_queues: Arc<RwLock<HashMap<PathBuf, VecDeque<LockRequest>>>>,
    /// Lock ownership graph for deadlock detection
    ownership_graph: Arc<RwLock<LockGraph>>,
    /// Condition variables for lock waiters
    wait_conditions: Arc<RwLock<HashMap<u64, Arc<(Mutex<bool>, Condvar)>>>>,
}

/// Individual file lock
#[derive(Debug, Clone)]
pub struct FileLock {
    /// Lock type (shared or exclusive)
    pub lock_type: LockType,
    /// Owner handle/process ID
    pub owner: u64,
    /// Optional byte range (start, length)
    pub range: Option<ByteRange>,
    /// Lock acquisition time
    pub acquired_at: Instant,
    /// Lock ID for tracking
    pub lock_id: u64,
}

/// Byte range for range locking
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ByteRange {
    pub start: u64,
    pub length: u64,
}

impl ByteRange {
    pub fn new(start: u64, length: u64) -> Self {
        Self { start, length }
    }
    
    pub fn end(&self) -> u64 {
        self.start + self.length
    }
    
    /// Check if two ranges overlap
    pub fn overlaps(&self, other: &ByteRange) -> bool {
        self.start < other.end() && other.start < self.end()
    }
    
    /// Check if this range fully contains another
    pub fn contains(&self, other: &ByteRange) -> bool {
        self.start <= other.start && self.end() >= other.end()
    }
}

/// Lock type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockType {
    /// Shared/read lock - multiple readers allowed
    Shared,
    /// Exclusive/write lock - single writer only
    Exclusive,
}

/// Lock request waiting in queue
#[derive(Debug, Clone)]
struct LockRequest {
    /// Requesting handle ID
    requester: u64,
    /// Requested lock type
    lock_type: LockType,
    /// Optional byte range
    range: Option<ByteRange>,
    /// Request timestamp
    requested_at: Instant,
    /// Optional timeout
    timeout: Option<Duration>,
}

/// Lock ownership graph for deadlock detection
struct LockGraph {
    /// Map of who owns what locks
    ownership: HashMap<u64, HashSet<PathBuf>>,
    /// Map of who is waiting for what
    waiting: HashMap<u64, PathBuf>,
    /// Lock dependencies
    dependencies: HashMap<u64, HashSet<u64>>,
}

impl LockGraph {
    fn new() -> Self {
        Self {
            ownership: HashMap::new(),
            waiting: HashMap::new(),
            dependencies: HashMap::new(),
        }
    }
    
    /// Add a lock ownership
    fn add_ownership(&mut self, owner: u64, path: PathBuf) {
        self.ownership.entry(owner).or_insert_with(HashSet::new).insert(path);
    }
    
    /// Remove a lock ownership
    fn remove_ownership(&mut self, owner: u64, path: &Path) {
        if let Some(paths) = self.ownership.get_mut(&owner) {
            paths.remove(path);
            if paths.is_empty() {
                self.ownership.remove(&owner);
            }
        }
    }
    
    /// Add a wait dependency
    fn add_wait(&mut self, waiter: u64, path: PathBuf, current_owners: Vec<u64>) {
        self.waiting.insert(waiter, path);
        
        // Add dependencies to current lock owners
        for owner in current_owners {
            self.dependencies.entry(waiter)
                .or_insert_with(HashSet::new)
                .insert(owner);
        }
    }
    
    /// Remove a wait dependency
    fn remove_wait(&mut self, waiter: u64) {
        self.waiting.remove(&waiter);
        self.dependencies.remove(&waiter);
    }
    
    /// Check for deadlock using cycle detection
    fn has_deadlock(&self, new_waiter: u64) -> bool {
        let mut visited = HashSet::new();
        let mut stack = HashSet::new();
        
        self.detect_cycle(new_waiter, &mut visited, &mut stack)
    }
    
    /// DFS cycle detection
    fn detect_cycle(&self, node: u64, visited: &mut HashSet<u64>, stack: &mut HashSet<u64>) -> bool {
        visited.insert(node);
        stack.insert(node);
        
        if let Some(deps) = self.dependencies.get(&node) {
            for &dep in deps {
                if !visited.contains(&dep) {
                    if self.detect_cycle(dep, visited, stack) {
                        return true;
                    }
                } else if stack.contains(&dep) {
                    // Found a cycle
                    return true;
                }
            }
        }
        
        stack.remove(&node);
        false
    }
}

impl FileLockManager {
    /// Create a new lock manager
    pub fn new() -> Self {
        Self {
            locks: Arc::new(RwLock::new(HashMap::new())),
            wait_queues: Arc::new(RwLock::new(HashMap::new())),
            ownership_graph: Arc::new(RwLock::new(LockGraph::new())),
            wait_conditions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Acquire a lock on a file
    pub fn acquire_lock(
        &self,
        path: &Path,
        owner: u64,
        lock_type: LockType,
        range: Option<ByteRange>,
        timeout: Option<Duration>,
    ) -> Result<u64, String> {
        // Generate lock ID
        let lock_id = self.generate_lock_id();
        
        // Check if we can acquire immediately
        if self.can_acquire_lock(path, owner, lock_type, &range)? {
            self.grant_lock(path, owner, lock_type, range, lock_id)?;
            return Ok(lock_id);
        }
        
        // Check for potential deadlock before waiting
        if self.would_cause_deadlock(owner, path)? {
            return Err("Lock acquisition would cause deadlock".to_string());
        }
        
        // Add to wait queue
        self.add_to_wait_queue(path, owner, lock_type, range.clone(), timeout)?;
        
        // Wait for lock with timeout
        match self.wait_for_lock(owner, timeout) {
            Ok(true) => {
                // Lock acquired
                self.grant_lock(path, owner, lock_type, range, lock_id)?;
                Ok(lock_id)
            }
            Ok(false) => {
                // Timeout
                self.remove_from_wait_queue(path, owner)?;
                Err("Lock acquisition timed out".to_string())
            }
            Err(e) => {
                self.remove_from_wait_queue(path, owner)?;
                Err(e)
            }
        }
    }
    
    /// Try to acquire a lock without waiting
    pub fn try_acquire_lock(
        &self,
        path: &Path,
        owner: u64,
        lock_type: LockType,
        range: Option<ByteRange>,
    ) -> Result<Option<u64>, String> {
        if self.can_acquire_lock(path, owner, lock_type, &range)? {
            let lock_id = self.generate_lock_id();
            self.grant_lock(path, owner, lock_type, range, lock_id)?;
            Ok(Some(lock_id))
        } else {
            Ok(None)
        }
    }
    
    /// Release a lock
    pub fn release_lock(&self, path: &Path, lock_id: u64) -> Result<(), String> {
        let mut locks_map = self.locks.write()
            .map_err(|e| format!("Failed to acquire locks: {}", e))?;
        
        if let Some(file_locks) = locks_map.get_mut(path) {
            // Find and remove the lock
            if let Some(pos) = file_locks.iter().position(|l| l.lock_id == lock_id) {
                let lock = file_locks.remove(pos);
                
                // Update ownership graph
                let mut graph = self.ownership_graph.write()
                    .map_err(|e| format!("Failed to acquire graph lock: {}", e))?;
                graph.remove_ownership(lock.owner, path);
                drop(graph);
                
                // Clean up empty entries
                if file_locks.is_empty() {
                    locks_map.remove(path);
                }
                
                // Process wait queue
                drop(locks_map);
                self.process_wait_queue(path)?;
                
                Ok(())
            } else {
                Err(format!("Lock {} not found", lock_id))
            }
        } else {
            Err("No locks found for file".to_string())
        }
    }
    
    /// Release all locks owned by a handle
    pub fn release_all_locks(&self, owner: u64) -> Result<(), String> {
        let locks_map = self.locks.read()
            .map_err(|e| format!("Failed to acquire locks: {}", e))?;
        
        // Collect all paths with locks owned by this handle
        let mut paths_to_process = Vec::new();
        for (path, file_locks) in locks_map.iter() {
            if file_locks.iter().any(|l| l.owner == owner) {
                paths_to_process.push(path.clone());
            }
        }
        drop(locks_map);
        
        // Release locks for each path
        for path in paths_to_process {
            self.release_locks_for_owner(&path, owner)?;
        }
        
        Ok(())
    }
    
    /// Upgrade a shared lock to exclusive
    pub fn upgrade_lock(&self, path: &Path, lock_id: u64) -> Result<(), String> {
        let mut locks_map = self.locks.write()
            .map_err(|e| format!("Failed to acquire locks: {}", e))?;
        
        if let Some(file_locks) = locks_map.get_mut(path) {
            // Find the lock
            if let Some(lock) = file_locks.iter_mut().find(|l| l.lock_id == lock_id) {
                if lock.lock_type != LockType::Shared {
                    return Err("Lock is not shared".to_string());
                }
                
                // Check if upgrade is possible
                let other_locks: Vec<_> = file_locks.iter()
                    .filter(|l| l.lock_id != lock_id)
                    .filter(|l| {
                        if let (Some(r1), Some(r2)) = (&lock.range, &l.range) {
                            r1.overlaps(r2)
                        } else {
                            lock.range.is_none() || l.range.is_none()
                        }
                    })
                    .collect();
                
                if !other_locks.is_empty() {
                    return Err("Cannot upgrade: other locks exist".to_string());
                }
                
                // Upgrade the lock
                lock.lock_type = LockType::Exclusive;
                Ok(())
            } else {
                Err(format!("Lock {} not found", lock_id))
            }
        } else {
            Err("No locks found for file".to_string())
        }
    }
    
    /// Downgrade an exclusive lock to shared
    pub fn downgrade_lock(&self, path: &Path, lock_id: u64) -> Result<(), String> {
        let mut locks_map = self.locks.write()
            .map_err(|e| format!("Failed to acquire locks: {}", e))?;
        
        if let Some(file_locks) = locks_map.get_mut(path) {
            if let Some(lock) = file_locks.iter_mut().find(|l| l.lock_id == lock_id) {
                if lock.lock_type != LockType::Exclusive {
                    return Err("Lock is not exclusive".to_string());
                }
                
                // Downgrade the lock
                lock.lock_type = LockType::Shared;
                
                // Process wait queue as shared locks might now be grantable
                drop(locks_map);
                self.process_wait_queue(path)?;
                
                Ok(())
            } else {
                Err(format!("Lock {} not found", lock_id))
            }
        } else {
            Err("No locks found for file".to_string())
        }
    }
    
    /// Get all locks for a file
    pub fn get_locks(&self, path: &Path) -> Result<Vec<FileLock>, String> {
        let locks_map = self.locks.read()
            .map_err(|e| format!("Failed to acquire locks: {}", e))?;
        
        Ok(locks_map.get(path)
            .map(|locks| locks.clone())
            .unwrap_or_else(Vec::new))
    }
    
    /// Check if a specific byte range is locked
    pub fn is_range_locked(&self, path: &Path, range: &ByteRange, for_write: bool) -> Result<bool, String> {
        let locks_map = self.locks.read()
            .map_err(|e| format!("Failed to acquire locks: {}", e))?;
        
        if let Some(file_locks) = locks_map.get(path) {
            for lock in file_locks {
                // Check lock type compatibility
                if for_write || lock.lock_type == LockType::Exclusive {
                    // Check range overlap
                    if let Some(lock_range) = &lock.range {
                        if lock_range.overlaps(range) {
                            return Ok(true);
                        }
                    } else {
                        // Whole file lock
                        return Ok(true);
                    }
                }
            }
        }
        
        Ok(false)
    }
    
    // Helper methods
    
    fn can_acquire_lock(
        &self,
        path: &Path,
        owner: u64,
        lock_type: LockType,
        range: &Option<ByteRange>,
    ) -> Result<bool, String> {
        let locks_map = self.locks.read()
            .map_err(|e| format!("Failed to acquire locks: {}", e))?;
        
        if let Some(file_locks) = locks_map.get(path) {
            for existing_lock in file_locks {
                // Skip locks owned by the same owner
                if existing_lock.owner == owner {
                    continue;
                }
                
                // Check compatibility
                if !self.locks_compatible(lock_type, existing_lock.lock_type, range, &existing_lock.range) {
                    return Ok(false);
                }
            }
        }
        
        Ok(true)
    }
    
    fn locks_compatible(
        &self,
        new_type: LockType,
        existing_type: LockType,
        new_range: &Option<ByteRange>,
        existing_range: &Option<ByteRange>,
    ) -> bool {
        // Check range overlap first
        let ranges_overlap = match (new_range, existing_range) {
            (Some(r1), Some(r2)) => r1.overlaps(r2),
            (None, _) | (_, None) => true, // Whole file locks always overlap
        };
        
        if !ranges_overlap {
            return true; // Non-overlapping ranges are always compatible
        }
        
        // Check type compatibility for overlapping ranges
        match (new_type, existing_type) {
            (LockType::Shared, LockType::Shared) => true,
            _ => false,
        }
    }
    
    fn grant_lock(
        &self,
        path: &Path,
        owner: u64,
        lock_type: LockType,
        range: Option<ByteRange>,
        lock_id: u64,
    ) -> Result<(), String> {
        let mut locks_map = self.locks.write()
            .map_err(|e| format!("Failed to acquire locks: {}", e))?;
        
        let lock = FileLock {
            lock_type,
            owner,
            range,
            acquired_at: Instant::now(),
            lock_id,
        };
        
        locks_map.entry(path.to_path_buf())
            .or_insert_with(Vec::new)
            .push(lock);
        
        // Update ownership graph
        let mut graph = self.ownership_graph.write()
            .map_err(|e| format!("Failed to acquire graph lock: {}", e))?;
        graph.add_ownership(owner, path.to_path_buf());
        graph.remove_wait(owner);
        
        Ok(())
    }
    
    fn would_cause_deadlock(&self, waiter: u64, path: &Path) -> Result<bool, String> {
        let locks_map = self.locks.read()
            .map_err(|e| format!("Failed to acquire locks: {}", e))?;
        
        let mut current_owners = Vec::new();
        if let Some(file_locks) = locks_map.get(path) {
            for lock in file_locks {
                if lock.owner != waiter {
                    current_owners.push(lock.owner);
                }
            }
        }
        drop(locks_map);
        
        let mut graph = self.ownership_graph.write()
            .map_err(|e| format!("Failed to acquire graph lock: {}", e))?;
        
        // Temporarily add the wait dependency
        graph.add_wait(waiter, path.to_path_buf(), current_owners);
        
        // Check for deadlock
        let has_deadlock = graph.has_deadlock(waiter);
        
        // Remove the temporary dependency
        graph.remove_wait(waiter);
        
        Ok(has_deadlock)
    }
    
    fn add_to_wait_queue(
        &self,
        path: &Path,
        requester: u64,
        lock_type: LockType,
        range: Option<ByteRange>,
        timeout: Option<Duration>,
    ) -> Result<(), String> {
        let mut queues = self.wait_queues.write()
            .map_err(|e| format!("Failed to acquire wait queues: {}", e))?;
        
        let request = LockRequest {
            requester,
            lock_type,
            range,
            requested_at: Instant::now(),
            timeout,
        };
        
        queues.entry(path.to_path_buf())
            .or_insert_with(VecDeque::new)
            .push_back(request);
        
        // Create condition variable for this waiter
        let mut conditions = self.wait_conditions.write()
            .map_err(|e| format!("Failed to acquire conditions: {}", e))?;
        conditions.insert(requester, Arc::new((Mutex::new(false), Condvar::new())));
        
        Ok(())
    }
    
    fn remove_from_wait_queue(&self, path: &Path, requester: u64) -> Result<(), String> {
        let mut queues = self.wait_queues.write()
            .map_err(|e| format!("Failed to acquire wait queues: {}", e))?;
        
        if let Some(queue) = queues.get_mut(path) {
            queue.retain(|r| r.requester != requester);
            if queue.is_empty() {
                queues.remove(path);
            }
        }
        
        // Remove condition variable
        let mut conditions = self.wait_conditions.write()
            .map_err(|e| format!("Failed to acquire conditions: {}", e))?;
        conditions.remove(&requester);
        
        Ok(())
    }
    
    fn wait_for_lock(&self, waiter: u64, timeout: Option<Duration>) -> Result<bool, String> {
        let conditions = self.wait_conditions.read()
            .map_err(|e| format!("Failed to acquire conditions: {}", e))?;
        
        if let Some(cond_var) = conditions.get(&waiter) {
            let cond_var = Arc::clone(cond_var);
            drop(conditions);
            
            let (lock, condvar) = &**cond_var;
            let mut granted = lock.lock().unwrap();
            
            if let Some(timeout) = timeout {
                let result = condvar.wait_timeout(granted, timeout).unwrap();
                Ok(*result.0)
            } else {
                *granted = condvar.wait(granted).unwrap();
                Ok(*granted)
            }
        } else {
            Err("No condition variable found for waiter".to_string())
        }
    }
    
    fn process_wait_queue(&self, path: &Path) -> Result<(), String> {
        let mut queues = self.wait_queues.write()
            .map_err(|e| format!("Failed to acquire wait queues: {}", e))?;
        
        if let Some(queue) = queues.get_mut(path) {
            let mut granted = Vec::new();
            
            // Try to grant locks to waiters
            for request in queue.iter() {
                if self.can_acquire_lock(path, request.requester, request.lock_type, &request.range)? {
                    granted.push(request.requester);
                    
                    // Signal the waiter
                    let conditions = self.wait_conditions.read()
                        .map_err(|e| format!("Failed to acquire conditions: {}", e))?;
                    
                    if let Some(cond_var) = conditions.get(&request.requester) {
                        let (lock, condvar) = &***cond_var;
                        let mut grant = lock.lock().unwrap();
                        *grant = true;
                        condvar.notify_one();
                    }
                }
            }
            
            // Remove granted requests from queue
            for requester in granted {
                queue.retain(|r| r.requester != requester);
            }
            
            if queue.is_empty() {
                queues.remove(path);
            }
        }
        
        Ok(())
    }
    
    fn release_locks_for_owner(&self, path: &Path, owner: u64) -> Result<(), String> {
        let mut locks_map = self.locks.write()
            .map_err(|e| format!("Failed to acquire locks: {}", e))?;
        
        if let Some(file_locks) = locks_map.get_mut(path) {
            file_locks.retain(|l| l.owner != owner);
            
            if file_locks.is_empty() {
                locks_map.remove(path);
            }
        }
        
        // Update ownership graph
        let mut graph = self.ownership_graph.write()
            .map_err(|e| format!("Failed to acquire graph lock: {}", e))?;
        graph.remove_ownership(owner, path);
        drop(graph);
        
        // Process wait queue
        drop(locks_map);
        self.process_wait_queue(path)?;
        
        Ok(())
    }
    
    fn generate_lock_id(&self) -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        COUNTER.fetch_add(1, Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_byte_range_overlap() {
        let range1 = ByteRange::new(0, 100);
        let range2 = ByteRange::new(50, 100);
        let range3 = ByteRange::new(100, 50);
        let range4 = ByteRange::new(200, 50);
        
        assert!(range1.overlaps(&range2));
        assert!(range2.overlaps(&range1));
        assert!(range1.overlaps(&range3));
        assert!(!range1.overlaps(&range4));
    }
    
    #[test]
    fn test_shared_locks_compatible() {
        let manager = FileLockManager::new();
        let path = Path::new("/test/file");
        
        // Acquire first shared lock
        let lock1 = manager.acquire_lock(path, 1, LockType::Shared, None, None).unwrap();
        
        // Second shared lock should succeed
        let lock2 = manager.acquire_lock(path, 2, LockType::Shared, None, None).unwrap();
        
        assert_ne!(lock1, lock2);
    }
    
    #[test]
    fn test_exclusive_lock_blocks() {
        let manager = FileLockManager::new();
        let path = Path::new("/test/file");
        
        // Acquire exclusive lock
        let _lock1 = manager.acquire_lock(path, 1, LockType::Exclusive, None, None).unwrap();
        
        // Try to acquire another lock (should fail immediately with try_acquire)
        let result = manager.try_acquire_lock(path, 2, LockType::Shared, None).unwrap();
        assert!(result.is_none());
    }
    
    #[test]
    fn test_range_locking() {
        let manager = FileLockManager::new();
        let path = Path::new("/test/file");
        
        // Lock first range
        let range1 = ByteRange::new(0, 100);
        let _lock1 = manager.acquire_lock(path, 1, LockType::Exclusive, Some(range1), None).unwrap();
        
        // Lock non-overlapping range should succeed
        let range2 = ByteRange::new(200, 100);
        let _lock2 = manager.acquire_lock(path, 2, LockType::Exclusive, Some(range2), None).unwrap();
        
        // Check if overlapping range is locked
        let range3 = ByteRange::new(50, 100);
        assert!(manager.is_range_locked(path, &range3, true).unwrap());
        
        // Check if non-overlapping range is not locked
        let range4 = ByteRange::new(150, 50);
        assert!(!manager.is_range_locked(path, &range4, true).unwrap());
    }
}