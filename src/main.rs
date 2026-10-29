// Hide the console window in Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod control;
mod metronome;
mod midi;
mod playback;
mod render;
mod synth;
mod widgets;

use std::path::PathBuf;

// The pure modules live in the core crate; this keeps their `crate::name` paths working here.
use roll_model_core::{analysis, config, controller, controller_layout, drums, loader, musicxml, rudiments, score, trigger};

fn main() -> eframe::Result<()> {
    // Optional: a MusicXML file to open on start-up.
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Roll Model")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([760.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Roll Model",
        options,
        Box::new(move |cc| Ok(Box::new(app::DrumApp::new(cc, initial)))),
    )
}
