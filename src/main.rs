#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod core;
mod cli;
mod gui;

use std::env;
use std::process;
use eframe::egui::Vec2;
use gui::OmpSyncApp;

fn main() {
    let args: Vec<String> = env::args().collect();

    // If CLI arguments are provided, run CLI mode and exit
    if args.len() > 1 {
        // Handle CLI console output on Windows if run as GUI subsystem
        #[cfg(windows)]
        unsafe {
            extern "system" {
                fn AttachConsole(dw_process_id: u32) -> i32;
            }
            AttachConsole(0xFFFFFFFF);
        }

        let code = cli::run_cli(&args[1..]);
        process::exit(code);
    }

    // Otherwise, launch modern desktop GUI
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("OMP 多服务器配置与技能双向同步助手 (Rust 跨平台版)")
            .with_inner_size(Vec2::new(1100.0, 850.0))
            .with_min_inner_size(Vec2::new(800.0, 600.0)),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "OMP 多服务器配置与技能双向同步助手",
        native_options,
        Box::new(|cc| Ok(Box::new(OmpSyncApp::new(cc)))),
    ) {
        eprintln!("Failed to launch GUI: {e}");
        process::exit(1);
    }
}
