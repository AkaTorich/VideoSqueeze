// One conversion from start to end: read packets, copy or decode → turn → scale → limit the rate → encode,
// write into the chosen container. A half-written file is removed on error or stop.
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;

use super::gate::Gate;
use super::plan::{self, AudioAction, Container, Plan, VideoAction};
use super::settings::Settings;
use crate::codec::audio::{AacEncoder, AudioDecoder};
use crate::codec::encode::H264Encoder;
use crate::codec::video::VideoDecoder;
use crate::codec::{scale, Frame};
use crate::media::{self, h264, AudioCodec, AudioTrack, Kind, MediaInfo, Packet, VideoCodec, VideoTrack};
use crate::mux::avi_out::AviMuxer;
use crate::mux::gif_out::GifWriter;
use crate::mux::mkv_out::MkvMuxer;
use crate::mux::mp4_out::{Brand, Mp4Muxer};
use crate::mux::raw_out::{FrameFile, WavWriter};
use crate::mux::{AudioConfig, AudioOut, Muxer, OutPacket, VideoConfig, VideoOut};

/// Stages shown in the window: 0 — opening, 1 — converting, 2 — writing the file out.
pub trait Reporter {
    fn progress(&self, stage: usize, fraction: f64, speed: f64);
    fn cancelled(&self) -> bool;
}

pub const CANCELLED: &str = "cancelled";

#[derive(Serialize, Clone, Debug)]
pub struct JobResult {
    pub out_path: String,
    pub out_size: u64,
    pub in_size: u64,
    pub seconds: f64,
    pub frames: u64,
    pub notes: Vec<String>,
}

pub fn run(path: &Path, settings: &Settings, rep: &dyn Reporter) -> Result<JobResult, String> {
    rep.progress(0, 0.0, 0.0);
    let mut demux = media::open(path)?;
    let info = demux.info().clone();
    let mut plan = plan::make(path, &info, settings)?;
    if let Some(dir) = plan.out_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create output folder: {e}"))?;
    }
    plan.out_path = plan::unique(&plan.out_path);
    let started = Instant::now();
    let mut ctx = Ctx { rep, info: &info, started, frames: 0, notes: plan.notes.clone() };

    let outcome = match plan.container {
        Container::Gif => run_gif(&mut *demux, &plan, &mut ctx),
        Container::Mp3 | Container::Ac3 | Container::Wav => run_raw(&mut *demux, &plan, &mut ctx),
        _ => run_av(&mut *demux, &plan, &mut ctx),
    };
    if let Err(e) = outcome {
        let _ = std::fs::remove_file(&plan.out_path);
        return Err(e);
    }
    rep.progress(2, 1.0, 0.0);
    let out_size = std::fs::metadata(&plan.out_path).map(|m| m.len()).unwrap_or(0);
    Ok(JobResult {
        out_path: plan.out_path.to_string_lossy().into_owned(),
        out_size,
        in_size: info.size,
        seconds: started.elapsed().as_secs_f64(),
        frames: ctx.frames,
        notes: ctx.notes,
    })
}

struct Ctx<'a> {
    rep: &'a dyn Reporter,
    info: &'a MediaInfo,
    started: Instant,
    frames: u64,
    notes: Vec<String>,
}

impl Ctx<'_> {
    fn check(&self) -> Result<(), String> {
        if self.rep.cancelled() {
            Err(CANCELLED.into())
        } else {
            Ok(())
        }
    }

    /// Share of the work done by media time `t`; `weight` — the part of the bar the main pass takes.
    fn tick(&self, t: f64, weight: f64) {
        let d = self.info.duration.max(0.001);
        let speed = self.frames as f64 / self.started.elapsed().as_secs_f64().max(0.001);
        self.rep.progress(1, (t / d).clamp(0.0, 1.0) * weight, speed);
    }
}

/// Picture pipeline after decoding: turn, scale, rate gate.
struct Picture {
    rotate: u32,
    width: usize,
    height: usize,
    gate: Gate,
}

impl Picture {
    fn process(&mut self, f: Frame) -> Vec<Frame> {
        let f = if self.rotate != 0 { scale::rotate_frame(&f, self.rotate) } else { f };
        let f = scale::scale_frame(&f, self.width, self.height);
        self.gate.push(f)
    }
}

// ── video containers (MP4, MOV, M4A, MKV, MKA, AVI) ─────

fn audio_out(a: &AudioTrack) -> AudioOut {
    let config = match a.codec {
        AudioCodec::Aac => AudioConfig::Aac(a.asc.clone()),
        AudioCodec::Mpeg => AudioConfig::Mpeg(match a.name.as_str() {
            "MP1" => 1,
            "MP2" => 2,
            _ => 3,
        }),
        AudioCodec::Ac3 => AudioConfig::Ac3,
        AudioCodec::Pcm => AudioConfig::Pcm(a.bits),
        _ => {
            let (id, private) = a.mkv_codec.clone().unwrap_or_default();
            AudioConfig::Mkv(id, private)
        }
    };
    AudioOut { config, rate: a.sample_rate, channels: a.channels, bitrate: a.bitrate }
}

/// Container setup for copying the source video; `first` — a packet to dig settings out of when the
/// container had none (in-band SPS/PPS, VOL headers).
fn copy_config(v: &VideoTrack, first: Option<&[u8]>) -> Option<VideoConfig> {
    match v.codec {
        VideoCodec::H264 => v.avc.clone().or_else(|| first.and_then(|d| h264::AvcConfig::from_nals(h264::split_prefixed(d, 4)))).map(VideoConfig::Avc),
        VideoCodec::Hevc => Some(VideoConfig::Hevc(v.extradata.clone())),
        VideoCodec::Mpeg4 => {
            if !v.extradata.is_empty() {
                return Some(VideoConfig::Mpeg4(v.extradata.clone()));
            }
            let d = first?;
            let vol = d.windows(4).position(|w| w == [0, 0, 1, 0xB6]).map(|p| d[..p].to_vec()).unwrap_or_default();
            Some(VideoConfig::Mpeg4(vol))
        }
        VideoCodec::Mjpeg => Some(VideoConfig::Mjpeg),
        VideoCodec::MsMpeg4V1 | VideoCodec::MsMpeg4V2 | VideoCodec::MsMpeg4V3 => Some(VideoConfig::MsMpeg4),
        VideoCodec::Other => Some(match &v.mkv_codec {
            Some((id, p)) => VideoConfig::Mkv(id.clone(), p.clone()),
            None => VideoConfig::MsMpeg4,
        }),
    }
}

struct Av<'p> {
    plan: &'p Plan,
    muxer: Option<Box<dyn Muxer>>,
    vcfg: Option<VideoOut>,
    acfg: Option<AudioOut>,
    need_video: bool,
    need_audio: bool,
    pending: Vec<(bool, OutPacket)>,
}

impl Av<'_> {
    fn ready(&self) -> bool {
        (!self.need_video || self.vcfg.is_some()) && (!self.need_audio || self.acfg.is_some())
    }

    fn open(&mut self) -> Result<(), String> {
        let path = &self.plan.out_path;
        let v = self.vcfg.clone();
        let a = self.acfg.clone();
        let m: Box<dyn Muxer> = match self.plan.container {
            Container::Mp4 => Box::new(Mp4Muxer::create(path, Brand::Mp4, v, a)?),
            Container::Mov => Box::new(Mp4Muxer::create(path, Brand::Mov, v, a)?),
            Container::M4a => Box::new(Mp4Muxer::create(path, Brand::M4a, None, a)?),
            Container::Mkv | Container::Mka => Box::new(MkvMuxer::create(path, v, a)?),
            Container::Avi => Box::new(AviMuxer::create(path, v, a)?),
            _ => return Err("internal: wrong container".into()),
        };
        self.muxer = Some(m);
        // what waited for the track setup goes out in time order
        let mut pending = std::mem::take(&mut self.pending);
        pending.sort_by(|a, b| a.1.dts.unwrap_or(a.1.pts).partial_cmp(&b.1.dts.unwrap_or(b.1.pts)).unwrap_or(std::cmp::Ordering::Equal));
        for (video, p) in pending {
            self.put(video, p)?;
        }
        Ok(())
    }

    fn put(&mut self, video: bool, p: OutPacket) -> Result<(), String> {
        if self.muxer.is_none() {
            self.pending.push((video, p));
            if self.ready() {
                self.open()?;
            }
            return Ok(());
        }
        let m = self.muxer.as_mut().unwrap();
        if video {
            m.write_video(&p)
        } else {
            m.write_audio(&p)
        }
    }
}

fn run_av(demux: &mut dyn media::Demuxer, plan: &Plan, ctx: &mut Ctx) -> Result<(), String> {
    let info = ctx.info;
    let mut av = Av {
        plan,
        muxer: None,
        vcfg: None,
        acfg: None,
        need_video: plan.video != VideoAction::None,
        need_audio: plan.audio != AudioAction::None,
        pending: Vec::new(),
    };

    // video side
    let vtrack = info.video.clone();
    let mut vdec: Option<VideoDecoder> = None;
    let mut venc: Option<H264Encoder> = None;
    let mut pic: Option<Picture> = None;
    let mut out_fps = 25.0;
    let mut out_bitrate = 0u64;
    match (&plan.video, vtrack.as_ref()) {
        (VideoAction::Encode { width, height, fps, bitrate, rotate, cfr }, Some(v)) => {
            vdec = Some(VideoDecoder::new(v)?);
            pic = Some(Picture { rotate: *rotate, width: *width as usize, height: *height as usize, gate: Gate::new(*fps, *cfr) });
            out_fps = *fps;
            out_bitrate = *bitrate;
        }
        (VideoAction::Copy, Some(v)) => {
            if let Some(cfg) = copy_config(v, None) {
                av.vcfg = Some(VideoOut { config: cfg, width: v.width, height: v.height, fps: v.fps, matrix: v.matrix, fourcc: v.fourcc });
            }
        }
        _ => {}
    }
    let keep_matrix = matches!(plan.container, Container::Mp4 | Container::Mov);

    // audio side
    let atrack = info.audio.clone();
    let mut adec: Option<AudioDecoder> = None;
    let mut aenc: Option<AacEncoder> = None;
    let mut aac_kbps = 0;
    let mut audio_t0: Option<f64> = None;
    match (&plan.audio, atrack.as_ref()) {
        (AudioAction::Copy, Some(a)) => av.acfg = Some(audio_out(a)),
        (AudioAction::Encode(k), Some(a)) => {
            adec = Some(AudioDecoder::new(a)?);
            aac_kbps = *k;
        }
        _ => {}
    }

    let weight = if matches!(plan.video, VideoAction::Encode { .. }) || matches!(plan.audio, AudioAction::Encode(_)) { 0.97 } else { 0.8 };
    let mut last_t = 0f64;

    // encoded picture → packet
    let encode = |venc: &mut Option<H264Encoder>, av: &mut Av, f: &Frame, frames: &mut u64| -> Result<(), String> {
        if venc.is_none() {
            let keyint = (out_fps * 5.0).round().max(1.0) as u32;
            *venc = Some(H264Encoder::new(out_bitrate, out_fps, keyint)?);
        }
        let enc = venc.as_mut().unwrap();
        if let Some(e) = enc.encode(f)? {
            *frames += 1;
            if av.vcfg.is_none() {
                if let Some(cfg) = enc.config.clone() {
                    let v = vtrack.as_ref().unwrap();
                    av.vcfg = Some(VideoOut {
                        config: VideoConfig::Avc(cfg),
                        width: f.w as u32,
                        height: f.h as u32,
                        fps: out_fps,
                        matrix: if keep_matrix { v.matrix } else { None },
                        fourcc: *b"H264",
                    });
                }
            }
            av.put(true, OutPacket { pts: f.pts, dts: Some(f.pts), dur: 1.0 / out_fps, key: e.key, data: e.data })?;
        }
        Ok(())
    };

    // containers with an edit list keep the encoder's priming and skip it on playback; the others drop it
    let keeps_priming = matches!(plan.container, Container::Mp4 | Container::Mov | Container::M4a);
    let put_units = |aenc: &AacEncoder, av: &mut Av, t0: f64, units: Vec<Vec<u8>>| -> Result<(), String> {
        let first = aenc.frames_out - units.len() as u64;
        for (k, frame) in units.into_iter().enumerate() {
            let pts = aenc.unit_pts(t0, first + k as u64);
            if pts < t0 - 1e-6 && !keeps_priming {
                continue;
            }
            av.put(false, OutPacket { pts, dts: None, dur: aenc.frame_dur(), key: true, data: frame })?;
        }
        Ok(())
    };
    let audio_encode = |aenc: &mut AacEncoder, av: &mut Av, t0: f64, samples: &[i16], channels: u16| -> Result<(), String> {
        let units = aenc.push(samples, channels)?;
        put_units(aenc, av, t0, units)
    };

    while let Some(p) = demux.next()? {
        ctx.check()?;
        let Packet { kind, .. } = p;
        match kind {
            Kind::Video => match &plan.video {
                VideoAction::Copy => {
                    if av.vcfg.is_none() {
                        let v = vtrack.as_ref().unwrap();
                        if let Some(cfg) = copy_config(v, Some(&p.data)) {
                            av.vcfg = Some(VideoOut { config: cfg, width: v.width, height: v.height, fps: v.fps, matrix: v.matrix, fourcc: v.fourcc });
                        }
                    }
                    ctx.frames += 1;
                    last_t = last_t.max(p.pts);
                    av.put(true, OutPacket { pts: p.pts, dts: p.dts, dur: p.dur, key: p.key, data: p.data })?;
                }
                VideoAction::Encode { .. } => {
                    let frames = vdec.as_mut().unwrap().feed(&p)?;
                    for f in frames {
                        for g in pic.as_mut().unwrap().process(f) {
                            last_t = last_t.max(g.pts);
                            encode(&mut venc, &mut av, &g, &mut ctx.frames)?;
                        }
                    }
                }
                _ => {}
            },
            Kind::Audio => match &plan.audio {
                AudioAction::Copy => {
                    if plan.video == VideoAction::None {
                        last_t = last_t.max(p.pts);
                    }
                    av.put(false, OutPacket { pts: p.pts, dts: p.dts, dur: p.dur, key: true, data: p.data })?;
                }
                AudioAction::Encode(_) => {
                    let dec = adec.as_mut().unwrap();
                    let samples = dec.decode(&p.data);
                    if samples.is_empty() {
                        continue;
                    }
                    if aenc.is_none() {
                        let e = AacEncoder::new(dec.rate, dec.channels, aac_kbps * 1000)?;
                        av.acfg = Some(AudioOut { config: AudioConfig::Aac(e.asc()), rate: e.rate, channels: e.channels, bitrate: aac_kbps as u64 * 1000 });
                        audio_t0 = Some(p.pts);
                        aenc = Some(e);
                        if av.muxer.is_none() && av.ready() && !av.pending.is_empty() {
                            av.open()?;
                        }
                    }
                    if plan.video == VideoAction::None {
                        last_t = last_t.max(p.pts);
                    }
                    let ch = dec.channels;
                    audio_encode(aenc.as_mut().unwrap(), &mut av, audio_t0.unwrap(), &samples, ch)?;
                }
                _ => {}
            },
        }
        ctx.tick(last_t, weight);
    }

    // drain
    if let (Some(dec), Some(p)) = (vdec.as_mut(), pic.as_mut()) {
        for f in dec.finish()? {
            for g in p.process(f) {
                encode(&mut venc, &mut av, &g, &mut ctx.frames)?;
            }
        }
    }
    if let Some(e) = aenc.as_mut() {
        let units = e.finish()?;
        put_units(e, &mut av, audio_t0.unwrap_or(0.0), units)?;
    }
    if av.muxer.is_none() {
        if av.need_video && av.vcfg.is_none() {
            return Err("no picture could be decoded from this file".into());
        }
        if av.need_audio && av.acfg.is_none() {
            av.need_audio = false;
            av.pending.retain(|(video, _)| *video);
            ctx.notes.push("AUDIO COULD NOT BE DECODED — LEFT OUT".into());
        }
        av.open()?;
    }
    let m = av.muxer.take().unwrap();
    let rep = ctx.rep;
    rep.progress(2, weight, 0.0);
    m.finish(&|f| rep.progress(2, weight + (1.0 - weight) * f, 0.0))
}

// ── GIF ─────────────────────────────────────────────────

fn run_gif(demux: &mut dyn media::Demuxer, plan: &Plan, ctx: &mut Ctx) -> Result<(), String> {
    let VideoAction::Gif { width, height, fps, rotate } = plan.video.clone() else { return Err("internal: no GIF plan".into()) };
    let v = ctx.info.video.clone().ok_or("file has no video")?;
    let mut dec = VideoDecoder::new(&v)?;
    let mut pic = Picture { rotate, width: width as usize, height: height as usize, gate: Gate::new(fps, false) };
    let mut gif = GifWriter::create(&plan.out_path, width as usize, height as usize)?;
    let mut last_t = 0f64;
    while let Some(p) = demux.next()? {
        ctx.check()?;
        if p.kind != Kind::Video {
            continue;
        }
        for f in dec.feed(&p)? {
            for g in pic.process(f) {
                last_t = last_t.max(g.pts);
                ctx.frames += 1;
                gif.push(&g)?;
            }
        }
        ctx.tick(last_t, 0.99);
    }
    for f in dec.finish()? {
        for g in pic.process(f) {
            ctx.frames += 1;
            gif.push(&g)?;
        }
    }
    if ctx.frames == 0 {
        return Err("no picture could be decoded from this file".into());
    }
    gif.finish()
}

// ── raw audio files (MP3, AC-3, WAV) ────────────────────

fn run_raw(demux: &mut dyn media::Demuxer, plan: &Plan, ctx: &mut Ctx) -> Result<(), String> {
    let a = ctx.info.audio.clone().ok_or("file has no audio")?;
    let path: PathBuf = plan.out_path.clone();
    let mut frames_file = None;
    let mut wav = None;
    let mut dec = None;
    match (plan.container, &plan.audio) {
        (Container::Wav, AudioAction::Decode) => {
            dec = Some(AudioDecoder::new(&a)?);
            wav = Some(WavWriter::create(&path, a.sample_rate, a.channels, 16)?);
        }
        (Container::Wav, _) => wav = Some(WavWriter::create(&path, a.sample_rate, a.channels, a.bits.max(8))?),
        _ => frames_file = Some(FrameFile::create(&path)?),
    }
    let mut any = false;
    while let Some(p) = demux.next()? {
        ctx.check()?;
        if p.kind != Kind::Audio {
            continue;
        }
        if let Some(d) = dec.as_mut() {
            let s = d.decode(&p.data);
            if !s.is_empty() {
                let w = wav.as_mut().unwrap();
                w.rate = d.rate;
                w.channels = d.channels;
                w.write_i16(&s)?;
                any = true;
            }
        } else if let Some(w) = wav.as_mut() {
            w.write(&p.data)?;
            any = true;
        } else if let Some(f) = frames_file.as_mut() {
            f.write(&p.data)?;
            any = true;
        }
        ctx.tick(p.pts, 0.99);
    }
    if !any {
        return Err("no audio could be read from this file".into());
    }
    if let Some(w) = wav {
        w.finish()?;
    }
    if let Some(f) = frames_file {
        f.finish()?;
    }
    Ok(())
}
