use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    /// Absolute path on disk. Used by tile-color hashing.
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
    /// True when this entry is a symlink. treescape never follows symlinks —
    /// the node is leaf-like (no children, tiny size = the link inode's
    /// footprint) and the renderer distinguishes it with a `→` glyph.
    pub is_link: bool,
    /// Children are `Arc<Node>` so the incremental scan builder can rebuild
    /// a single directory without re-cloning every descendant. Sibling
    /// subtrees are shared across publish ticks by Arc reference.
    pub children: Vec<Arc<Node>>,
}

impl Node {
    pub fn new_file(path: PathBuf, size: u64) -> Self {
        let name = leaf_name(&path);
        Self {
            name,
            path,
            size,
            is_dir: false,
            is_link: false,
            children: Vec::new(),
        }
    }

    /// Symlink that was *not* descended into. Reported size is the symlink
    /// inode's own footprint (usually <100 bytes).
    pub fn new_link(path: PathBuf, size: u64) -> Self {
        let name = leaf_name(&path);
        Self {
            name,
            path,
            size,
            is_dir: false,
            is_link: true,
            children: Vec::new(),
        }
    }

    pub fn new_dir(path: PathBuf, children: Vec<Arc<Node>>) -> Self {
        let name = leaf_name(&path);
        let size = children.iter().map(|c| c.size).sum();
        let mut node = Self {
            name,
            path,
            size,
            is_dir: true,
            is_link: false,
            children,
        };
        node.children
            .sort_by(|a, b| b.size.cmp(&a.size).then(a.name.cmp(&b.name)));
        node
    }

    pub fn total_count(&self) -> usize {
        let mut stack: Vec<&Node> = vec![self];
        let mut n = 0;
        while let Some(node) = stack.pop() {
            n += 1;
            stack.extend(node.children.iter().map(|c| c.as_ref()));
        }
        n
    }
}

fn leaf_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, size: u64) -> Arc<Node> {
        Arc::new(Node::new_file(PathBuf::from(name), size))
    }

    #[test]
    fn new_dir_sums_child_sizes() {
        let dir = Node::new_dir(
            PathBuf::from("/d"),
            vec![file("a", 10), file("b", 20), file("c", 30)],
        );
        assert_eq!(dir.size, 60);
        assert!(dir.is_dir);
        assert!(!dir.is_link);
    }

    #[test]
    fn new_dir_sorts_children_desc_by_size() {
        let dir = Node::new_dir(
            PathBuf::from("/d"),
            vec![file("small", 1), file("big", 100), file("mid", 50)],
        );
        let names: Vec<&str> = dir.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["big", "mid", "small"]);
    }

    #[test]
    fn new_dir_breaks_size_ties_alphabetically() {
        let dir = Node::new_dir(
            PathBuf::from("/d"),
            vec![file("zeta", 10), file("alpha", 10), file("mike", 10)],
        );
        let names: Vec<&str> = dir.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "mike", "zeta"]);
    }

    #[test]
    fn new_link_is_leaf_marked_as_link() {
        let link = Node::new_link(PathBuf::from("/some/link"), 42);
        assert!(link.is_link);
        assert!(!link.is_dir);
        assert_eq!(link.size, 42);
        assert!(link.children.is_empty());
    }

    #[test]
    fn total_count_includes_self_and_descendants() {
        let sub = Arc::new(Node::new_dir(
            PathBuf::from("/r/sub"),
            vec![file("b", 1), file("c", 1)],
        ));
        let root = Node::new_dir(PathBuf::from("/r"), vec![file("a", 1), sub]);
        assert_eq!(root.total_count(), 5);
    }

    #[test]
    fn total_count_iterative_handles_deep_tree() {
        let mut node: Arc<Node> = Arc::new(Node::new_file(PathBuf::from("/leaf"), 1));
        for i in 0..2000 {
            node = Arc::new(Node::new_dir(PathBuf::from(format!("/d{i}")), vec![node]));
        }
        assert_eq!(node.total_count(), 2001);
    }
}
