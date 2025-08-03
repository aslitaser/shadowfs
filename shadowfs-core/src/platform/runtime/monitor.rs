//! Feature monitoring for watching system changes

use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;
use crate::types::mount::Platform;
use crate::error::{ShadowError, Result};
use crate::platform::runtime::types::{FeatureChange, FeatureType};
use crate::platform::runtime::detector::RuntimeDetector;

/// Feature monitor for watching system changes
pub struct FeatureMonitor {
    detector: Arc<RuntimeDetector>,
    callbacks: Arc<Mutex<Vec<Box<dyn Fn(FeatureChange) + Send + 'static>>>>,
    running: Arc<RwLock<bool>>,
}

impl FeatureMonitor {
    /// Create a new feature monitor
    pub fn new(detector: Arc<RuntimeDetector>) -> Self {
        Self {
            detector,
            callbacks: Arc::new(Mutex::new(Vec::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Add a callback for feature changes
    pub fn watch_for_changes<F>(&self, callback: F)
    where
        F: Fn(FeatureChange) + Send + 'static,
    {
        let mut callbacks = self.callbacks.lock().unwrap();
        callbacks.push(Box::new(callback));
    }
    
    /// Start monitoring for changes
    pub fn start(&self) -> Result<thread::JoinHandle<()>> {
        let mut running = self.running.write().unwrap();
        if *running {
            return Err(ShadowError::InvalidConfiguration {
                message: "Monitor already running".to_string(),
            });
        }
        *running = true;
        
        let detector = Arc::clone(&self.detector);
        let callbacks = Arc::clone(&self.callbacks);
        let running_flag = Arc::clone(&self.running);
        let platform = Platform::current();
        
        let handle = thread::spawn(move || {
            #[cfg(target_os = "linux")]
            {
                if platform == Platform::Linux {
                    Self::monitor_linux(detector, callbacks, running_flag);
                    return;
                }
            }
            
            #[cfg(target_os = "macos")]
            {
                if platform == Platform::MacOS {
                    Self::monitor_macos(detector, callbacks, running_flag);
                    return;
                }
            }
            
            #[cfg(target_os = "windows")]
            {
                if platform == Platform::Windows {
                    Self::monitor_windows(detector, callbacks, running_flag);
                    return;
                }
            }
            
            // Fallback for any platform
            while *running_flag.read().unwrap() {
                thread::sleep(Duration::from_secs(1));
            }
        });
        
        Ok(handle)
    }
    
    /// Stop monitoring
    pub fn stop(&self) {
        let mut running = self.running.write().unwrap();
        *running = false;
    }
    
    #[cfg(target_os = "linux")]
    fn monitor_linux(
        detector: Arc<RuntimeDetector>,
        callbacks: Arc<Mutex<Vec<Box<dyn Fn(FeatureChange) + Send + 'static>>>>,
        running: Arc<RwLock<bool>>,
    ) {
        use inotify::{Inotify, WatchMask};
        
        let mut inotify = match Inotify::init() {
            Ok(i) => i,
            Err(_) => return,
        };
        
        // Watch for FUSE module changes
        let _ = inotify.add_watch("/proc/modules", WatchMask::MODIFY);
        let _ = inotify.add_watch("/dev", WatchMask::CREATE | WatchMask::DELETE);
        
        let mut buffer = [0u8; 4096];
        let mut last_fuse_status = detector.detect_on_demand(FeatureType::FuseAvailable, false);
        
        while *running.read().unwrap() {
            if let Ok(events) = inotify.read_events(&mut buffer) {
                for _event in events {
                    // Check if FUSE status changed
                    let new_status = detector.detect_on_demand(FeatureType::FuseAvailable, true);
                    
                    if new_status.available != last_fuse_status.available {
                        let change = if new_status.available {
                            FeatureChange::Available {
                                feature: FeatureType::FuseAvailable,
                                details: new_status.details.clone(),
                            }
                        } else {
                            FeatureChange::Unavailable {
                                feature: FeatureType::FuseAvailable,
                                reason: new_status.details.clone(),
                            }
                        };
                        
                        let callbacks = callbacks.lock().unwrap();
                        for callback in callbacks.iter() {
                            callback(change.clone());
                        }
                        
                        last_fuse_status = new_status;
                    }
                }
            }
            
            thread::sleep(Duration::from_millis(100));
        }
    }
    
    #[cfg(target_os = "macos")]
    fn monitor_macos(
        detector: Arc<RuntimeDetector>,
        callbacks: Arc<Mutex<Vec<Box<dyn Fn(FeatureChange) + Send + 'static>>>>,
        running: Arc<RwLock<bool>>,
    ) {
        // Use FSEvents to monitor filesystem changes
        // This is a simplified implementation
        let mut last_macfuse_status = detector.detect_on_demand(FeatureType::MacFuseAvailable, false);
        let mut last_fskit_status = detector.detect_on_demand(FeatureType::FSKitAvailable, false);
        
        while *running.read().unwrap() {
            thread::sleep(Duration::from_secs(5));
            
            // Check macFUSE status
            let new_macfuse = detector.detect_on_demand(FeatureType::MacFuseAvailable, true);
            if new_macfuse.available != last_macfuse_status.available {
                let change = if new_macfuse.available {
                    FeatureChange::Available {
                        feature: FeatureType::MacFuseAvailable,
                        details: new_macfuse.details.clone(),
                    }
                } else {
                    FeatureChange::Unavailable {
                        feature: FeatureType::MacFuseAvailable,
                        reason: new_macfuse.details.clone(),
                    }
                };
                
                let callbacks = callbacks.lock().unwrap();
                for callback in callbacks.iter() {
                    callback(change.clone());
                }
                
                last_macfuse_status = new_macfuse;
            }
            
            // Check FSKit status
            let new_fskit = detector.detect_on_demand(FeatureType::FSKitAvailable, true);
            if new_fskit.available != last_fskit_status.available {
                let change = if new_fskit.available {
                    FeatureChange::Available {
                        feature: FeatureType::FSKitAvailable,
                        details: new_fskit.details.clone(),
                    }
                } else {
                    FeatureChange::Unavailable {
                        feature: FeatureType::FSKitAvailable,
                        reason: new_fskit.details.clone(),
                    }
                };
                
                let callbacks = callbacks.lock().unwrap();
                for callback in callbacks.iter() {
                    callback(change.clone());
                }
                
                last_fskit_status = new_fskit;
            }
        }
    }
    
    #[cfg(target_os = "windows")]
    fn monitor_windows(
        detector: Arc<RuntimeDetector>,
        callbacks: Arc<Mutex<Vec<Box<dyn Fn(FeatureChange) + Send + 'static>>>>,
        running: Arc<RwLock<bool>>,
    ) {
        // Monitor Windows features using WMI or registry polling
        let mut last_projfs_status = detector.detect_on_demand(FeatureType::ProjFSAvailable, false);
        let mut last_dev_mode = detector.detect_on_demand(FeatureType::DeveloperMode, false);
        
        while *running.read().unwrap() {
            thread::sleep(Duration::from_secs(5));
            
            // Check ProjFS status
            let new_projfs = detector.detect_on_demand(FeatureType::ProjFSAvailable, true);
            if new_projfs.available != last_projfs_status.available {
                let change = if new_projfs.available {
                    FeatureChange::Available {
                        feature: FeatureType::ProjFSAvailable,
                        details: new_projfs.details.clone(),
                    }
                } else {
                    FeatureChange::Unavailable {
                        feature: FeatureType::ProjFSAvailable,
                        reason: new_projfs.details.clone(),
                    }
                };
                
                let callbacks = callbacks.lock().unwrap();
                for callback in callbacks.iter() {
                    callback(change.clone());
                }
                
                last_projfs_status = new_projfs;
            }
            
            // Check Developer Mode
            let new_dev_mode = detector.detect_on_demand(FeatureType::DeveloperMode, true);
            if new_dev_mode.available != last_dev_mode.available {
                let change = if new_dev_mode.available {
                    FeatureChange::Available {
                        feature: FeatureType::DeveloperMode,
                        details: new_dev_mode.details.clone(),
                    }
                } else {
                    FeatureChange::Unavailable {
                        feature: FeatureType::DeveloperMode,
                        reason: new_dev_mode.details.clone(),
                    }
                };
                
                let callbacks = callbacks.lock().unwrap();
                for callback in callbacks.iter() {
                    callback(change.clone());
                }
                
                last_dev_mode = new_dev_mode;
            }
        }
    }
}