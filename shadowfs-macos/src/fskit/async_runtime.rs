use std::collections::BinaryHeap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use dispatch::{Queue, QueueAttribute, QueuePriority};
use tokio::sync::{mpsc, oneshot, Semaphore};
use tokio::time::{timeout, sleep};

use crate::Result;

/// Priority levels for filesystem operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OperationPriority {
    /// Critical operations (metadata, directory listings)
    Critical = 3,
    /// High priority (reads)
    High = 2,
    /// Normal priority (writes)
    Normal = 1,
    /// Low priority (background tasks)
    Low = 0,
}

/// Operation types for scheduling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    Read,
    Write,
    Metadata,
    Directory,
    Attribute,
    Sync,
}

impl OperationType {
    fn priority(&self) -> OperationPriority {
        match self {
            Self::Metadata | Self::Directory => OperationPriority::Critical,
            Self::Read | Self::Attribute => OperationPriority::High,
            Self::Write => OperationPriority::Normal,
            Self::Sync => OperationPriority::Low,
        }
    }
}

/// Queued operation with priority
struct QueuedOperation {
    id: u64,
    priority: OperationPriority,
    operation_type: OperationType,
    created_at: Instant,
    execute: Box<dyn FnOnce() + Send + 'static>,
}

impl PartialEq for QueuedOperation {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for QueuedOperation {}

impl PartialOrd for QueuedOperation {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueuedOperation {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.created_at.cmp(&self.created_at))
    }
}

/// Write coalescing entry
struct CoalescedWrite {
    path: String,
    data: Vec<u8>,
    offset: u64,
    pending_count: usize,
    wakers: Vec<Waker>,
}

/// FSKit-Tokio async runtime bridge
pub struct AsyncRuntime {
    /// Dispatch queue for FSKit operations
    dispatch_queue: Queue,
    /// Operation queue with priority scheduling
    operation_queue: Arc<Mutex<BinaryHeap<QueuedOperation>>>,
    /// Write coalescing map
    coalesced_writes: Arc<DashMap<String, Arc<Mutex<CoalescedWrite>>>>,
    /// Tokio runtime handle
    runtime: tokio::runtime::Handle,
    /// Operation counter for unique IDs
    operation_counter: Arc<std::sync::atomic::AtomicU64>,
    /// Semaphore for concurrency control
    concurrency_limiter: Arc<Semaphore>,
    /// Metrics collector
    metrics: Arc<Metrics>,
    /// Shutdown signal
    shutdown_tx: mpsc::Sender<()>,
    shutdown_rx: Arc<Mutex<mpsc::Receiver<()>>>,
}

impl AsyncRuntime {
    /// Create a new async runtime bridge
    pub fn new(max_concurrent_ops: usize) -> Result<Self> {
        let dispatch_queue = Queue::create(
            "com.shadowfs.fskit.async",
            QueueAttribute::Concurrent,
        );
        
        let runtime = tokio::runtime::Handle::current();
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        
        let runtime_bridge = Self {
            dispatch_queue,
            operation_queue: Arc::new(Mutex::new(BinaryHeap::new())),
            coalesced_writes: Arc::new(DashMap::new()),
            runtime,
            operation_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            concurrency_limiter: Arc::new(Semaphore::new(max_concurrent_ops)),
            metrics: Arc::new(Metrics::new()),
            shutdown_tx,
            shutdown_rx: Arc::new(Mutex::new(shutdown_rx)),
        };
        
        runtime_bridge.start_executor();
        Ok(runtime_bridge)
    }
    
    /// Start the operation executor
    fn start_executor(&self) {
        let queue = Arc::clone(&self.operation_queue);
        let limiter = Arc::clone(&self.concurrency_limiter);
        let metrics = Arc::clone(&self.metrics);
        let mut shutdown_rx = self.shutdown_rx.lock().unwrap();
        
        self.runtime.spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => break,
                    _ = sleep(Duration::from_millis(1)) => {
                        if let Ok(_permit) = limiter.try_acquire() {
                            if let Some(op) = queue.lock().unwrap().pop() {
                                let start = Instant::now();
                                metrics.record_queue_depth(queue.lock().unwrap().len());
                                
                                tokio::task::spawn_blocking(move || {
                                    (op.execute)();
                                    metrics.record_operation_latency(
                                        op.operation_type,
                                        start.elapsed(),
                                    );
                                });
                            }
                        }
                    }
                }
            }
        });
    }
    
    /// Queue an operation with priority
    pub fn queue_operation<F>(
        &self,
        operation_type: OperationType,
        priority: Option<OperationPriority>,
        f: F,
    ) -> FSKitFuture<()>
    where
        F: FnOnce() + Send + 'static,
    {
        let id = self.operation_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let priority = priority.unwrap_or_else(|| operation_type.priority());
        
        let (tx, rx) = oneshot::channel();
        
        let operation = QueuedOperation {
            id,
            priority,
            operation_type,
            created_at: Instant::now(),
            execute: Box::new(move || {
                f();
                let _ = tx.send(());
            }),
        };
        
        self.operation_queue.lock().unwrap().push(operation);
        
        FSKitFuture::new(async move {
            rx.await.map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "Operation cancelled")
            })
        })
    }
    
    /// Coalesce write operations
    pub fn coalesce_write(
        &self,
        path: String,
        data: Vec<u8>,
        offset: u64,
    ) -> FSKitFuture<()> {
        let coalesced_writes = Arc::clone(&self.coalesced_writes);
        
        FSKitFuture::new(async move {
            let entry = coalesced_writes.entry(path.clone()).or_insert_with(|| {
                Arc::new(Mutex::new(CoalescedWrite {
                    path: path.clone(),
                    data: Vec::new(),
                    offset,
                    pending_count: 0,
                    wakers: Vec::new(),
                }))
            });
            
            let mut write = entry.lock().unwrap();
            
            // Merge data
            if write.offset + write.data.len() as u64 == offset {
                write.data.extend_from_slice(&data);
            } else {
                // Non-contiguous write, flush existing and start new
                if !write.data.is_empty() {
                    // Flush existing data
                    drop(write);
                    coalesced_writes.remove(&path);
                    
                    // Start new coalesced write
                    coalesced_writes.insert(
                        path.clone(),
                        Arc::new(Mutex::new(CoalescedWrite {
                            path,
                            data,
                            offset,
                            pending_count: 1,
                            wakers: Vec::new(),
                        })),
                    );
                } else {
                    write.data = data;
                    write.offset = offset;
                }
            }
            
            write.pending_count += 1;
            
            // Auto-flush after threshold
            if write.data.len() > 1024 * 1024 || write.pending_count > 10 {
                // Flush logic here
                write.data.clear();
                write.pending_count = 0;
                
                // Wake all waiters
                for waker in write.wakers.drain(..) {
                    waker.wake();
                }
            }
            
            Ok(())
        })
    }
    
    /// Convert FSKit callback to future
    pub fn callback_to_future<T, F>(
        &self,
        callback: F,
    ) -> FSKitFuture<T>
    where
        T: Send + 'static,
        F: FnOnce(Box<dyn FnOnce(Result<T>) + Send>) + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        
        self.dispatch_queue.exec_async(move || {
            callback(Box::new(move |result| {
                let _ = tx.send(result);
            }));
        });
        
        FSKitFuture::new(async move {
            rx.await.unwrap_or_else(|_| {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Callback cancelled",
                ).into())
            })
        })
    }
    
    /// Dispatch to queue and await result
    pub fn dispatch_async<F, T>(&self, f: F) -> FSKitFuture<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        
        self.dispatch_queue.exec_async(move || {
            let result = f();
            let _ = tx.send(result);
        });
        
        FSKitFuture::new(async move {
            rx.await.map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "Dispatch cancelled").into()
            })
        })
    }
    
    /// Get current metrics
    pub fn metrics(&self) -> Arc<Metrics> {
        Arc::clone(&self.metrics)
    }
    
    /// Shutdown the runtime
    pub async fn shutdown(&self) {
        let _ = self.shutdown_tx.send(()).await;
    }
}

/// FSKit-compatible future wrapper
pub struct FSKitFuture<T> {
    inner: Pin<Box<dyn Future<Output = Result<T>> + Send>>,
    cancellation_token: Option<oneshot::Receiver<()>>,
    timeout_duration: Option<Duration>,
}

impl<T: Send + 'static> FSKitFuture<T> {
    /// Create a new FSKit future
    pub fn new<F>(future: F) -> Self
    where
        F: Future<Output = Result<T>> + Send + 'static,
    {
        Self {
            inner: Box::pin(future),
            cancellation_token: None,
            timeout_duration: None,
        }
    }
    
    /// Add cancellation support
    pub fn with_cancellation(mut self, token: oneshot::Receiver<()>) -> Self {
        self.cancellation_token = Some(token);
        self
    }
    
    /// Add timeout
    pub fn with_timeout(mut self, duration: Duration) -> Self {
        self.timeout_duration = Some(duration);
        self
    }
    
    /// Execute the future with all modifiers
    pub async fn execute(self) -> Result<T> {
        let future = self.inner;
        
        if let Some(duration) = self.timeout_duration {
            match timeout(duration, future).await {
                Ok(result) => result,
                Err(_) => Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Operation timed out",
                ).into()),
            }
        } else if let Some(mut cancel) = self.cancellation_token {
            tokio::select! {
                result = future => result,
                _ = &mut cancel => Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "Operation cancelled",
                ).into()),
            }
        } else {
            future.await
        }
    }
}

impl<T> Future for FSKitFuture<T> {
    type Output = Result<T>;
    
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(ref mut cancel) = self.cancellation_token {
            match Pin::new(cancel).poll(cx) {
                Poll::Ready(_) => {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "Operation cancelled",
                    ).into()));
                }
                Poll::Pending => {}
            }
        }
        
        self.inner.as_mut().poll(cx)
    }
}

/// Performance metrics collector
pub struct Metrics {
    operation_latencies: DashMap<OperationType, Vec<Duration>>,
    queue_depths: Arc<Mutex<Vec<usize>>>,
    throughput: Arc<std::sync::atomic::AtomicU64>,
    error_count: Arc<std::sync::atomic::AtomicU64>,
}

impl Metrics {
    fn new() -> Self {
        Self {
            operation_latencies: DashMap::new(),
            queue_depths: Arc::new(Mutex::new(Vec::new())),
            throughput: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            error_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }
    
    fn record_operation_latency(&self, op_type: OperationType, latency: Duration) {
        self.operation_latencies
            .entry(op_type)
            .or_insert_with(Vec::new)
            .push(latency);
        
        self.throughput.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    
    fn record_queue_depth(&self, depth: usize) {
        self.queue_depths.lock().unwrap().push(depth);
    }
    
    /// Get average latency for operation type
    pub fn average_latency(&self, op_type: OperationType) -> Option<Duration> {
        self.operation_latencies.get(&op_type).and_then(|latencies| {
            if latencies.is_empty() {
                None
            } else {
                let sum: Duration = latencies.iter().sum();
                Some(sum / latencies.len() as u32)
            }
        })
    }
    
    /// Get current queue depth
    pub fn current_queue_depth(&self) -> usize {
        self.queue_depths
            .lock()
            .unwrap()
            .last()
            .copied()
            .unwrap_or(0)
    }
    
    /// Get total throughput
    pub fn total_throughput(&self) -> u64 {
        self.throughput.load(std::sync::atomic::Ordering::Relaxed)
    }
    
    /// Get error count
    pub fn error_count(&self) -> u64 {
        self.error_count.load(std::sync::atomic::Ordering::Relaxed)
    }
    
    /// Increment error count
    pub fn record_error(&self) {
        self.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    
    /// Get percentile latency
    pub fn percentile_latency(&self, op_type: OperationType, percentile: f64) -> Option<Duration> {
        self.operation_latencies.get(&op_type).and_then(|latencies| {
            if latencies.is_empty() {
                None
            } else {
                let mut sorted = latencies.clone();
                sorted.sort();
                let index = ((percentile / 100.0) * sorted.len() as f64) as usize;
                sorted.get(index.min(sorted.len() - 1)).copied()
            }
        })
    }
    
    /// Clear all metrics
    pub fn clear(&self) {
        self.operation_latencies.clear();
        self.queue_depths.lock().unwrap().clear();
        self.throughput.store(0, std::sync::atomic::Ordering::Relaxed);
        self.error_count.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Async callback wrapper
pub struct AsyncCallback<T> {
    waker: Option<Waker>,
    result: Option<Result<T>>,
}

impl<T> AsyncCallback<T> {
    /// Create a new async callback
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            waker: None,
            result: None,
        }))
    }
    
    /// Set the result and wake the future
    pub fn complete(callback: &Arc<Mutex<Self>>, result: Result<T>) {
        let mut cb = callback.lock().unwrap();
        cb.result = Some(result);
        if let Some(waker) = cb.waker.take() {
            waker.wake();
        }
    }
    
    /// Create a future that waits for completion
    pub fn as_future(callback: Arc<Mutex<Self>>) -> impl Future<Output = Result<T>> {
        AsyncCallbackFuture { callback }
    }
}

struct AsyncCallbackFuture<T> {
    callback: Arc<Mutex<AsyncCallback<T>>>,
}

impl<T> Future for AsyncCallbackFuture<T> {
    type Output = Result<T>;
    
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut cb = self.callback.lock().unwrap();
        
        if let Some(result) = cb.result.take() {
            Poll::Ready(result)
        } else {
            cb.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

/// Operation context for propagation
#[derive(Debug, Clone)]
pub struct OperationContext {
    pub request_id: String,
    pub user_id: Option<String>,
    pub trace_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub deadline: Option<Instant>,
    pub metadata: Arc<DashMap<String, String>>,
}

impl OperationContext {
    /// Create a new operation context
    pub fn new(request_id: String) -> Self {
        Self {
            request_id,
            user_id: None,
            trace_id: None,
            parent_span_id: None,
            deadline: None,
            metadata: Arc::new(DashMap::new()),
        }
    }
    
    /// Set deadline for the operation
    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }
    
    /// Check if deadline has passed
    pub fn is_expired(&self) -> bool {
        self.deadline.map_or(false, |d| Instant::now() > d)
    }
    
    /// Add metadata
    pub fn add_metadata(&self, key: String, value: String) {
        self.metadata.insert(key, value);
    }
    
    /// Get metadata
    pub fn get_metadata(&self, key: &str) -> Option<String> {
        self.metadata.get(key).map(|v| v.clone())
    }
}

/// Thread pool manager for FSKit operations
pub struct ThreadManager {
    read_pool: Arc<rayon::ThreadPool>,
    write_pool: Arc<rayon::ThreadPool>,
    metadata_pool: Arc<rayon::ThreadPool>,
}

impl ThreadManager {
    /// Create a new thread manager
    pub fn new() -> Result<Self> {
        let read_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .thread_name(|i| format!("fskit-read-{}", i))
            .build()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        
        let write_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .thread_name(|i| format!("fskit-write-{}", i))
            .build()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        
        let metadata_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .thread_name(|i| format!("fskit-meta-{}", i))
            .build()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        
        Ok(Self {
            read_pool: Arc::new(read_pool),
            write_pool: Arc::new(write_pool),
            metadata_pool: Arc::new(metadata_pool),
        })
    }
    
    /// Execute operation on appropriate thread pool
    pub fn execute<F, T>(&self, op_type: OperationType, f: F) -> T
    where
        F: FnOnce() -> T + Send,
        T: Send,
    {
        match op_type {
            OperationType::Read | OperationType::Attribute => {
                self.read_pool.install(f)
            }
            OperationType::Write | OperationType::Sync => {
                self.write_pool.install(f)
            }
            OperationType::Metadata | OperationType::Directory => {
                self.metadata_pool.install(f)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_priority_scheduling() {
        let runtime = AsyncRuntime::new(10).unwrap();
        
        let low_future = runtime.queue_operation(
            OperationType::Sync,
            Some(OperationPriority::Low),
            || println!("Low priority"),
        );
        
        let high_future = runtime.queue_operation(
            OperationType::Read,
            Some(OperationPriority::High),
            || println!("High priority"),
        );
        
        // High priority should complete first
        high_future.execute().await.unwrap();
        low_future.execute().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_write_coalescing() {
        let runtime = AsyncRuntime::new(10).unwrap();
        
        let write1 = runtime.coalesce_write(
            "/test/file".to_string(),
            vec![1, 2, 3],
            0,
        );
        
        let write2 = runtime.coalesce_write(
            "/test/file".to_string(),
            vec![4, 5, 6],
            3,
        );
        
        write1.execute().await.unwrap();
        write2.execute().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_future_timeout() {
        let future = FSKitFuture::new(async {
            sleep(Duration::from_secs(2)).await;
            Ok(42)
        })
        .with_timeout(Duration::from_millis(100));
        
        let result = future.execute().await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_future_cancellation() {
        let (cancel_tx, cancel_rx) = oneshot::channel();
        
        let future = FSKitFuture::new(async {
            sleep(Duration::from_secs(2)).await;
            Ok(42)
        })
        .with_cancellation(cancel_rx);
        
        cancel_tx.send(()).unwrap();
        
        let result = future.execute().await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_async_callback() {
        let callback = AsyncCallback::<i32>::new();
        let future_callback = Arc::clone(&callback);
        
        tokio::spawn(async move {
            sleep(Duration::from_millis(100)).await;
            AsyncCallback::complete(&callback, Ok(42));
        });
        
        let result = AsyncCallback::as_future(future_callback).await;
        assert_eq!(result.unwrap(), 42);
    }
    
    #[tokio::test]
    async fn test_metrics_collection() {
        let runtime = AsyncRuntime::new(10).unwrap();
        
        for _ in 0..10 {
            runtime.queue_operation(
                OperationType::Read,
                None,
                || std::thread::sleep(Duration::from_millis(10)),
            ).execute().await.unwrap();
        }
        
        let metrics = runtime.metrics();
        assert!(metrics.total_throughput() > 0);
        assert!(metrics.average_latency(OperationType::Read).is_some());
    }
    
    #[tokio::test]
    async fn test_operation_context() {
        let context = OperationContext::new("req-123".to_string())
            .with_deadline(Instant::now() + Duration::from_secs(5));
        
        context.add_metadata("user".to_string(), "alice".to_string());
        
        assert_eq!(context.request_id, "req-123");
        assert!(!context.is_expired());
        assert_eq!(context.get_metadata("user"), Some("alice".to_string()));
    }
}