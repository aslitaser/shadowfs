//! Directory cache for managing parent-child relationships.

use crate::types::ShadowPath;
use dashmap::{DashMap, DashSet};

/// Cache for directory structure and parent-child relationships.
#[derive(Debug)]
pub struct DirectoryCache {
    /// Map of directory paths to their immediate children
    children: DashMap<ShadowPath, DashSet<String>>,
}

impl DirectoryCache {
    /// Creates a new DirectoryCache.
    pub fn new() -> Self {
        Self {
            children: DashMap::new(),
        }
    }
    
    /// Adds a child to a parent directory.
    ///
    /// # Arguments
    /// * `parent` - Parent directory path
    /// * `child_name` - Name of the child entry (not full path)
    pub fn add_child(&self, parent: &ShadowPath, child_name: &str) {
        self.children
            .entry(parent.clone())
            .or_insert_with(DashSet::new)
            .insert(child_name.to_string());
    }
    
    /// Removes a child from a parent directory.
    ///
    /// # Arguments
    /// * `parent` - Parent directory path
    /// * `child_name` - Name of the child entry to remove
    ///
    /// # Returns
    /// true if the child was removed, false if it didn't exist
    pub fn remove_child(&self, parent: &ShadowPath, child_name: &str) -> bool {
        if let Some(children) = self.children.get(parent) {
            let removed = children.remove(child_name).is_some();
            
            // Clean up empty parent entry
            if children.is_empty() {
                drop(children);
                self.children.remove(parent);
            }
            
            removed
        } else {
            false
        }
    }
    
    /// Gets all children of a directory.
    ///
    /// # Arguments
    /// * `parent` - Parent directory path
    ///
    /// # Returns
    /// Vector of child entry names
    pub fn get_children(&self, parent: &ShadowPath) -> Vec<String> {
        self.children
            .get(parent)
            .map(|children| children.iter().map(|entry| entry.key().clone()).collect())
            .unwrap_or_default()
    }
    
    /// Checks if a directory has any children.
    ///
    /// # Arguments
    /// * `parent` - Parent directory path
    ///
    /// # Returns
    /// true if the directory has children
    pub fn has_children(&self, parent: &ShadowPath) -> bool {
        self.children
            .get(parent)
            .map(|children| !children.is_empty())
            .unwrap_or(false)
    }
    
    /// Checks if a specific child exists in a directory.
    ///
    /// # Arguments
    /// * `parent` - Parent directory path
    /// * `child_name` - Name of the child to check
    ///
    /// # Returns
    /// true if the child exists
    pub fn has_child(&self, parent: &ShadowPath, child_name: &str) -> bool {
        self.children
            .get(parent)
            .map(|children| children.contains(child_name))
            .unwrap_or(false)
    }
    
    /// Removes all children of a directory.
    ///
    /// # Arguments
    /// * `parent` - Parent directory path
    ///
    /// # Returns
    /// Vector of removed child names
    pub fn clear_children(&self, parent: &ShadowPath) -> Vec<String> {
        self.children
            .remove(parent)
            .map(|(_, children)| children.into_iter().collect())
            .unwrap_or_default()
    }
    
    /// Gets all directories that are being tracked.
    ///
    /// # Returns
    /// Vector of all parent directory paths
    pub fn get_all_parents(&self) -> Vec<ShadowPath> {
        self.children.iter().map(|entry| entry.key().clone()).collect()
    }
    
    /// Gets the total number of directories being tracked.
    pub fn directory_count(&self) -> usize {
        self.children.len()
    }
    
    /// Gets the total number of child entries across all directories.
    pub fn total_child_count(&self) -> usize {
        self.children
            .iter()
            .map(|entry| entry.value().len())
            .sum()
    }
}

impl Default for DirectoryCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Path traversal utilities for directory operations.
pub struct PathTraversal;

impl PathTraversal {
    /// Gets the chain of parent directories for a given path.
    ///
    /// # Arguments
    /// * `path` - Path to get parents for
    ///
    /// # Returns
    /// Vector of parent paths, ordered from immediate parent to root
    ///
    /// # Example
    /// For path "/a/b/c/d", returns ["/a/b/c", "/a/b", "/a", "/"]
    pub fn get_parent_chain(path: &ShadowPath) -> Vec<ShadowPath> {
        let mut parents = Vec::new();
        let mut current = path.clone();
        
        while let Some(parent) = current.parent() {
            parents.push(parent.clone());
            current = parent;
        }
        
        parents
    }
    
    /// Gets all paths that are children (direct or indirect) of the given path.
    ///
    /// # Arguments
    /// * `path` - Parent path to find children for
    /// * `all_paths` - All paths to search through
    ///
    /// # Returns
    /// Vector of child paths
    pub fn find_affected_children(path: &ShadowPath, all_paths: &[ShadowPath]) -> Vec<ShadowPath> {
        let path_str = path.to_string();
        
        all_paths
            .iter()
            .filter(|child_path| {
                let child_str = child_path.to_string();
                child_str != path_str && child_str.starts_with(&path_str)
            })
            .cloned()
            .collect()
    }
    
    /// Extracts the filename component from a path.
    ///
    /// # Arguments
    /// * `path` - Path to extract filename from
    ///
    /// # Returns
    /// The filename as a string, or empty string for root
    pub fn get_filename(path: &ShadowPath) -> String {
        path.file_name()
            .map(|name| name.to_string())
            .unwrap_or_default()
    }
    
    /// Checks if one path is a parent of another.
    ///
    /// # Arguments
    /// * `potential_parent` - Path that might be a parent
    /// * `potential_child` - Path that might be a child
    ///
    /// # Returns
    /// true if potential_parent is a parent of potential_child
    pub fn is_parent_of(potential_parent: &ShadowPath, potential_child: &ShadowPath) -> bool {
        let parent_str = potential_parent.to_string();
        let child_str = potential_child.to_string();
        
        if parent_str == child_str {
            return false;
        }
        
        child_str.starts_with(&parent_str) && 
        (parent_str.ends_with('/') || child_str.chars().nth(parent_str.len()) == Some('/'))
    }
    
    /// Checks if one path is an immediate parent of another.
    ///
    /// # Arguments
    /// * `potential_parent` - Path that might be an immediate parent
    /// * `potential_child` - Path that might be an immediate child
    ///
    /// # Returns
    /// true if potential_parent is the immediate parent of potential_child
    pub fn is_immediate_parent_of(potential_parent: &ShadowPath, potential_child: &ShadowPath) -> bool {
        potential_child.parent()
            .map(|parent| parent == *potential_parent)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_directory_cache_basic_operations() {
        let cache = DirectoryCache::new();
        let parent = ShadowPath::new("/test".into());
        
        // Initially no children
        assert!(cache.get_children(&parent).is_empty());
        assert!(!cache.has_children(&parent));
        assert!(!cache.has_child(&parent, "file.txt"));
        
        // Add children
        cache.add_child(&parent, "file1.txt");
        cache.add_child(&parent, "file2.txt");
        cache.add_child(&parent, "subdir");
        
        // Check children exist
        assert!(cache.has_children(&parent));
        assert!(cache.has_child(&parent, "file1.txt"));
        assert!(cache.has_child(&parent, "subdir"));
        assert!(!cache.has_child(&parent, "nonexistent"));
        
        let children = cache.get_children(&parent);
        assert_eq!(children.len(), 3);
        assert!(children.contains(&"file1.txt".to_string()));
        assert!(children.contains(&"file2.txt".to_string()));
        assert!(children.contains(&"subdir".to_string()));
    }
    
    #[test]
    fn test_directory_cache_remove_child() {
        let cache = DirectoryCache::new();
        let parent = ShadowPath::new("/test".into());
        
        cache.add_child(&parent, "file1.txt");
        cache.add_child(&parent, "file2.txt");
        
        // Remove existing child
        assert!(cache.remove_child(&parent, "file1.txt"));
        assert!(!cache.has_child(&parent, "file1.txt"));
        assert!(cache.has_child(&parent, "file2.txt"));
        
        // Remove non-existing child
        assert!(!cache.remove_child(&parent, "nonexistent"));
        
        // Remove last child should clean up parent entry
        assert!(cache.remove_child(&parent, "file2.txt"));
        assert!(!cache.has_children(&parent));
        assert_eq!(cache.directory_count(), 0);
    }
    
    #[test]
    fn test_directory_cache_clear_children() {
        let cache = DirectoryCache::new();
        let parent = ShadowPath::new("/test".into());
        
        cache.add_child(&parent, "file1.txt");
        cache.add_child(&parent, "file2.txt");
        cache.add_child(&parent, "subdir");
        
        let removed = cache.clear_children(&parent);
        assert_eq!(removed.len(), 3);
        assert!(!cache.has_children(&parent));
        assert_eq!(cache.directory_count(), 0);
    }
    
    #[test]
    fn test_directory_cache_statistics() {
        let cache = DirectoryCache::new();
        let parent1 = ShadowPath::new("/dir1".into());
        let parent2 = ShadowPath::new("/dir2".into());
        
        cache.add_child(&parent1, "file1.txt");
        cache.add_child(&parent1, "file2.txt");
        cache.add_child(&parent2, "file3.txt");
        
        assert_eq!(cache.directory_count(), 2);
        assert_eq!(cache.total_child_count(), 3);
        
        let all_parents = cache.get_all_parents();
        assert_eq!(all_parents.len(), 2);
        assert!(all_parents.contains(&parent1));
        assert!(all_parents.contains(&parent2));
    }
    
    #[test]
    fn test_path_traversal_parent_chain() {
        let path = ShadowPath::new("/a/b/c/d".into());
        let parents = PathTraversal::get_parent_chain(&path);
        
        assert_eq!(parents.len(), 4);
        assert_eq!(parents[0].to_string(), "/a/b/c");
        assert_eq!(parents[1].to_string(), "/a/b");
        assert_eq!(parents[2].to_string(), "/a");
        assert_eq!(parents[3].to_string(), "/");
    }
    
    #[test]
    fn test_path_traversal_find_affected_children() {
        let parent = ShadowPath::new("/test".into());
        let all_paths = vec![
            ShadowPath::new("/test/file1.txt".into()),
            ShadowPath::new("/test/subdir/file2.txt".into()),
            ShadowPath::new("/test/subdir".into()),
            ShadowPath::new("/other/file.txt".into()),
            ShadowPath::new("/test".into()), // Same as parent, should be excluded
        ];
        
        let children = PathTraversal::find_affected_children(&parent, &all_paths);
        
        assert_eq!(children.len(), 3);
        assert!(children.contains(&ShadowPath::new("/test/file1.txt".into())));
        assert!(children.contains(&ShadowPath::new("/test/subdir/file2.txt".into())));
        assert!(children.contains(&ShadowPath::new("/test/subdir".into())));
        assert!(!children.contains(&ShadowPath::new("/other/file.txt".into())));
        assert!(!children.contains(&parent));
    }
    
    #[test]
    fn test_path_traversal_get_filename() {
        assert_eq!(PathTraversal::get_filename(&ShadowPath::new("/test/file.txt".into())), "file.txt");
        assert_eq!(PathTraversal::get_filename(&ShadowPath::new("/test/subdir".into())), "subdir");
        assert_eq!(PathTraversal::get_filename(&ShadowPath::new("/".into())), "");
    }
    
    #[test]
    fn test_path_traversal_is_parent_of() {
        let parent = ShadowPath::new("/test".into());
        let child = ShadowPath::new("/test/file.txt".into());
        let grandchild = ShadowPath::new("/test/subdir/file.txt".into());
        let other = ShadowPath::new("/other/file.txt".into());
        
        assert!(PathTraversal::is_parent_of(&parent, &child));
        assert!(PathTraversal::is_parent_of(&parent, &grandchild));
        assert!(!PathTraversal::is_parent_of(&parent, &other));
        assert!(!PathTraversal::is_parent_of(&parent, &parent));
    }
    
    #[test]
    fn test_path_traversal_is_immediate_parent_of() {
        let parent = ShadowPath::new("/test".into());
        let child = ShadowPath::new("/test/file.txt".into());
        let grandchild = ShadowPath::new("/test/subdir/file.txt".into());
        
        assert!(PathTraversal::is_immediate_parent_of(&parent, &child));
        assert!(!PathTraversal::is_immediate_parent_of(&parent, &grandchild));
    }
}