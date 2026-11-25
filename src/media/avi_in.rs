// AVI reader (RIFF AVI with OpenDML 'AVIX' continuations): first video and first audio stream.
// AVI keeps no timestamps: video time = frame number × scale / rate; audio time follows its bytes or
// chunk count. MPEG audio and AC-3 are re-split into whole frames, AAC loses ADTS headers.
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use super::{aac, ac3, h264, mp3, AudioCodec, AudioTrack, Demuxer, Kind, MediaInfo, Packet, VideoCodec, VideoTrack};

fn le16(d: &[u8], i: usize) -> u16 {
    d.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).unwrap_or(0)
}
fn le32(d: &[u8], i: usize) -> u32 {
    d.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0)
}

/// RIFF chunks of an in-memory list payload: (id, payload).
fn chunks(data: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos + 8 <= data.len() {
        let id: [u8; 4] = data[pos..pos + 4].try_into().unwrap();
        let size = le32(data, pos + 4) as usize;
        let end = (pos + 8 + size).min(data.len());
        out.push((id, &data[pos + 8..end]));
        pos += 8 + size + (size & 1);
    }
    out
}

struct StreamHeader {
    kind: [u8; 4],
    scale: u32,
    rate: u32,
    start: u32,
    length: u32,
    sample_size: u32,
    format: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq)]
enum AudioMode {
    /// One packet per chunk; time from bytes (CBR) or chunk count (VBR).
    Plain,
    Mpeg,
    Ac3,
    Aac,
}

pub struct AviDemuxer {
    file: BufReader<File>,
    info: MediaInfo,
    /// movi list ranges in the file: (first chunk, end).
    movi: Vec<(u64, u64)>,
    range: usize,
    pos: u64,
    video_id: Option<u8>,
    audio_id: Option<u8>,
    frame_dur: f64,
    video_start: f64,
    video_n: u64,
    keyframes: Vec<bool>,
    annexb: bool,
    audio_mode: AudioMode,
    audio_vbr_dur: f64,
    audio_avg_bytes: u32,
    audio_bytes: u64,
    audio_chunks: u64,
    audio_start: f64,
    audio_time: f64,
    /// 32-bit integer PCM is passed on as 24-bit.
    pcm32: bool,
    mpeg: mp3::Framer,
    ac3: ac3::Framer,
    queue: std::collections::VecDeque<Packet>,
}

impl AviDemuxer {
    pub fn open(path: &Path) -> Result<Self, String> {
        let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
        let mut file = BufReader::new(File::open(path).map_err(|e| format!("cannot open file: {e}"))?);

        let mut hdrl: Vec<u8> = Vec::new();
        let mut movi: Vec<(u64, u64)> = Vec::new();
        let mut idx1: Vec<u8> = Vec::new();

        // walk RIFF 'AVI ' and every RIFF 'AVIX'
        let mut riff = 0u64;
        while riff + 12 <= size {
            file.seek(SeekFrom::Start(riff)).map_err(|e| e.to_string())?;
            let mut h = [0u8; 12];
            if file.read_exact(&mut h).is_err() || &h[0..4] != b"RIFF" {
                break;
            }
            let riff_size = le32(&h, 4) as u64;
            let riff_end = (riff + 8 + riff_size).min(size);
            let mut pos = riff + 12;
            while pos + 8 <= riff_end {
                file.seek(SeekFrom::Start(pos)).map_err(|e| e.to_string())?;
                let mut c = [0u8; 12];
                if file.read_exact(&mut c[..8]).is_err() {
                    break;
                }
                let csize = le32(&c, 4) as u64;
                if &c[0..4] == b"LIST" {
                    let _ = file.read_exact(&mut c[8..12]);
                    let end = (pos + 8 + csize).min(riff_end);
                    match &c[8..12] {
                        b"hdrl" if hdrl.is_empty() => {
                            hdrl = vec![0u8; (end - pos - 12) as usize];
                            file.read_exact(&mut hdrl).map_err(|e| e.to_string())?;
                        }
                        b"movi" => movi.push((pos + 12, end)),
                        _ => {}
                    }
                } else if &c[0..4] == b"idx1" && idx1.is_empty() {
                    let len = csize.min(riff_end - pos - 8) as usize;
                    idx1 = vec![0u8; len];
                    file.read_exact(&mut idx1).map_err(|e| e.to_string())?;
                }
                pos += 8 + csize + (csize & 1);
            }
            riff = riff_end + (riff_end & 1);
        }
        if hdrl.is_empty() || movi.is_empty() {
            return Err("damaged AVI file: no header or no data list".into());
        }

        // headers
        let mut avih_frames = 0u32;
        let mut avih_usec = 0u32;
        let mut total_frames = 0u32;
        let mut streams: Vec<StreamHeader> = Vec::new();
        for (id, body) in chunks(&hdrl) {
            if &id == b"avih" {
                avih_usec = le32(body, 0);
                avih_frames = le32(body, 16);
            } else if &id == b"LIST" && body.len() >= 4 {
                match &body[0..4] {
                    b"strl" => {
                        let mut sh = StreamHeader { kind: *b"    ", scale: 1, rate: 25, start: 0, length: 0, sample_size: 0, format: Vec::new() };
                        for (sid, sb) in chunks(&body[4..]) {
                            if &sid == b"strh" && sb.len() >= 48 {
                                sh.kind = sb[0..4].try_into().unwrap();
                                sh.scale = le32(sb, 20).max(1);
                                sh.rate = le32(sb, 24).max(1);
                                sh.start = le32(sb, 28);
                                sh.length = le32(sb, 32);
                                sh.sample_size = le32(sb, 44);
                            } else if &sid == b"strf" {
                                sh.format = sb.to_vec();
                            }
                        }
                        streams.push(sh);
                    }
                    b"odml" => {
                        for (oid, ob) in chunks(&body[4..]) {
                            if &oid == b"dmlh" {
                                total_frames = le32(ob, 0);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        let mut video = None;
        let mut audio = None;
        let mut video_id = None;
        let mut audio_id = None;
        let mut frame_dur = if avih_usec > 0 { avih_usec as f64 / 1e6 } else { 0.04 };
        let mut video_start = 0.0;
        let mut duration = 0f64;
        let mut annexb = false;
        let mut audio_mode = AudioMode::Plain;
        let mut audio_vbr_dur = 0.0;
        let mut audio_avg_bytes = 0;
        let mut audio_start = 0.0;
        let mut pcm32 = false;

        for (i, sh) in streams.iter().enumerate() {
            match &sh.kind {
                b"vids" if video.is_none() => {
                    let f = &sh.format;
                    let width = le32(f, 4);
                    let height = (le32(f, 8) as i32).unsigned_abs();
                    let fcc: [u8; 4] = f.get(16..20).map(|s| s.try_into().unwrap()).unwrap_or(*b"    ");
                    let (codec, name) = super::video_by_fourcc(&fcc);
                    frame_dur = sh.scale as f64 / sh.rate as f64;
                    video_start = sh.start as f64 * frame_dur;
                    let frames = total_frames.max(sh.length).max(avih_frames);
                    duration = duration.max(frames as f64 * frame_dur + video_start);
                    let extradata = f.get(40..).unwrap_or(&[]).to_vec();
                    let mut avc = None;
                    if codec == VideoCodec::H264 {
                        match h264::AvcConfig::parse(&extradata) {
                            Some((cfg, _)) => avc = Some(cfg),
                            None => annexb = true,
                        }
                    }
                    video = Some(VideoTrack {
                        codec,
                        name,
                        width,
                        height,
                        fps: 1.0 / frame_dur,
                        bitrate: 0,
                        avc,
                        extradata,
                        fourcc: fcc,
                        matrix: None,
                        mkv_codec: None,
                    });
                    video_id = Some(i as u8);
                }
                b"auds" if audio.is_none() => {
                    let f = &sh.format;
                    let mut tag = le16(f, 0);
                    let channels = le16(f, 2).max(1);
                    let rate = le32(f, 4);
                    let avg = le32(f, 8);
                    let mut bits = le16(f, 14);
                    let cb = le16(f, 16) as usize;
                    let extra = f.get(18..18 + cb).unwrap_or(&[]).to_vec();
                    if tag == 0xFFFE && extra.len() >= 8 {
                        tag = le16(&extra, 6);
                    }
                    let mut track = AudioTrack {
                        codec: AudioCodec::Other,
                        name: format!("0x{tag:04X}"),
                        sample_rate: rate,
                        channels,
                        bits,
                        bitrate: avg as u64 * 8,
                        asc: Vec::new(),
                        mkv_codec: None,
                    };
                    audio_start = sh.start as f64 * sh.scale as f64 / sh.rate as f64;
                    audio_vbr_dur = if sh.sample_size == 0 { sh.scale as f64 / sh.rate as f64 } else { 0.0 };
                    audio_avg_bytes = avg.max(1);
                    match tag {
                        0x0001 => {
                            track.codec = AudioCodec::Pcm;
                            track.name = "PCM".into();
                            if bits > 24 {
                                pcm32 = true;
                                bits = 24;
                                track.bits = 24;
                            }
                        }
                        0x0055 | 0x0050 => {
                            track.codec = AudioCodec::Mpeg;
                            track.name = super::mpeg_audio_name(Some(if tag == 0x50 { 2 } else { 3 }));
                            audio_mode = AudioMode::Mpeg;
                        }
                        0x2000 => {
                            track.codec = AudioCodec::Ac3;
                            track.name = "AC-3".into();
                            audio_mode = AudioMode::Ac3;
                        }
                        0x00FF | 0x1600 | 0x1610 | 0x706D | 0x4143 | 0xA106 => {
                            track.codec = AudioCodec::Aac;
                            track.name = "AAC".into();
                            track.asc = if extra.len() >= 2 { extra.clone() } else { aac::asc(2, rate, channels) };
                            audio_mode = AudioMode::Aac;
                            if audio_vbr_dur == 0.0 {
                                audio_vbr_dur = 1024.0 / rate.max(1) as f64;
                            }
                        }
                        0x0161 | 0x0162 => track.name = "WMA".into(),
                        0x2001 => track.name = "DTS".into(),
                        _ => {}
                    }
                    audio = Some(track);
                    audio_id = Some(i as u8);
                }
                _ => {}
            }
        }
        if video.is_none() && audio.is_none() {
            return Err("AVI file has no video or audio streams".into());
        }

        // keyframe flags of video chunks from the legacy index
        let mut keyframes = Vec::new();
        if let Some(vid) = video_id {
            let tag = [b'0' + vid / 10, b'0' + vid % 10];
            for e in idx1.chunks_exact(16) {
                if e[0..2] == tag && (e[2..4] == *b"dc" || e[2..4] == *b"db") {
                    keyframes.push(le32(e, 4) & 0x10 != 0);
                }
            }
        }

        let mut demux = AviDemuxer {
            file,
            info: MediaInfo { container: "AVI", duration, size, video, audio },
            movi,
            range: 0,
            pos: 0,
            video_id,
            audio_id,
            frame_dur,
            video_start,
            video_n: 0,
            keyframes,
            annexb,
            audio_mode,
            audio_vbr_dur,
            audio_avg_bytes,
            audio_bytes: 0,
            audio_chunks: 0,
            audio_start,
            audio_time: audio_start,
            pcm32,
            mpeg: mp3::Framer::default(),
            ac3: ac3::Framer::default(),
            queue: Default::default(),
        };
        demux.pos = demux.movi[0].0;
        demux.complete_info()?;
        Ok(demux)
    }

    /// Settings only the data carries: SPS/PPS of in-band H.264, VOL headers of MPEG-4, audio details.
    fn complete_info(&mut self) -> Result<(), String> {
        let need_avc = self.info.video.as_ref().map(|v| v.codec == VideoCodec::H264 && v.avc.is_none()).unwrap_or(false);
        let need_vol = self.info.video.as_ref().map(|v| v.codec == VideoCodec::Mpeg4 && v.extradata.is_empty()).unwrap_or(false);
        let need_audio = self.info.audio.as_ref().map(|a| matches!(a.codec, AudioCodec::Mpeg | AudioCodec::Ac3 | AudioCodec::Aac)).unwrap_or(false);
        let (mut got_v, mut got_a) = (!(need_avc || need_vol), !need_audio);
        let mut looked = 0;
        while (!got_v || !got_a) && looked < 400 {
            let Some((id, data)) = self.read_chunk()? else { break };
            looked += 1;
            if Some(id) == self.video_id && !got_v && !data.is_empty() {
                if let Some(v) = self.info.video.as_mut() {
                    if need_avc {
                        v.avc = h264::AvcConfig::from_nals(h264::split_annexb(&data));
                        got_v = v.avc.is_some();
                    } else {
                        // VOS/VO/VOL headers: everything before the first VOP start code
                        if let Some(p) = data.windows(4).position(|w| w == [0, 0, 1, 0xB6]) {
                            v.extradata = data[..p].to_vec();
                        }
                        got_v = true;
                    }
                }
            } else if Some(id) == self.audio_id && !got_a && !data.is_empty() {
                if let Some(a) = self.info.audio.as_mut() {
                    match a.codec {
                        AudioCodec::Mpeg => {
                            if let Some(p) = data.windows(4).position(|w| mp3::parse_header(w).is_some()) {
                                let layer = (data[p + 1] >> 1) & 3;
                                a.name = super::mpeg_audio_name(Some(4 - layer));
                                if let Some(fi) = mp3::parse_header(&data[p..]) {
                                    a.sample_rate = fi.rate;
                                    a.channels = fi.channels;
                                }
                            }
                        }
                        AudioCodec::Ac3 => {
                            if let Some(p) = data.windows(2).position(|w| w == [0x0B, 0x77]) {
                                if let Some(fi) = ac3::parse_header(&data[p..]) {
                                    a.sample_rate = fi.rate;
                                    a.channels = fi.channels;
                                    a.bitrate = fi.bitrate as u64;
                                }
                            }
                        }
                        AudioCodec::Aac => {
                            if let Some((asc, _)) = aac::parse_adts(&data) {
                                a.asc = asc;
                            }
                        }
                        _ => {}
                    }
                }
                got_a = true;
            }
        }
        // back to the start of the data
        self.range = 0;
        self.pos = self.movi[0].0;
        let abps = self.info.audio.as_ref().map(|a| a.bitrate).unwrap_or(0);
        if let Some(v) = self.info.video.as_mut() {
            v.bitrate = super::estimate_video_bitrate(self.info.size, self.info.duration, abps);
        }
        Ok(())
    }

    /// Next stream chunk of the movi lists: (stream number, data).
    fn read_chunk(&mut self) -> Result<Option<(u8, Vec<u8>)>, String> {
        loop {
            let Some(&(_, end)) = self.movi.get(self.range) else { return Ok(None) };
            if self.pos + 8 > end {
                self.range += 1;
                if let Some(&(start, _)) = self.movi.get(self.range) {
                    self.pos = start;
                }
                continue;
            }
            self.file.seek(SeekFrom::Start(self.pos)).map_err(|e| e.to_string())?;
            let mut h = [0u8; 8];
            if self.file.read_exact(&mut h).is_err() {
                return Ok(None);
            }
            let size = le32(&h, 4) as u64;
            if &h[0..4] == b"LIST" {
                // 'rec ' groups: step inside
                self.pos += 12;
                continue;
            }
            let next = self.pos + 8 + size + (size & 1);
            let digits = h[0].is_ascii_digit() && h[1].is_ascii_digit();
            if !digits || self.pos + 8 + size > end + 1 {
                self.pos = if next > self.pos { next } else { end };
                if !digits {
                    continue;
                }
                return Ok(None);
            }
            let id = (h[0] - b'0') * 10 + (h[1] - b'0');
            let mut data = vec![0u8; size as usize];
            self.file.read_exact(&mut data).map_err(|_| "AVI file is truncated".to_string())?;
            self.pos = next;
            return Ok(Some((id, data)));
        }
    }
}

impl Demuxer for AviDemuxer {
    fn info(&self) -> &MediaInfo {
        &self.info
    }

    fn next(&mut self) -> Result<Option<Packet>, String> {
        loop {
            if let Some(p) = self.queue.pop_front() {
                return Ok(Some(p));
            }
            let Some((id, data)) = self.read_chunk()? else { return Ok(None) };
            if Some(id) == self.video_id {
                let n = self.video_n;
                self.video_n += 1;
                if data.is_empty() {
                    continue; // dropped frame: time moves on
                }
                let pts = self.video_start + n as f64 * self.frame_dur;
                let codec = self.info.video.as_ref().map(|v| v.codec).unwrap_or(VideoCodec::Other);
                let (data, sniffed) = match codec {
                    VideoCodec::H264 => {
                        let d = if self.annexb { h264::join_prefixed(h264::split_annexb(&data)) } else { h264::normalize_prefixed(data, 4) };
                        let k = h264::has_idr(&d);
                        (d, k)
                    }
                    VideoCodec::Mpeg4 => {
                        // I-VOP: vop_coding_type 0 right after the VOP start code
                        let k = data.windows(5).any(|w| w[..4] == [0, 0, 1, 0xB6] && w[4] >> 6 == 0);
                        (data, k)
                    }
                    VideoCodec::Mjpeg => (data, true),
                    _ => (data, false),
                };
                let key = self.keyframes.get(n as usize).copied().unwrap_or(false) || sniffed || n == 0;
                return Ok(Some(Packet { kind: Kind::Video, pts, dts: Some(pts), dur: self.frame_dur, key, data }));
            }
            if Some(id) == self.audio_id && !data.is_empty() {
                let chunk_time = if self.audio_vbr_dur > 0.0 {
                    self.audio_start + self.audio_chunks as f64 * self.audio_vbr_dur
                } else {
                    self.audio_start + self.audio_bytes as f64 / self.audio_avg_bytes as f64
                };
                self.audio_chunks += 1;
                self.audio_bytes += data.len() as u64;
                match self.audio_mode {
                    AudioMode::Mpeg => {
                        for (frame, fi) in self.mpeg.push(&data) {
                            let d = fi.samples as f64 / fi.rate as f64;
                            self.queue.push_back(Packet { kind: Kind::Audio, pts: self.audio_time, dts: None, dur: d, key: true, data: frame });
                            self.audio_time += d;
                        }
                    }
                    AudioMode::Ac3 => {
                        for (frame, fi) in self.ac3.push(&data) {
                            let d = 1536.0 / fi.rate as f64;
                            self.queue.push_back(Packet { kind: Kind::Audio, pts: self.audio_time, dts: None, dur: d, key: true, data: frame });
                            self.audio_time += d;
                        }
                    }
                    AudioMode::Aac => {
                        let raw = match aac::parse_adts(&data) {
                            Some((_, h)) => data[h..].to_vec(),
                            None => data,
                        };
                        self.queue.push_back(Packet { kind: Kind::Audio, pts: chunk_time, dts: None, dur: self.audio_vbr_dur, key: true, data: raw });
                    }
                    AudioMode::Plain => {
                        let dur = if self.audio_vbr_dur > 0.0 { self.audio_vbr_dur } else { data.len() as f64 / self.audio_avg_bytes as f64 };
                        let data = if self.pcm32 { data.chunks_exact(4).flat_map(|c| [c[1], c[2], c[3]]).collect() } else { data };
                        self.queue.push_back(Packet { kind: Kind::Audio, pts: chunk_time, dts: None, dur, key: true, data });
                    }
                }
            }
        }
    }
}
