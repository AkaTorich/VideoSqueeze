// Video decoding into Frame: H.264 (OpenH264), MPEG-4 Part 2 — XviD, DivX 4/5/6, H.263 (oxideav-mpeg4video),
// Microsoft MPEG-4 v1/v2/v3 — DivX 3 (oxideav-msmpeg4), MJPEG (jpeg-decoder, both fields of interlaced AVI MJPEG).
// Frames come out in display order; their times are the packet times sorted (B-frames reorder).
use std::cmp::Reverse;
use std::collections::BinaryHeap;

use openh264::decoder::{Decoder as H264Dec, DecoderConfig, Flush};
use openh264::formats::YUVSource;
use openh264::OpenH264API;
use oxideav_mpeg4video::decoder::Mpeg4VideoDecoder;
use oxideav_mpeg4video::framestore::DecodedFrame;

use super::{color, Frame};
use crate::media::h264::{self, AvcConfig};
use crate::media::{Packet, VideoCodec, VideoTrack};

enum Inner {
    /// Codec chosen on the first packet (FourCCs lie: DIV3 can hold MPEG-4 Part 2 and back).
    Pending,
    H264 { dec: H264Dec, cfg: Option<AvcConfig>, primed: bool },
    Mpeg4(Box<Mpeg4VideoDecoder>),
    Ms(Box<dyn oxideav_core::Decoder>),
    Mjpeg,
}

pub struct VideoDecoder {
    inner: Inner,
    track: VideoTrack,
    times: BinaryHeap<Reverse<i64>>,
    last_pts: f64,
    frame_dur: f64,
    errors: u32,
    frames: u64,
}

fn micros(t: f64) -> i64 {
    (t * 1e6).round() as i64
}

impl VideoDecoder {
    pub fn new(track: &VideoTrack) -> Result<Self, String> {
        if !track.codec.decodable() {
            return Err(format!("{} video cannot be decoded", track.name));
        }
        let frame_dur = if track.fps > 0.0 { 1.0 / track.fps } else { 1.0 / 25.0 };
        Ok(VideoDecoder {
            inner: Inner::Pending,
            track: track.clone(),
            times: BinaryHeap::new(),
            last_pts: -frame_dur,
            frame_dur,
            errors: 0,
            frames: 0,
        })
    }

    fn start(&mut self, first: &[u8]) -> Result<(), String> {
        let mut codec = self.track.codec;
        if matches!(codec, VideoCodec::Mpeg4 | VideoCodec::MsMpeg4V1 | VideoCodec::MsMpeg4V2 | VideoCodec::MsMpeg4V3) {
            let probe: Vec<u8> = self.track.extradata.iter().chain(first.iter()).copied().collect();
            codec = match oxideav_msmpeg4::classify(&probe, Some(&self.track.fourcc)) {
                oxideav_msmpeg4::Classification::Mpeg4Part2 => VideoCodec::Mpeg4,
                oxideav_msmpeg4::Classification::MsMpeg4V1 => VideoCodec::MsMpeg4V1,
                oxideav_msmpeg4::Classification::MsMpeg4V2 => VideoCodec::MsMpeg4V2,
                oxideav_msmpeg4::Classification::MsMpeg4V3 => VideoCodec::MsMpeg4V3,
                _ => codec,
            };
        }
        self.inner = match codec {
            VideoCodec::H264 => {
                let config = DecoderConfig::new().flush_after_decode(Flush::NoFlush);
                let dec = H264Dec::with_api_config(OpenH264API::from_source(), config).map_err(|e| format!("H.264 decoder: {e}"))?;
                Inner::H264 { dec, cfg: self.track.avc.clone(), primed: false }
            }
            VideoCodec::Mpeg4 => {
                let options = oxideav_mpeg4video::DecodeOptions::default().with_ecosystem_compat(true).with_error_concealment(true);
                let mut dec = Box::new(Mpeg4VideoDecoder::with_options(options));
                dec.set_threads(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).min(8));
                if !self.track.extradata.is_empty() {
                    let _ = dec.decode(&self.track.extradata);
                }
                Inner::Mpeg4(dec)
            }
            VideoCodec::MsMpeg4V1 | VideoCodec::MsMpeg4V2 | VideoCodec::MsMpeg4V3 => {
                let id = match codec {
                    VideoCodec::MsMpeg4V1 => "msmpeg4v1",
                    VideoCodec::MsMpeg4V2 => "msmpeg4v2",
                    _ => "msmpeg4v3",
                };
                let mut reg = oxideav_core::CodecRegistry::new();
                oxideav_msmpeg4::register_codecs(&mut reg);
                let mut params = oxideav_core::CodecParameters::video(oxideav_core::CodecId::new(id));
                params.width = Some(self.track.width);
                params.height = Some(self.track.height);
                let dec = reg.first_decoder(&params).map_err(|e| format!("DivX 3 decoder: {e}"))?;
                Inner::Ms(dec)
            }
            VideoCodec::Mjpeg => Inner::Mjpeg,
            _ => return Err(format!("{} video cannot be decoded", self.track.name)),
        };
        Ok(())
    }

    /// Feed one packet; returns the frames that became ready.
    pub fn feed(&mut self, p: &Packet) -> Result<Vec<Frame>, String> {
        if p.data.is_empty() {
            return Ok(Vec::new());
        }
        if matches!(self.inner, Inner::Pending) {
            self.start(&p.data)?;
        }
        self.times.push(Reverse(micros(p.pts)));
        let raw = match self.decode(&p.data, p.key) {
            Ok(f) => f,
            Err(e) => {
                self.errors += 1;
                // damaged pictures are skipped; a stream that never decodes is an error
                if self.frames == 0 && self.errors > 50 {
                    return Err(e);
                }
                Vec::new()
            }
        };
        Ok(self.stamp(raw))
    }

    /// Pictures still held for reordering.
    pub fn finish(&mut self) -> Result<Vec<Frame>, String> {
        let raw = match &mut self.inner {
            Inner::H264 { dec, .. } => {
                let mut out = Vec::new();
                if let Ok(frames) = dec.flush_remaining() {
                    for y in frames {
                        out.push(from_openh264(&y));
                    }
                }
                out
            }
            Inner::Mpeg4(dec) => dec.flush().iter().map(|f| from_mpeg4(f, &self.track, dec.vol().map(|v| (v.width as usize, v.height as usize)))).collect(),
            Inner::Ms(dec) => {
                let _ = dec.flush();
                drain_core(dec.as_mut(), &self.track)
            }
            _ => Vec::new(),
        };
        Ok(self.stamp(raw))
    }

    fn stamp(&mut self, mut frames: Vec<Frame>) -> Vec<Frame> {
        for f in frames.iter_mut() {
            f.pts = match self.times.pop() {
                Some(Reverse(t)) => t as f64 / 1e6,
                None => self.last_pts + self.frame_dur,
            };
            self.last_pts = f.pts;
            self.frames += 1;
        }
        frames
    }

    fn decode(&mut self, data: &[u8], key: bool) -> Result<Vec<Frame>, String> {
        match &mut self.inner {
            Inner::H264 { dec, cfg, primed } => {
                let mut annexb = Vec::with_capacity(data.len() + 64);
                if !*primed || key {
                    if let Some(c) = cfg.as_ref() {
                        annexb.extend_from_slice(&c.to_annexb());
                    }
                    *primed = true;
                }
                annexb.extend_from_slice(&h264::prefixed_to_annexb(data));
                match dec.decode(&annexb) {
                    Ok(Some(y)) => Ok(vec![from_openh264(&y)]),
                    Ok(None) => Ok(Vec::new()),
                    Err(e) => Err(format!("H.264 decode error: {e}")),
                }
            }
            Inner::Mpeg4(dec) => {
                let frames = dec.decode_with_pts(data, None).map_err(|e| format!("MPEG-4 decode error: {e:?}"))?;
                let vol = dec.vol().map(|v| (v.width as usize, v.height as usize));
                Ok(frames.iter().map(|f| from_mpeg4(f, &self.track, vol)).collect())
            }
            Inner::Ms(dec) => {
                let pkt = oxideav_core::Packet::new(0, oxideav_core::TimeBase::new(1, 1000), data.to_vec());
                dec.send_packet(&pkt).map_err(|e| format!("DivX 3 decode error: {e}"))?;
                Ok(drain_core(dec.as_mut(), &self.track))
            }
            Inner::Mjpeg => decode_mjpeg(data, &self.track).map(|f| vec![f]),
            Inner::Pending => Ok(Vec::new()),
        }
    }
}

fn from_openh264(y: &openh264::decoder::DecodedYUV) -> Frame {
    let (w, h) = y.dimensions();
    let (ys, us, vs) = y.strides();
    Frame::from_planes(w, h, y.y(), ys, y.u(), us, y.v(), vs)
}

fn from_mpeg4(f: &DecodedFrame, track: &VideoTrack, vol: Option<(usize, usize)>) -> Frame {
    let (fw, fh) = (f.width(), f.height());
    // pictures are padded to whole macroblocks: crop to the picture size
    let (w, h) = vol.filter(|v| v.0 > 0 && v.1 > 0).unwrap_or((track.width as usize, track.height as usize));
    let (w, h) = (if w > 0 { w.min(fw) } else { fw }, if h > 0 { h.min(fh) } else { fh });
    if f.is_wide() {
        let shift = f.bits_per_pixel().saturating_sub(8);
        let narrow = |s: &[u16]| s.iter().map(|&v| (v >> shift).min(255) as u8).collect::<Vec<u8>>();
        let (y, u, v) = (narrow(f.luma_samples16().unwrap_or(&[])), narrow(f.cb_samples16().unwrap_or(&[])), narrow(f.cr_samples16().unwrap_or(&[])));
        return Frame::from_planes(w, h, &y, fw, &u, fw / 2, &v, fw / 2);
    }
    Frame::from_planes(w, h, f.luma_samples(), fw, f.cb_samples(), fw / 2, f.cr_samples(), fw / 2)
}

fn drain_core(dec: &mut dyn oxideav_core::Decoder, track: &VideoTrack) -> Vec<Frame> {
    let mut out = Vec::new();
    while let Ok(frame) = dec.receive_frame() {
        if let oxideav_core::Frame::Video(vf) = frame {
            let planes: Vec<&oxideav_core::VideoPlane> = vf.planes.iter().take(3).collect();
            if planes.len() < 3 {
                continue;
            }
            let w = track.width as usize;
            let h = track.height as usize;
            out.push(Frame::from_planes(w, h, &planes[0].data, planes[0].stride, &planes[1].data, planes[1].stride, &planes[2].data, planes[2].stride));
        }
    }
    out
}

fn decode_jpeg(data: &[u8]) -> Result<(Vec<u8>, usize, usize, usize), String> {
    let mut d = jpeg_decoder::Decoder::new(data);
    let px = d.decode().map_err(|e| format!("MJPEG decode error: {e}"))?;
    let info = d.info().ok_or("MJPEG: no picture")?;
    let bpp = match info.pixel_format {
        jpeg_decoder::PixelFormat::L8 => 1,
        jpeg_decoder::PixelFormat::RGB24 => 3,
        jpeg_decoder::PixelFormat::L16 => {
            let w = info.width as usize;
            let h = info.height as usize;
            return Ok((px.chunks_exact(2).map(|c| c[0]).collect(), w, h, 1));
        }
        jpeg_decoder::PixelFormat::CMYK32 => {
            let rgb: Vec<u8> = px
                .chunks_exact(4)
                .flat_map(|c| {
                    let k = c[3] as u32;
                    [(c[0] as u32 * k / 255) as u8, (c[1] as u32 * k / 255) as u8, (c[2] as u32 * k / 255) as u8]
                })
                .collect();
            return Ok((rgb, info.width as usize, info.height as usize, 3));
        }
    };
    Ok((px, info.width as usize, info.height as usize, bpp))
}

/// One MJPEG packet; interlaced AVI MJPEG stores two field pictures — they are woven back together.
fn decode_mjpeg(data: &[u8], track: &VideoTrack) -> Result<Frame, String> {
    let (px, w, h, bpp) = decode_jpeg(data)?;
    let full = track.height as usize;
    if full > 0 && h * 2 <= full + 2 && h * 2 >= full.saturating_sub(2) {
        // second field: the next SOI after the first picture's EOI
        let eoi = data.windows(2).position(|w| w == [0xFF, 0xD9]).map(|p| p + 2).unwrap_or(data.len());
        if let Some(soi) = data[eoi..].windows(2).position(|w| w == [0xFF, 0xD8]) {
            if let Ok((px2, w2, h2, bpp2)) = decode_jpeg(&data[eoi + soi..]) {
                if w2 == w && h2 == h && bpp2 == bpp {
                    let row = w * bpp;
                    let mut woven = vec![0u8; row * h * 2];
                    for j in 0..h {
                        woven[(2 * j) * row..(2 * j + 1) * row].copy_from_slice(&px[j * row..(j + 1) * row]);
                        woven[(2 * j + 1) * row..(2 * j + 2) * row].copy_from_slice(&px2[j * row..(j + 1) * row]);
                    }
                    return Ok(color::rgb_to_frame(&woven, w, h * 2, bpp, true));
                }
            }
        }
    }
    Ok(color::rgb_to_frame(&px, w, h, bpp, true))
}
