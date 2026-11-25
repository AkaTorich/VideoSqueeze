// Matroska / WebM reader (matroska-demuxer): first video and first audio track; codec IDs mapped to the
// program's codecs, VfW-in-Matroska (V_MS/VFW/FOURCC) and ACM (A_MS/ACM) unwrapped.
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use matroska_demuxer::{Frame, MatroskaFile, TrackType};

use super::{aac, ac3, h264, mp3, AudioCodec, AudioTrack, Demuxer, Kind, MediaInfo, Packet, VideoCodec, VideoTrack};

pub struct MkvDemuxer {
    mkv: MatroskaFile<BufReader<File>>,
    info: MediaInfo,
    video_no: Option<u64>,
    audio_no: Option<u64>,
    scale: f64,
    nal_len: Option<usize>,
    /// MPEG audio and AC-3 can come in laced or split blocks — re-split into frames.
    mpeg: Option<mp3::Framer>,
    ac3: Option<ac3::Framer>,
    pcm_big: Option<u16>,
    queue: std::collections::VecDeque<Packet>,
    frame: Frame,
}

impl MkvDemuxer {
    pub fn open(path: &Path) -> Result<Self, String> {
        let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
        let file = BufReader::new(File::open(path).map_err(|e| format!("cannot open file: {e}"))?);
        let mkv = MatroskaFile::open(file).map_err(|e| format!("cannot read Matroska file: {e}"))?;
        let scale = mkv.info().timestamp_scale().get() as f64 / 1e9;
        let duration = mkv.info().duration().map(|d| d * scale).unwrap_or(0.0);
        let container = if mkv.ebml_header().doc_type() == "webm" { "WEBM" } else { "MKV" };

        let mut video = None;
        let mut audio = None;
        let mut video_no = None;
        let mut audio_no = None;
        let mut nal_len = None;
        let mut mpeg = None;
        let mut ac3f = None;
        let mut pcm_big = None;

        for t in mkv.tracks() {
            let id = t.codec_id();
            let private = t.codec_private().map(|p| p.to_vec()).unwrap_or_default();
            match t.track_type() {
                TrackType::Video if video.is_none() => {
                    let v = t.video();
                    let width = v.map(|v| v.pixel_width().get() as u32).unwrap_or(0);
                    let height = v.map(|v| v.pixel_height().get() as u32).unwrap_or(0);
                    let fps = t.default_duration().map(|d| 1e9 / d.get() as f64).unwrap_or(0.0);
                    let mut track = VideoTrack {
                        codec: VideoCodec::Other,
                        name: id.trim_start_matches("V_").to_string(),
                        width,
                        height,
                        fps,
                        bitrate: 0,
                        avc: None,
                        extradata: Vec::new(),
                        fourcc: *b"    ",
                        matrix: None,
                        mkv_codec: Some((id.to_string(), private.clone())),
                    };
                    match id {
                        "V_MPEG4/ISO/AVC" => {
                            track.codec = VideoCodec::H264;
                            track.name = "H.264".into();
                            track.fourcc = *b"H264";
                            if let Some((cfg, n)) = h264::AvcConfig::parse(&private) {
                                track.avc = Some(cfg);
                                nal_len = Some(n);
                            } else {
                                nal_len = Some(4);
                            }
                        }
                        "V_MPEGH/ISO/HEVC" => {
                            track.codec = VideoCodec::Hevc;
                            track.name = "HEVC".into();
                            track.fourcc = *b"HEVC";
                            let mut h = private.clone();
                            nal_len = Some(h.get(21).map(|b| (b & 3) as usize + 1).unwrap_or(4));
                            if h.len() > 21 {
                                h[21] |= 3;
                            }
                            track.extradata = h;
                        }
                        "V_MPEG4/ISO/ASP" | "V_MPEG4/ISO/SP" | "V_MPEG4/ISO/AP" => {
                            track.codec = VideoCodec::Mpeg4;
                            track.name = "MPEG-4 PART 2".into();
                            track.fourcc = *b"XVID";
                            track.extradata = private.clone();
                        }
                        "V_MPEG4/MS/V3" => {
                            track.codec = VideoCodec::MsMpeg4V3;
                            track.name = "DIVX 3".into();
                            track.fourcc = *b"DIV3";
                        }
                        "V_MJPEG" => {
                            track.codec = VideoCodec::Mjpeg;
                            track.name = "MJPEG".into();
                            track.fourcc = *b"MJPG";
                        }
                        "V_MS/VFW/FOURCC" if private.len() >= 40 => {
                            // BITMAPINFOHEADER: compression FourCC at 16, codec setup after the 40-byte header
                            let fcc: [u8; 4] = private[16..20].try_into().unwrap();
                            let (codec, name) = super::video_by_fourcc(&fcc);
                            track.codec = codec;
                            track.name = name;
                            track.fourcc = fcc;
                            track.extradata = private[40..].to_vec();
                            if codec == VideoCodec::H264 {
                                if let Some((cfg, n)) = h264::AvcConfig::parse(&private[40..]) {
                                    track.avc = Some(cfg);
                                    nal_len = Some(n);
                                } else {
                                    nal_len = Some(0); // Annex B inside VfW
                                }
                            }
                        }
                        "V_VP8" => track.name = "VP8".into(),
                        "V_VP9" => track.name = "VP9".into(),
                        "V_AV1" => track.name = "AV1".into(),
                        "V_MPEG2" => track.name = "MPEG-2".into(),
                        _ => {}
                    }
                    video_no = Some(t.track_number().get());
                    video = Some(track);
                }
                TrackType::Audio if audio.is_none() => {
                    let a = t.audio();
                    let rate = a.map(|a| a.sampling_frequency() as u32).unwrap_or(48000);
                    let channels = a.map(|a| a.channels().get() as u16).unwrap_or(2);
                    let bits = a.and_then(|a| a.bit_depth()).map(|b| b.get() as u16).unwrap_or(16);
                    let mut track = AudioTrack {
                        codec: AudioCodec::Other,
                        name: id.trim_start_matches("A_").to_string(),
                        sample_rate: rate,
                        channels,
                        bits,
                        bitrate: 0,
                        asc: Vec::new(),
                        mkv_codec: Some((id.to_string(), private.clone())),
                    };
                    if id.starts_with("A_AAC") {
                        track.codec = AudioCodec::Aac;
                        track.name = "AAC".into();
                        track.asc = if private.len() >= 2 {
                            private.clone()
                        } else {
                            let obj = if id.contains("SBR") { 5 } else if id.contains("MAIN") { 1 } else { 2 };
                            aac::asc(obj, rate, channels)
                        };
                        if let Some((obj, r, ch)) = aac::parse_asc(&track.asc) {
                            if obj == 5 || obj == 29 {
                                track.name = "HE-AAC".into();
                            }
                            track.sample_rate = r;
                            if ch > 0 {
                                track.channels = ch;
                            }
                        }
                    } else if id == "A_MPEG/L3" || id == "A_MPEG/L2" || id == "A_MPEG/L1" {
                        track.codec = AudioCodec::Mpeg;
                        track.name = super::mpeg_audio_name(id.chars().last().and_then(|c| c.to_digit(10)).map(|d| d as u8));
                        mpeg = Some(mp3::Framer::default());
                    } else if id == "A_AC3" {
                        track.codec = AudioCodec::Ac3;
                        track.name = "AC-3".into();
                        ac3f = Some(ac3::Framer::default());
                    } else if id == "A_PCM/INT/LIT" {
                        track.codec = AudioCodec::Pcm;
                        track.name = "PCM".into();
                    } else if id == "A_PCM/INT/BIG" {
                        track.codec = AudioCodec::Pcm;
                        track.name = "PCM".into();
                        pcm_big = Some(bits);
                    } else if id == "A_OPUS" {
                        track.codec = AudioCodec::Opus;
                        track.name = "OPUS".into();
                    } else if id == "A_VORBIS" {
                        track.codec = AudioCodec::Vorbis;
                        track.name = "VORBIS".into();
                    } else if id == "A_EAC3" {
                        track.name = "E-AC-3".into();
                    } else if id == "A_DTS" {
                        track.name = "DTS".into();
                    } else if id == "A_FLAC" {
                        track.name = "FLAC".into();
                    }
                    audio_no = Some(t.track_number().get());
                    audio = Some(track);
                }
                _ => {}
            }
        }
        if video.is_none() && audio.is_none() {
            return Err("file has no video or audio tracks".into());
        }
        if let Some(v) = video.as_mut() {
            let abps = audio.as_ref().map(|a| a.bitrate).unwrap_or(0).max(if audio.is_some() { 128_000 } else { 0 });
            v.bitrate = super::estimate_video_bitrate(size, duration, abps);
        }
        Ok(MkvDemuxer {
            mkv,
            info: MediaInfo { container, duration, size, video, audio },
            video_no,
            audio_no,
            scale,
            nal_len,
            mpeg,
            ac3: ac3f,
            pcm_big,
            queue: Default::default(),
            frame: Frame::default(),
        })
    }
}

impl Demuxer for MkvDemuxer {
    fn info(&self) -> &MediaInfo {
        &self.info
    }

    fn next(&mut self) -> Result<Option<Packet>, String> {
        loop {
            if let Some(p) = self.queue.pop_front() {
                return Ok(Some(p));
            }
            let more = self.mkv.next_frame(&mut self.frame).map_err(|e| format!("read error: {e}"))?;
            if !more {
                return Ok(None);
            }
            let pts = self.frame.timestamp as f64 * self.scale;
            let dur = self.frame.duration.map(|d| d as f64 * self.scale).unwrap_or(0.0);
            let data = std::mem::take(&mut self.frame.data);
            if Some(self.frame.track) == self.video_no {
                let (data, idr) = match self.nal_len {
                    Some(0) => {
                        let nals = h264::split_annexb(&data);
                        let idr = nals.iter().any(|n| h264::nal_type(n) == h264::NAL_IDR);
                        (h264::join_prefixed(nals), idr)
                    }
                    Some(n) => {
                        let d = h264::normalize_prefixed(data, n);
                        let idr = self.info.video.as_ref().map(|v| v.codec == VideoCodec::H264).unwrap_or(false) && h264::has_idr(&d);
                        (d, idr)
                    }
                    None => (data, false),
                };
                let key = self.frame.is_keyframe.unwrap_or(false) || idr;
                return Ok(Some(Packet { kind: Kind::Video, pts, dts: None, dur, key, data }));
            }
            if Some(self.frame.track) == self.audio_no {
                if let Some(f) = self.mpeg.as_mut() {
                    let mut t = pts;
                    for (frame, fi) in f.push(&data) {
                        let d = fi.samples as f64 / fi.rate as f64;
                        self.queue.push_back(Packet { kind: Kind::Audio, pts: t, dts: Some(t), dur: d, key: true, data: frame });
                        t += d;
                    }
                    continue;
                }
                if let Some(f) = self.ac3.as_mut() {
                    let mut t = pts;
                    for (frame, fi) in f.push(&data) {
                        let d = 1536.0 / fi.rate as f64;
                        self.queue.push_back(Packet { kind: Kind::Audio, pts: t, dts: Some(t), dur: d, key: true, data: frame });
                        t += d;
                    }
                    continue;
                }
                let data = match self.pcm_big {
                    Some(bits) if bits > 8 => {
                        let mut d = data;
                        for c in d.chunks_exact_mut((bits / 8) as usize) {
                            c.reverse();
                        }
                        d
                    }
                    _ => data,
                };
                return Ok(Some(Packet { kind: Kind::Audio, pts, dts: Some(pts), dur, key: true, data }));
            }
        }
    }
}
