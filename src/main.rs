#![windows_subsystem = "windows"]

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use eframe::egui;
use rfd::FileDialog;
use walkdir::WalkDir;
use rayon::prelude::*;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}, Mutex};
use rayon::iter::ParallelBridge;

enum SearchMessage {
    Finished(usize),
    Error(String),
    Counter(usize),
    FileProcessed(usize),
}

struct MyApp {
    keywords: String,
    dropped_paths: Vec<PathBuf>,
    status: String,
    search_in_progress: bool,
    tx: Option<Sender<SearchMessage>>,
    rx: Option<Receiver<SearchMessage>>,
    output_dir: Option<PathBuf>,
    style_applied: bool,
    live_counter: usize,
    file_counter: usize,
    case_sensitive: bool,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            keywords: String::new(),
            dropped_paths: Vec::new(),
            status: "Ready".to_owned(),
            search_in_progress: false,
            tx: None,
            rx: None,
            output_dir: None,
            style_applied: false,
            live_counter: 0,
            file_counter: 0,
            case_sensitive: false,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.style_applied {
            let mut style = (*ctx.style()).clone();
            let mut visuals = egui::Visuals::dark();
            visuals.window_corner_radius = egui::CornerRadius::same(10);
            visuals.widgets.inactive = egui::style::WidgetVisuals {
                bg_fill: egui::Color32::from_rgb(30, 30, 30),
                ..visuals.widgets.inactive.clone()
            };
            visuals.widgets.hovered = egui::style::WidgetVisuals {
                bg_fill: egui::Color32::from_rgb(50, 50, 50),
                ..visuals.widgets.hovered.clone()
            };
            visuals.widgets.active = egui::style::WidgetVisuals {
                bg_fill: egui::Color32::from_rgb(70, 70, 70),
                ..visuals.widgets.active.clone()
            };
            style.visuals = visuals;
            ctx.set_style(style);
            self.style_applied = true;
        }

        for dropped in &ctx.input(|i| i.raw.dropped_files.clone()) {
            if let Some(path) = &dropped.path {
                if !self.dropped_paths.contains(path) {
                    self.dropped_paths.push(path.clone());
                }
            }
        }

        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    SearchMessage::Finished(total_matches) => {
                        self.status = format!("Search complete. Found {} matching lines.", total_matches);
                        self.search_in_progress = false;
                        self.live_counter = total_matches;
                    }
                    SearchMessage::Error(err) => {
                        self.status = format!("Error: {}", err);
                        self.search_in_progress = false;
                    }
                    SearchMessage::Counter(count) => {
                        self.live_counter = count;
                    }
                    SearchMessage::FileProcessed(file_count) => {
                        self.file_counter = file_count;
                    }
                }
            }
        }

        egui::SidePanel::left("file_panel").min_width(250.0).show(ctx, |ui| {
            ui.heading("Files");
            ui.label("Drag & drop files anywhere, or use the buttons below.");

            if ui.button("Browse Files").clicked() {
                if let Some(paths) = FileDialog::new()
                    .add_filter("Text", &["txt"])
                    .set_title("Select Files")
                    .pick_files()
                {
                    for path in paths {
                        if !self.dropped_paths.contains(&path) {
                            self.dropped_paths.push(path);
                        }
                    }
                }
            }

            if ui.button("Browse Folder").clicked() {
                if let Some(path) = FileDialog::new()
                    .set_title("Select Folder")
                    .pick_folder()
                {
                    if !self.dropped_paths.contains(&path) {
                        self.dropped_paths.push(path);
                    }
                }
            }

            ui.separator();
            ui.label("Selected Files/Directories:");

            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                let mut indices_to_remove = Vec::new();
                for (i, path) in self.dropped_paths.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(path.display().to_string());
                        if ui.button("X")
                            .on_hover_text("Remove this file")
                            .clicked()
                        {
                            indices_to_remove.push(i);
                        }
                    });
                }
                for i in indices_to_remove.into_iter().rev() {
                    self.dropped_paths.remove(i);
                }
            });

            if ui.button("Clear All Files").clicked() {
                self.dropped_paths.clear();
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("seekr");
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.label("Keywords (; separated):");
                    let keywords_edit = egui::TextEdit::singleline(&mut self.keywords)
                        .hint_text("Example: error;warning;critical failure");
                    ui.add(keywords_edit);
                });
                
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Case Sensitive Search:").strong());
                    ui.add_space(10.0);
                    ui.checkbox(&mut self.case_sensitive, "")
                        .on_hover_text("Enable case sensitive search");
                });

                ui.add_space(10.0);

                if ui.button("Select Output Directory").clicked() {
                    if let Some(dir) = FileDialog::new().set_title("Select Output Directory").pick_folder() {
                        self.output_dir = Some(dir);
                    }
                }

                if let Some(ref dir) = self.output_dir {
                    ui.label(format!("Output Directory: {}", dir.display()));
                } else {
                    ui.label("No output directory selected");
                }

                ui.add_space(10.0);

                if self.search_in_progress {
                    ui.add_enabled(false, egui::Button::new("Search"));
                } else if ui.button("Search").clicked() {
                    if self.keywords.trim().is_empty() {
                        self.status = "Please enter a search keyword.".to_owned();
                    } else if self.dropped_paths.is_empty() {
                        self.status = "Please select at least one file or directory.".to_owned();
                    } else if self.output_dir.is_none() {
                        self.status = "Please select an output directory.".to_owned();
                    } else {
                        self.start_search();
                    }
                }

                ui.add_space(10.0);
                ui.label(format!("Matching Lines: {}", self.live_counter));
                ui.label(format!("Files Processed: {}", self.file_counter));
                ui.label(format!("Status: {}", self.status));
            });
        });
    }
}

impl MyApp {
    fn start_search(&mut self) {
        self.status = "Search in progress...".to_owned();
        self.search_in_progress = true;
        self.live_counter = 0;
        self.file_counter = 0;

        let (tx, rx) = mpsc::channel();
        self.tx = Some(tx.clone());
        self.rx = Some(rx);

        let keywords: Vec<String> = self.keywords
            .split(';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if keywords.is_empty() {
            self.status = "Please enter at least one search keyword.".to_owned();
            self.search_in_progress = false;
            return;
        }

        let cs = self.case_sensitive;

        let paths = self.dropped_paths.clone();
        let output_dir = self.output_dir.clone().unwrap();
        let output_file_path = output_dir.join("output.txt");

        let global_match_counter = Arc::new(AtomicUsize::new(0));
        let global_file_counter = Arc::new(AtomicUsize::new(0));

        thread::spawn(move || {
            let output_file = match File::create(&output_file_path) {
                Ok(file) => Arc::new(Mutex::new(file)),
                Err(e) => {
                    let _ = tx.send(SearchMessage::Error(format!("Failed to create {}: {}", output_file_path.display(), e)));
                    return;
                }
            };
            let mut total_matches = 0;
            
            for path in paths {
                if path.is_dir() {
                    let counts: Vec<usize> = WalkDir::new(&path)
                        .into_iter()
                        .filter_map(|entry| entry.ok())
                        .filter(|e| {
                            e.file_type().is_file() &&
                            e.path() != output_file_path.as_path() &&
                            e.path()
                                .extension()
                                .map(|ext| ext.to_string_lossy().to_lowercase() == "txt")
                                .unwrap_or(false)
                        })
                        .par_bridge()
                        .map(|e| {
                            process_file_write(
                                e.path(),
                                &keywords,
                                &output_file,
                                &tx,
                                &global_match_counter,
                                &global_file_counter,
                                cs
                            ).unwrap_or(0)
                        })
                        .collect();
                    for count in counts { total_matches += count; }
                } else if path.is_file() &&
                    path != output_file_path &&
                    path.extension()
                        .map(|ext| ext.to_string_lossy().to_lowercase() == "txt")
                        .unwrap_or(false)
                {
                    let count = process_file_write(
                        &path,
                        &keywords,
                        &output_file,
                        &tx,
                        &global_match_counter,
                        &global_file_counter,
                        cs
                    ).unwrap_or(0);
                    total_matches += count;
                }
            }
            let _ = tx.send(SearchMessage::Finished(total_matches));
        });
    }
}

fn process_file_write(
    file_path: &Path,
    keywords: &[String],
    output_file: &Arc<Mutex<File>>,
    tx: &Sender<SearchMessage>,
    global_match_counter: &Arc<AtomicUsize>,
    global_file_counter: &Arc<AtomicUsize>,
    case_sensitive: bool
) -> Result<usize, Box<dyn std::error::Error>> {
    let file_count = global_file_counter.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = tx.send(SearchMessage::FileProcessed(file_count));

    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut match_count = 0;
    for line_result in reader.lines() {
        let line = line_result?;
        let line_to_check = if case_sensitive { line.clone() } else { line.to_lowercase() };
        if keywords.iter().any(|k| {
            if case_sensitive {
                line.contains(k)
            } else {
                line_to_check.contains(&k.to_lowercase())
            }
        }) {
            let match_index = global_match_counter.fetch_add(1, Ordering::SeqCst) + 1;
            {
                let mut out = output_file.lock().unwrap();
                 writeln!(out, "{}. {}: {}", match_index, file_path.display(), line)?;
                out.flush().ok();
            }
            match_count += 1;
            let _ = tx.send(SearchMessage::Counter(match_index));
        }
    }
    Ok(match_count)
}

fn main() {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "seekr",
        native_options,
        Box::new(|_cc| Ok(Box::new(MyApp::default()))),
    ).unwrap_or_else(|e| eprintln!("Error running application: {}", e));
}