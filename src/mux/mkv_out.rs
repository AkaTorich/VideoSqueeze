// Matroska writer (MKV video, MKA audio-only): EBML header, segment with seek head, info, tracks,
// clusters of SimpleBlocks (millisecond timestamps), cues for seeking. Sizes and duration are patched at the end.
use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

use super::{io, AudioConfig, AudioOut, Muxer, OutPacket, VideoConfig, VideoOut};

const VOID_RESERVE: usize = 96;
const CLUSTER_MS: i64 = 5000;

fn id_bytes(id: u32) -> Vec<u8> {
    let b = id.to_be_bytes();
    let skip = b.iter().position(|&x| x != 0).unwrap_or(3);
    b[skip..].to_vec()
}

fn vint(len: u64) -> Vec<u8> {
    for n in 1..=8u32 {
        if len < (1u64 << (7 * n)) - 1 {
            let mut v = len | (1u64 << (7 * n));
            let mut out = vec![0u8; n as usize];
            for i in (0..n as usize).rev() {
                out[i] = (v & 0xFF) as u8;
                v >>= 8;
            }
            return out;
        }
    }
    vec![0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
}

fn el(id: u32, body: &[u8]) -> Vec<u8> {
    let mut v = id_bytes(id);
    v.extend_from_slice(&vint(body.len() as u64));
    v.extend_from_slice(body);
    v
}

fn uint(id: u32, x: u64) -> Vec<u8> {
    let b = x.to_be_bytes();
    let skip = b.iter().position(|&x| x != 0).unwrap_or(7);
    el(id, &b[skip..])
}

fn float(id: u32, x: f64) -> Vec<u8> {
    el(id, &x.to_be_bytes())
}

fn string(id: u32, s: &str) -> Vec<u8> {
    el(id, s.as_bytes())
}

pub struct MkvMuxer {
    out: BufWriter<File>,
    segment_size_at: u64,
    segment_data: u64,
    void_at: u64,
    info_at: u64,
    tracks_at: u64,
    duration_at: u64,
    video_no: Option<u8>,
    audio_no: Option<u8>,
    cluster: Vec<u8>,
    cluster_start: Option<i64>,
    cluster_has_key: bool,
    cues: Vec<(i64, u64)>,
    end_ms: f64,
}

impl MkvMuxer {
    pub fn create(path: &Path, video: Option<VideoOut>, audio: Option<AudioOut>) -> Result<Self, String> {
        let mut out = BufWriter::with_capacity(1 << 20, File::create(path).map_err(io)?);
        let doc = "matroska";
        let header = el(
            0x1A45DFA3,
            &[uint(0x4286, 1), uint(0x42F7, 1), uint(0x42F2, 4), uint(0x42F3, 8), string(0x4282, doc), uint(0x4287, 4), uint(0x4285, 2)].concat(),
        );
        out.write_all(&header).map_err(io)?;
        let segment_size_at = header.len() as u64 + 4;
        out.write_all(&id_bytes(0x18538067)).map_err(io)?;
        out.write_all(&[0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]).map_err(io)?;
        let segment_data = segment_size_at + 8;

        // room for the seek head, written at the end
        let void_at = segment_data;
        let mut void = vec![0xEC, 0x01, 0, 0, 0, 0, 0, 0, (VOID_RESERVE - 9) as u8];
        void.resize(VOID_RESERVE, 0);
        out.write_all(&void).map_err(io)?;

        // info, duration placeholder
        let info_at = void_at + VOID_RESERVE as u64;
        let info_body = [uint(0x2AD7B1, 1_000_000), string(0x4D80, "VideoSqueeze"), string(0x5741, "VideoSqueeze"), float(0x4489, 0.0)].concat();
        let info = el(0x1549A966, &info_body);
        let duration_at = info_at + info.len() as u64 - 8;
        out.write_all(&info).map_err(io)?;

        // tracks
        let tracks_at = info_at + info.len() as u64;
        let mut entries = Vec::new();
        let mut video_no = None;
        let mut audio_no = None;
        let mut n = 1u8;
        if let Some(v) = &video {
            let (codec, private): (String, Vec<u8>) = match &v.config {
                VideoConfig::Avc(c) => ("V_MPEG4/ISO/AVC".into(), c.to_avcc()),
                VideoConfig::Hevc(h) => ("V_MPEGH/ISO/HEVC".into(), h.clone()),
                VideoConfig::Mpeg4(d) => ("V_MPEG4/ISO/ASP".into(), d.clone()),
                VideoConfig::Mjpeg => ("V_MJPEG".into(), Vec::new()),
                VideoConfig::MsMpeg4 => ("V_MS/VFW/FOURCC".into(), super::avi_out::bitmap_info(v, &[])),
                VideoConfig::Mkv(id, p) => (id.clone(), p.clone()),
            };
            let mut e = vec![uint(0xD7, n as u64), uint(0x73C5, n as u64 * 1001), uint(0x83, 1), uint(0x9C, 0), string(0x86, &codec), string(0x22B59C, "und")];
            if !private.is_empty() {
                e.push(el(0x63A2, &private));
            }
            if v.fps > 0.0 {
                e.push(uint(0x23E383, (1e9 / v.fps).round() as u64));
            }
            let video_el = [uint(0xB0, v.width as u64), uint(0xBA, v.height as u64)];
            e.push(el(0xE0, &video_el.concat()));
            entries.push(el(0xAE, &e.concat()));
            video_no = Some(n);
            n += 1;
        }
        if let Some(a) = &audio {
            let (codec, private): (String, Vec<u8>) = match &a.config {
                AudioConfig::Aac(asc) => ("A_AAC".into(), asc.clone()),
                AudioConfig::Mpeg(layer) => (format!("A_MPEG/L{layer}"), Vec::new()),
                AudioConfig::Ac3 => ("A_AC3".into(), Vec::new()),
                AudioConfig::Pcm(_) => ("A_PCM/INT/LIT".into(), Vec::new()),
                AudioConfig::Mkv(id, p) => (id.clone(), p.clone()),
            };
            let mut e = vec![uint(0xD7, n as u64), uint(0x73C5, n as u64 * 1001), uint(0x83, 2), uint(0x9C, 0), string(0x86, &codec), string(0x22B59C, "und")];
            if !private.is_empty() {
                e.push(el(0x63A2, &private));
            }
            let mut au = vec![float(0xB5, a.rate as f64), uint(0x9F, a.channels as u64)];
            if let AudioConfig::Pcm(bits) = a.config {
                au.push(uint(0x6264, bits as u64));
            }
            e.push(el(0xE1, &au.concat()));
            entries.push(el(0xAE, &e.concat()));
            audio_no = Some(n);
        }
        out.write_all(&el(0x1654AE6B, &entries.concat())).map_err(io)?;

        Ok(MkvMuxer {
            out,
            segment_size_at,
            segment_data,
            void_at,
            info_at,
            tracks_at,
            duration_at,
            video_no,
            audio_no,
            cluster: Vec::new(),
            cluster_start: None,
            cluster_has_key: false,
            cues: Vec::new(),
            end_ms: 0.0,
        })
    }

    fn flush_cluster(&mut self) -> Result<(), String> {
        let Some(start) = self.cluster_start.take() else { return Ok(()) };
        let pos = self.out.stream_position().map_err(io)? - self.segment_data;
        if self.cluster_has_key {
            self.cues.push((start, pos));
        }
        let body = [uint(0xE7, start.max(0) as u64), std::mem::take(&mut self.cluster)].concat();
        self.out.write_all(&el(0x1F43B675, &body)).map_err(io)?;
        self.cluster_has_key = false;
        Ok(())
    }

    fn block(&mut self, track: u8, p: &OutPacket, video: bool) -> Result<(), String> {
        let ms = (p.pts * 1000.0).round() as i64;
        let start_new = match self.cluster_start {
            None => true,
            Some(s) => {
                let rel = ms - s;
                rel > 32000 || rel < -32000 || self.cluster.len() > 8 << 20 || (video && p.key && rel >= CLUSTER_MS / 5) || (rel >= CLUSTER_MS && (video || self.video_no.is_none()))
            }
        };
        if start_new {
            self.flush_cluster()?;
            self.cluster_start = Some(ms.max(0));
        }
        let rel = (ms - self.cluster_start.unwrap()) as i16;
        if video && p.key || self.video_no.is_none() {
            self.cluster_has_key = true;
        }
        let mut b = vec![0x80 | track];
        b.extend_from_slice(&rel.to_be_bytes());
        b.push(if p.key { 0x80 } else { 0 });
        b.extend_from_slice(&p.data);
        self.cluster.extend_from_slice(&el(0xA3, &b));
        self.end_ms = self.end_ms.max((p.pts + p.dur) * 1000.0);
        Ok(())
    }
}

impl Muxer for MkvMuxer {
    fn write_video(&mut self, p: &OutPacket) -> Result<(), String> {
        match self.video_no {
            Some(n) => self.block(n, p, true),
            None => Ok(()),
        }
    }

    fn write_audio(&mut self, p: &OutPacket) -> Result<(), String> {
        match self.audio_no {
            Some(n) => self.block(n, p, false),
            None => Ok(()),
        }
    }

    fn finish(mut self: Box<Self>, progress: &dyn Fn(f64)) -> Result<(), String> {
        self.flush_cluster()?;
        // cues
        let cues_pos = self.out.stream_position().map_err(io)? - self.segment_data;
        let track = self.video_no.or(self.audio_no).unwrap_or(1);
        let points: Vec<u8> = self
            .cues
            .iter()
            .flat_map(|&(t, pos)| el(0xBB, &[uint(0xB3, t.max(0) as u64), el(0xB7, &[uint(0xF7, track as u64), uint(0xF1, pos)].concat())].concat()))
            .collect();
        if !points.is_empty() {
            self.out.write_all(&el(0x1C53BB6B, &points)).map_err(io)?;
        }
        let end = self.out.stream_position().map_err(io)?;

        // seek head into the reserved space
        let seek = |id: u32, pos: u64| el(0x4DBB, &[el(0x53AB, &id_bytes(id)), uint(0x53AC, pos)].concat());
        let mut seeks = vec![seek(0x1549A966, self.info_at - self.segment_data), seek(0x1654AE6B, self.tracks_at - self.segment_data)];
        if !points.is_empty() {
            seeks.push(seek(0x1C53BB6B, cues_pos));
        }
        let head = el(0x114D9B74, &seeks.concat());
        let mut reserved = head.clone();
        let rest = VOID_RESERVE - head.len();
        // fill the remainder with a Void element (1-byte id, 1-byte size)
        reserved.push(0xEC);
        reserved.push(0x80 | (rest - 2) as u8);
        reserved.resize(VOID_RESERVE, 0);
        self.out.seek(SeekFrom::Start(self.void_at)).map_err(io)?;
        self.out.write_all(&reserved).map_err(io)?;

        // duration and segment size
        self.out.seek(SeekFrom::Start(self.duration_at)).map_err(io)?;
        self.out.write_all(&self.end_ms.to_be_bytes()).map_err(io)?;
        self.out.seek(SeekFrom::Start(self.segment_size_at)).map_err(io)?;
        let size = end - self.segment_data;
        let mut v = vec![0x01];
        v.extend_from_slice(&size.to_be_bytes()[1..]);
        self.out.write_all(&v).map_err(io)?;
        self.out.flush().map_err(io)?;
        progress(1.0);
        Ok(())
    }
}
