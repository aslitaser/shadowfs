use std::sync::{Arc, Mutex};
use std::thread;
use std::collections::BinaryHeap;
use std::cmp::{Ordering, Reverse};
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering as AtomicOrdering};
use tokio::sync::{mpsc, oneshot, Semaphore, RwLock};
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn, trace};
use windows::core::Result;
use windows::Win32::Storage::ProjectedFileSystem::*;

use crate::error::WindowsError;

const DEFAULT_WORKER_THREADS: usize = 4;
const DEFAULT_QUEUE_SIZE: usize = 1000;
const DEFAULT_MAX_CONCURRENT_OPS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskPriority {
    Critical = 0,  // Highest priority - read operations
    High = 1,      // Directory enumeration
    Normal = 2,    // Metadata operations
    Low = 3,       // Notifications
}

impl TaskPriority {
    fn from_request(request: &CallbackRequest) -> Self {
        match request {
            CallbackRequest::GetFileData { .. } => TaskPriority::Critical,
            CallbackRequest::GetPlaceholderInfo { .. } => TaskPriority::High,
            CallbackRequest::StartDirectoryEnumeration { .. } |
            CallbackRequest::GetDirectoryEnumeration { .. } |
            CallbackRequest::EndDirectoryEnumeration { .. } => TaskPriority::High,
            CallbackRequest::QueryFileName { .. } => TaskPriority::Normal,
            CallbackRequest::Notification { .. } => TaskPriority::Low,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CallbackRequest {
    GetPlaceholderInfo {
        callback_data: PRJ_CALLBACK_DATA,
        response: oneshot::Sender<Result<()>>,
    },
    GetFileData {
        callback_data: PRJ_CALLBACK_DATA,
        byte_offset: u64,
        length: u32,
        response: oneshot::Sender<Result<()>>,
    },
    QueryFileName {
        callback_data: PRJ_CALLBACK_DATA,
        file_path_name: String,
        response: oneshot::Sender<Result<()>>,
    },
    StartDirectoryEnumeration {
        callback_data: PRJ_CALLBACK_DATA,
        dir_id: PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT,
        file_path_name: String,
        search_expression: Option<String>,
        response: oneshot::Sender<Result<()>>,
    },
    EndDirectoryEnumeration {
        callback_data: PRJ_CALLBACK_DATA,
        dir_id: PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT,
        response: oneshot::Sender<Result<()>>,
    },
    GetDirectoryEnumeration {
        callback_data: PRJ_CALLBACK_DATA,
        dir_id: PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT,
        search_expression: Option<String>,
        restart_scan: bool,
        response: oneshot::Sender<Result<()>>,
    },
    Notification {
        callback_data: PRJ_CALLBACK_DATA,
        notification_type: PRJ_NOTIFICATION,
        destination_file_name: Option<String>,
        operation_parameters: Option<PRJ_NOTIFICATION_PARAMETERS>,
        response: oneshot::Sender<Result<()>>,
    },
}

#[derive(Debug)]
struct PrioritizedTask {
    request: CallbackRequest,
    priority: TaskPriority,
    sequence: u64,
    cancellation_token: CancellationToken,
    submitted_at: std::time::Instant,
}

impl PartialEq for PrioritizedTask {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence == other.sequence
    }
}

impl Eq for PrioritizedTask {}

impl PartialOrd for PrioritizedTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrioritizedTask {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.priority.cmp(&other.priority) {
            Ordering::Equal => self.sequence.cmp(&other.sequence),
            other => other,
        }
    }
}

struct PriorityQueue {
    heap: Arc<RwLock<BinaryHeap<Reverse<PrioritizedTask>>>>,
    sequence_counter: AtomicU64,
    active_tasks: Arc<RwLock<Vec<(u64, CancellationToken)>>>,
}

impl PriorityQueue {
    fn new() -> Self {
        Self {
            heap: Arc::new(RwLock::new(BinaryHeap::new())),
            sequence_counter: AtomicU64::new(0),
            active_tasks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    async fn push(&self, request: CallbackRequest) -> CancellationToken {
        let sequence = self.sequence_counter.fetch_add(1, AtomicOrdering::SeqCst);
        let priority = TaskPriority::from_request(&request);
        let cancellation_token = CancellationToken::new();
        
        let task = PrioritizedTask {
            request,
            priority,
            sequence,
            cancellation_token: cancellation_token.clone(),
            submitted_at: std::time::Instant::now(),
        };

        let mut heap = self.heap.write().await;
        heap.push(Reverse(task));
        
        trace!("Added task with priority {:?}, sequence {}", priority, sequence);
        cancellation_token
    }

    async fn pop(&self) -> Option<PrioritizedTask> {
        let mut heap = self.heap.write().await;
        
        while let Some(Reverse(task)) = heap.pop() {
            if !task.cancellation_token.is_cancelled() {
                let mut active = self.active_tasks.write().await;
                active.push((task.sequence, task.cancellation_token.clone()));
                return Some(task);
            }
            trace!("Skipping cancelled task with sequence {}", task.sequence);
        }
        None
    }

    async fn cancel_task(&self, sequence: u64) -> bool {
        let active = self.active_tasks.read().await;
        for (seq, token) in active.iter() {
            if *seq == sequence {
                token.cancel();
                return true;
            }
        }
        false
    }

    async fn remove_active(&self, sequence: u64) {
        let mut active = self.active_tasks.write().await;
        active.retain(|(seq, _)| *seq != sequence);
    }

    async fn len(&self) -> usize {
        self.heap.read().await.len()
    }

    async fn clear_cancelled(&self) {
        let mut heap = self.heap.write().await;
        let valid_tasks: Vec<_> = heap
            .drain()
            .filter(|Reverse(task)| !task.cancellation_token.is_cancelled())
            .collect();
        
        for task in valid_tasks {
            heap.push(task);
        }
    }
}

pub struct AsyncBridge {
    priority_queue: Arc<PriorityQueue>,
    worker_handles: Vec<thread::JoinHandle<()>>,
    runtime_handle: Handle,
    semaphore: Arc<Semaphore>,
    metrics: Arc<Mutex<BridgeMetrics>>,
    shutdown_token: CancellationToken,
    is_running: Arc<AtomicBool>,
}

#[derive(Debug, Default, Clone)]
struct BridgeMetrics {
    total_requests: u64,
    completed_requests: u64,
    failed_requests: u64,
    dropped_requests: u64,
    cancelled_requests: u64,
    current_queue_size: usize,
    peak_queue_size: usize,
    priority_stats: [u64; 4],  // Stats per priority level
}

impl AsyncBridge {
    pub fn new(runtime_handle: Handle) -> Result<Self> {
        Self::with_config(
            runtime_handle,
            DEFAULT_WORKER_THREADS,
            DEFAULT_QUEUE_SIZE,
            DEFAULT_MAX_CONCURRENT_OPS,
        )
    }

    pub fn with_config(
        runtime_handle: Handle,
        worker_threads: usize,
        _queue_size: usize,
        max_concurrent_ops: usize,
    ) -> Result<Self> {
        let priority_queue = Arc::new(PriorityQueue::new());
        let semaphore = Arc::new(Semaphore::new(max_concurrent_ops));
        let metrics = Arc::new(Mutex::new(BridgeMetrics::default()));
        let shutdown_token = CancellationToken::new();
        let is_running = Arc::new(AtomicBool::new(true));

        let mut worker_handles = Vec::with_capacity(worker_threads);

        for i in 0..worker_threads {
            let handle = runtime_handle.clone();
            let sem = semaphore.clone();
            let metrics = metrics.clone();
            let queue = priority_queue.clone();
            let shutdown = shutdown_token.clone();
            let running = is_running.clone();
            
            let worker_handle = thread::Builder::new()
                .name(format!("shadowfs-async-bridge-worker-{}", i))
                .spawn(move || {
                    handle.block_on(async {
                        while running.load(AtomicOrdering::Relaxed) {
                            tokio::select! {
                                _ = shutdown.cancelled() => {
                                    debug!("Worker thread {} shutting down", i);
                                    break;
                                }
                                _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                                    if let Some(task) = queue.pop().await {
                                        if task.cancellation_token.is_cancelled() {
                                            let mut m = metrics.lock().unwrap();
                                            m.cancelled_requests += 1;
                                            queue.remove_active(task.sequence).await;
                                            continue;
                                        }

                                        let permit = match sem.try_acquire() {
                                            Ok(permit) => permit,
                                            Err(_) => {
                                                warn!("Backpressure limit reached, waiting for permit");
                                                match sem.acquire().await {
                                                    Ok(permit) => permit,
                                                    Err(e) => {
                                                        error!("Failed to acquire semaphore permit: {}", e);
                                                        Self::handle_error_response(&task.request);
                                                        queue.remove_active(task.sequence).await;
                                                        continue;
                                                    }
                                                }
                                            }
                                        };

                                        let elapsed = task.submitted_at.elapsed();
                                        if elapsed > std::time::Duration::from_secs(5) {
                                            warn!("Task waited {} ms before processing (priority: {:?})", 
                                                  elapsed.as_millis(), task.priority);
                                        }

                                        {
                                            let mut m = metrics.lock().unwrap();
                                            m.priority_stats[task.priority as usize] += 1;
                                        }

                                        Self::process_request(task.request, &metrics).await;
                                        queue.remove_active(task.sequence).await;
                                        drop(permit);
                                    }
                                }
                            }
                        }
                    });
                })
                .map_err(|e| WindowsError::ThreadCreation(e.to_string()))?;

            worker_handles.push(worker_handle);
        }

        let cleanup_queue = priority_queue.clone();
        let cleanup_shutdown = shutdown_token.clone();
        let cleanup_handle = runtime_handle.clone();
        
        thread::spawn(move || {
            cleanup_handle.block_on(async {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
                loop {
                    tokio::select! {
                        _ = cleanup_shutdown.cancelled() => break,
                        _ = interval.tick() => {
                            cleanup_queue.clear_cancelled().await;
                        }
                    }
                }
            });
        });

        Ok(Self {
            priority_queue,
            worker_handles,
            runtime_handle,
            semaphore,
            metrics,
            shutdown_token,
            is_running,
        })
    }

    async fn process_request(request: CallbackRequest, metrics: &Arc<Mutex<BridgeMetrics>>) {
        debug!("Processing async request: {:?}", std::mem::discriminant(&request));
        
        let result = match request {
            CallbackRequest::GetPlaceholderInfo { callback_data, response } => {
                let result = Self::handle_get_placeholder_info(callback_data).await;
                let _ = response.send(result);
            }
            CallbackRequest::GetFileData { callback_data, byte_offset, length, response } => {
                let result = Self::handle_get_file_data(callback_data, byte_offset, length).await;
                let _ = response.send(result);
            }
            CallbackRequest::QueryFileName { callback_data, file_path_name, response } => {
                let result = Self::handle_query_file_name(callback_data, file_path_name).await;
                let _ = response.send(result);
            }
            CallbackRequest::StartDirectoryEnumeration { 
                callback_data, 
                dir_id, 
                file_path_name, 
                search_expression, 
                response 
            } => {
                let result = Self::handle_start_directory_enumeration(
                    callback_data,
                    dir_id,
                    file_path_name,
                    search_expression
                ).await;
                let _ = response.send(result);
            }
            CallbackRequest::EndDirectoryEnumeration { callback_data, dir_id, response } => {
                let result = Self::handle_end_directory_enumeration(callback_data, dir_id).await;
                let _ = response.send(result);
            }
            CallbackRequest::GetDirectoryEnumeration {
                callback_data,
                dir_id,
                search_expression,
                restart_scan,
                response
            } => {
                let result = Self::handle_get_directory_enumeration(
                    callback_data,
                    dir_id,
                    search_expression,
                    restart_scan
                ).await;
                let _ = response.send(result);
            }
            CallbackRequest::Notification {
                callback_data,
                notification_type,
                destination_file_name,
                operation_parameters,
                response
            } => {
                let result = Self::handle_notification(
                    callback_data,
                    notification_type,
                    destination_file_name,
                    operation_parameters
                ).await;
                let _ = response.send(result);
            }
        };

        let mut m = metrics.lock().unwrap();
        m.completed_requests += 1;
        m.current_queue_size = m.current_queue_size.saturating_sub(1);
    }

    fn handle_error_response(request: &CallbackRequest) {
        let error = Err(WindowsError::AsyncProcessing("Backpressure limit exceeded".into()).into());
        
        match request {
            CallbackRequest::GetPlaceholderInfo { response, .. } |
            CallbackRequest::GetFileData { response, .. } |
            CallbackRequest::QueryFileName { response, .. } |
            CallbackRequest::StartDirectoryEnumeration { response, .. } |
            CallbackRequest::EndDirectoryEnumeration { response, .. } |
            CallbackRequest::GetDirectoryEnumeration { response, .. } |
            CallbackRequest::Notification { response, .. } => {
                let _ = response.send(error);
            }
        }
    }

    pub fn send_callback(&self, request: CallbackRequest) -> Result<()> {
        self.sender.try_send(request)
            .map_err(|e| {
                if e.is_full() {
                    warn!("AsyncBridge queue full, applying backpressure");
                    WindowsError::QueueFull(DEFAULT_QUEUE_SIZE).into()
                } else {
                    error!("AsyncBridge channel closed");
                    WindowsError::ChannelClosed.into()
                }
            })
    }

    pub async fn send_callback_async(&self, request: CallbackRequest) -> Result<()> {
        self.sender.send(request).await
            .map_err(|_| {
                error!("AsyncBridge channel closed");
                WindowsError::ChannelClosed.into()
            })
    }

    pub fn get_metrics(&self) -> BridgeMetrics {
        self.metrics.lock().unwrap().clone()
    }

    pub fn shutdown(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            for _ in 0..self.worker_handles.len() {
                let _ = shutdown_tx.try_send(());
            }
        }

        for handle in self.worker_handles.drain(..) {
            let _ = handle.join();
        }
    }

    async fn handle_get_placeholder_info(callback_data: PRJ_CALLBACK_DATA) -> Result<()> {
        Ok(())
    }

    async fn handle_get_file_data(
        callback_data: PRJ_CALLBACK_DATA,
        byte_offset: u64,
        length: u32,
    ) -> Result<()> {
        Ok(())
    }

    async fn handle_query_file_name(
        callback_data: PRJ_CALLBACK_DATA,
        file_path_name: String,
    ) -> Result<()> {
        Ok(())
    }

    async fn handle_start_directory_enumeration(
        callback_data: PRJ_CALLBACK_DATA,
        dir_id: PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT,
        file_path_name: String,
        search_expression: Option<String>,
    ) -> Result<()> {
        Ok(())
    }

    async fn handle_end_directory_enumeration(
        callback_data: PRJ_CALLBACK_DATA,
        dir_id: PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT,
    ) -> Result<()> {
        Ok(())
    }

    async fn handle_get_directory_enumeration(
        callback_data: PRJ_CALLBACK_DATA,
        dir_id: PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT,
        search_expression: Option<String>,
        restart_scan: bool,
    ) -> Result<()> {
        Ok(())
    }

    async fn handle_notification(
        callback_data: PRJ_CALLBACK_DATA,
        notification_type: PRJ_NOTIFICATION,
        destination_file_name: Option<String>,
        operation_parameters: Option<PRJ_NOTIFICATION_PARAMETERS>,
    ) -> Result<()> {
        Ok(())
    }
}

impl Drop for AsyncBridge {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_async_bridge_creation() {
        let runtime = Runtime::new().unwrap();
        let bridge = AsyncBridge::new(runtime.handle().clone());
        assert!(bridge.is_ok());
    }

    #[test]
    fn test_metrics_tracking() {
        let runtime = Runtime::new().unwrap();
        let bridge = AsyncBridge::new(runtime.handle().clone()).unwrap();
        
        let metrics = bridge.get_metrics();
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.completed_requests, 0);
        assert_eq!(metrics.failed_requests, 0);
    }
}