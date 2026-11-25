// H.264 encoding (OpenH264): bitrate-controlled, no frame skipping, a keyframe every few seconds.
// Output: 4-byte length-prefixed NAL units without SPS/PPS (those go into the container's avcC).
use openh264::encoder::{BitRate, Complexity, Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod, RateControlMode};
use openh264::formats::YUVSlices;
use openh264::{OpenH264API, Timestamp};

use super::Frame;
use crate::media::h264::{self, AvcConfig};

pub struct H264Encoder {
    enc: Encoder,
    pub config: Option<AvcConfig>,
}

pub struct Encoded {
    pub data: Vec<u8>,
    pub key: bool,
}

impl H264Encoder {
    /// `keyint` — frames between keyframes.
    pub fn new(bitrate: u64, fps: f64, keyint: u32) -> Result<Self, String> {
        let config = EncoderConfig::new()
            .bitrate(BitRate::from_bps(bitrate.clamp(20_000, 200_000_000) as u32))
            .max_frame_rate(FrameRate::from_hz(fps.clamp(1.0, 240.0) as f32))
            .rate_control_mode(RateControlMode::Bitrate)
            .skip_frames(false)
            .complexity(Complexity::High)
            .intra_frame_period(IntraFramePeriod::from_num_frames(keyint.max(1)));
        let enc = Encoder::with_api_config(OpenH264API::from_source(), config).map_err(|e| format!("H.264 encoder: {e}"))?;
        Ok(H264Encoder { enc, config: None })
    }

    pub fn encode(&mut self, f: &Frame) -> Result<Option<Encoded>, String> {
        let src = YUVSlices::new((&f.y, &f.u, &f.v), (f.w, f.h), (f.w, f.cw(), f.cw()));
        let ts = Timestamp::from_millis((f.pts.max(0.0) * 1000.0) as u64);
        let bs = self.enc.encode_at(&src, ts).map_err(|e| format!("H.264 encode error: {e}"))?;
        if matches!(bs.frame_type(), FrameType::Skip | FrameType::Invalid) {
            return Ok(None);
        }
        let annexb = bs.to_vec();
        let nals = h264::split_annexb(&annexb);
        if self.config.is_none() {
            self.config = AvcConfig::from_nals(nals.iter().copied());
        }
        let key = nals.iter().any(|n| h264::nal_type(n) == h264::NAL_IDR);
        let body: Vec<&[u8]> = nals
            .into_iter()
            .filter(|n| !matches!(h264::nal_type(n), h264::NAL_SPS | h264::NAL_PPS | h264::NAL_AUD))
            .collect();
        if body.is_empty() {
            return Ok(None);
        }
        Ok(Some(Encoded { data: h264::join_prefixed(body), key }))
    }
}
