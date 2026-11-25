// What happens to a file under the settings: copy or re-encode each track, output size and rate, bitrates,
// the output file name and the expected size. The window shows it before the run; the run follows it.
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::settings::{AudioMode, Format, Settings, SizeMode, VideoMode};
use crate::codec::{audio, scale};
use crate::media::{AudioCodec, AudioTrack, MediaInfo, VideoCodec, VideoTrack};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Container {
    Mp4,
    Mov,
    M4a,
    Mkv,
    Mka,
    Avi,
    Gif,
    Mp3,
    Ac3,
    Wav,
}

impl Container {
    pub fn ext(self, mpeg_layer: u8) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mov => "mov",
            Container::M4a => "m4a",
            Container::Mkv => "mkv",
            Container::Mka => "mka",
            Container::Avi => "avi",
            Container::Gif => "gif",
            Container::Mp3 => match mpeg_layer {
                1 => "mp1",
                2 => "mp2",
                _ => "mp3",
            },
            Container::Ac3 => "ac3",
            Container::Wav => "wav",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum VideoAction {
    None,
    Copy,
    Encode { width: u32, height: u32, fps: f64, bitrate: u64, rotate: u32, cfr: bool },
    Gif { width: u32, height: u32, fps: f64, rotate: u32 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum AudioAction {
    None,
    Copy,
    /// AAC at this bitrate.
    Encode(u32),
    /// Decode to PCM (WAV).
    Decode,
}

#[derive(Clone, Debug)]
pub struct Plan {
    pub container: Container,
    pub video: VideoAction,
    pub audio: AudioAction,
    pub out_path: PathBuf,
    pub est_bytes: u64,
    pub notes: Vec<String>,
}

/// What the window shows about a plan.
#[derive(Serialize, Clone, Debug)]
pub struct PlanView {
    pub out_name: String,
    pub out_dir: String,
    pub ext: String,
    pub video: String,
    pub audio: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub est_bytes: u64,
    pub notes: Vec<String>,
    pub error: Option<String>,
}

fn mpeg_layer(a: &AudioTrack) -> u8 {
    match a.name.as_str() {
        "MP1" => 1,
        "MP2" => 2,
        _ => 3,
    }
}

fn video_copyable(c: Container, v: &VideoTrack) -> bool {
    match c {
        Container::Mp4 | Container::Mov => matches!(v.codec, VideoCodec::H264 | VideoCodec::Hevc | VideoCodec::Mpeg4 | VideoCodec::Mjpeg),
        Container::Mkv => v.codec != VideoCodec::Other || v.mkv_codec.is_some() || v.fourcc != *b"    ",
        Container::Avi => v.codec != VideoCodec::Other || v.fourcc != *b"    ",
        _ => false,
    }
}

fn audio_copyable(c: Container, a: &AudioTrack) -> bool {
    match c {
        Container::Mp4 | Container::Mov | Container::M4a => matches!(a.codec, AudioCodec::Aac | AudioCodec::Mpeg | AudioCodec::Ac3),
        Container::Mkv | Container::Mka => matches!(a.codec, AudioCodec::Aac | AudioCodec::Mpeg | AudioCodec::Ac3 | AudioCodec::Pcm) || a.mkv_codec.is_some(),
        Container::Avi => matches!(a.codec, AudioCodec::Aac | AudioCodec::Mpeg | AudioCodec::Ac3 | AudioCodec::Pcm),
        _ => false,
    }
}

fn auto_aac_kbps(channels: u16) -> u32 {
    match channels {
        1 => 96,
        2 => 160,
        n => 64 * n.min(8) as u32,
    }
}

fn even(x: f64) -> u32 {
    ((x / 2.0).round() as u32 * 2).max(2)
}

/// Output picture size: source turned by `rotate`, scaled down to `height` lines (0 — keep), even sides.
fn out_size(v: &VideoTrack, rotate: u32, height: u32) -> (u32, u32) {
    let (mut w, mut h) = (v.width.max(2) as f64, v.height.max(2) as f64);
    if rotate == 90 || rotate == 270 {
        std::mem::swap(&mut w, &mut h);
    }
    if height > 0 && (height as f64) < h {
        let k = height as f64 / h;
        (even(w * k), even(h * k))
    } else {
        ((w as u32) & !1, (h as u32) & !1)
    }
}

pub fn make(path: &Path, info: &MediaInfo, s: &Settings) -> Result<Plan, String> {
    let mut notes = Vec::new();
    let dur = info.duration.max(0.001);
    let mut container = match s.format {
        Format::Mp4 => Container::Mp4,
        Format::Mov => Container::Mov,
        Format::Mkv => Container::Mkv,
        Format::Avi => Container::Avi,
        Format::Gif => Container::Gif,
        Format::M4a => Container::M4a,
        Format::Wav => Container::Wav,
        Format::Audio => Container::M4a,
    };

    if !s.format.is_video() && s.format != Format::Gif {
        return plan_audio_only(path, info, s, container, notes);
    }

    // ── video ──
    let mut video = VideoAction::None;
    if s.format.is_video() || s.format == Format::Gif {
        let Some(v) = info.video.as_ref() else {
            if s.format == Format::Gif {
                return Err("file has no video".into());
            }
            return plan_audio_only(path, info, s, container, notes);
        };
        if s.format == Format::Gif {
            if !v.codec.decodable() {
                return Err(format!("{} video cannot be decoded", v.name));
            }
            let rotate = scale::matrix_rotation(v.matrix);
            let (w, h) = out_size(v, rotate, s.height);
            let fps = if s.fps > 0.0 && (v.fps <= 0.0 || s.fps < v.fps) { s.fps } else if v.fps > 0.0 { v.fps.min(50.0) } else { 25.0 };
            video = VideoAction::Gif { width: w, height: h, fps, rotate };
        } else {
            let can_copy = video_copyable(container, v);
            let decodable = v.codec.decodable();
            let want_copy = s.video_mode == VideoMode::Copy;
            if want_copy && can_copy {
                video = VideoAction::Copy;
            } else if decodable {
                if want_copy {
                    notes.push(format!("{} CAN'T BE COPIED INTO {} — RE-ENCODED", v.name, container.ext(3).to_uppercase()));
                }
                let rotate = if matches!(container, Container::Mp4 | Container::Mov) { 0 } else { scale::matrix_rotation(v.matrix) };
                let (w, h) = out_size(v, rotate, s.height);
                let src_fps = if v.fps > 0.0 { v.fps } else { 25.0 };
                let fps = if s.fps > 0.0 && s.fps < src_fps { s.fps } else { src_fps };
                video = VideoAction::Encode { width: w, height: h, fps, bitrate: 0, rotate, cfr: container == Container::Avi };
            } else {
                // can't decode: keep the picture as it is, in a container that holds it
                if !can_copy {
                    container = Container::Mkv;
                    notes.push("SAVED AS MKV — THE ONLY CONTAINER FOR THIS CODEC".into());
                }
                notes.push(format!("{} CAN'T BE RE-ENCODED — COPIED AS IS", v.name));
                video = VideoAction::Copy;
            }
        }
    }

    // ── audio ──
    let audio = match info.audio.as_ref() {
        None => AudioAction::None,
        Some(_) if s.audio == AudioMode::Remove || s.format == Format::Gif => AudioAction::None,
        Some(a) => {
            // uncompressed sound is not worth keeping in a video file: AUTO turns it into AAC
            let copy_ok = audio_copyable(container, a) && !(a.codec == AudioCodec::Pcm && a.codec.decodable() && s.audio == AudioMode::Auto);
            if s.audio == AudioMode::Auto && copy_ok {
                AudioAction::Copy
            } else if a.codec.decodable() {
                AudioAction::Encode(if s.audio == AudioMode::Aac { s.audio_kbps.clamp(32, 512) } else { auto_aac_kbps(a.channels) })
            } else if copy_ok {
                notes.push(format!("{} AUDIO CAN'T BE RE-ENCODED — COPIED", a.name));
                AudioAction::Copy
            } else if container == Container::Mp4 || container == Container::Mov {
                container = Container::Mkv;
                notes.push(format!("SAVED AS MKV TO KEEP {} AUDIO", a.name));
                AudioAction::Copy
            } else {
                notes.push(format!("{} AUDIO CAN'T BE KEPT IN {} — LEFT OUT", a.name, container.ext(3).to_uppercase()));
                AudioAction::None
            }
        }
    };

    // ── bitrates and size ──
    let audio_bps = match (&audio, info.audio.as_ref()) {
        (AudioAction::Copy, Some(a)) => a.bitrate.max(32_000),
        (AudioAction::Encode(k), _) => *k as u64 * 1000,
        _ => 0,
    };
    let mut est = 0f64;
    match &mut video {
        VideoAction::Copy => {
            let v = info.video.as_ref().unwrap();
            est += v.bitrate as f64 * dur / 8.0;
        }
        VideoAction::Encode { width, height, fps, bitrate, .. } => {
            let v = info.video.as_ref().unwrap();
            let px = *width as f64 * *height as f64;
            let bps = match s.size_mode {
                SizeMode::Quality => {
                    let want = s.quality_bpp.max(0.005) * px * *fps;
                    // never spend more than the source did on the same picture
                    let src_px = (v.width.max(1) as f64) * (v.height.max(1) as f64);
                    let src_fps = if v.fps > 0.0 { v.fps } else { *fps };
                    let cap = v.bitrate as f64 * (px / src_px).min(1.0) * (*fps / src_fps).min(1.0);
                    if v.bitrate > 0 { want.min(cap.max(want * 0.35)) } else { want }
                }
                SizeMode::Target => {
                    let total = s.target_mb.max(0.01) * 1024.0 * 1024.0 * 8.0 * 0.97;
                    let v_bits = total - audio_bps as f64 * dur;
                    if v_bits <= 0.0 {
                        notes.push("TARGET SIZE IS SMALLER THAN THE AUDIO — VIDEO AT MINIMUM".into());
                    }
                    (v_bits / dur).max(40_000.0)
                }
                SizeMode::Bitrate => s.bitrate_kbps.max(40) as f64 * 1000.0,
            };
            *bitrate = bps as u64;
            est += bps * dur / 8.0;
        }
        VideoAction::Gif { width, height, fps, .. } => {
            est += *width as f64 * *height as f64 * *fps * dur * 0.45;
        }
        VideoAction::None => {}
    }
    est += audio_bps as f64 * dur / 8.0;
    est *= 1.01;

    let layer = info.audio.as_ref().map(mpeg_layer).unwrap_or(3);
    let out_path = out_path(path, s, container.ext(layer));
    Ok(Plan { container, video, audio, out_path, est_bytes: est as u64, notes })
}

/// Audio-only outputs (and video formats for a file without video).
fn plan_audio_only(path: &Path, info: &MediaInfo, s: &Settings, container: Container, mut notes: Vec<String>) -> Result<Plan, String> {
    let a = info.audio.as_ref().ok_or("file has no audio")?;
    let dur = info.duration.max(0.001);
    let (container, audio) = match s.format {
        Format::Audio => match a.codec {
            AudioCodec::Aac => (Container::M4a, AudioAction::Copy),
            AudioCodec::Mpeg => (Container::Mp3, AudioAction::Copy),
            AudioCodec::Ac3 => (Container::Ac3, AudioAction::Copy),
            AudioCodec::Pcm => (Container::Wav, AudioAction::Copy),
            _ if a.mkv_codec.is_some() => (Container::Mka, AudioAction::Copy),
            _ => return Err(format!("{} audio can't be extracted", a.name)),
        },
        Format::M4a => {
            if a.codec == AudioCodec::Aac && s.audio != AudioMode::Aac {
                (Container::M4a, AudioAction::Copy)
            } else if a.codec.decodable() {
                (Container::M4a, AudioAction::Encode(if s.audio == AudioMode::Aac { s.audio_kbps.clamp(32, 512) } else { auto_aac_kbps(a.channels) }))
            } else if a.mkv_codec.is_some() {
                notes.push(format!("{} AUDIO CAN'T BE CONVERTED — SAVED AS MKA", a.name));
                (Container::Mka, AudioAction::Copy)
            } else {
                return Err(format!("{} audio can't be converted", a.name));
            }
        }
        Format::Wav => {
            if a.codec.decodable() {
                (Container::Wav, if a.codec == AudioCodec::Pcm { AudioAction::Copy } else { AudioAction::Decode })
            } else if a.mkv_codec.is_some() {
                notes.push(format!("{} AUDIO CAN'T BE DECODED — SAVED AS MKA", a.name));
                (Container::Mka, AudioAction::Copy)
            } else {
                return Err(format!("{} audio can't be decoded", a.name));
            }
        }
        _ => {
            // a video format for a file with sound only
            let action = if s.audio == AudioMode::Remove {
                return Err("file has no video and audio is set to REMOVE".into());
            } else if audio_copyable(container, a) && s.audio == AudioMode::Auto {
                AudioAction::Copy
            } else if a.codec.decodable() {
                AudioAction::Encode(auto_aac_kbps(a.channels))
            } else {
                return Err(format!("{} audio can't be kept", a.name));
            };
            (container, action)
        }
    };
    let est = match &audio {
        AudioAction::Copy => a.bitrate as f64 * dur / 8.0,
        AudioAction::Encode(k) => *k as f64 * 1000.0 * dur / 8.0,
        AudioAction::Decode => a.sample_rate as f64 * a.channels as f64 * 2.0 * dur,
        AudioAction::None => 0.0,
    };
    let out_path = out_path(path, s, container.ext(mpeg_layer(a)));
    Ok(Plan { container, video: VideoAction::None, audio, out_path, est_bytes: est as u64, notes })
}

/// `<folder>\<name><suffix>.<ext>`; never the source itself.
fn out_path(src: &Path, s: &Settings, ext: &str) -> PathBuf {
    let dir = if s.out_dir.trim().is_empty() { src.parent().map(|p| p.to_path_buf()).unwrap_or_default() } else { PathBuf::from(s.out_dir.trim()) };
    let stem = src.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "video".into());
    let suffix: String = s.suffix.chars().filter(|c| !r#"\/:*?"<>|"#.contains(*c)).collect();
    let mut p = dir.join(format!("{stem}{suffix}.{ext}"));
    if same_file(&p, src) {
        p = dir.join(format!("{stem}{suffix} (converted).{ext}"));
    }
    p
}

fn same_file(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

/// A free name: "name.ext", "name (2).ext", ...
pub fn unique(p: &Path) -> PathBuf {
    if !p.exists() {
        return p.to_path_buf();
    }
    let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    let stem = p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    (2..10000).map(|i| dir.join(format!("{stem} ({i}).{ext}"))).find(|c| !c.exists()).unwrap_or_else(|| p.to_path_buf())
}

pub fn view(plan: &Result<Plan, String>, info: &MediaInfo) -> PlanView {
    match plan {
        Err(e) => PlanView {
            out_name: String::new(),
            out_dir: String::new(),
            ext: String::new(),
            video: String::new(),
            audio: String::new(),
            width: 0,
            height: 0,
            fps: 0.0,
            est_bytes: 0,
            notes: Vec::new(),
            error: Some(e.to_uppercase()),
        },
        Ok(p) => {
            let vname = info.video.as_ref().map(|v| v.name.clone()).unwrap_or_default();
            let aname = info.audio.as_ref().map(|a| a.name.clone()).unwrap_or_default();
            let (video, width, height, fps) = match &p.video {
                VideoAction::None => ("NONE".to_string(), 0, 0, 0.0),
                VideoAction::Copy => {
                    let v = info.video.as_ref().unwrap();
                    (format!("COPY · {vname}"), v.width, v.height, v.fps)
                }
                VideoAction::Encode { width, height, fps, bitrate, .. } => (format!("H.264 · {}", kbps(*bitrate)), *width, *height, *fps),
                VideoAction::Gif { width, height, fps, .. } => ("GIF · 256 COLOURS".to_string(), *width, *height, *fps),
            };
            let audio = match &p.audio {
                AudioAction::None => "NONE".to_string(),
                AudioAction::Copy => format!("COPY · {aname}"),
                AudioAction::Encode(k) => {
                    let rate = info.audio.as_ref().map(|a| audio::aac_rate_for(a.sample_rate)).unwrap_or(48000);
                    format!("AAC · {k} KBIT/S · {:.1} KHZ", rate as f64 / 1000.0)
                }
                AudioAction::Decode => "PCM 16-BIT".to_string(),
            };
            PlanView {
                out_name: p.out_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                out_dir: p.out_path.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default(),
                ext: p.out_path.extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default(),
                video,
                audio,
                width,
                height,
                fps,
                est_bytes: p.est_bytes,
                notes: p.notes.clone(),
                error: None,
            }
        }
    }
}

fn kbps(bps: u64) -> String {
    if bps >= 10_000_000 {
        format!("{:.0} MBIT/S", bps as f64 / 1e6)
    } else if bps >= 1_000_000 {
        format!("{:.1} MBIT/S", bps as f64 / 1e6)
    } else {
        format!("{} KBIT/S", bps / 1000)
    }
}
