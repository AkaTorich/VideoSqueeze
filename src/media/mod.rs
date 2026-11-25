// Reading source files: container demuxers (MP4/MOV/3GP, MKV/WebM, AVI) that turn a file into a description
// (MediaInfo) and a stream of packets in decode order. Packets are normalized on the way out:
// H.264/HEVC — NAL units with 4-byte length prefixes; MPEG audio and AC-3 — one frame per packet;
// PCM — signed little-endian (8-bit stays unsigned, as in WAV).
pub mod aac;
pub mod ac3;
mod avi_in;
pub mod h264;
mod mkv_in;
pub mod mp3;
mod mp4_in;

use std::io::Read;
use std::path::Path;

use serde::Serialize;

pub const EXTENSIONS: &[&str] = &["mp4", "m4v", "mov", "3gp", "3g2", "mkv", "webm", "avi", "divx", "xvid"];

pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    H264,
    Hevc,
    /// MPEG-4 Part 2: XviD, DivX 4/5/6, MP4V, H.263 short header.
    Mpeg4,
    /// Microsoft MPEG-4 v1/v2/v3 (DivX 3).
    MsMpeg4V1,
    MsMpeg4V2,
    MsMpeg4V3,
    Mjpeg,
    Other,
}

impl VideoCodec {
    pub fn decodable(self) -> bool {
        !matches!(self, VideoCodec::Hevc | VideoCodec::Other)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    Aac,
    /// MPEG-1/2 audio layer I, II or III.
    Mpeg,
    Pcm,
    Ac3,
    Opus,
    Vorbis,
    Other,
}

impl AudioCodec {
    pub fn decodable(self) -> bool {
        matches!(self, AudioCodec::Aac | AudioCodec::Mpeg | AudioCodec::Pcm | AudioCodec::Ac3)
    }
}

#[derive(Clone, Debug)]
pub struct VideoTrack {
    pub codec: VideoCodec,
    /// Name shown in the window ("H.264", "XVID", ...).
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// Bits per second; 0 — unknown.
    pub bitrate: u64,
    /// H.264: SPS and PPS.
    pub avc: Option<h264::AvcConfig>,
    /// Codec setup bytes: hvcC for HEVC, VOL headers for MPEG-4 Part 2.
    pub extradata: Vec<u8>,
    /// AVI FourCC of the source (for copying into AVI).
    pub fourcc: [u8; 4],
    /// MP4 display matrix (rotation of phone videos), a b u c d v x y w.
    pub matrix: Option<[i32; 9]>,
    /// Matroska codec ID and private data — lets any video be copied into MKV unchanged.
    pub mkv_codec: Option<(String, Vec<u8>)>,
}

#[derive(Clone, Debug)]
pub struct AudioTrack {
    pub codec: AudioCodec,
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    /// Bits per sample for PCM.
    pub bits: u16,
    pub bitrate: u64,
    /// AudioSpecificConfig for AAC.
    pub asc: Vec<u8>,
    pub mkv_codec: Option<(String, Vec<u8>)>,
}

#[derive(Clone, Debug)]
pub struct MediaInfo {
    pub container: &'static str,
    pub duration: f64,
    pub size: u64,
    pub video: Option<VideoTrack>,
    pub audio: Option<AudioTrack>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Video,
    Audio,
}

#[derive(Clone, Debug)]
pub struct Packet {
    pub kind: Kind,
    /// Presentation time, seconds.
    pub pts: f64,
    /// Decode time, seconds, when the container stores it.
    pub dts: Option<f64>,
    /// Duration, seconds; 0 — unknown.
    pub dur: f64,
    pub key: bool,
    pub data: Vec<u8>,
}

pub trait Demuxer: Send {
    fn info(&self) -> &MediaInfo;
    fn next(&mut self) -> Result<Option<Packet>, String>;
}

/// Open a file by its contents (the extension can lie).
pub fn open(path: &Path) -> Result<Box<dyn Demuxer>, String> {
    let mut head = [0u8; 12];
    let mut f = std::fs::File::open(path).map_err(|e| format!("cannot open file: {e}"))?;
    let n = f.read(&mut head).map_err(|e| format!("cannot read file: {e}"))?;
    drop(f);
    if n < 12 {
        return Err("file is too short".into());
    }
    if &head[0..4] == b"RIFF" && &head[8..12] == b"AVI " {
        Ok(Box::new(avi_in::AviDemuxer::open(path)?))
    } else if head[0..4] == [0x1A, 0x45, 0xDF, 0xA3] {
        Ok(Box::new(mkv_in::MkvDemuxer::open(path)?))
    } else {
        Ok(Box::new(mp4_in::Mp4Demuxer::open(path)?))
    }
}

/// Description of a file without reading its packets.
pub fn probe(path: &Path) -> Result<MediaInfo, String> {
    Ok(open(path)?.info().clone())
}

/// Bits per second spent on video, when the container doesn't say: file size minus audio.
pub(crate) fn estimate_video_bitrate(size: u64, duration: f64, audio_bps: u64) -> u64 {
    if duration <= 0.0 {
        return 0;
    }
    ((size as f64 * 8.0 / duration) as u64).saturating_sub(audio_bps)
}

/// Video codec by AVI/MP4 FourCC (case-insensitive) → (codec, display name).
pub(crate) fn video_by_fourcc(fourcc: &[u8; 4]) -> (VideoCodec, String) {
    let up: Vec<u8> = fourcc.iter().map(|b| b.to_ascii_uppercase()).collect();
    let name = String::from_utf8_lossy(&up).trim().to_string();
    let codec = match &up[..] {
        b"H264" | b"X264" | b"AVC1" | b"DAVC" | b"V264" | b"VSSH" => VideoCodec::H264,
        b"HEVC" | b"HVC1" | b"HEV1" | b"H265" | b"X265" => VideoCodec::Hevc,
        b"XVID" | b"DIVX" | b"DX50" | b"FMP4" | b"MP4V" | b"M4S2" | b"MP4S" | b"3IV2" | b"BLZ0" | b"DXGM" | b"RMP4" | b"S263" | b"H263" | b"U263" => {
            VideoCodec::Mpeg4
        }
        b"MP41" | b"MPG4" => VideoCodec::MsMpeg4V1,
        b"MP42" | b"DIV2" => VideoCodec::MsMpeg4V2,
        b"DIV3" | b"DIV4" | b"DIV5" | b"DIV6" | b"MP43" | b"MPG3" | b"DVX3" | b"AP41" | b"COL1" => VideoCodec::MsMpeg4V3,
        b"MJPG" | b"AVRN" | b"LJPG" | b"JPGL" | b"MJPA" | b"JPEG" | b"DMB1" => VideoCodec::Mjpeg,
        _ => VideoCodec::Other,
    };
    let shown = match codec {
        VideoCodec::H264 => "H.264".to_string(),
        VideoCodec::Hevc => "HEVC".to_string(),
        VideoCodec::Mjpeg => "MJPEG".to_string(),
        VideoCodec::MsMpeg4V3 => format!("DIVX 3 ({name})"),
        _ => name,
    };
    (codec, shown)
}

pub(crate) fn mpeg_audio_name(layer_hint: Option<u8>) -> String {
    match layer_hint {
        Some(1) => "MP1".into(),
        Some(2) => "MP2".into(),
        _ => "MP3".into(),
    }
}
