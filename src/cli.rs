// Work without a window (checks and scripts). The program has no console, so answers go to a JSON file:
//   VideoSqueeze.exe --probe <file> --out <result.json>
//   VideoSqueeze.exe --convert <file> --settings <settings.json> --out <result.json>
// settings.json has the same fields the window sends (job/settings.rs).
use std::path::Path;

use videosqueeze::job::run::{self, Reporter};
use videosqueeze::job::settings::Settings;

struct Quiet;

impl Reporter for Quiet {
    fn progress(&self, _stage: usize, _fraction: f64, _speed: f64) {}
    fn cancelled(&self) -> bool {
        false
    }
}

fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

pub fn run(args: &[String]) -> Option<i32> {
    let out = arg(args, "--out");
    let (value, code) = if let Some(file) = arg(args, "--probe") {
        match videosqueeze::media::probe(Path::new(file)) {
            Ok(i) => (
                serde_json::json!({
                    "container": i.container,
                    "duration": i.duration,
                    "size": i.size,
                    "video": i.video.as_ref().map(|v| serde_json::json!({"codec": v.codec, "name": v.name, "width": v.width, "height": v.height, "fps": v.fps, "bitrate": v.bitrate})),
                    "audio": i.audio.as_ref().map(|a| serde_json::json!({"codec": a.codec, "name": a.name, "rate": a.sample_rate, "channels": a.channels, "bitrate": a.bitrate})),
                }),
                0,
            ),
            Err(e) => (serde_json::json!({ "error": e }), 1),
        }
    } else if let Some(file) = arg(args, "--convert") {
        let settings: Result<Settings, String> = arg(args, "--settings")
            .ok_or_else(|| "--settings <file.json> is required".to_string())
            .and_then(|p| std::fs::read_to_string(p).map_err(|e| e.to_string()))
            .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()));
        match settings.and_then(|s| run::run(Path::new(file), &s, &Quiet)) {
            Ok(r) => (serde_json::to_value(r).unwrap_or_default(), 0),
            Err(e) => (serde_json::json!({ "error": e }), 1),
        }
    } else {
        return None;
    };
    if let Some(o) = out {
        let _ = std::fs::write(o, serde_json::to_string_pretty(&value).unwrap_or_default());
    }
    Some(code)
}
