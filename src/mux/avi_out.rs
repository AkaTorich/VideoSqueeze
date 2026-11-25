// AVI writer: RIFF 'AVI ' with hdrl, movi and idx1; past 1 GB the data continues in OpenDML 'AVIX' parts.
// AVI has no timestamps: video is one chunk per frame at a fixed rate (gaps become empty "dropped" chunks),
// H.264 goes as Annex B with SPS/PPS before every keyframe.
use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

use super::{io, AudioConfig, AudioOut, Muxer, OutPacket, VideoConfig, VideoOut};
use crate::media::h264;

const RIFF_LIMIT: u64 = 1 << 30;

/// BITMAPINFOHEADER for a video stream (also used for V_MS/VFW/FOURCC in Matroska).
pub fn bitmap_info(v: &VideoOut, extra: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(40 + extra.len());
    b.extend_from_slice(&(40 + extra.len() as u32).to_le_bytes());
    b.extend_from_slice(&(v.width as i32).to_le_bytes());
    b.extend_from_slice(&(v.height as i32).to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&24u16.to_le_bytes());
    b.extend_from_slice(&v.fourcc);
    b.extend_from_slice(&(v.width * v.height * 3).to_le_bytes());
    b.extend_from_slice(&[0; 16]);
    b.extend_from_slice(extra);
    b
}

fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(body.len() + 9);
    v.extend_from_slice(id);
    v.extend_from_slice(&(body.len() as u32).to_le_bytes());
    v.extend_from_slice(body);
    if body.len() % 2 == 1 {
        v.push(0);
    }
    v
}

fn list(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = kind.to_vec();
    b.extend_from_slice(body);
    chunk(b"LIST", &b)
}

struct Le(Vec<u8>);
impl Le {
    fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn raw(&mut self, v: &[u8]) -> &mut Self {
        self.0.extend_from_slice(v);
        self
    }
}

pub struct AviMuxer {
    out: BufWriter<File>,
    video: Option<VideoOut>,
    audio: Option<AudioOut>,
    /// fps as rate/scale.
    rate: u32,
    scale: u32,
    hdrl_at: u64,
    hdrl_len: usize,
    riff_start: u64,
    movi_start: u64,
    first_riff: bool,
    idx1: Vec<u8>,
    frames: u32,
    frames_first: u32,
    audio_bytes: u64,
    audio_chunks: u32,
    max_chunk: u32,
    avc: Option<Vec<u8>>,
    t0: Option<f64>,
}

impl AviMuxer {
    pub fn create(path: &Path, video: Option<VideoOut>, audio: Option<AudioOut>) -> Result<Self, String> {
        let out = BufWriter::with_capacity(1 << 20, File::create(path).map_err(io)?);
        let fps = video.as_ref().map(|v| v.fps).filter(|f| *f > 0.0).unwrap_or(25.0);
        // exact NTSC rates, otherwise milli-fps
        let (rate, scale) = if (fps - 30000.0 / 1001.0).abs() < 0.01 {
            (30000, 1001)
        } else if (fps - 24000.0 / 1001.0).abs() < 0.01 {
            (24000, 1001)
        } else if (fps - 60000.0 / 1001.0).abs() < 0.01 {
            (60000, 1001)
        } else {
            ((fps * 1000.0).round() as u32, 1000)
        };
        let avc = video.as_ref().and_then(|v| match &v.config {
            VideoConfig::Avc(c) => Some(c.to_annexb()),
            _ => None,
        });
        let mut m = AviMuxer {
            out,
            video,
            audio,
            rate,
            scale,
            hdrl_at: 12,
            hdrl_len: 0,
            riff_start: 0,
            movi_start: 0,
            first_riff: true,
            idx1: Vec::new(),
            frames: 0,
            frames_first: 0,
            audio_bytes: 0,
            audio_chunks: 0,
            max_chunk: 0,
            avc,
            t0: None,
        };
        let hdrl = m.hdrl();
        m.hdrl_len = hdrl.len();
        m.out.write_all(b"RIFF\0\0\0\0AVI ").map_err(io)?;
        m.out.write_all(&hdrl).map_err(io)?;
        // padding so the data starts on a 2 KB boundary (as most writers do)
        let pos = 12 + hdrl.len() + 8;
        let pad = (2048 - pos % 2048) % 2048;
        m.out.write_all(&chunk(b"JUNK", &vec![0u8; pad])).map_err(io)?;
        m.open_movi()?;
        Ok(m)
    }

    fn hdrl(&self) -> Vec<u8> {
        let streams = self.video.is_some() as u32 + self.audio.is_some() as u32;
        let (w, h) = self.video.as_ref().map(|v| (v.width, v.height)).unwrap_or((0, 0));
        let usec = (self.scale as f64 * 1e6 / self.rate as f64).round() as u32;
        let mut avih = Le(Vec::new());
        avih.u32(usec).u32(0).u32(0).u32(0x10 | 0x100).u32(self.frames_first).u32(0).u32(streams).u32(self.max_chunk.max(1 << 16)).u32(w).u32(h).raw(&[0; 16]);
        let mut body = chunk(b"avih", &avih.0);

        if let Some(v) = &self.video {
            let fcc: [u8; 4] = match &v.config {
                VideoConfig::Avc(_) => *b"H264",
                VideoConfig::Mpeg4(_) => if v.fourcc == *b"    " { *b"XVID" } else { v.fourcc },
                VideoConfig::Mjpeg => *b"MJPG",
                _ => v.fourcc,
            };
            let mut vo = v.clone();
            vo.fourcc = fcc;
            let mut strh = Le(Vec::new());
            strh.raw(b"vids").raw(&fcc).u32(0).u16(0).u16(0).u32(0).u32(self.scale).u32(self.rate).u32(0).u32(self.frames).u32(self.max_chunk.max(1 << 16)).u32(u32::MAX).u32(0);
            strh.u16(0).u16(0).u16(w as u16).u16(h as u16);
            body.extend_from_slice(&list(b"strl", &[chunk(b"strh", &strh.0), chunk(b"strf", &bitmap_info(&vo, &[]))].concat()));
        }
        if let Some(a) = &self.audio {
            let avg = (a.bitrate / 8).max(1) as u32;
            let (tag, scale, rate, ssize, align, bits, extra): (u16, u32, u32, u32, u16, u16, Vec<u8>) = match &a.config {
                AudioConfig::Pcm(bits) => {
                    let align = a.channels * (*bits / 8).max(1);
                    (1, align as u32, a.rate * align as u32, align as u32, align, *bits, Vec::new())
                }
                AudioConfig::Mpeg(layer) => {
                    let spf = if *layer == 1 { 384 } else if *layer == 3 && a.rate < 32000 { 576 } else { 1152 };
                    let extra = if *layer == 3 {
                        let mut e = Le(Vec::new());
                        e.u16(1).u32(2).u16((avg as u64 * spf as u64 / a.rate.max(1) as u64) as u16).u16(1).u16(1393);
                        e.0
                    } else {
                        Vec::new()
                    };
                    (if *layer == 3 { 0x55 } else { 0x50 }, spf, a.rate, 0, spf as u16, 0, extra)
                }
                AudioConfig::Ac3 => (0x2000, 1, avg, 1, 1, 0, Vec::new()),
                AudioConfig::Aac(asc) => (0xFF, 1024, a.rate, 0, 1024, 16, asc.clone()),
                AudioConfig::Mkv(..) => (0, 1, avg, 1, 1, 0, Vec::new()),
            };
            let length = if ssize == 0 { self.audio_chunks } else { (self.audio_bytes / ssize as u64) as u32 };
            let mut strh = Le(Vec::new());
            strh.raw(b"auds").raw(&[0; 4]).u32(0).u16(0).u16(0).u32(0).u32(scale).u32(rate).u32(0).u32(length).u32(self.max_chunk.max(4096)).u32(u32::MAX).u32(ssize).raw(&[0; 8]);
            let mut wf = Le(Vec::new());
            wf.u16(tag).u16(a.channels).u32(a.rate).u32(if matches!(a.config, AudioConfig::Pcm(_)) { rate } else { avg }).u16(align).u16(bits).u16(extra.len() as u16).raw(&extra);
            body.extend_from_slice(&list(b"strl", &[chunk(b"strh", &strh.0), chunk(b"strf", &wf.0)].concat()));
        }
        let mut dmlh = Le(Vec::new());
        dmlh.u32(self.frames).raw(&[0; 244]);
        body.extend_from_slice(&list(b"odml", &chunk(b"dmlh", &dmlh.0)));
        list(b"hdrl", &body)
    }

    fn open_movi(&mut self) -> Result<(), String> {
        let pos = self.out.stream_position().map_err(io)?;
        if !self.first_riff {
            self.riff_start = pos;
            self.out.write_all(b"RIFF\0\0\0\0AVIX").map_err(io)?;
        }
        self.movi_start = self.out.stream_position().map_err(io)? + 8;
        self.out.write_all(b"LIST\0\0\0\0movi").map_err(io)?;
        Ok(())
    }

    /// Close the current movi list (and idx1 for the first RIFF) and patch their sizes.
    fn close_riff(&mut self) -> Result<(), String> {
        let end = self.out.stream_position().map_err(io)?;
        let movi_size = (end - (self.movi_start - 8) - 8) as u32;
        if self.first_riff {
            let idx = std::mem::take(&mut self.idx1);
            self.out.write_all(&chunk(b"idx1", &idx)).map_err(io)?;
            self.frames_first = self.frames;
        }
        let riff_end = self.out.stream_position().map_err(io)?;
        self.out.seek(SeekFrom::Start(self.movi_start - 4)).map_err(io)?;
        self.out.write_all(&movi_size.to_le_bytes()).map_err(io)?;
        self.out.seek(SeekFrom::Start(self.riff_start + 4)).map_err(io)?;
        self.out.write_all(&((riff_end - self.riff_start - 8) as u32).to_le_bytes()).map_err(io)?;
        self.out.seek(SeekFrom::Start(riff_end)).map_err(io)?;
        self.first_riff = false;
        Ok(())
    }

    fn put(&mut self, id: &[u8; 4], data: &[u8], key: bool) -> Result<(), String> {
        let pos = self.out.stream_position().map_err(io)?;
        if pos - self.riff_start + data.len() as u64 > RIFF_LIMIT {
            self.close_riff()?;
            self.open_movi()?;
        }
        let pos = self.out.stream_position().map_err(io)?;
        if self.first_riff {
            self.idx1.extend_from_slice(id);
            self.idx1.extend_from_slice(&(if key { 0x10u32 } else { 0 }).to_le_bytes());
            self.idx1.extend_from_slice(&((pos - self.movi_start) as u32).to_le_bytes());
            self.idx1.extend_from_slice(&(data.len() as u32).to_le_bytes());
        }
        self.out.write_all(&chunk(id, data)).map_err(io)?;
        self.max_chunk = self.max_chunk.max(data.len() as u32);
        Ok(())
    }
}

impl Muxer for AviMuxer {
    fn write_video(&mut self, p: &OutPacket) -> Result<(), String> {
        let frame_dur = self.scale as f64 / self.rate as f64;
        // keep the fixed rate: missing frames become empty chunks
        if let Some(dts) = p.dts {
            let t0 = *self.t0.get_or_insert(dts);
            let slot = ((dts - t0) / frame_dur).round() as i64;
            while (self.frames as i64) < slot {
                self.put(b"00dc", &[], false)?;
                self.frames += 1;
            }
        }
        let data = match (&self.video.as_ref().map(|v| &v.config), &self.avc) {
            (Some(VideoConfig::Avc(_)), Some(params)) => {
                let mut d = if p.key { params.clone() } else { Vec::new() };
                d.extend_from_slice(&h264::prefixed_to_annexb(&p.data));
                d
            }
            (Some(VideoConfig::Hevc(_)), _) => h264::prefixed_to_annexb(&p.data),
            _ => p.data.clone(),
        };
        self.put(b"00dc", &data, p.key)?;
        self.frames += 1;
        Ok(())
    }

    fn write_audio(&mut self, p: &OutPacket) -> Result<(), String> {
        let id = if self.video.is_some() { b"01wb" } else { b"00wb" };
        self.put(id, &p.data, true)?;
        self.audio_bytes += p.data.len() as u64;
        self.audio_chunks += 1;
        Ok(())
    }

    fn finish(mut self: Box<Self>, progress: &dyn Fn(f64)) -> Result<(), String> {
        self.close_riff()?;
        // header with the real counts (same length as the placeholder)
        let hdrl = self.hdrl();
        if hdrl.len() == self.hdrl_len {
            self.out.seek(SeekFrom::Start(self.hdrl_at)).map_err(io)?;
            self.out.write_all(&hdrl).map_err(io)?;
        }
        self.out.flush().map_err(io)?;
        progress(1.0);
        Ok(())
    }
}
