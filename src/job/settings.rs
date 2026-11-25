// Conversion settings as the window sends them.
use serde::Deserialize;

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Mp4,
    Mov,
    Mkv,
    Avi,
    Gif,
    /// Audio track in its own format, unchanged.
    Audio,
    M4a,
    Wav,
}

impl Format {
    pub fn is_video(self) -> bool {
        matches!(self, Format::Mp4 | Format::Mov | Format::Mkv | Format::Avi)
    }
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum VideoMode {
    Encode,
    Copy,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum SizeMode {
    Quality,
    Target,
    Bitrate,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum AudioMode {
    /// Keep the audio when the container can hold it, otherwise AAC.
    Auto,
    /// Always AAC at `audio_kbps`.
    Aac,
    Remove,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub format: Format,
    pub video_mode: VideoMode,
    pub size_mode: SizeMode,
    /// Bits per pixel per frame for the quality mode.
    pub quality_bpp: f64,
    pub target_mb: f64,
    pub bitrate_kbps: u32,
    /// Output height in lines; 0 — as the source.
    pub height: u32,
    /// Output frame rate; 0 — as the source.
    pub fps: f64,
    pub audio: AudioMode,
    pub audio_kbps: u32,
    /// Output folder; empty — next to the source.
    pub out_dir: String,
    pub suffix: String,
}
