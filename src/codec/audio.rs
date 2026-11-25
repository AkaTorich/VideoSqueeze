// Audio: decoding to interleaved 16-bit PCM in WAV channel order (AAC and HE-AAC — FDK AAC, MPEG audio —
// Symphonia, AC-3 — oxideav-ac3, PCM as is) and AAC-LC encoding (FDK AAC) with resampling to a rate AAC accepts.
use fdk_aac_rs::decoder::{DecodeFlags, Decoder as AacDec, DecoderError};
use fdk_aac_rs::encoder::{Aot, Bitrate, ChannelMode, ChannelOrder, Encoder as AacEnc, EncoderError, Transport};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CodecParameters, DecoderOptions, CODEC_TYPE_MP1, CODEC_TYPE_MP2, CODEC_TYPE_MP3};
use symphonia::core::formats::Packet as SymPacket;

use crate::media::{AudioCodec, AudioTrack};

enum Inner {
    Aac { dec: Box<AacDec>, buf: Vec<i16> },
    Sym(Box<dyn symphonia::core::codecs::Decoder>),
    Ac3(Box<dyn oxideav_core::Decoder>),
    Pcm { bits: u16 },
}

pub struct AudioDecoder {
    inner: Inner,
    pub rate: u32,
    pub channels: u16,
    ts: u64,
}

impl AudioDecoder {
    pub fn new(t: &AudioTrack) -> Result<Self, String> {
        let inner = match t.codec {
            AudioCodec::Aac => {
                let dec = AacDec::with_config(&t.asc).map_err(|e| format!("AAC decoder: {e:?}"))?;
                Inner::Aac { dec: Box::new(dec), buf: vec![0i16; 8 * 2048 * 2] }
            }
            AudioCodec::Mpeg => {
                let mut params = CodecParameters::new();
                let ty = match t.name.as_str() {
                    "MP1" => CODEC_TYPE_MP1,
                    "MP2" => CODEC_TYPE_MP2,
                    _ => CODEC_TYPE_MP3,
                };
                params.for_codec(ty).with_sample_rate(t.sample_rate.max(1));
                let dec = symphonia::default::get_codecs()
                    .make(&params, &DecoderOptions::default())
                    .map_err(|e| format!("{} decoder: {e}", t.name))?;
                Inner::Sym(dec)
            }
            AudioCodec::Ac3 => {
                let params = oxideav_core::CodecParameters::audio(oxideav_core::CodecId::new("ac3"));
                Inner::Ac3(oxideav_ac3::decoder::make_decoder(&params).map_err(|e| format!("AC-3 decoder: {e}"))?)
            }
            AudioCodec::Pcm => Inner::Pcm { bits: t.bits },
            _ => return Err(format!("{} audio cannot be decoded", t.name)),
        };
        Ok(AudioDecoder { inner, rate: t.sample_rate, channels: t.channels.max(1), ts: 0 })
    }

    /// One packet → interleaved samples (rate and channel count may be corrected by the stream).
    pub fn decode(&mut self, data: &[u8]) -> Vec<i16> {
        match &mut self.inner {
            Inner::Aac { dec, buf } => {
                if dec.fill(data).is_err() {
                    return Vec::new();
                }
                let mut out = Vec::new();
                loop {
                    match dec.decode_frame(buf, DecodeFlags::empty()) {
                        Ok(frame) => {
                            out.extend_from_slice(frame.samples());
                            let info = dec.stream_info();
                            if info.sample_rate() > 0 {
                                self.rate = info.sample_rate() as u32;
                            }
                            if info.num_channels() > 0 {
                                self.channels = info.num_channels() as u16;
                            }
                        }
                        Err(DecoderError::NotEnoughBits) => break,
                        Err(_) => break,
                    }
                }
                out
            }
            Inner::Sym(dec) => {
                let pkt = SymPacket::new_from_slice(0, self.ts, 0, data);
                self.ts += 1;
                match dec.decode(&pkt) {
                    Ok(buf) => {
                        let spec = *buf.spec();
                        self.rate = spec.rate;
                        self.channels = spec.channels.count() as u16;
                        let mut sb = SampleBuffer::<i16>::new(buf.capacity() as u64, spec);
                        sb.copy_interleaved_ref(buf);
                        sb.samples().to_vec()
                    }
                    Err(_) => Vec::new(),
                }
            }
            Inner::Ac3(dec) => {
                let pkt = oxideav_core::Packet::new(0, oxideav_core::TimeBase::new(1, 48000), data.to_vec());
                if dec.send_packet(&pkt).is_err() {
                    return Vec::new();
                }
                let mut out = Vec::new();
                while let Ok(oxideav_core::Frame::Audio(f)) = dec.receive_frame() {
                    let Some(bytes) = f.data.first() else { continue };
                    if f.samples > 0 {
                        self.channels = (bytes.len() / 2 / f.samples as usize).max(1) as u16;
                    }
                    if let Some(fi) = crate::media::ac3::parse_header(data) {
                        self.rate = fi.rate;
                    }
                    out.extend(bytes.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])));
                }
                out
            }
            Inner::Pcm { bits } => match *bits {
                8 => data.iter().map(|&b| ((b as i16) - 128) << 8).collect(),
                24 => data.chunks_exact(3).map(|c| i16::from_le_bytes([c[1], c[2]])).collect(),
                _ => data.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect(),
            },
        }
    }
}

/// Rates an AAC stream can carry.
const AAC_RATES: [u32; 12] = [96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000];

pub fn aac_rate_for(rate: u32) -> u32 {
    if AAC_RATES.contains(&rate) {
        return rate;
    }
    *AAC_RATES.iter().filter(|&&r| r >= rate).min().unwrap_or(&48000)
}

/// Channel count the AAC encoder takes: 1–6 (more are cut to 5.1).
pub fn aac_channels_for(ch: u16) -> u16 {
    ch.clamp(1, 6)
}

/// Linear resampler for interleaved samples, keeping the fractional position between calls.
struct Resampler {
    from: u32,
    to: u32,
    channels: usize,
    pos: f64,
    prev: Vec<i16>,
}

impl Resampler {
    fn process(&mut self, input: &[i16]) -> Vec<i16> {
        let ch = self.channels;
        let frames = input.len() / ch;
        if frames == 0 {
            return Vec::new();
        }
        let step = self.from as f64 / self.to as f64;
        let mut out = Vec::with_capacity((frames as f64 / step) as usize * ch + ch);
        let sample = |i: isize, c: usize| -> f64 {
            if i < 0 {
                self.prev.get(c).copied().unwrap_or(0) as f64
            } else {
                input[i as usize * ch + c] as f64
            }
        };
        while self.pos < frames as f64 - 1.0 + 1e-9 {
            let i = self.pos.floor() as isize;
            let frac = self.pos - i as f64;
            for c in 0..ch {
                let a = sample(i, c);
                let b = sample((i + 1).min(frames as isize - 1), c);
                out.push((a + (b - a) * frac).round().clamp(-32768.0, 32767.0) as i16);
            }
            self.pos += step;
        }
        self.pos -= frames as f64;
        self.prev = input[(frames - 1) * ch..frames * ch].to_vec();
        out
    }
}

pub struct AacEncoder {
    enc: AacEnc,
    pub rate: u32,
    pub channels: u16,
    /// Priming samples per channel at the start of the encoded stream.
    pub delay: u32,
    asc: Vec<u8>,
    resampler: Option<Resampler>,
    pending: Vec<i16>,
    out: Vec<u8>,
    pub frames_out: u64,
}

fn enc_err(e: EncoderError) -> String {
    format!("AAC encoder: {e:?}")
}

impl AacEncoder {
    pub fn new(in_rate: u32, in_channels: u16, bitrate: u32) -> Result<Self, String> {
        let rate = aac_rate_for(in_rate);
        let channels = aac_channels_for(in_channels);
        let mode = match channels {
            1 => ChannelMode::Mono,
            2 => ChannelMode::Stereo,
            3 => ChannelMode::Mode1_2,
            4 => ChannelMode::Mode1_2_1,
            5 => ChannelMode::Mode1_2_2,
            _ => ChannelMode::Mode1_2_2_1,
        };
        // the bitrate the encoder accepts for this layout and rate
        let per_channel_max = (rate as f64 * 6.0) as u32;
        let bitrate = bitrate.clamp(8_000 * channels as u32, per_channel_max * channels as u32);
        let mut enc = AacEnc::builder()
            .aot(Aot::AacLc)
            .sample_rate(rate)
            .channel_mode(mode)
            .channel_order(ChannelOrder::Wave)
            .bitrate(Bitrate::Cbr(bitrate))
            .transport(Transport::Raw)
            .afterburner(true)
            .build()
            .map_err(|e| format!("AAC encoder: {e:?}"))?;
        let info = enc.info().map_err(enc_err)?;
        let asc = info.config().to_vec();
        let delay = info.delay();
        let out = vec![0u8; info.max_out_buf_bytes().max(8192) as usize];
        let resampler = (rate != in_rate).then(|| Resampler { from: in_rate, to: rate, channels: channels as usize, pos: 0.0, prev: Vec::new() });
        Ok(AacEncoder { enc, rate, channels, delay, asc, resampler, pending: Vec::new(), out, frames_out: 0 })
    }

    pub fn asc(&self) -> Vec<u8> {
        self.asc.clone()
    }

    /// Fit samples with another channel count to the encoder's layout.
    fn remap(&self, input: &[i16], from: u16) -> Vec<i16> {
        let to = self.channels as usize;
        let from = from.max(1) as usize;
        if from == to {
            return input.to_vec();
        }
        let mut out = Vec::with_capacity(input.len() / from * to);
        for f in input.chunks_exact(from) {
            for c in 0..to {
                out.push(if from == 1 { f[0] } else if c < from { f[c] } else { 0 });
            }
        }
        out
    }

    /// Interleaved samples in → raw AAC access units out.
    pub fn push(&mut self, input: &[i16], channels: u16) -> Result<Vec<Vec<u8>>, String> {
        let mut samples = self.remap(input, channels);
        if let Some(r) = self.resampler.as_mut() {
            samples = r.process(&samples);
        }
        self.pending.extend_from_slice(&samples);
        let hop = 1024 * self.channels as usize;
        let mut units = Vec::new();
        while self.pending.len() >= hop {
            let r = self.enc.encode(&self.pending[..hop], &mut self.out).map_err(enc_err)?;
            let used = r.in_samples().max(1).min(self.pending.len());
            if r.out_bytes() > 0 {
                units.push(self.out[..r.out_bytes()].to_vec());
                self.frames_out += 1;
            }
            self.pending.drain(..used);
        }
        Ok(units)
    }

    /// Remaining samples and the encoder's tail.
    pub fn finish(&mut self) -> Result<Vec<Vec<u8>>, String> {
        let mut units = Vec::new();
        while !self.pending.is_empty() {
            let r = self.enc.encode(&self.pending, &mut self.out).map_err(enc_err)?;
            if r.out_bytes() > 0 {
                units.push(self.out[..r.out_bytes()].to_vec());
                self.frames_out += 1;
            }
            let used = r.in_samples().min(self.pending.len());
            if used == 0 {
                break;
            }
            self.pending.drain(..used);
        }
        loop {
            match self.enc.flush(&mut self.out) {
                Ok(r) => {
                    if r.out_bytes() > 0 {
                        units.push(self.out[..r.out_bytes()].to_vec());
                        self.frames_out += 1;
                    }
                }
                Err(EncoderError::EncodeEof) => break,
                Err(e) => return Err(enc_err(e)),
            }
        }
        Ok(units)
    }

    /// Duration of one AAC frame, seconds.
    pub fn frame_dur(&self) -> f64 {
        1024.0 / self.rate as f64
    }

    /// Presentation time of access unit `n` (0-based) for audio starting at `t0`; the priming makes
    /// the first units fall before `t0`.
    pub fn unit_pts(&self, t0: f64, n: u64) -> f64 {
        t0 + (n as f64 * 1024.0 - self.delay as f64) / self.rate as f64
    }
}
