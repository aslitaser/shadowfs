use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::{timeout, Instant};
use tokio_util::sync::CancellationToken;
use windows::core::Result;
use windows::Win32::Storage::ProjectedFileSystem::*;
use tracing::{debug, trace, warn, error};

use crate::error::WindowsError;
use super::async_bridge::{AsyncBridge, CallbackRequest};

// Default timeout values (in milliseconds)
const DEFAULT_READ_TIMEOUT_MS: u64 = 30000;      // 30 seconds for reads
const DEFAULT_ENUM_TIMEOUT_MS: u64 = 10000;      // 10 seconds for directory enumeration
const DEFAULT_METADATA_TIMEOUT_MS: u64 = 5000;   // 5 seconds for metadata
const DEFAULT_NOTIFICATION_TIMEOUT_MS: u64 = 2000; // 2 seconds for notifications

// Critical operation timeouts (shorter for better UX)
const CRITICAL_READ_TIMEOUT_MS: u64 = 5000;      // 5 seconds for critical reads
const CRITICAL_METADATA_TIMEOUT_MS: u64 = 1000;  // 1 second for critical metadata

#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    pub read_timeout: Duration,
    pub enum_timeout: Duration,
    pub metadata_timeout: Duration,
    pub notification_timeout: Duration,
    pub critical_read_timeout: Duration,
    pub critical_metadata_timeout: Duration,
    pub enable_graceful_degradation: bool,
    pub retry_on_timeout: bool,
    pub max_retries: u32,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            read_timeout: Duration::from_millis(DEFAULT_READ_TIMEOUT_MS),
            enum_timeout: Duration::from_millis(DEFAULT_ENUM_TIMEOUT_MS),
            metadata_timeout: Duration::from_millis(DEFAULT_METADATA_TIMEOUT_MS),
            notification_timeout: Duration::from_millis(DEFAULT_NOTIFICATION_TIMEOUT_MS),
            critical_read_timeout: Duration::from_millis(CRITICAL_READ_TIMEOUT_MS),
            critical_metadata_timeout: Duration::from_millis(CRITICAL_METADATA_TIMEOUT_MS),
            enable_graceful_degradation: true,
            retry_on_timeout: true,
            max_retries: 2,
        }
    }
}

impl TimeoutConfig {
    pub fn aggressive() -> Self {
        Self {
            read_timeout: Duration::from_millis(5000),
            enum_timeout: Duration::from_millis(3000),
            metadata_timeout: Duration::from_millis(1000),
            notification_timeout: Duration::from_millis(500),
            critical_read_timeout: Duration::from_millis(2000),
            critical_metadata_timeout: Duration::from_millis(500),
            enable_graceful_degradation: true,
            retry_on_timeout: false,
            max_retries: 0,
        }
    }

    pub fn relaxed() -> Self {
        Self {
            read_timeout: Duration::from_millis(60000),
            enum_timeout: Duration::from_millis(30000),
            metadata_timeout: Duration::from_millis(15000),
            notification_timeout: Duration::from_millis(10000),
            critical_read_timeout: Duration::from_millis(15000),
            critical_metadata_timeout: Duration::from_millis(5000),
            enable_graceful_degradation: false,
            retry_on_timeout: true,
            max_retries: 3,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct TimeoutMetrics {
    pub total_operations: u64,
    pub timed_out_operations: u64,
    pub retried_operations: u64,
    pub degraded_operations: u64,
    pub average_response_time_ms: f64,
    pub max_response_time_ms: u64,
}

pub struct TimeoutManager {
    config: TimeoutConfig,
    metrics: Arc<tokio::sync::RwLock<TimeoutMetrics>>,
    operation_times: Arc<tokio::sync::RwLock<Vec<u64>>>,
}

impl TimeoutManager {
    pub fn new(config: TimeoutConfig) -> Self {
        Self {
            config,
            metrics: Arc::new(tokio::sync::RwLock::new(TimeoutMetrics::default())),
            operation_times: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }
    
    pub fn with_defaults() -> Self {
        Self::new(TimeoutConfig::default())
    }
    
    pub async fn record_operation(&self, duration_ms: u64, timed_out: bool, retried: bool, degraded: bool) {
        let mut metrics = self.metrics.write().await;
        let mut times = self.operation_times.write().await;
        
        metrics.total_operations += 1;
        if timed_out {
            metrics.timed_out_operations += 1;
        }
        if retried {
            metrics.retried_operations += 1;
        }
        if degraded {
            metrics.degraded_operations += 1;
        }
        
        times.push(duration_ms);
        if times.len() > 1000 {
            times.remove(0); // Keep only last 1000 operations
        }
        
        metrics.average_response_time_ms = times.iter().sum::<u64>() as f64 / times.len() as f64;
        metrics.max_response_time_ms = *times.iter().max().unwrap_or(&0);
    }
    
    pub async fn get_metrics(&self) -> TimeoutMetrics {
        self.metrics.read().await.clone()
    }
    
    pub fn config(&self) -> &TimeoutConfig {
        &self.config
    }
    
    pub async fn adjust_timeouts_based_on_metrics(&mut self) {
        let metrics = self.get_metrics().await;
        
        // Auto-adjust timeouts based on failure rate
        let failure_rate = if metrics.total_operations > 0 {
            metrics.timed_out_operations as f64 / metrics.total_operations as f64
        } else {
            0.0
        };
        
        if failure_rate > 0.2 {
            // More than 20% timeouts - relax timeouts
            warn!("High timeout rate ({:.1}%), relaxing timeouts", failure_rate * 100.0);
            self.config.read_timeout = self.config.read_timeout * 3 / 2;
            self.config.enum_timeout = self.config.enum_timeout * 3 / 2;
            self.config.metadata_timeout = self.config.metadata_timeout * 3 / 2;
        } else if failure_rate < 0.05 && metrics.average_response_time_ms < 1000.0 {
            // Less than 5% timeouts and fast responses - tighten timeouts
            debug!("Low timeout rate ({:.1}%), tightening timeouts", failure_rate * 100.0);
            self.config.read_timeout = self.config.read_timeout * 2 / 3;
            self.config.enum_timeout = self.config.enum_timeout * 2 / 3;
            self.config.metadata_timeout = self.config.metadata_timeout * 2 / 3;
        }
    }
    
    pub async fn report_health(&self) -> HealthStatus {
        let metrics = self.get_metrics().await;
        
        let failure_rate = if metrics.total_operations > 0 {
            metrics.timed_out_operations as f64 / metrics.total_operations as f64
        } else {
            0.0
        };
        
        if failure_rate > 0.5 {
            HealthStatus::Critical
        } else if failure_rate > 0.2 {
            HealthStatus::Degraded
        } else if failure_rate > 0.1 {
            HealthStatus::Warning
        } else {
            HealthStatus::Healthy
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Warning,
    Degraded,
    Critical,
}

#[derive(Debug)]
struct TimeoutWrapper<F> {
    future: F,
    timeout_duration: Duration,
    started_at: Instant,
    operation_name: String,
    attempt: u32,
    max_attempts: u32,
}

impl<F> TimeoutWrapper<F> {
    fn new(future: F, timeout_duration: Duration, operation_name: String, max_attempts: u32) -> Self {
        Self {
            future,
            timeout_duration,
            started_at: Instant::now(),
            operation_name,
            attempt: 1,
            max_attempts,
        }
    }
}

async fn with_timeout<T>(
    future: impl Future<Output = Result<T>>,
    duration: Duration,
    operation_name: &str,
) -> Result<T> {
    let start = Instant::now();
    
    match timeout(duration, future).await {
        Ok(result) => {
            let elapsed = start.elapsed();
            if elapsed > duration * 80 / 100 {
                warn!(
                    "{} took {:.2}s (approaching timeout of {:.2}s)",
                    operation_name,
                    elapsed.as_secs_f64(),
                    duration.as_secs_f64()
                );
            } else {
                trace!(
                    "{} completed in {:.2}s",
                    operation_name,
                    elapsed.as_secs_f64()
                );
            }
            result
        }
        Err(_) => {
            error!(
                "{} timed out after {:.2}s",
                operation_name,
                duration.as_secs_f64()
            );
            Err(WindowsError::AsyncProcessing(
                format!("{} timed out after {:?}", operation_name, duration)
            ).into())
        }
    }
}

async fn with_retry<T>(
    mut op: impl FnMut() -> impl Future<Output = Result<T>>,
    duration: Duration,
    operation_name: &str,
    max_retries: u32,
) -> Result<T> {
    let mut attempt = 0;
    let mut last_error = None;
    
    while attempt <= max_retries {
        if attempt > 0 {
            debug!("Retrying {} (attempt {}/{})", operation_name, attempt + 1, max_retries + 1);
            tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
        }
        
        match with_timeout(op(), duration, operation_name).await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
                attempt += 1;
            }
        }
    }
    
    error!(
        "{} failed after {} retries",
        operation_name,
        max_retries + 1
    );
    Err(last_error.unwrap())
}

#[derive(Debug)]
pub struct ReadFileFuture {
    receiver: oneshot::Receiver<Result<()>>,
    cancellation_token: CancellationToken,
    byte_offset: u64,
    length: u32,
    cancelled: bool,
    timeout_config: Option<TimeoutConfig>,
    is_critical: bool,
    started_at: Instant,
}

impl ReadFileFuture {
    pub fn new(
        bridge: &Arc<AsyncBridge>,
        callback_data: PRJ_CALLBACK_DATA,
        byte_offset: u64,
        length: u32,
    ) -> Result<Self> {
        Self::with_config(bridge, callback_data, byte_offset, length, None)
    }
    
    pub fn with_config(
        bridge: &Arc<AsyncBridge>,
        callback_data: PRJ_CALLBACK_DATA,
        byte_offset: u64,
        length: u32,
        timeout_config: Option<TimeoutConfig>,
    ) -> Result<Self> {
        let (tx, rx) = oneshot::channel();
        
        let request = CallbackRequest::GetFileData {
            callback_data,
            byte_offset,
            length,
            response: tx,
        };
        
        let cancellation_token = bridge.send_callback(request)?;
        
        Ok(Self {
            receiver: rx,
            cancellation_token,
            byte_offset,
            length,
            cancelled: false,
            timeout_config,
            is_critical: length <= 4096, // Small reads are considered critical
            started_at: Instant::now(),
        })
    }
    
    pub async fn read(self) -> Result<()> {
        let config = self.timeout_config.clone().unwrap_or_default();
        let timeout_duration = if self.is_critical {
            config.critical_read_timeout
        } else {
            config.read_timeout
        };
        
        let operation_name = format!("ReadFile(offset={}, len={})", self.byte_offset, self.length);
        
        if config.retry_on_timeout {
            let receiver = Arc::new(tokio::sync::Mutex::new(Some(self.receiver)));
            let receiver_clone = receiver.clone();
            
            with_retry(
                || async {
                    let mut guard = receiver_clone.lock().await;
                    if let Some(rx) = guard.take() {
                        match rx.await {
                            Ok(result) => result,
                            Err(_) => Err(WindowsError::ChannelClosed.into()),
                        }
                    } else {
                        Err(WindowsError::AsyncProcessing("Receiver already consumed".into()).into())
                    }
                },
                timeout_duration,
                &operation_name,
                config.max_retries,
            ).await
        } else {
            with_timeout(
                async {
                    match self.receiver.await {
                        Ok(result) => result,
                        Err(_) => Err(WindowsError::ChannelClosed.into()),
                    }
                },
                timeout_duration,
                &operation_name,
            ).await
        }
    }
    
    pub async fn read_with_degradation(self) -> Result<()> {
        let config = self.timeout_config.clone().unwrap_or_default();
        
        if !config.enable_graceful_degradation {
            return self.read().await;
        }
        
        // Try with normal timeout first
        match self.read().await {
            Ok(result) => Ok(result),
            Err(e) => {
                warn!("Read operation failed, attempting degraded mode: {:?}", e);
                
                // For degraded mode, we return a partial success
                // This allows the system to continue with cached or default data
                if self.is_critical {
                    // Critical reads must succeed
                    Err(e)
                } else {
                    // Non-critical reads can be skipped
                    warn!("Degrading read operation for offset {} length {}", 
                          self.byte_offset, self.length);
                    Ok(())
                }
            }
        }
    }
    
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.cancellation_token.cancel();
        self.receiver.close();
        
        let elapsed = self.started_at.elapsed();
        debug!("Read operation cancelled after {:.2}s", elapsed.as_secs_f64());
    }
    
    pub fn set_critical(&mut self, critical: bool) {
        self.is_critical = critical;
    }
    
    pub fn byte_offset(&self) -> u64 {
        self.byte_offset
    }
    
    pub fn length(&self) -> u32 {
        self.length
    }
}

impl Future for ReadFileFuture {
    type Output = Result<()>;
    
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.cancelled {
            return Poll::Ready(Err(WindowsError::AsyncProcessing("Operation cancelled".into()).into()));
        }
        
        match Pin::new(&mut self.receiver).poll(cx) {
            Poll::Ready(Ok(result)) => {
                trace!("ReadFileFuture completed for offset {} length {}", 
                       self.byte_offset, self.length);
                Poll::Ready(result)
            }
            Poll::Ready(Err(_)) => {
                debug!("ReadFileFuture channel closed");
                Poll::Ready(Err(WindowsError::ChannelClosed.into()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Debug)]
pub struct EnumerateDirectoryFuture {
    receiver: oneshot::Receiver<Result<()>>,
    cancellation_token: CancellationToken,
    dir_id: PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT,
    search_expression: Option<String>,
    restart_scan: bool,
    cancelled: bool,
    timeout_config: Option<TimeoutConfig>,
    started_at: Instant,
}

#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    pub file_name: String,
    pub is_directory: bool,
    pub file_size: u64,
    pub created_time: i64,
    pub last_write_time: i64,
    pub change_time: i64,
    pub attributes: u32,
}

impl EnumerateDirectoryFuture {
    pub fn new(
        bridge: &Arc<AsyncBridge>,
        callback_data: PRJ_CALLBACK_DATA,
        dir_id: PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT,
        search_expression: Option<String>,
        restart_scan: bool,
    ) -> Result<Self> {
        Self::with_config(bridge, callback_data, dir_id, search_expression, restart_scan, None)
    }
    
    pub fn with_config(
        bridge: &Arc<AsyncBridge>,
        callback_data: PRJ_CALLBACK_DATA,
        dir_id: PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT,
        search_expression: Option<String>,
        restart_scan: bool,
        timeout_config: Option<TimeoutConfig>,
    ) -> Result<Self> {
        let (tx, rx) = oneshot::channel();
        
        let request = CallbackRequest::GetDirectoryEnumeration {
            callback_data,
            dir_id,
            search_expression: search_expression.clone(),
            restart_scan,
            response: tx,
        };
        
        let cancellation_token = bridge.send_callback(request)?;
        
        Ok(Self {
            receiver: rx,
            cancellation_token,
            dir_id,
            search_expression,
            restart_scan,
            cancelled: false,
            timeout_config,
            started_at: Instant::now(),
        })
    }
    
    pub fn start_enumeration(
        bridge: &Arc<AsyncBridge>,
        callback_data: PRJ_CALLBACK_DATA,
        dir_id: PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT,
        file_path_name: String,
        search_expression: Option<String>,
    ) -> Result<CancellationToken> {
        let (tx, _rx) = oneshot::channel();
        
        let request = CallbackRequest::StartDirectoryEnumeration {
            callback_data,
            dir_id,
            file_path_name,
            search_expression,
            response: tx,
        };
        
        bridge.send_callback(request)
    }
    
    pub fn end_enumeration(
        bridge: &Arc<AsyncBridge>,
        callback_data: PRJ_CALLBACK_DATA,
        dir_id: PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT,
    ) -> Result<CancellationToken> {
        let (tx, _rx) = oneshot::channel();
        
        let request = CallbackRequest::EndDirectoryEnumeration {
            callback_data,
            dir_id,
            response: tx,
        };
        
        bridge.send_callback(request)
    }
    
    pub async fn enumerate(self) -> Result<()> {
        let config = self.timeout_config.clone().unwrap_or_default();
        let operation_name = format!("EnumerateDirectory(search={:?})", self.search_expression);
        
        with_timeout(
            async {
                match self.receiver.await {
                    Ok(result) => result,
                    Err(_) => Err(WindowsError::ChannelClosed.into()),
                }
            },
            config.enum_timeout,
            &operation_name,
        ).await
    }
    
    pub async fn enumerate_with_fallback(self) -> Result<()> {
        let config = self.timeout_config.clone().unwrap_or_default();
        
        match self.enumerate().await {
            Ok(result) => Ok(result),
            Err(e) => {
                if config.enable_graceful_degradation {
                    warn!("Directory enumeration failed, returning empty result: {:?}", e);
                    Ok(()) // Return empty directory listing on failure
                } else {
                    Err(e)
                }
            }
        }
    }
    
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.cancellation_token.cancel();
        self.receiver.close();
        
        let elapsed = self.started_at.elapsed();
        debug!("Directory enumeration cancelled after {:.2}s", elapsed.as_secs_f64());
    }
    
    pub fn search_expression(&self) -> Option<&String> {
        self.search_expression.as_ref()
    }
    
    pub fn is_restart_scan(&self) -> bool {
        self.restart_scan
    }
}

impl Future for EnumerateDirectoryFuture {
    type Output = Result<()>;
    
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.cancelled {
            return Poll::Ready(Err(WindowsError::AsyncProcessing("Operation cancelled".into()).into()));
        }
        
        match Pin::new(&mut self.receiver).poll(cx) {
            Poll::Ready(Ok(result)) => {
                trace!("EnumerateDirectoryFuture completed with search expression: {:?}", 
                       self.search_expression);
                Poll::Ready(result)
            }
            Poll::Ready(Err(_)) => {
                debug!("EnumerateDirectoryFuture channel closed");
                Poll::Ready(Err(WindowsError::ChannelClosed.into()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Debug)]
pub struct GetMetadataFuture {
    receiver: oneshot::Receiver<Result<()>>,
    cancellation_token: CancellationToken,
    file_path_name: String,
    cancelled: bool,
    timeout_config: Option<TimeoutConfig>,
    is_critical: bool,
    started_at: Instant,
}

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub file_name: String,
    pub is_directory: bool,
    pub is_placeholder: bool,
    pub file_size: u64,
    pub created_time: i64,
    pub last_access_time: i64,
    pub last_write_time: i64,
    pub change_time: i64,
    pub attributes: u32,
    pub reparse_tag: u32,
    pub version_info: Vec<u8>,
}

impl GetMetadataFuture {
    pub fn new(
        bridge: &Arc<AsyncBridge>,
        callback_data: PRJ_CALLBACK_DATA,
        file_path_name: String,
    ) -> Result<Self> {
        Self::with_config(bridge, callback_data, file_path_name, None)
    }
    
    pub fn with_config(
        bridge: &Arc<AsyncBridge>,
        callback_data: PRJ_CALLBACK_DATA,
        file_path_name: String,
        timeout_config: Option<TimeoutConfig>,
    ) -> Result<Self> {
        let (tx, rx) = oneshot::channel();
        
        let is_critical = file_path_name.ends_with(".exe") || 
                          file_path_name.ends_with(".dll") ||
                          file_path_name.contains("system");
        
        let request = CallbackRequest::GetPlaceholderInfo {
            callback_data,
            response: tx,
        };
        
        let cancellation_token = bridge.send_callback(request)?;
        
        Ok(Self {
            receiver: rx,
            cancellation_token,
            file_path_name,
            cancelled: false,
            timeout_config,
            is_critical,
            started_at: Instant::now(),
        })
    }
    
    pub fn query_file_name(
        bridge: &Arc<AsyncBridge>,
        callback_data: PRJ_CALLBACK_DATA,
        file_path_name: String,
    ) -> Result<Self> {
        Self::query_file_name_with_config(bridge, callback_data, file_path_name, None)
    }
    
    pub fn query_file_name_with_config(
        bridge: &Arc<AsyncBridge>,
        callback_data: PRJ_CALLBACK_DATA,
        file_path_name: String,
        timeout_config: Option<TimeoutConfig>,
    ) -> Result<Self> {
        let (tx, rx) = oneshot::channel();
        
        let is_critical = file_path_name.ends_with(".exe") || 
                          file_path_name.ends_with(".dll") ||
                          file_path_name.contains("system");
        
        let request = CallbackRequest::QueryFileName {
            callback_data,
            file_path_name: file_path_name.clone(),
            response: tx,
        };
        
        let cancellation_token = bridge.send_callback(request)?;
        
        Ok(Self {
            receiver: rx,
            cancellation_token,
            file_path_name,
            cancelled: false,
            timeout_config,
            is_critical,
            started_at: Instant::now(),
        })
    }
    
    pub async fn get(self) -> Result<()> {
        let config = self.timeout_config.clone().unwrap_or_default();
        let timeout_duration = if self.is_critical {
            config.critical_metadata_timeout
        } else {
            config.metadata_timeout
        };
        
        let operation_name = format!("GetMetadata({})", self.file_path_name);
        
        if config.retry_on_timeout && self.is_critical {
            let receiver = Arc::new(tokio::sync::Mutex::new(Some(self.receiver)));
            let receiver_clone = receiver.clone();
            
            with_retry(
                || async {
                    let mut guard = receiver_clone.lock().await;
                    if let Some(rx) = guard.take() {
                        match rx.await {
                            Ok(result) => result,
                            Err(_) => Err(WindowsError::ChannelClosed.into()),
                        }
                    } else {
                        Err(WindowsError::AsyncProcessing("Receiver already consumed".into()).into())
                    }
                },
                timeout_duration,
                &operation_name,
                config.max_retries,
            ).await
        } else {
            with_timeout(
                async {
                    match self.receiver.await {
                        Ok(result) => result,
                        Err(_) => Err(WindowsError::ChannelClosed.into()),
                    }
                },
                timeout_duration,
                &operation_name,
            ).await
        }
    }
    
    pub async fn get_with_fallback(self) -> Result<()> {
        let config = self.timeout_config.clone().unwrap_or_default();
        let file_path = self.file_path_name.clone();
        
        match self.get().await {
            Ok(result) => Ok(result),
            Err(e) => {
                if config.enable_graceful_degradation {
                    warn!("Metadata fetch failed for {}, using defaults: {:?}", file_path, e);
                    Ok(()) // Return default metadata on failure
                } else {
                    Err(e)
                }
            }
        }
    }
    
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.cancellation_token.cancel();
        self.receiver.close();
        
        let elapsed = self.started_at.elapsed();
        debug!("Metadata operation for {} cancelled after {:.2}s", 
               self.file_path_name, elapsed.as_secs_f64());
    }
    
    pub fn file_path(&self) -> &str {
        &self.file_path_name
    }
}

impl Future for GetMetadataFuture {
    type Output = Result<()>;
    
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.cancelled {
            return Poll::Ready(Err(WindowsError::AsyncProcessing("Operation cancelled".into()).into()));
        }
        
        match Pin::new(&mut self.receiver).poll(cx) {
            Poll::Ready(Ok(result)) => {
                trace!("GetMetadataFuture completed for path: {}", self.file_path_name);
                Poll::Ready(result)
            }
            Poll::Ready(Err(_)) => {
                debug!("GetMetadataFuture channel closed");
                Poll::Ready(Err(WindowsError::ChannelClosed.into()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct BatchReadFuture {
    futures: Vec<ReadFileFuture>,
}

impl BatchReadFuture {
    pub fn new(
        bridge: &Arc<AsyncBridge>,
        callback_data: PRJ_CALLBACK_DATA,
        reads: Vec<(u64, u32)>, // (offset, length) pairs
    ) -> Result<Self> {
        let mut futures = Vec::with_capacity(reads.len());
        
        for (offset, length) in reads {
            futures.push(ReadFileFuture::new(bridge, callback_data, offset, length)?);
        }
        
        Ok(Self { futures })
    }
    
    pub async fn read_all(self) -> Result<()> {
        use futures::future::join_all;
        
        let read_results = join_all(self.futures.into_iter().map(|f| f.read())).await;
        
        for result in read_results {
            result?;
        }
        
        Ok(())
    }
}

pub struct NotificationFuture {
    receiver: oneshot::Receiver<Result<()>>,
    notification_type: PRJ_NOTIFICATION,
    cancelled: bool,
}

impl NotificationFuture {
    pub fn new(
        bridge: &Arc<AsyncBridge>,
        callback_data: PRJ_CALLBACK_DATA,
        notification_type: PRJ_NOTIFICATION,
        destination_file_name: Option<String>,
        operation_parameters: Option<PRJ_NOTIFICATION_PARAMETERS>,
    ) -> Result<Self> {
        let (tx, rx) = oneshot::channel();
        
        let request = CallbackRequest::Notification {
            callback_data,
            notification_type,
            destination_file_name,
            operation_parameters,
            response: tx,
        };
        
        bridge.send_callback(request)?;
        
        Ok(Self {
            receiver: rx,
            notification_type,
            cancelled: false,
        })
    }
    
    pub async fn wait(self) -> Result<()> {
        match self.receiver.await {
            Ok(result) => result,
            Err(_) => Err(WindowsError::ChannelClosed.into()),
        }
    }
    
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.receiver.close();
    }
    
    pub fn notification_type(&self) -> PRJ_NOTIFICATION {
        self.notification_type
    }
}

impl Future for NotificationFuture {
    type Output = Result<()>;
    
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.cancelled {
            return Poll::Ready(Err(WindowsError::AsyncProcessing("Operation cancelled".into()).into()));
        }
        
        match Pin::new(&mut self.receiver).poll(cx) {
            Poll::Ready(Ok(result)) => {
                trace!("NotificationFuture completed for type: {:?}", self.notification_type);
                Poll::Ready(result)
            }
            Poll::Ready(Err(_)) => {
                debug!("NotificationFuture channel closed");
                Poll::Ready(Err(WindowsError::ChannelClosed.into()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_future_creation() {
        let runtime = Runtime::new().unwrap();
        let bridge = Arc::new(AsyncBridge::new(runtime.handle().clone()).unwrap());
        
        let callback_data = unsafe { std::mem::zeroed() };
        
        let read_future = ReadFileFuture::new(&bridge, callback_data, 0, 1024);
        assert!(read_future.is_ok());
        
        let enum_future = EnumerateDirectoryFuture::new(
            &bridge,
            callback_data,
            unsafe { std::mem::zeroed() },
            None,
            false,
        );
        assert!(enum_future.is_ok());
        
        let meta_future = GetMetadataFuture::new(
            &bridge,
            callback_data,
            "test.txt".to_string(),
        );
        assert!(meta_future.is_ok());
    }
}