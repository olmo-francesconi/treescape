use crate::tree::Node;
use jwalk::WalkDir;
use std::{
    cmp::Reverse,
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

/// Counters updated by the walker thread in real time. Lock-free reads from
/// the UI so the file/byte tally ticks up smoothly between tree publishes.
#[derive(Default)]
pub struct ScanCounters {
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    /// Filesystem entries we couldn't read (walk error or metadata failure).
    /// Surfaced in the footer so users know the sizes are undercounting.
    pub unreadable: AtomicU64,
}

pub struct ScanShared {
    pub tree: Arc<Node>,
    pub counters: Arc<ScanCounters>,
    pub current_path: Option<PathBuf>,
    pub done: bool,
    pub started_at: Instant,
    pub finished_at: Option<Instant>,
}

impl ScanShared {
    fn empty(root_path: PathBuf, counters: Arc<ScanCounters>) -> Self {
        let tree = Node::new_dir(root_path, Vec::new());
        Self {
            tree: Arc::new(tree),
            counters,
            current_path: None,
            done: false,
            started_at: Instant::now(),
            finished_at: None,
        }
    }

    pub fn files_scanned(&self) -> u64 {
        self.counters.files.load(Ordering::Relaxed)
    }
    pub fn bytes_scanned(&self) -> u64 {
        self.counters.bytes.load(Ordering::Relaxed)
    }
    pub fn unreadable(&self) -> u64 {
        self.counters.unreadable.load(Ordering::Relaxed)
    }
}

/// One filesystem entry handed from walker → builder.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EntryKind {
    Dir,
    File,
    Link,
}

#[derive(Debug, Clone)]
struct EntryDelta {
    path: PathBuf,
    size: u64,
    kind: EntryKind,
}

struct WalkerOut {
    pending: Vec<EntryDelta>,
    last_seen: Option<PathBuf>,
}

pub fn start_scan(root: PathBuf) -> Arc<Mutex<ScanShared>> {
    let counters: Arc<ScanCounters> = Arc::new(ScanCounters::default());
    let shared = Arc::new(Mutex::new(ScanShared::empty(
        root.clone(),
        counters.clone(),
    )));
    let walker_out = Arc::new(Mutex::new(WalkerOut {
        pending: Vec::new(),
        last_seen: None,
    }));
    let (done_tx, done_rx) = mpsc::channel::<()>();

    spawn_walker(root.clone(), walker_out.clone(), counters, done_tx);
    spawn_builder(root, walker_out, done_rx, shared.clone());

    shared
}

fn spawn_walker(
    root: PathBuf,
    walker_out: Arc<Mutex<WalkerOut>>,
    counters: Arc<ScanCounters>,
    done_tx: mpsc::Sender<()>,
) {
    thread::spawn(move || {
        // Symlinks are never followed: treescape answers "where do bytes live
        // on disk", and following links double-counts the target's bytes
        // (or worse, hangs on cycles). Links are surfaced as their own
        // small leaf nodes instead. Hidden files are always scanned so
        // parent totals stay honest; the UI filters them at render time.
        let walker = WalkDir::new(&root)
            .follow_links(false)
            .skip_hidden(false)
            .sort(false);

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    counters.unreadable.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
            };
            let path = entry.path();
            let ft = entry.file_type();
            let (size, kind, had_meta_err) = if ft.is_dir() {
                (0u64, EntryKind::Dir, false)
            } else if ft.is_file() {
                match entry.metadata() {
                    Ok(m) => (m.len(), EntryKind::File, false),
                    Err(_) => (0, EntryKind::File, true),
                }
            } else if ft.is_symlink() {
                // Symlink that the walker chose not to follow (default mode,
                // or --follow-links with a broken target). Report it as a
                // small leaf so users can see it exists. symlink_metadata
                // returns the link inode's own size, not the target's.
                let size = std::fs::symlink_metadata(&path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                (size, EntryKind::Link, false)
            } else {
                continue;
            };

            match kind {
                EntryKind::Dir => {}
                EntryKind::File | EntryKind::Link => {
                    // Count links toward the file tally — they're entries
                    // we scanned — but their tiny size barely moves bytes.
                    counters.files.fetch_add(1, Ordering::Relaxed);
                    counters.bytes.fetch_add(size, Ordering::Relaxed);
                }
            }
            if had_meta_err {
                counters.unreadable.fetch_add(1, Ordering::Relaxed);
            }

            let mut w = walker_out.lock().unwrap();
            w.pending.push(EntryDelta {
                path: path.clone(),
                size,
                kind,
            });
            w.last_seen = Some(path);
        }

        let _ = done_tx.send(());
    });
}

fn spawn_builder(
    root: PathBuf,
    walker_out: Arc<Mutex<WalkerOut>>,
    done_rx: mpsc::Receiver<()>,
    shared: Arc<Mutex<ScanShared>>,
) {
    thread::spawn(move || {
        let mut builder = IncrementalTree::new(root.clone());
        let mut walker_finished = false;

        loop {
            if !walker_finished {
                // PERF: builder no longer rebuilds the whole tree per tick,
                // so even short intervals are cheap. Counters update live via
                // atomics so we don't need a fast tree publish to keep the
                // footer's "N files" feeling responsive.
                let files_so_far = shared.lock().unwrap().files_scanned();
                let interval = if files_so_far < 500_000 { 250 } else { 500 };
                match done_rx.recv_timeout(Duration::from_millis(interval)) {
                    Ok(_) | Err(RecvTimeoutError::Disconnected) => {
                        walker_finished = true;
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                }
            }

            let (deltas, last_seen) = {
                let mut w = walker_out.lock().unwrap();
                let deltas = std::mem::take(&mut w.pending);
                (deltas, w.last_seen.clone())
            };

            for d in deltas {
                builder.apply(d);
            }

            let tree = builder.publish();

            let mut s = shared.lock().unwrap();
            s.tree = tree;
            s.current_path = if walker_finished { None } else { last_seen };
            if walker_finished {
                s.done = true;
                s.finished_at = Some(Instant::now());
                return;
            }
        }
    });
}

/// Persistent in-memory representation of the scan tree.
///
/// Applying a delta touches only the spine from the new entry up to the
/// root (marking ancestors dirty). Publish rebuilds only the directories
/// whose contents changed since the last publish; the rest of the tree is
/// shared by `Arc<Node>` reference.
///
/// Per-tick cost: O(K · depth + dirty · branching) where K is the number
/// of new deltas this tick, instead of the previous O(N · log N) full
/// rebuild.
struct IncrementalTree {
    root: PathBuf,
    /// Latest Arc<Node> for every path we've materialized.
    nodes: HashMap<PathBuf, Arc<Node>>,
    /// Adjacency: parent path → child paths in insertion order.
    /// jwalk emits each entry once, so we don't need to dedupe.
    children: HashMap<PathBuf, Vec<PathBuf>>,
    /// Dirs whose Arc<Node> needs rebuilding. Ordered by `Reverse(depth)`
    /// so `pop_first` yields the deepest dirty dir first — children are
    /// always fresh by the time we rebuild their parent.
    dirty: BTreeSet<(Reverse<usize>, PathBuf)>,
}

impl IncrementalTree {
    fn new(root: PathBuf) -> Self {
        let mut t = Self {
            root: root.clone(),
            nodes: HashMap::new(),
            children: HashMap::new(),
            dirty: BTreeSet::new(),
        };
        t.children.insert(root.clone(), Vec::new());
        t.dirty.insert((Reverse(depth(&root)), root));
        t
    }

    fn apply(&mut self, delta: EntryDelta) {
        match delta.kind {
            EntryKind::Dir => {
                self.children.entry(delta.path.clone()).or_default();
            }
            EntryKind::File => {
                self.nodes.insert(
                    delta.path.clone(),
                    Arc::new(Node::new_file(delta.path.clone(), delta.size)),
                );
            }
            EntryKind::Link => {
                self.nodes.insert(
                    delta.path.clone(),
                    Arc::new(Node::new_link(delta.path.clone(), delta.size)),
                );
            }
        }

        // The walker emits the root itself as a dir entry. It has no parent
        // under our control — just mark it dirty.
        if delta.path == self.root {
            let root = self.root.clone();
            self.mark_dirty(&root);
            return;
        }

        if let Some(parent) = delta.path.parent() {
            let parent = parent.to_path_buf();
            self.children
                .entry(parent.clone())
                .or_default()
                .push(delta.path);
            self.mark_dirty(&parent);
        }
    }

    fn mark_dirty(&mut self, path: &Path) {
        let mut p = path.to_path_buf();
        loop {
            self.dirty.insert((Reverse(depth(&p)), p.clone()));
            if p == self.root {
                break;
            }
            match p.parent() {
                Some(parent) => p = parent.to_path_buf(),
                None => break,
            }
        }
    }

    fn publish(&mut self) -> Arc<Node> {
        // Drain dirty deepest-first so each parent sees fresh child Arcs.
        while let Some((_, path)) = self.dirty.pop_first() {
            let kids: Vec<Arc<Node>> = self
                .children
                .get(&path)
                .map(|cs| {
                    cs.iter()
                        .filter_map(|c| self.nodes.get(c).cloned())
                        .collect()
                })
                .unwrap_or_default();
            let node = Arc::new(Node::new_dir(path.clone(), kids));
            self.nodes.insert(path, node);
        }
        self.nodes
            .get(&self.root)
            .cloned()
            .unwrap_or_else(|| Arc::new(Node::new_dir(self.root.clone(), Vec::new())))
    }
}

fn depth(p: &Path) -> usize {
    p.components().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(path: &str, size: u64, is_dir: bool) -> EntryDelta {
        EntryDelta {
            path: PathBuf::from(path),
            size,
            kind: if is_dir {
                EntryKind::Dir
            } else {
                EntryKind::File
            },
        }
    }

    fn link_delta(path: &str, size: u64) -> EntryDelta {
        EntryDelta {
            path: PathBuf::from(path),
            size,
            kind: EntryKind::Link,
        }
    }

    /// Helper: build a tree from a fixed delta sequence using IncrementalTree.
    fn build(deltas: Vec<EntryDelta>, root: &str) -> Arc<Node> {
        let mut t = IncrementalTree::new(PathBuf::from(root));
        for d in deltas {
            t.apply(d);
        }
        t.publish()
    }

    #[test]
    fn empty_scan_yields_empty_root() {
        let t = build(vec![delta("/r", 0, true)], "/r");
        assert!(t.is_dir);
        assert_eq!(t.children.len(), 0);
        assert_eq!(t.size, 0);
    }

    #[test]
    fn single_file_under_root() {
        let t = build(
            vec![delta("/r", 0, true), delta("/r/a.txt", 100, false)],
            "/r",
        );
        assert_eq!(t.size, 100);
        assert_eq!(t.children.len(), 1);
        assert_eq!(t.children[0].name, "a.txt");
        assert_eq!(t.children[0].size, 100);
    }

    #[test]
    fn nested_dirs_aggregate_sizes() {
        // /r
        //   /r/sub (dir)
        //     /r/sub/a (10)
        //     /r/sub/b (20)
        //   /r/c (30)
        let t = build(
            vec![
                delta("/r", 0, true),
                delta("/r/sub", 0, true),
                delta("/r/sub/a", 10, false),
                delta("/r/sub/b", 20, false),
                delta("/r/c", 30, false),
            ],
            "/r",
        );
        assert_eq!(t.size, 60);
        // Sorted desc by size: sub (30) then c (30) — same size, alpha tiebreak: c < sub
        let names: Vec<&str> = t.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["c", "sub"]);
        let sub = t.children.iter().find(|c| c.name == "sub").unwrap();
        assert_eq!(sub.size, 30);
        assert_eq!(sub.children.len(), 2);
    }

    #[test]
    fn deltas_in_arbitrary_order_produce_same_tree() {
        // jwalk gives parent-before-child, but the builder shouldn't care.
        let interleaved = build(
            vec![
                delta("/r", 0, true),
                delta("/r/sub/a", 10, false), // leaf before dir
                delta("/r/sub", 0, true),
                delta("/r/sub/b", 20, false),
            ],
            "/r",
        );
        assert_eq!(interleaved.size, 30);
        let sub = interleaved
            .children
            .iter()
            .find(|c| c.name == "sub")
            .unwrap();
        assert_eq!(sub.size, 30);
        assert_eq!(sub.children.len(), 2);
    }

    #[test]
    fn second_publish_picks_up_new_deltas() {
        let mut t = IncrementalTree::new(PathBuf::from("/r"));
        t.apply(delta("/r", 0, true));
        t.apply(delta("/r/a", 10, false));
        let first = t.publish();
        assert_eq!(first.size, 10);

        t.apply(delta("/r/b", 20, false));
        let second = t.publish();
        assert_eq!(second.size, 30);
        assert_eq!(second.children.len(), 2);

        // First publish's Arc should still see its original size — Arc-shared
        // immutability means past snapshots aren't mutated.
        assert_eq!(first.size, 10);
        assert_eq!(first.children.len(), 1);
    }

    #[test]
    fn link_delta_becomes_leaf_node_with_is_link_set() {
        let mut t = IncrementalTree::new(PathBuf::from("/r"));
        t.apply(delta("/r", 0, true));
        t.apply(link_delta("/r/symlink", 12));
        let tree = t.publish();
        let link = tree.children.iter().find(|c| c.name == "symlink").unwrap();
        assert!(link.is_link);
        assert!(!link.is_dir);
        assert_eq!(link.size, 12);
        assert!(link.children.is_empty());
    }

    #[test]
    fn unchanged_subtree_is_shared_between_publishes() {
        // After tick 1, /r/sub has one file. Tick 2 adds an unrelated file
        // at /r/other. /r/sub's Arc<Node> should be reused (same pointer).
        let mut t = IncrementalTree::new(PathBuf::from("/r"));
        t.apply(delta("/r", 0, true));
        t.apply(delta("/r/sub", 0, true));
        t.apply(delta("/r/sub/a", 10, false));
        let tick1 = t.publish();
        let sub1 = tick1
            .children
            .iter()
            .find(|c| c.name == "sub")
            .cloned()
            .unwrap();

        t.apply(delta("/r/other", 5, false));
        let tick2 = t.publish();
        let sub2 = tick2
            .children
            .iter()
            .find(|c| c.name == "sub")
            .cloned()
            .unwrap();

        assert!(
            Arc::ptr_eq(&sub1, &sub2),
            "untouched subtree should share Arc across publishes"
        );
    }
}
