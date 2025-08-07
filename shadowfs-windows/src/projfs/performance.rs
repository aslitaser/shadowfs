use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use std::collections::VecDeque;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use dashmap::DashMap;

use super::TaskPriority;

/// Performance metrics for callback operations
#[derive(Debug, Clone)]
pub struct CallbackMetrics {
    pub operation_type: String,
    pub count: u64,
    pub total_latency_ms: u64,
    pub min_latency_ms: u64,
    pub max_latency_ms: u64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: u64,
    pub p95_latency_ms: u64,
    pub p99_latency_ms: u64,
}

/// Queue depth metrics
#[derive(Debug, Clone)]
pub struct QueueMetrics {
    pub current_depth: usize,
    pub max_depth: usize,
    pub avg_depth: f64,
    pub total_enqueued: u64,
    pub total_dequeued: u64,
    pub total_dropped: u64,
    pub by_priority: [PriorityMetrics; 4],
}

#[derive(Debug, Clone, Default)]
pub struct PriorityMetrics {
    pub current_count: usize,
    pub total_processed: u64,
    pub avg_wait_time_ms: f64,
    pub max_wait_time_ms: u64,
}

/// Thread pool utilization metrics
#[derive(Debug, Clone)]
pub struct ThreadPoolMetrics {
    pub total_threads: usize,
    pub active_threads: usize,
    pub idle_threads: usize,
    pub utilization_percent: f64,
    pub tasks_completed: u64,
    pub tasks_failed: u64,
    pub avg_task_duration_ms: f64,
    pub thread_cpu_percent: Vec<f64>,
}

/// Overall system performance metrics
#[derive(Debug, Clone)]
pub struct SystemMetrics {
    pub uptime_seconds: u64,
    pub total_callbacks: u64,
    pub callbacks_per_second: f64,
    pub memory_usage_mb: f64,
    pub backpressure_events: u64,
    pub timeout_events: u64,
    pub error_rate: f64,
}

/// Histogram for latency tracking
struct LatencyHistogram {
    buckets: Vec<(u64, AtomicU64)>, // (bucket_ms, count)
    total_samples: AtomicU64,
}

impl LatencyHistogram {
    fn new() -> Self {
        let buckets = vec![
            1, 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10000
        ].into_iter()
        .map(|ms| (ms, AtomicU64::new(0)))
        .collect();

        Self {
            buckets,
            total_samples: AtomicU64::new(0),
        }
    }

    fn record(&self, latency_ms: u64) {
        self.total_samples.fetch_add(1, Ordering::Relaxed);
        
        for (bucket_ms, count) in &self.buckets {
            if latency_ms <= *bucket_ms {
                count.fetch_add(1, Ordering::Relaxed);
                break;
            }
        }
    }

    fn percentile(&self, p: f64) -> u64 {
        let total = self.total_samples.load(Ordering::Relaxed) as f64;
        if total == 0.0 {
            return 0;
        }

        let target = (total * p / 100.0) as u64;
        let mut cumulative = 0u64;

        for (bucket_ms, count) in &self.buckets {
            cumulative += count.load(Ordering::Relaxed);
            if cumulative >= target {
                return *bucket_ms;
            }
        }

        self.buckets.last().map(|(ms, _)| *ms).unwrap_or(0)
    }
}

/// Performance monitor for the async bridge
pub struct PerformanceMonitor {
    // Callback latency tracking
    callback_latencies: Arc<DashMap<String, LatencyHistogram>>,
    callback_counts: Arc<DashMap<String, AtomicU64>>,
    
    // Queue depth tracking
    queue_depth: Arc<AtomicUsize>,
    max_queue_depth: Arc<AtomicUsize>,
    queue_depth_samples: Arc<RwLock<VecDeque<usize>>>,
    enqueued_total: Arc<AtomicU64>,
    dequeued_total: Arc<AtomicU64>,
    dropped_total: Arc<AtomicU64>,
    
    // Priority-specific metrics
    priority_queues: Arc<[DashMap<u64, Instant>; 4]>,
    priority_processed: Arc<[AtomicU64; 4]>,
    priority_wait_times: Arc<[RwLock<Vec<u64>>; 4]>,
    
    // Thread pool metrics
    thread_count: Arc<AtomicUsize>,
    active_threads: Arc<AtomicUsize>,
    tasks_completed: Arc<AtomicU64>,
    tasks_failed: Arc<AtomicU64>,
    task_durations: Arc<RwLock<VecDeque<u64>>>,
    
    // System metrics
    start_time: Instant,
    backpressure_events: Arc<AtomicU64>,
    timeout_events: Arc<AtomicU64>,
    error_count: Arc<AtomicU64>,
    
    // Monitoring interval
    sample_interval: Duration,
}

impl PerformanceMonitor {
    pub fn new(thread_count: usize) -> Self {
        Self {
            callback_latencies: Arc::new(DashMap::new()),
            callback_counts: Arc::new(DashMap::new()),
            
            queue_depth: Arc::new(AtomicUsize::new(0)),
            max_queue_depth: Arc::new(AtomicUsize::new(0)),
            queue_depth_samples: Arc::new(RwLock::new(VecDeque::with_capacity(1000))),
            enqueued_total: Arc::new(AtomicU64::new(0)),
            dequeued_total: Arc::new(AtomicU64::new(0)),
            dropped_total: Arc::new(AtomicU64::new(0)),
            
            priority_queues: Arc::new([
                DashMap::new(),
                DashMap::new(),
                DashMap::new(),
                DashMap::new(),
            ]),
            priority_processed: Arc::new([
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ]),
            priority_wait_times: Arc::new([
                RwLock::new(Vec::new()),
                RwLock::new(Vec::new()),
                RwLock::new(Vec::new()),
                RwLock::new(Vec::new()),
            ]),
            
            thread_count: Arc::new(AtomicUsize::new(thread_count)),
            active_threads: Arc::new(AtomicUsize::new(0)),
            tasks_completed: Arc::new(AtomicU64::new(0)),
            tasks_failed: Arc::new(AtomicU64::new(0)),
            task_durations: Arc::new(RwLock::new(VecDeque::with_capacity(1000))),
            
            start_time: Instant::now(),
            backpressure_events: Arc::new(AtomicU64::new(0)),
            timeout_events: Arc::new(AtomicU64::new(0)),
            error_count: Arc::new(AtomicU64::new(0)),
            
            sample_interval: Duration::from_millis(100),
        }
    }

    /// Record callback start
    pub fn record_callback_start(&self, operation: &str) -> CallbackTimer {
        self.callback_counts
            .entry(operation.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);

        CallbackTimer {
            operation: operation.to_string(),
            start_time: Instant::now(),
            monitor: self.clone(),
        }
    }

    /// Record callback completion
    pub fn record_callback_end(&self, operation: &str, duration: Duration) {
        let latency_ms = duration.as_millis() as u64;
        
        self.callback_latencies
            .entry(operation.to_string())
            .or_insert_with(LatencyHistogram::new)
            .record(latency_ms);

        if latency_ms > 1000 {
            warn!("Slow callback detected: {} took {}ms", operation, latency_ms);
        }
    }

    /// Record task enqueued
    pub fn record_enqueue(&self, priority: TaskPriority, task_id: u64) {
        self.enqueued_total.fetch_add(1, Ordering::Relaxed);
        
        let depth = self.queue_depth.fetch_add(1, Ordering::AcqRel) + 1;
        self.update_max_depth(depth);
        
        self.priority_queues[priority as usize].insert(task_id, Instant::now());
    }

    /// Record task dequeued
    pub fn record_dequeue(&self, priority: TaskPriority, task_id: u64) {
        self.dequeued_total.fetch_add(1, Ordering::Relaxed);
        self.queue_depth.fetch_sub(1, Ordering::AcqRel);
        
        if let Some((_, enqueue_time)) = self.priority_queues[priority as usize].remove(&task_id) {
            let wait_time_ms = enqueue_time.elapsed().as_millis() as u64;
            
            let priority_idx = priority as usize;
            self.priority_processed[priority_idx].fetch_add(1, Ordering::Relaxed);
            
            // Record wait time
            tokio::spawn({
                let wait_times = self.priority_wait_times[priority_idx].clone();
                async move {
                    let mut times = wait_times.write().await;
                    times.push(wait_time_ms);
                    if times.len() > 1000 {
                        times.remove(0);
                    }
                }
            });
            
            if wait_time_ms > 5000 {
                warn!("Long queue wait time for {:?} priority: {}ms", priority, wait_time_ms);
            }
        }
    }

    /// Record task dropped
    pub fn record_drop(&self, priority: TaskPriority, task_id: u64) {
        self.dropped_total.fetch_add(1, Ordering::Relaxed);
        self.queue_depth.fetch_sub(1, Ordering::AcqRel);
        self.priority_queues[priority as usize].remove(&task_id);
    }

    /// Record thread activity
    pub fn record_thread_active(&self) {
        self.active_threads.fetch_add(1, Ordering::AcqRel);
    }

    pub fn record_thread_idle(&self) {
        self.active_threads.fetch_sub(1, Ordering::AcqRel);
    }

    /// Record task completion
    pub fn record_task_complete(&self, duration: Duration, success: bool) {
        if success {
            self.tasks_completed.fetch_add(1, Ordering::Relaxed);
        } else {
            self.tasks_failed.fetch_add(1, Ordering::Relaxed);
        }

        let duration_ms = duration.as_millis() as u64;
        tokio::spawn({
            let durations = self.task_durations.clone();
            async move {
                let mut d = durations.write().await;
                d.push_back(duration_ms);
                if d.len() > 1000 {
                    d.pop_front();
                }
            }
        });
    }

    /// Record backpressure event
    pub fn record_backpressure(&self) {
        self.backpressure_events.fetch_add(1, Ordering::Relaxed);
    }

    /// Record timeout event
    pub fn record_timeout(&self) {
        self.timeout_events.fetch_add(1, Ordering::Relaxed);
    }

    /// Record error
    pub fn record_error(&self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Update max queue depth
    fn update_max_depth(&self, current: usize) {
        let mut max = self.max_queue_depth.load(Ordering::Relaxed);
        while current > max {
            match self.max_queue_depth.compare_exchange_weak(
                max,
                current,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => max = x,
            }
        }
    }

    /// Sample current queue depth
    pub async fn sample_queue_depth(&self) {
        let depth = self.queue_depth.load(Ordering::Relaxed);
        let mut samples = self.queue_depth_samples.write().await;
        samples.push_back(depth);
        if samples.len() > 1000 {
            samples.pop_front();
        }
    }

    /// Get callback metrics
    pub async fn get_callback_metrics(&self) -> Vec<CallbackMetrics> {
        let mut metrics = Vec::new();

        for entry in self.callback_latencies.iter() {
            let operation = entry.key().clone();
            let histogram = entry.value();
            let count = self.callback_counts
                .get(&operation)
                .map(|c| c.value().load(Ordering::Relaxed))
                .unwrap_or(0);

            if count > 0 {
                metrics.push(CallbackMetrics {
                    operation_type: operation,
                    count,
                    total_latency_ms: 0, // Would need additional tracking
                    min_latency_ms: histogram.percentile(0.0),
                    max_latency_ms: histogram.percentile(100.0),
                    avg_latency_ms: 0.0, // Would need additional tracking
                    p50_latency_ms: histogram.percentile(50.0),
                    p95_latency_ms: histogram.percentile(95.0),
                    p99_latency_ms: histogram.percentile(99.0),
                });
            }
        }

        metrics
    }

    /// Get queue metrics
    pub async fn get_queue_metrics(&self) -> QueueMetrics {
        let samples = self.queue_depth_samples.read().await;
        let avg_depth = if !samples.is_empty() {
            samples.iter().sum::<usize>() as f64 / samples.len() as f64
        } else {
            0.0
        };

        let mut by_priority = [
            PriorityMetrics::default(),
            PriorityMetrics::default(),
            PriorityMetrics::default(),
            PriorityMetrics::default(),
        ];

        for i in 0..4 {
            let wait_times = self.priority_wait_times[i].read().await;
            by_priority[i] = PriorityMetrics {
                current_count: self.priority_queues[i].len(),
                total_processed: self.priority_processed[i].load(Ordering::Relaxed),
                avg_wait_time_ms: if !wait_times.is_empty() {
                    wait_times.iter().sum::<u64>() as f64 / wait_times.len() as f64
                } else {
                    0.0
                },
                max_wait_time_ms: wait_times.iter().max().copied().unwrap_or(0),
            };
        }

        QueueMetrics {
            current_depth: self.queue_depth.load(Ordering::Relaxed),
            max_depth: self.max_queue_depth.load(Ordering::Relaxed),
            avg_depth,
            total_enqueued: self.enqueued_total.load(Ordering::Relaxed),
            total_dequeued: self.dequeued_total.load(Ordering::Relaxed),
            total_dropped: self.dropped_total.load(Ordering::Relaxed),
            by_priority,
        }
    }

    /// Get thread pool metrics
    pub async fn get_thread_pool_metrics(&self) -> ThreadPoolMetrics {
        let total = self.thread_count.load(Ordering::Relaxed);
        let active = self.active_threads.load(Ordering::Relaxed);
        let idle = total.saturating_sub(active);
        let utilization = if total > 0 {
            (active as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let durations = self.task_durations.read().await;
        let avg_duration = if !durations.is_empty() {
            durations.iter().sum::<u64>() as f64 / durations.len() as f64
        } else {
            0.0
        };

        ThreadPoolMetrics {
            total_threads: total,
            active_threads: active,
            idle_threads: idle,
            utilization_percent: utilization,
            tasks_completed: self.tasks_completed.load(Ordering::Relaxed),
            tasks_failed: self.tasks_failed.load(Ordering::Relaxed),
            avg_task_duration_ms: avg_duration,
            thread_cpu_percent: vec![0.0; total], // Would need OS-specific implementation
        }
    }

    /// Get system metrics
    pub async fn get_system_metrics(&self) -> SystemMetrics {
        let uptime = self.start_time.elapsed().as_secs();
        let total_callbacks = self.callback_counts
            .iter()
            .map(|e| e.value().load(Ordering::Relaxed))
            .sum();
        
        let callbacks_per_second = if uptime > 0 {
            total_callbacks as f64 / uptime as f64
        } else {
            0.0
        };

        let total_tasks = self.tasks_completed.load(Ordering::Relaxed) + 
                         self.tasks_failed.load(Ordering::Relaxed);
        let error_rate = if total_tasks > 0 {
            self.error_count.load(Ordering::Relaxed) as f64 / total_tasks as f64
        } else {
            0.0
        };

        SystemMetrics {
            uptime_seconds: uptime,
            total_callbacks,
            callbacks_per_second,
            memory_usage_mb: 0.0, // Would need process memory tracking
            backpressure_events: self.backpressure_events.load(Ordering::Relaxed),
            timeout_events: self.timeout_events.load(Ordering::Relaxed),
            error_rate,
        }
    }

    /// Start periodic monitoring
    pub fn start_monitoring(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            
            loop {
                interval.tick().await;
                
                // Sample queue depth
                self.sample_queue_depth().await;
                
                // Log metrics periodically
                let system = self.get_system_metrics().await;
                let queue = self.get_queue_metrics().await;
                let threads = self.get_thread_pool_metrics().await;
                
                info!(
                    "Performance: callbacks/s={:.2}, queue_depth={}, thread_util={:.1}%, errors={:.2}%",
                    system.callbacks_per_second,
                    queue.current_depth,
                    threads.utilization_percent,
                    system.error_rate * 100.0
                );
                
                // Warn on concerning metrics
                if queue.current_depth > queue.max_depth * 80 / 100 {
                    warn!("Queue depth high: {} (max: {})", queue.current_depth, queue.max_depth);
                }
                
                if threads.utilization_percent > 90.0 {
                    warn!("Thread pool utilization high: {:.1}%", threads.utilization_percent);
                }
                
                if system.error_rate > 0.05 {
                    warn!("Error rate high: {:.2}%", system.error_rate * 100.0);
                }
            }
        });
    }

    /// Generate performance report
    pub async fn generate_report(&self) -> String {
        let callbacks = self.get_callback_metrics().await;
        let queue = self.get_queue_metrics().await;
        let threads = self.get_thread_pool_metrics().await;
        let system = self.get_system_metrics().await;

        let mut report = String::new();
        report.push_str("=== Performance Report ===\n\n");
        
        report.push_str(&format!("System Metrics:\n"));
        report.push_str(&format!("  Uptime: {} seconds\n", system.uptime_seconds));
        report.push_str(&format!("  Total Callbacks: {}\n", system.total_callbacks));
        report.push_str(&format!("  Callbacks/sec: {:.2}\n", system.callbacks_per_second));
        report.push_str(&format!("  Error Rate: {:.2}%\n", system.error_rate * 100.0));
        report.push_str(&format!("  Backpressure Events: {}\n", system.backpressure_events));
        report.push_str(&format!("  Timeout Events: {}\n\n", system.timeout_events));

        report.push_str("Queue Metrics:\n");
        report.push_str(&format!("  Current Depth: {}\n", queue.current_depth));
        report.push_str(&format!("  Max Depth: {}\n", queue.max_depth));
        report.push_str(&format!("  Average Depth: {:.2}\n", queue.avg_depth));
        report.push_str(&format!("  Total Enqueued: {}\n", queue.total_enqueued));
        report.push_str(&format!("  Total Dropped: {}\n\n", queue.total_dropped));

        report.push_str("Priority Queue Breakdown:\n");
        for (i, priority) in ["Critical", "High", "Normal", "Low"].iter().enumerate() {
            let m = &queue.by_priority[i];
            report.push_str(&format!("  {} Priority:\n", priority));
            report.push_str(&format!("    Processed: {}\n", m.total_processed));
            report.push_str(&format!("    Avg Wait: {:.2}ms\n", m.avg_wait_time_ms));
            report.push_str(&format!("    Max Wait: {}ms\n", m.max_wait_time_ms));
        }

        report.push_str("\nThread Pool Metrics:\n");
        report.push_str(&format!("  Total Threads: {}\n", threads.total_threads));
        report.push_str(&format!("  Active Threads: {}\n", threads.active_threads));
        report.push_str(&format!("  Utilization: {:.1}%\n", threads.utilization_percent));
        report.push_str(&format!("  Tasks Completed: {}\n", threads.tasks_completed));
        report.push_str(&format!("  Tasks Failed: {}\n", threads.tasks_failed));
        report.push_str(&format!("  Avg Task Duration: {:.2}ms\n\n", threads.avg_task_duration_ms));

        report.push_str("Callback Latencies:\n");
        for metric in callbacks {
            report.push_str(&format!("  {}:\n", metric.operation_type));
            report.push_str(&format!("    Count: {}\n", metric.count));
            report.push_str(&format!("    P50: {}ms\n", metric.p50_latency_ms));
            report.push_str(&format!("    P95: {}ms\n", metric.p95_latency_ms));
            report.push_str(&format!("    P99: {}ms\n", metric.p99_latency_ms));
        }

        report
    }
}

impl Clone for PerformanceMonitor {
    fn clone(&self) -> Self {
        Self {
            callback_latencies: self.callback_latencies.clone(),
            callback_counts: self.callback_counts.clone(),
            queue_depth: self.queue_depth.clone(),
            max_queue_depth: self.max_queue_depth.clone(),
            queue_depth_samples: self.queue_depth_samples.clone(),
            enqueued_total: self.enqueued_total.clone(),
            dequeued_total: self.dequeued_total.clone(),
            dropped_total: self.dropped_total.clone(),
            priority_queues: self.priority_queues.clone(),
            priority_processed: self.priority_processed.clone(),
            priority_wait_times: self.priority_wait_times.clone(),
            thread_count: self.thread_count.clone(),
            active_threads: self.active_threads.clone(),
            tasks_completed: self.tasks_completed.clone(),
            tasks_failed: self.tasks_failed.clone(),
            task_durations: self.task_durations.clone(),
            start_time: self.start_time,
            backpressure_events: self.backpressure_events.clone(),
            timeout_events: self.timeout_events.clone(),
            error_count: self.error_count.clone(),
            sample_interval: self.sample_interval,
        }
    }
}

/// Timer for measuring callback duration
pub struct CallbackTimer {
    operation: String,
    start_time: Instant,
    monitor: PerformanceMonitor,
}

impl Drop for CallbackTimer {
    fn drop(&mut self) {
        let duration = self.start_time.elapsed();
        self.monitor.record_callback_end(&self.operation, duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_performance_monitor() {
        let monitor = Arc::new(PerformanceMonitor::new(4));
        
        // Record some operations
        monitor.record_enqueue(TaskPriority::Critical, 1);
        monitor.record_thread_active();
        
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        monitor.record_dequeue(TaskPriority::Critical, 1);
        monitor.record_task_complete(Duration::from_millis(10), true);
        monitor.record_thread_idle();
        
        // Check metrics
        let queue_metrics = monitor.get_queue_metrics().await;
        assert_eq!(queue_metrics.total_enqueued, 1);
        assert_eq!(queue_metrics.total_dequeued, 1);
        
        let thread_metrics = monitor.get_thread_pool_metrics().await;
        assert_eq!(thread_metrics.tasks_completed, 1);
    }

    #[test]
    fn test_latency_histogram() {
        let histogram = LatencyHistogram::new();
        
        // Record some latencies
        histogram.record(5);
        histogram.record(15);
        histogram.record(50);
        histogram.record(100);
        histogram.record(500);
        
        // Check percentiles
        assert!(histogram.percentile(50.0) <= 100);
        assert!(histogram.percentile(90.0) <= 500);
    }
}