//! Non-interactive scan bench. Walks a path and reports publish-thread
//! behaviour: total wall time, peak file/byte counts, number of publishes,
//! and longest gap between publishes.
//!
//! Run: `cargo run --release --example scan_bench -- /path/to/scan`

use std::{
    env,
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use treescape_core::scan;

fn main() {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let root = path.canonicalize().expect("cannot canonicalize path");

    println!("scanning {} …", root.display());
    let state = scan::start_scan(root);
    let started = Instant::now();

    let mut publishes: u64 = 0;
    let mut last_tree_ptr: Option<*const _> = None;
    let mut last_publish_at = Instant::now();
    let mut max_gap = Duration::ZERO;

    loop {
        thread::sleep(Duration::from_millis(20));
        let s = state.lock().unwrap();
        let cur_ptr = Arc::as_ptr(&s.tree);

        if last_tree_ptr != Some(cur_ptr) {
            publishes += 1;
            let now = Instant::now();
            let gap = now.duration_since(last_publish_at);
            if gap > max_gap {
                max_gap = gap;
            }
            last_publish_at = now;
            last_tree_ptr = Some(cur_ptr);
        }

        if s.done {
            let elapsed = started.elapsed();
            let top: Vec<(String, u64)> = s
                .tree
                .children
                .iter()
                .take(5)
                .map(|c| (c.name.clone(), c.size))
                .collect();
            let n_dirs = count_dirs(&s.tree);

            println!();
            println!("── done in {:.2}s ─────────────────", elapsed.as_secs_f64());
            println!("  files       : {}", s.files_scanned());
            println!(
                "  bytes       : {}",
                humansize::format_size(s.bytes_scanned(), humansize::BINARY)
            );
            println!("  unreadable  : {}", s.unreadable());
            println!("  total nodes : {}", s.tree.total_count());
            println!("  directories : {}", n_dirs);
            println!("  publishes   : {}", publishes);
            println!(
                "  avg pub gap : {:.1}ms",
                elapsed.as_millis() as f64 / publishes.max(1) as f64
            );
            println!("  max pub gap : {:.1}ms", max_gap.as_millis() as f64);
            println!();
            println!("  top children of root (by size):");
            for (name, size) in top {
                println!(
                    "    {:>10}  {}",
                    humansize::format_size(size, humansize::BINARY),
                    name
                );
            }
            return;
        }
    }
}

fn count_dirs(node: &treescape_core::tree::Node) -> u64 {
    let mut stack = vec![node];
    let mut n = 0;
    while let Some(node) = stack.pop() {
        if node.is_dir {
            n += 1;
        }
        stack.extend(node.children.iter().map(|c| c.as_ref()));
    }
    n
}
