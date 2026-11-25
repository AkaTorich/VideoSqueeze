// VideoSqueeze: video compressor and converter — MP4/MOV/3GP, MKV/WebM, AVI in; MP4, MOV, MKV, AVI, GIF,
// M4A, MP3/AC-3, WAV out. Window — Tauri 2 (ui/); the conversion core is the library (lib.rs);
// queue — worker, window commands — commands, work without a window — cli.
#![windows_subsystem = "windows"]

mod boot;
mod cli;
mod commands;
mod shape;
mod worker;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // work without a window (see cli.rs)
    if let Some(code) = cli::run(&args) {
        std::process::exit(code);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(worker::Queue::default())
        .manage(boot::Boot::default())
        .manage(shape::Shape::default())
        .setup(|app| {
            worker::start(app.handle().clone());
            boot::start(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            boot::boot_step,
            boot::boot_status,
            boot::app_ready,
            commands::probe_files,
            commands::plan_files,
            commands::enqueue_jobs,
            commands::cancel_queue,
            commands::scan_paths,
            commands::reveal_path,
            shape::set_window_shape,
        ])
        .run(tauri::generate_context!())
        .expect("VideoSqueeze failed to start");
}
