# seekr

A fast, cross-platform desktop app for searching large collections of `.txt` files for keywords, built in Rust with [egui](https://github.com/emilk/egui)/[eframe](https://github.com/emilk/egui/tree/master/crates/eframe). Point it at files or whole folders, give it a list of keywords, and it scans every matching line in parallel across all your CPU cores and writes the results to a single output file.

## Features

- **Drag & drop** files and folders, or pick them with native file/folder dialogs
- **Multi-keyword search** — separate keywords with `;` (matches any keyword)
- **Recursive folder search** — walks subdirectories automatically
- **Parallel processing** — files are scanned concurrently using [rayon](https://github.com/rayon-rs/rayon)
- **Live progress** — matching-line and files-processed counters update in real time while the search runs
- **Case-sensitive toggle**
- **Aggregated output** — every match is written to `output.txt` as `N. <file path>: <matching line>`
- Dark, rounded UI

## Demo

1. Add one or more files/folders (drag & drop, **Browse Files**, or **Browse Folder**).
2. Enter keywords separated by `;`, e.g. `error;warning;critical failure`.
3. Click **Select Output Directory**.
4. Click **Search**. Results stream into `output.txt` in your chosen directory.

## Build & Run

Requires a [Rust toolchain](https://rustup.rs/).

```bash
git clone <repo-url>
cd seekr
cargo run --release
```

> **macOS note:** run with `--release`. A strict runtime type-check in an older transitive dependency (`objc2`, pulled in via `eframe 0.31`) aborts on recent macOS in debug builds; the check is compiled out in release builds.

## How it works

- The GUI runs on the main thread; each search spawns a background worker thread so the UI stays responsive.
- Folders are traversed with [walkdir](https://github.com/BurntSushi/walkdir), filtered to `.txt` files, and processed in parallel via `rayon`'s `par_bridge`.
- Each file is read line by line; a line is a match if it contains any of the keywords (substring match).
- A shared, mutex-guarded handle appends matches to `output.txt`; an atomic counter assigns each match a number.
- Progress and results are sent back to the UI over an `mpsc` channel.

## Limitations

- Only `.txt` files are searched; other extensions are skipped.
- Output is always `output.txt` and is **overwritten** on each run.
- Matching is substring-based (not whole-word or regex).
- With parallel folder searches, the order of lines in `output.txt` is non-deterministic.

## Tech stack

Rust · egui/eframe (GUI) · rayon (parallelism) · walkdir (traversal) · rfd (native file dialogs)
