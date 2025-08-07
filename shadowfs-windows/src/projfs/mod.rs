pub mod provider;
pub mod callbacks;
pub mod virtualization;
pub mod async_bridge;
pub mod futures;
pub mod performance;

pub use provider::{ProjFSProvider, ProjFSConfig, ProjFSHandle};
pub use callbacks::{
    CallbackContext,
    start_directory_enumeration_callback,
    get_directory_enumeration_callback,
    end_directory_enumeration_callback,
    get_placeholder_info_callback,
    get_file_data_callback,
};
pub use virtualization::VirtualizationRoot;
pub use async_bridge::{AsyncBridge, CallbackRequest, TaskPriority};
pub use futures::{
    ReadFileFuture,
    EnumerateDirectoryFuture, 
    GetMetadataFuture,
    BatchReadFuture,
    NotificationFuture,
    DirectoryEntry,
    FileMetadata,
    TimeoutConfig,
    TimeoutManager,
    TimeoutMetrics,
    HealthStatus,
};
pub use performance::{
    PerformanceMonitor,
    CallbackMetrics,
    QueueMetrics,
    ThreadPoolMetrics,
    SystemMetrics,
    CallbackTimer,
};