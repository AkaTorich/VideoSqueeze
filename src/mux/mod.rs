// Writing output files: MP4/MOV/M4A, MKV/MKA, AVI (all through the Muxer trait), animated GIF and raw audio
// files (ADTS AAC, MP3/AC-3 frames, WAV).
pub mod avi_out;
pub mod gif_out;
pub mod mkv_out;
pub mod mp4_out;
pub mod raw_out;

use crate::media::h264::AvcConfig;

#[derive(Clone, Debug)]
pub enum VideoConfig {
    Avc(AvcConfig),
    /// hvcC record (4-byte NAL lengths).
    Hevc(Vec<u8>),
    /// MPEG-4 Part 2 with its VOL headers.
    Mpeg4(Vec<u8>),
    /// Microsoft MPEG-4 (DivX 3) — only AVI and MKV (as VfW) can hold it.
    MsMpeg4,
    Mjpeg,
    /// Any other codec, copied into MKV as it was.
    Mkv(String, Vec<u8>),
}

#[derive(Clone, Debug)]
pub struct VideoOut {
    pub config: VideoConfig,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub matrix: Option<[i32; 9]>,
    pub fourcc: [u8; 4],
}

#[derive(Clone, Debug)]
pub enum AudioConfig {
    Aac(Vec<u8>),
    /// MPEG audio layer 1, 2 or 3.
    Mpeg(u8),
    Ac3,
    Pcm(u16),
    Mkv(String, Vec<u8>),
}

#[derive(Clone, Debug)]
pub struct AudioOut {
    pub config: AudioConfig,
    pub rate: u32,
    pub channels: u16,
    pub bitrate: u64,
}

#[derive(Clone, Debug)]
pub struct OutPacket {
    pub pts: f64,
    /// Decode time when known (B-frames); `None` — derived from the presentation order.
    pub dts: Option<f64>,
    pub dur: f64,
    pub key: bool,
    pub data: Vec<u8>,
}

pub trait Muxer {
    fn write_video(&mut self, p: &OutPacket) -> Result<(), String>;
    fn write_audio(&mut self, p: &OutPacket) -> Result<(), String>;
    /// Complete the file; `progress` gets 0…1 for the final copy pass, where there is one.
    fn finish(self: Box<Self>, progress: &dyn Fn(f64)) -> Result<(), String>;
}

pub(crate) fn io(e: std::io::Error) -> String {
    format!("cannot write output file: {e}")
}
