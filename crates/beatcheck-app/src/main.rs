#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod brand;
mod config;
mod export_panel;
mod hud;
mod launch;
mod waveform;

use app::BeatCheckApp;
use beatcheck_audio::AudioPlayer;
use std::path::PathBuf;
use std::sync::Arc;

fn main() -> eframe::Result<()> {
    // Ignore OS flags such as macOS LaunchServices `-psn_0_...`
    let initial_file = std::env::args()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(PathBuf::from);

    // Initialize Windows WASAPI / CoreAudio stream BEFORE winit initializes STA COM on the UI thread!
    let player = AudioPlayer::new();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("BeatCheck — Cut on the beat. Every frame.")
            .with_icon(Arc::new(brand::load_window_icon()))
            .with_inner_size([1380.0, 820.0])
            .with_min_inner_size([980.0, 620.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    let mut player_slot = Some(player);
    eframe::run_native(
        brand::BRAND_NAME,
        native_options,
        Box::new(move |cc| {
            let p = player_slot.take().unwrap_or_else(AudioPlayer::new);
            Ok(Box::new(BeatCheckApp::new(cc, p, initial_file)))
        }),
    )
}
