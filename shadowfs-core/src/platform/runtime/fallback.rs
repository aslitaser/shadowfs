//! Fallback mechanisms for feature operations

use crate::error::{Result, ShadowError};

/// Fallback mechanism for feature operations
pub struct FallbackMechanism {
    primary_method: String,
    fallback_methods: Vec<String>,
    notification_handler: Option<Box<dyn Fn(&str) + Send + Sync>>,
}

impl FallbackMechanism {
    /// Create a new fallback mechanism
    pub fn new(primary: impl Into<String>) -> Self {
        Self {
            primary_method: primary.into(),
            fallback_methods: Vec::new(),
            notification_handler: None,
        }
    }
    
    /// Add a fallback method
    pub fn with_fallback(mut self, method: impl Into<String>) -> Self {
        self.fallback_methods.push(method.into());
        self
    }
    
    /// Set notification handler for fallback events
    pub fn with_notification<F>(mut self, handler: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.notification_handler = Some(Box::new(handler));
        self
    }
    
    /// Execute with fallback
    pub fn execute<F, T>(&self, operation: F) -> Result<T>
    where
        F: Fn(&str) -> Result<T>,
    {
        // Try primary method
        match operation(&self.primary_method) {
            Ok(result) => Ok(result),
            Err(primary_err) => {
                // Notify about primary failure
                if let Some(handler) = &self.notification_handler {
                    handler(&format!(
                        "Primary method '{}' failed: {}. Trying fallbacks...",
                        self.primary_method, primary_err
                    ));
                }
                
                // Try fallback methods
                for (i, method) in self.fallback_methods.iter().enumerate() {
                    match operation(method) {
                        Ok(result) => {
                            if let Some(handler) = &self.notification_handler {
                                handler(&format!(
                                    "Fallback method '{}' succeeded",
                                    method
                                ));
                            }
                            return Ok(result);
                        }
                        Err(err) => {
                            if let Some(handler) = &self.notification_handler {
                                handler(&format!(
                                    "Fallback method {} of {} failed: {}",
                                    i + 1,
                                    self.fallback_methods.len(),
                                    err
                                ));
                            }
                        }
                    }
                }
                
                // All methods failed
                Err(primary_err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fallback_mechanism() {
        let fallback = FallbackMechanism::new("primary")
            .with_fallback("secondary")
            .with_fallback("tertiary");
        
        // Test successful primary
        let result = fallback.execute(|method| {
            if method == "primary" {
                Ok(42)
            } else {
                Err(ShadowError::InvalidConfiguration {
                    message: "Not primary".to_string(),
                })
            }
        });
        assert_eq!(result.unwrap(), 42);
        
        // Test fallback to secondary
        let result = fallback.execute(|method| {
            if method == "secondary" {
                Ok(24)
            } else {
                Err(ShadowError::InvalidConfiguration {
                    message: "Not secondary".to_string(),
                })
            }
        });
        assert_eq!(result.unwrap(), 24);
    }
}