// Standalone benchmark for seekr's parallel search core.
// Generates a synthetic corpus of .txt files, then times the parallel keyword scan.
// Run with: cargo run --release --bin bench

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write, BufWriter};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rayon::iter::ParallelBridge;
use rayon::prelude::*;
use walkdir::WalkDir;

const NUM_FILES: usize = 10_000;
const LINES_PER_FILE: usize = 500;

fn generate_corpus(dir: &Path) -> u64 {
    fs::create_dir_all(dir).unwrap();
    // Mostly non-matching filler with a rare "needle" injected on ~1% of lines,
    // matching realistic keyword-search hit rates (measures scan speed, not write contention).
    let filler = "the quick brown fox jumps over the lazy dog near the riverbank at dawn";
    let needle = "ERROR connection refused while reaching upstream service node";
    let mut total_bytes = 0u64;
    for f in 0..NUM_FILES {
        let path = dir.join(format!("file_{f:05}.txt"));
        let file = File::create(&path).unwrap();
        let mut w = BufWriter::new(file);
        for l in 0..LINES_PER_FILE {
            let line = if (f * LINES_PER_FILE + l) % 100 == 0 { needle } else { filler };
            writeln!(w, "{line}").unwrap();
            total_bytes += line.len() as u64 + 1;
        }
    }
    total_bytes
}

fn scan_file(
    path: &Path,
    keywords: &[String],
    out: &Arc<Mutex<File>>,
    matches: &Arc<AtomicUsize>,
) -> usize {
    let reader = BufReader::new(File::open(path).unwrap());
    let mut local = 0;
    for line in reader.lines() {
        let line = line.unwrap();
        let lc = line.to_lowercase();
        if keywords.iter().any(|k| lc.contains(k)) {
            let idx = matches.fetch_add(1, Ordering::SeqCst) + 1;
            let mut o = out.lock().unwrap();
            writeln!(o, "{idx}. {}: {line}", path.display()).unwrap();
            local += 1;
        }
    }
    local
}

fn main() {
    let dir = std::env::temp_dir().join("seekr_bench_corpus");
    println!("Generating corpus: {NUM_FILES} files x {LINES_PER_FILE} lines ...");
    let total_bytes = generate_corpus(&dir);
    let total_mb = total_bytes as f64 / (1024.0 * 1024.0);

    let keywords: Vec<String> = ["error", "warning", "critical failure"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    let out_path = dir.join("output.txt");
    let out = Arc::new(Mutex::new(File::create(&out_path).unwrap()));
    let matches = Arc::new(AtomicUsize::new(0));

    let threads = rayon::current_num_threads();
    println!("Scanning with {threads} threads ...");

    let start = Instant::now();
    let total: usize = WalkDir::new(&dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path() != out_path.as_path()
                && e.path().extension().map(|x| x == "txt").unwrap_or(false)
        })
        .par_bridge()
        .map(|e| scan_file(e.path(), &keywords, &out, &matches))
        .sum();
    let elapsed = start.elapsed();

    let secs = elapsed.as_secs_f64();
    println!("\n=== seekr benchmark ===");
    println!("Files scanned : {NUM_FILES}");
    println!("Total size    : {total_mb:.1} MB");
    println!("Matches found : {total}");
    println!("Threads       : {threads}");
    println!("Elapsed       : {secs:.3} s");
    println!("Throughput    : {:.0} files/s, {:.1} MB/s", NUM_FILES as f64 / secs, total_mb / secs);

    let _ = fs::remove_dir_all(&dir);
}
