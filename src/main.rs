// Hide the console window in Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod analysis;
mod app;
mod config;
mod control;
mod drums;
mod loader;
mod metronome;
mod midi;
mod musicxml;
mod playback;
mod render;
mod score;
mod synth;
mod widgets;
mod xml;

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    // Optional: a MusicXML file to open on start-up.
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Drum Tutor")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([760.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Drum Tutor",
        options,
        Box::new(move |cc| Ok(Box::new(app::DrumApp::new(cc, initial)))),
    )
}
