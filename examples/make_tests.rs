// Test files with known content for checking VideoSqueeze without other software: a moving picture
// (gradient, travelling box, a white bar per second) and a tone per channel. Written into target/testfiles.
// Run: cargo run --release --example make_tests
use std::path::{Path, PathBuf};

use videosqueeze::codec::audio::AacEncoder;
use videosqueeze::codec::encode::H264Encoder;
use videosqueeze::codec::{color, Frame};
use videosqueeze::mux::avi_out::AviMuxer;
use videosqueeze::mux::mkv_out::MkvMuxer;
use videosqueeze::mux::mp4_out::{Brand, Mp4Muxer};
use videosqueeze::mux::{AudioConfig, AudioOut, Muxer, OutPacket, VideoConfig, VideoOut};

fn picture(i: usize, fps: f64, w: usize, h: usize) -> Frame {
    let t = i as f64 / fps;
    let mut rgb = vec![0u8; w * h * 3];
    let bx = ((t * 0.4).fract() * (w as f64 - h as f64 / 4.0)) as usize;
    for y in 0..h {
        for x in 0..w {
            let p = (y * w + x) * 3;
            rgb[p] = (x * 255 / w) as u8;
            rgb[p + 1] = (y * 255 / h) as u8;
            rgb[p + 2] = ((t * 60.0) as usize % 256) as u8;
            let size = h / 4;
            if x >= bx && x < bx + size && y >= h / 3 && y < h / 3 + size {
                rgb[p] = 255;
                rgb[p + 1] = 230;
                rgb[p + 2] = 10;
            }
            // seconds counter: one white bar per whole second at the bottom
            let sec = t as usize;
            if y > h - h / 12 && x < (sec + 1) * w / 12 && x % (w / 12) > 4 {
                rgb[p] = 255;
                rgb[p + 1] = 255;
                rgb[p + 2] = 255;
            }
        }
    }
    let mut f = color::rgb_to_frame(&rgb, w, h, 3, false);
    f.pts = t;
    f
}

fn tone(rate: u32, channels: u16, from: usize, count: usize) -> Vec<i16> {
    let mut out = Vec::with_capacity(count * channels as usize);
    for n in from..from + count {
        let t = n as f64 / rate as f64;
        for c in 0..channels {
            let freq = 440.0 * (1.0 + c as f64 * 0.5);
            out.push(((2.0 * std::f64::consts::PI * freq * t).sin() * 9000.0) as i16);
        }
    }
    out
}

fn h264(out: &Path, container: &str, w: usize, h: usize, fps: f64, secs: f64, matrix: Option<[i32; 9]>) {
    let frames = (secs * fps) as usize;
    let mut enc = H264Encoder::new(2_000_000, fps, (fps * 2.0) as u32).unwrap();
    let mut aac = AacEncoder::new(48000, 2, 160_000).unwrap();
    let mut video = Vec::new();
    let mut cfg = None;
    for i in 0..frames {
        let f = picture(i, fps, w, h);
        if let Some(e) = enc.encode(&f).unwrap() {
            video.push(OutPacket { pts: f.pts, dts: Some(f.pts), dur: 1.0 / fps, key: e.key, data: e.data });
        }
        if cfg.is_none() {
            cfg = enc.config.clone();
        }
    }
    let samples = (secs * 48000.0) as usize;
    let mut audio = Vec::new();
    let units: Vec<Vec<u8>> = aac.push(&tone(48000, 2, 0, samples), 2).unwrap().into_iter().chain(aac.finish().unwrap()).collect();
    for (n, frame) in units.into_iter().enumerate() {
        let pts = aac.unit_pts(0.0, n as u64);
        if container == "mkv" && pts < 0.0 {
            continue;
        }
        audio.push(OutPacket { pts, dts: None, dur: 1024.0 / 48000.0, key: true, data: frame });
    }
    let v = VideoOut { config: VideoConfig::Avc(cfg.unwrap()), width: w as u32, height: h as u32, fps, matrix, fourcc: *b"H264" };
    let a = AudioOut { config: AudioConfig::Aac(aac.asc()), rate: 48000, channels: 2, bitrate: 160_000 };
    let mut m: Box<dyn Muxer> = match container {
        "mkv" => Box::new(MkvMuxer::create(out, Some(v), Some(a)).unwrap()),
        _ => Box::new(Mp4Muxer::create(out, Brand::Mp4, Some(v), Some(a)).unwrap()),
    };
    interleave(&mut *m, video, audio);
    m.finish(&|_| {}).unwrap();
}

fn interleave(m: &mut dyn Muxer, video: Vec<OutPacket>, audio: Vec<OutPacket>) {
    let (mut vi, mut ai) = (0, 0);
    while vi < video.len() || ai < audio.len() {
        let take_video = ai >= audio.len() || (vi < video.len() && video[vi].dts.unwrap_or(video[vi].pts) <= audio[ai].pts);
        if take_video {
            m.write_video(&video[vi]).unwrap();
            vi += 1;
        } else {
            m.write_audio(&audio[ai]).unwrap();
            ai += 1;
        }
    }
}

fn pcm_packets(rate: u32, channels: u16, secs: f64, chunk: usize) -> Vec<OutPacket> {
    let total = (secs * rate as f64) as usize;
    let mut out = Vec::new();
    let mut n = 0;
    while n < total {
        let count = chunk.min(total - n);
        let data: Vec<u8> = tone(rate, channels, n, count).iter().flat_map(|s| s.to_le_bytes()).collect();
        out.push(OutPacket { pts: n as f64 / rate as f64, dts: None, dur: count as f64 / rate as f64, key: true, data });
        n += count;
    }
    out
}

/// MPEG-4 Part 2 (XviD) or MS-MPEG4 v3 (DivX 3) through the oxideav encoders, into AVI with PCM audio.
fn oxideav_avi(out: &Path, codec: &str, fourcc: [u8; 4], w: usize, h: usize, fps: f64, secs: f64, b_frames: u32) {
    use oxideav_core::{CodecId, CodecParameters, Frame as CoreFrame, Rational, VideoFrame, VideoPlane};
    let mut params = CodecParameters::video(CodecId::new(codec));
    params.width = Some(w as u32);
    params.height = Some(h as u32);
    params.frame_rate = Some(Rational::new(fps as i64, 1));
    params.pixel_format = Some(oxideav_core::PixelFormat::Yuv420P);
    let mut enc: Box<dyn oxideav_core::Encoder> = if codec == "mpeg4video" {
        params.options = oxideav_core::CodecOptions::default().set("bf", b_frames.to_string()).set("qp", "4");
        oxideav_mpeg4video::encoder::make_encoder(&params).unwrap()
    } else {
        params.options = oxideav_core::CodecOptions::default();
        oxideav_msmpeg4::encoder::make_encoder(&params).unwrap()
    };
    let frames = (secs * fps) as usize;
    let mut video = Vec::new();
    let take = |enc: &mut Box<dyn oxideav_core::Encoder>, video: &mut Vec<OutPacket>| {
        while let Ok(p) = enc.receive_packet() {
            let n = video.len();
            video.push(OutPacket { pts: n as f64 / fps, dts: Some(n as f64 / fps), dur: 1.0 / fps, key: p.flags.keyframe, data: p.data });
        }
    };
    for i in 0..frames {
        let f = picture(i, fps, w, h);
        let cw = f.cw();
        let frame = CoreFrame::Video(VideoFrame {
            pts: Some(i as i64),
            planes: vec![VideoPlane { stride: w, data: f.y }, VideoPlane { stride: cw, data: f.u }, VideoPlane { stride: cw, data: f.v }],
        });
        enc.send_frame(&frame).unwrap();
        take(&mut enc, &mut video);
    }
    enc.flush().unwrap();
    take(&mut enc, &mut video);
    let extradata = Vec::new();
    let config = if codec == "mpeg4video" { VideoConfig::Mpeg4(extradata) } else { VideoConfig::MsMpeg4 };
    let v = VideoOut { config, width: w as u32, height: h as u32, fps, matrix: None, fourcc };
    let a = AudioOut { config: AudioConfig::Pcm(16), rate: 44100, channels: 2, bitrate: 44100 * 32 };
    let mut m = AviMuxer::create(out, Some(v), Some(a)).unwrap();
    interleave(&mut m, video, pcm_packets(44100, 2, secs, 4410));
    Box::new(m).finish(&|_| {}).unwrap();
}

fn mjpeg_avi(out: &Path, w: usize, h: usize, fps: f64, secs: f64) {
    let frames = (secs * fps) as usize;
    let mut video = Vec::new();
    for i in 0..frames {
        let f = picture(i, fps, w, h);
        let rgba = color::frame_to_rgba(&f);
        let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        let mut jpeg = Vec::new();
        jpeg_encoder::Encoder::new(&mut jpeg, 85).encode(&rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb).unwrap();
        video.push(OutPacket { pts: f.pts, dts: Some(f.pts), dur: 1.0 / fps, key: true, data: jpeg });
    }
    let v = VideoOut { config: VideoConfig::Mjpeg, width: w as u32, height: h as u32, fps, matrix: None, fourcc: *b"MJPG" };
    let a = AudioOut { config: AudioConfig::Pcm(16), rate: 22050, channels: 1, bitrate: 22050 * 16 };
    let mut m = AviMuxer::create(out, Some(v), Some(a)).unwrap();
    interleave(&mut m, video, pcm_packets(22050, 1, secs, 2205));
    Box::new(m).finish(&|_| {}).unwrap();
}

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("testfiles");
    std::fs::create_dir_all(&dir).unwrap();
    let turned = Some([0, 0x10000, 0, -0x10000, 0, 0, 0, 0, 0x40000000]);
    h264(&dir.join("t1_h264_aac.mp4"), "mp4", 1280, 720, 30.0, 8.0, None);
    println!("t1");
    h264(&dir.join("t2_h264_aac.mkv"), "mkv", 960, 540, 25.0, 6.0, None);
    println!("t2");
    oxideav_avi(&dir.join("t3_xvid_bframes.avi"), "mpeg4video", *b"XVID", 640, 360, 25.0, 6.0, 2);
    println!("t3");
    oxideav_avi(&dir.join("t4_divx3.avi"), "msmpeg4v3", *b"DIV3", 512, 288, 25.0, 6.0, 0);
    println!("t4");
    mjpeg_avi(&dir.join("t5_mjpeg.avi"), 640, 480, 15.0, 5.0);
    println!("t5");
    h264(&dir.join("t6_rotated.mp4"), "mp4", 640, 360, 30.0, 4.0, turned);
    println!("t6 → {}", dir.display());
}
