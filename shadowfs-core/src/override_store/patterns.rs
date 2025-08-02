//! Advanced pattern matching and rule-based overrides for shadowfs.

use crate::types::{ShadowPath, FileMetadata};
use bytes::Bytes;
use regex::Regex;
use std::collections::{HashMap, BTreeMap};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, Duration};
use std::fmt;

/// Pattern matching rules for file overrides
#[derive(Debug, Clone)]
pub enum OverrideRule {
    /// Exact path match
    Exact(ShadowPath),
    /// Path prefix match
    Prefix(ShadowPath),
    /// Path suffix match
    Suffix(String),
    /// Regular expression match on path
    Regex(Regex),
    /// Glob pattern match
    Glob(String),
}

impl OverrideRule {
    /// Tests if a path matches this rule
    pub fn matches(&self, path: &ShadowPath) -> bool {
        let path_str = path.to_string();
        
        match self {
            OverrideRule::Exact(pattern) => path == pattern,
            OverrideRule::Prefix(prefix) => path_str.starts_with(&prefix.to_string()),
            OverrideRule::Suffix(suffix) => path_str.ends_with(suffix),
            OverrideRule::Regex(regex) => regex.is_match(&path_str),
            OverrideRule::Glob(pattern) => {
                // Simple glob matching (*, ?, [])
                glob_match(pattern, &path_str)
            }
        }
    }
    
    /// Creates a regex rule from a pattern string
    pub fn regex(pattern: &str) -> Result<Self, regex::Error> {
        Ok(OverrideRule::Regex(Regex::new(pattern)?))
    }
}

/// Rule priority for ordering multiple matching rules
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RulePriority(pub u32);

impl RulePriority {
    pub const HIGHEST: RulePriority = RulePriority(u32::MAX);
    pub const HIGH: RulePriority = RulePriority(1000);
    pub const MEDIUM: RulePriority = RulePriority(500);
    pub const LOW: RulePriority = RulePriority(100);
    pub const LOWEST: RulePriority = RulePriority(0);
}

/// File transformation function type
pub type TransformFn = Box<dyn Fn(&[u8]) -> Result<Bytes, TransformError> + Send + Sync>;

/// Errors that can occur during transformation
#[derive(Debug)]
pub enum TransformError {
    /// Invalid UTF-8 encoding
    EncodingError(std::str::Utf8Error),
    /// Transformation failed
    TransformFailed(String),
    /// IO error during transformation
    IoError(std::io::Error),
}

impl fmt::Display for TransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransformError::EncodingError(e) => write!(f, "Encoding error: {}", e),
            TransformError::TransformFailed(msg) => write!(f, "Transform failed: {}", msg),
            TransformError::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for TransformError {}

/// Chain of transformations to apply to file content
#[derive(Clone)]
pub struct TransformChain {
    transforms: Vec<Arc<TransformFn>>,
}

impl std::fmt::Debug for TransformChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransformChain")
            .field("transform_count", &self.transforms.len())
            .finish()
    }
}

impl TransformChain {
    /// Creates a new empty transform chain
    pub fn new() -> Self {
        Self {
            transforms: Vec::new(),
        }
    }
    
    /// Adds a transformation to the chain
    pub fn add_transform(mut self, transform: TransformFn) -> Self {
        self.transforms.push(Arc::new(transform));
        self
    }
    
    /// Applies all transformations in order
    pub fn apply(&self, data: &[u8]) -> Result<Bytes, TransformError> {
        let mut result = Bytes::copy_from_slice(data);
        
        for transform in &self.transforms {
            result = transform(&result)?;
        }
        
        Ok(result)
    }
    
    /// Checks if the chain is empty
    pub fn is_empty(&self) -> bool {
        self.transforms.is_empty()
    }
}

impl Default for TransformChain {
    fn default() -> Self {
        Self::new()
    }
}

/// Common file transformations
pub mod transforms {
    use super::*;
    
    /// Converts text to uppercase
    pub fn uppercase() -> TransformFn {
        Box::new(|data: &[u8]| {
            let text = std::str::from_utf8(data)
                .map_err(TransformError::EncodingError)?;
            Ok(Bytes::from(text.to_uppercase()))
        })
    }
    
    /// Converts text to lowercase
    pub fn lowercase() -> TransformFn {
        Box::new(|data: &[u8]| {
            let text = std::str::from_utf8(data)
                .map_err(TransformError::EncodingError)?;
            Ok(Bytes::from(text.to_lowercase()))
        })
    }
    
    /// Converts line endings to Unix (LF)
    pub fn unix_line_endings() -> TransformFn {
        Box::new(|data: &[u8]| {
            let text = std::str::from_utf8(data)
                .map_err(TransformError::EncodingError)?;
            let converted = text.replace("\r\n", "\n").replace("\r", "\n");
            Ok(Bytes::from(converted))
        })
    }
    
    /// Converts line endings to Windows (CRLF)
    pub fn windows_line_endings() -> TransformFn {
        Box::new(|data: &[u8]| {
            let text = std::str::from_utf8(data)
                .map_err(TransformError::EncodingError)?;
            // First normalize to LF, then convert to CRLF
            let normalized = text.replace("\r\n", "\n").replace("\r", "\n");
            let converted = normalized.replace("\n", "\r\n");
            Ok(Bytes::from(converted))
        })
    }
    
    /// Adds a prefix to the file content
    pub fn add_prefix(prefix: String) -> TransformFn {
        Box::new(move |data: &[u8]| {
            let mut result = Vec::with_capacity(prefix.len() + data.len());
            result.extend_from_slice(prefix.as_bytes());
            result.extend_from_slice(data);
            Ok(Bytes::from(result))
        })
    }
    
    /// Adds a suffix to the file content
    pub fn add_suffix(suffix: String) -> TransformFn {
        Box::new(move |data: &[u8]| {
            let mut result = Vec::with_capacity(data.len() + suffix.len());
            result.extend_from_slice(data);
            result.extend_from_slice(suffix.as_bytes());
            Ok(Bytes::from(result))
        })
    }
    
    /// Replaces all occurrences of a pattern with replacement text
    pub fn replace_text(pattern: String, replacement: String) -> TransformFn {
        Box::new(move |data: &[u8]| {
            let text = std::str::from_utf8(data)
                .map_err(TransformError::EncodingError)?;
            let result = text.replace(&pattern, &replacement);
            Ok(Bytes::from(result))
        })
    }
    
    /// Removes trailing whitespace from each line
    pub fn trim_lines() -> TransformFn {
        Box::new(|data: &[u8]| {
            let text = std::str::from_utf8(data)
                .map_err(TransformError::EncodingError)?;
            let result = text
                .lines()
                .map(|line| line.trim_end())
                .collect::<Vec<_>>()
                .join("\n");
            Ok(Bytes::from(result))
        })
    }
}

/// Conditions for when an override should be active
#[derive(Debug, Clone)]
pub enum OverrideCondition {
    /// Always active
    Always,
    /// Active during specific time range
    TimeRange {
        start: SystemTime,
        end: SystemTime,
    },
    /// Active for specific users
    UserMatch(Vec<String>),
    /// Active if file size matches criteria
    FileSizeRange {
        min: Option<u64>,
        max: Option<u64>,
    },
    /// Active if file was modified within duration
    ModifiedWithin(Duration),
    /// Active if environment variable matches
    EnvVar {
        name: String,
        value: Option<String>,
    },
    /// Compound condition (all must be true)
    And(Vec<OverrideCondition>),
    /// Compound condition (any must be true)
    Or(Vec<OverrideCondition>),
}

impl OverrideCondition {
    /// Tests if the condition is currently met
    pub fn is_active(&self, metadata: Option<&FileMetadata>) -> bool {
        match self {
            OverrideCondition::Always => true,
            OverrideCondition::TimeRange { start, end } => {
                let now = SystemTime::now();
                now >= *start && now <= *end
            }
            OverrideCondition::UserMatch(users) => {
                // In a real implementation, this would check the current user
                // For now, we'll assume the first user in the list
                !users.is_empty()
            }
            OverrideCondition::FileSizeRange { min, max } => {
                if let Some(meta) = metadata {
                    let size = meta.size;
                    let min_ok = min.map_or(true, |m| size >= m);
                    let max_ok = max.map_or(true, |m| size <= m);
                    min_ok && max_ok
                } else {
                    false
                }
            }
            OverrideCondition::ModifiedWithin(duration) => {
                if let Some(meta) = metadata {
                    if let Ok(elapsed) = meta.modified.elapsed() {
                        elapsed <= *duration
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            OverrideCondition::EnvVar { name, value } => {
                match std::env::var(name) {
                    Ok(env_value) => {
                        value.as_ref().map_or(true, |v| *v == env_value)
                    }
                    Err(_) => value.is_none(),
                }
            }
            OverrideCondition::And(conditions) => {
                conditions.iter().all(|c| c.is_active(metadata))
            }
            OverrideCondition::Or(conditions) => {
                conditions.iter().any(|c| c.is_active(metadata))
            }
        }
    }
}

/// Template variable substitution
#[derive(Debug, Clone)]
pub struct OverrideTemplate {
    /// Template content with variables like ${var_name}
    template: String,
    /// Variable values
    variables: HashMap<String, String>,
}

impl OverrideTemplate {
    /// Creates a new template
    pub fn new(template: String) -> Self {
        Self {
            template,
            variables: HashMap::new(),
        }
    }
    
    /// Adds a variable for substitution
    pub fn with_variable(mut self, name: String, value: String) -> Self {
        self.variables.insert(name, value);
        self
    }
    
    /// Adds variables from environment
    pub fn with_env_vars(mut self, env_vars: &[&str]) -> Self {
        for var_name in env_vars {
            if let Ok(value) = std::env::var(var_name) {
                self.variables.insert(var_name.to_string(), value);
            }
        }
        self
    }
    
    /// Expands the template with current variables
    pub fn expand(&self, path: &ShadowPath) -> Result<Bytes, TemplateError> {
        let mut result = self.template.clone();
        
        // Add path-based variables
        let variables = self.build_variables(path);
        
        // Replace variables in format ${var_name}
        for (name, value) in variables {
            let pattern = format!("${{{}}}", name);
            result = result.replace(&pattern, &value);
        }
        
        // Check for unresolved variables
        if result.contains("${") {
            return Err(TemplateError::UnresolvedVariables);
        }
        
        Ok(Bytes::from(result))
    }
    
    fn build_variables(&self, path: &ShadowPath) -> HashMap<String, String> {
        let mut vars = self.variables.clone();
        
        let path_str = path.to_string();
        vars.insert("path".to_string(), path_str.clone());
        vars.insert("filename".to_string(), 
                   path.file_name().unwrap_or_default().to_string());
        vars.insert("parent".to_string(), 
                   path.parent().map_or_else(String::new, |p| p.to_string()));
        vars.insert("extension".to_string(),
                   path.extension().unwrap_or_default());
        
        // Add timestamp
        if let Ok(timestamp) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
            vars.insert("timestamp".to_string(), timestamp.as_secs().to_string());
        }
        
        vars
    }
}

/// Template expansion errors
#[derive(Debug)]
pub enum TemplateError {
    UnresolvedVariables,
    InvalidTemplate(String),
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TemplateError::UnresolvedVariables => write!(f, "Template contains unresolved variables"),
            TemplateError::InvalidTemplate(msg) => write!(f, "Invalid template: {}", msg),
        }
    }
}

impl std::error::Error for TemplateError {}

/// Copy-on-write override entry
#[derive(Debug, Clone)]
pub enum CowContent {
    /// Reference to original content (not yet copied)
    Reference(ShadowPath),
    /// Owned content that has been modified
    Owned(Bytes),
    /// Transformed content (lazily computed)
    Transformed {
        source: ShadowPath,
        chain: TransformChain,
        cached_result: Arc<Mutex<Option<Bytes>>>,
    },
}

impl CowContent {
    /// Creates a reference to original content
    pub fn reference(path: ShadowPath) -> Self {
        CowContent::Reference(path)
    }
    
    /// Creates owned content
    pub fn owned(content: Bytes) -> Self {
        CowContent::Owned(content)
    }
    
    /// Creates transformed content that will be computed on demand
    pub fn transformed(source: ShadowPath, chain: TransformChain) -> Self {
        CowContent::Transformed {
            source,
            chain,
            cached_result: Arc::new(Mutex::new(None)),
        }
    }
    
    /// Gets the content, loading/transforming as needed
    pub fn get_content(&self, loader: &dyn ContentLoader) -> Result<Bytes, CowError> {
        match self {
            CowContent::Reference(path) => {
                loader.load_content(path)
            }
            CowContent::Owned(content) => {
                Ok(content.clone())
            }
            CowContent::Transformed { source, chain, cached_result } => {
                // Check cache first
                {
                    let cache = cached_result.lock().unwrap();
                    if let Some(cached) = cache.as_ref() {
                        return Ok(cached.clone());
                    }
                }
                
                // Load and transform
                let original = loader.load_content(source)?;
                let transformed = chain.apply(&original)
                    .map_err(CowError::TransformError)?;
                
                // Cache the result
                {
                    let mut cache = cached_result.lock().unwrap();
                    *cache = Some(transformed.clone());
                }
                
                Ok(transformed)
            }
        }
    }
    
    /// Converts to owned content, triggering copy if needed
    pub fn to_owned(&mut self, loader: &dyn ContentLoader) -> Result<(), CowError> {
        match self {
            CowContent::Reference(_) | CowContent::Transformed { .. } => {
                let content = self.get_content(loader)?;
                *self = CowContent::Owned(content);
                Ok(())
            }
            CowContent::Owned(_) => Ok(()),
        }
    }
}

/// Trait for loading content from the underlying filesystem
pub trait ContentLoader {
    fn load_content(&self, path: &ShadowPath) -> Result<Bytes, CowError>;
}

/// Copy-on-write errors
#[derive(Debug)]
pub enum CowError {
    LoadError(String),
    TransformError(TransformError),
    IoError(std::io::Error),
}

impl fmt::Display for CowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CowError::LoadError(msg) => write!(f, "Load error: {}", msg),
            CowError::TransformError(e) => write!(f, "Transform error: {}", e),
            CowError::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for CowError {}

/// Complete rule for file overrides
#[derive(Clone)]
pub struct OverrideRuleEntry {
    /// Pattern matching rule
    pub rule: OverrideRule,
    /// Priority for rule ordering
    pub priority: RulePriority,
    /// Conditions for when this rule is active
    pub condition: OverrideCondition,
    /// Content for the override
    pub content: OverrideContentType,
}

/// Different types of override content
#[derive(Clone)]
pub enum OverrideContentType {
    /// Static content
    Static(Bytes),
    /// Template-based content
    Template(OverrideTemplate),
    /// Copy-on-write content
    CopyOnWrite(CowContent),
    /// Transformed content
    Transformed {
        source: Box<OverrideContentType>,
        chain: TransformChain,
    },
}

impl OverrideContentType {
    /// Resolves the content for a given path
    pub fn resolve(&self, path: &ShadowPath, loader: &dyn ContentLoader) -> Result<Bytes, ResolveError> {
        match self {
            OverrideContentType::Static(content) => Ok(content.clone()),
            OverrideContentType::Template(template) => {
                template.expand(path).map_err(ResolveError::TemplateError)
            }
            OverrideContentType::CopyOnWrite(cow) => {
                cow.get_content(loader).map_err(ResolveError::CowError)
            }
            OverrideContentType::Transformed { source, chain } => {
                let base_content = source.resolve(path, loader)?;
                chain.apply(&base_content).map_err(ResolveError::TransformError)
            }
        }
    }
}

/// Content resolution errors
#[derive(Debug)]
pub enum ResolveError {
    TemplateError(TemplateError),
    CowError(CowError),
    TransformError(TransformError),
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::TemplateError(e) => write!(f, "Template error: {}", e),
            ResolveError::CowError(e) => write!(f, "COW error: {}", e),
            ResolveError::TransformError(e) => write!(f, "Transform error: {}", e),
        }
    }
}

impl std::error::Error for ResolveError {}

/// Set of override rules with priority-based matching
pub struct RuleSet {
    /// Rules stored in priority order (highest first)
    rules: RwLock<BTreeMap<RulePriority, Vec<OverrideRuleEntry>>>,
}

impl RuleSet {
    /// Creates a new empty rule set
    pub fn new() -> Self {
        Self {
            rules: RwLock::new(BTreeMap::new()),
        }
    }
    
    /// Adds a rule to the set
    pub fn add_rule(&self, rule: OverrideRuleEntry) {
        let mut rules = self.rules.write().unwrap();
        rules.entry(rule.priority)
            .or_insert_with(Vec::new)
            .push(rule);
    }
    
    /// Finds the first matching rule for a path
    pub fn find_match(&self, path: &ShadowPath, metadata: Option<&FileMetadata>) -> Option<OverrideRuleEntry> {
        let rules = self.rules.read().unwrap();
        
        // Iterate through priorities in descending order
        for (_, rule_list) in rules.iter().rev() {
            for rule in rule_list {
                if rule.rule.matches(path) && rule.condition.is_active(metadata) {
                    return Some(rule.clone());
                }
            }
        }
        
        None
    }
    
    /// Finds all matching rules for a path
    pub fn find_all_matches(&self, path: &ShadowPath, metadata: Option<&FileMetadata>) -> Vec<OverrideRuleEntry> {
        let rules = self.rules.read().unwrap();
        let mut matches = Vec::new();
        
        // Iterate through priorities in descending order
        for (_, rule_list) in rules.iter().rev() {
            for rule in rule_list {
                if rule.rule.matches(path) && rule.condition.is_active(metadata) {
                    matches.push(rule.clone());
                }
            }
        }
        
        matches
    }
    
    /// Removes all rules with the given priority
    pub fn remove_priority(&self, priority: RulePriority) -> Option<Vec<OverrideRuleEntry>> {
        let mut rules = self.rules.write().unwrap();
        rules.remove(&priority)
    }
    
    /// Gets the number of rules in the set
    pub fn rule_count(&self) -> usize {
        let rules = self.rules.read().unwrap();
        rules.values().map(|v| v.len()).sum()
    }
    
    /// Clears all rules
    pub fn clear(&self) {
        let mut rules = self.rules.write().unwrap();
        rules.clear();
    }
}

impl Default for RuleSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple glob pattern matching
fn glob_match(pattern: &str, text: &str) -> bool {
    glob_match_recursive(pattern, text, 0, 0)
}

fn glob_match_recursive(pattern: &str, text: &str, p_idx: usize, t_idx: usize) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    
    if p_idx == pattern_chars.len() {
        return t_idx == text_chars.len();
    }
    
    if t_idx == text_chars.len() {
        return pattern_chars[p_idx..].iter().all(|&c| c == '*');
    }
    
    match pattern_chars[p_idx] {
        '*' => {
            // Try matching zero or more characters
            glob_match_recursive(pattern, text, p_idx + 1, t_idx) ||
            glob_match_recursive(pattern, text, p_idx, t_idx + 1)
        }
        '?' => {
            // Match exactly one character
            glob_match_recursive(pattern, text, p_idx + 1, t_idx + 1)
        }
        c => {
            // Match literal character
            if text_chars[t_idx] == c {
                glob_match_recursive(pattern, text, p_idx + 1, t_idx + 1)
            } else {
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_override_rule_matching() {
        let path = ShadowPath::new("/home/user/test.txt".into());
        
        let exact_rule = OverrideRule::Exact(path.clone());
        assert!(exact_rule.matches(&path));
        
        let prefix_rule = OverrideRule::Prefix(ShadowPath::new("/home".into()));
        assert!(prefix_rule.matches(&path));
        
        let suffix_rule = OverrideRule::Suffix(".txt".to_string());
        assert!(suffix_rule.matches(&path));
        
        let glob_rule = OverrideRule::Glob("*.txt".to_string());
        assert!(glob_rule.matches(&path));
    }
    
    #[test]
    fn test_transform_chain() {
        let chain = TransformChain::new()
            .add_transform(transforms::uppercase())
            .add_transform(transforms::add_prefix("PREFIX: ".to_string()));
        
        let input = b"hello world";
        let result = chain.apply(input).unwrap();
        assert_eq!(result, Bytes::from("PREFIX: HELLO WORLD"));
    }
    
    #[test]
    fn test_template_expansion() {
        let template = OverrideTemplate::new("Hello ${name}!".to_string())
            .with_variable("name".to_string(), "World".to_string());
        
        let path = ShadowPath::new("/test".into());
        let result = template.expand(&path).unwrap();
        assert!(result.starts_with(b"Hello World!"));
    }
    
    #[test]
    fn test_glob_matching() {
        assert!(glob_match("*.txt", "file.txt"));
        assert!(glob_match("test.*", "test.rs"));
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("*.txt", "file.rs"));
        assert!(glob_match("**", "anything/goes/here"));
    }
    
    #[test]
    fn test_rule_set_priority() {
        let rule_set = RuleSet::new();
        
        let high_rule = OverrideRuleEntry {
            rule: OverrideRule::Suffix(".txt".to_string()),
            priority: RulePriority::HIGH,
            condition: OverrideCondition::Always,
            content: OverrideContentType::Static(Bytes::from("high")),
        };
        
        let low_rule = OverrideRuleEntry {
            rule: OverrideRule::Suffix(".txt".to_string()),
            priority: RulePriority::LOW,
            condition: OverrideCondition::Always,
            content: OverrideContentType::Static(Bytes::from("low")),
        };
        
        rule_set.add_rule(low_rule);
        rule_set.add_rule(high_rule);
        
        let path = ShadowPath::new("/test.txt".into());
        let match_result = rule_set.find_match(&path, None).unwrap();
        assert_eq!(match_result.priority, RulePriority::HIGH);
    }
}