// Commands the window calls.
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::State;

use videosqueeze::job::plan::{self, PlanView};
use videosqueeze::job::settings::Settings;
use videosqueeze::media::{self, MediaInfo};
use crate::worker::{Job, Queue};

#[derive(Deserialize)]
pub struct JobIn {
    pub id: u64,
    pub path: String,
}

/// What the window shows about a source file.
#[derive(Serialize)]
pub struct ProbeView {
    pub path: String,
    pub ok: bool,
    pub error: Option<String>,
    pub container: String,
    pub size: u64,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub video: String,
    pub video_bitrate: u64,
    pub decodable: bool,
    pub audio: String,
    pub audio_bitrate: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub rotation: u32,
}

fn probe_view(path: &str, info: &Result<MediaInfo, String>) -> ProbeView {
    let mut v = ProbeView {
        path: path.to_string(),
        ok: info.is_ok(),
        error: info.as_ref().err().map(|e| e.to_uppercase()),
        container: String::new(),
        size: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        duration: 0.0,
        width: 0,
        height: 0,
        fps: 0.0,
        video: String::new(),
        video_bitrate: 0,
        decodable: false,
        audio: String::new(),
        audio_bitrate: 0,
        sample_rate: 0,
        channels: 0,
        rotation: 0,
    };
    if let Ok(i) = info {
        v.container = i.container.to_string();
        v.duration = i.duration;
        if let Some(t) = &i.video {
            v.width = t.width;
            v.height = t.height;
            v.fps = t.fps;
            v.video = t.name.clone();
            v.video_bitrate = t.bitrate;
            v.decodable = t.codec.decodable();
            v.rotation = videosqueeze::codec::scale::matrix_rotation(t.matrix);
        }
        if let Some(a) = &i.audio {
            v.audio = a.name.clone();
            v.audio_bitrate = a.bitrate;
            v.sample_rate = a.sample_rate;
            v.channels = a.channels;
        }
    }
    v
}

/// Read file descriptions (in parallel).
#[tauri::command]
pub async fn probe_files(paths: Vec<String>) -> Vec<ProbeView> {
    tauri::async_runtime::spawn_blocking(move || {
        paths.par_iter().map(|p| probe_view(p, &media::probe(Path::new(p)))).collect()
    })
    .await
    .unwrap_or_default()
}

/// Plans of files under the settings (the window shows the expected result before the run).
#[tauri::command]
pub async fn plan_files(paths: Vec<String>, settings: Settings) -> Vec<PlanView> {
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .par_iter()
            .map(|p| match media::probe(Path::new(p)) {
                Ok(info) => plan::view(&plan::make(Path::new(p), &info, &settings), &info),
                Err(e) => plan::view(&Err(e), &media::MediaInfo { container: "", duration: 0.0, size: 0, video: None, audio: None }),
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// Queue files for conversion; progress comes as job-progress and job-done events.
#[tauri::command]
pub fn enqueue_jobs(queue: State<Queue>, jobs: Vec<JobIn>, settings: Settings) {
    queue.push(jobs.into_iter().map(|j| Job { id: j.id, path: j.path, settings: settings.clone() }).collect());
}

/// Drop everything waiting and stop the current conversion.
#[tauri::command]
pub fn cancel_queue(queue: State<Queue>) {
    queue.cancel_all();
}

/// Files and folders (with subfolders) → video files in name order.
#[tauri::command]
pub async fn scan_paths(paths: Vec<String>) -> Vec<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = Vec::new();
        for p in paths {
            collect(Path::new(&p), &mut out);
        }
        out.into_iter().map(|p| p.to_string_lossy().into_owned()).collect()
    })
    .await
    .unwrap_or_default()
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let Ok(rd) = std::fs::read_dir(path) else { return };
        let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort_by_key(|p| p.to_string_lossy().to_lowercase());
        for e in entries {
            collect(&e, out);
        }
    } else if media::is_video(path) {
        out.push(path.to_path_buf());
    }
}

/// Show a file selected in the file manager (Explorer, Finder) or, on Linux, open its folder.
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("explorer.exe");
        if p.is_file() {
            c.raw_arg(format!("/select,\"{path}\""));
        } else {
            c.raw_arg(format!("\"{path}\""));
        }
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        if p.is_file() {
            c.arg("-R");
        }
        c.arg(p);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(if p.is_file() { p.parent().unwrap_or(p) } else { p });
        c
    };
    cmd.spawn().map(|_| ()).map_err(|e| format!("cannot open the file manager: {e}"))
}
