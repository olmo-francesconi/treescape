//! Headless scan benchmark.
//!
//!   cargo run --release --example bench -- /some/path
//!
//! Spins up the scan, polls until `done == true`, prints wall-clock + counts.

use std::env;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

fn main() {
    let path: PathBuf = env::args()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: bench <path>");
    let path = path.canonicalize().expect("canonicalize");

    let t0 = std::time::Instant::now();
    let shared = treescape_core::scan::start_scan(path.clone());

    loop {
        let s = shared.lock().unwrap();
        if s.done {
            let elapsed = t0.elapsed();
            println!(
                "scanned {}\n  files  : {}\n  bytes  : {}\n  elapsed: {:.3}s\n  rate   : {:.0} files/s",
                path.display(),
                s.files_scanned(),
                s.bytes_scanned(),
                elapsed.as_secs_f64(),
                s.files_scanned() as f64 / elapsed.as_secs_f64()
            );
            return;
        }
        drop(s);
        thread::sleep(Duration::from_millis(50));
    }
}
