// H.264 bitstream helpers: NAL units in Annex B (start codes) and length-prefixed (MP4/MKV) form,
// and the avcC decoder configuration record (SPS and PPS).

pub const NAL_IDR: u8 = 5;
pub const NAL_SPS: u8 = 7;
pub const NAL_PPS: u8 = 8;
pub const NAL_AUD: u8 = 9;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AvcConfig {
    pub sps: Vec<Vec<u8>>,
    pub pps: Vec<Vec<u8>>,
}

impl AvcConfig {
    /// avcC record → configuration and the NAL length size (1, 2 or 4 bytes).
    pub fn parse(avcc: &[u8]) -> Option<(Self, usize)> {
        if avcc.len() < 7 || avcc[0] != 1 {
            return None;
        }
        let nal_len = (avcc[4] & 3) as usize + 1;
        let mut pos = 5;
        let mut cfg = AvcConfig::default();
        let n_sps = (avcc[pos] & 0x1F) as usize;
        pos += 1;
        for _ in 0..n_sps {
            let len = u16::from_be_bytes([*avcc.get(pos)?, *avcc.get(pos + 1)?]) as usize;
            cfg.sps.push(avcc.get(pos + 2..pos + 2 + len)?.to_vec());
            pos += 2 + len;
        }
        let n_pps = *avcc.get(pos)? as usize;
        pos += 1;
        for _ in 0..n_pps {
            let len = u16::from_be_bytes([*avcc.get(pos)?, *avcc.get(pos + 1)?]) as usize;
            cfg.pps.push(avcc.get(pos + 2..pos + 2 + len)?.to_vec());
            pos += 2 + len;
        }
        (!cfg.sps.is_empty() && !cfg.pps.is_empty()).then_some((cfg, nal_len))
    }

    /// SPS and PPS picked out of a frame's NAL units.
    pub fn from_nals<'a>(nals: impl IntoIterator<Item = &'a [u8]>) -> Option<Self> {
        let mut cfg = AvcConfig::default();
        for n in nals {
            match nal_type(n) {
                NAL_SPS if !cfg.sps.contains(&n.to_vec()) => cfg.sps.push(n.to_vec()),
                NAL_PPS if !cfg.pps.contains(&n.to_vec()) => cfg.pps.push(n.to_vec()),
                _ => {}
            }
        }
        (!cfg.sps.is_empty() && !cfg.pps.is_empty()).then_some(cfg)
    }

    /// avcC record with 4-byte NAL lengths.
    pub fn to_avcc(&self) -> Vec<u8> {
        let sps0 = &self.sps[0];
        let mut out = vec![1, sps0.get(1).copied().unwrap_or(66), sps0.get(2).copied().unwrap_or(0), sps0.get(3).copied().unwrap_or(30), 0xFF];
        out.push(0xE0 | self.sps.len() as u8);
        for s in &self.sps {
            out.extend_from_slice(&(s.len() as u16).to_be_bytes());
            out.extend_from_slice(s);
        }
        out.push(self.pps.len() as u8);
        for p in &self.pps {
            out.extend_from_slice(&(p.len() as u16).to_be_bytes());
            out.extend_from_slice(p);
        }
        out
    }

    /// SPS and PPS as Annex B (start codes) — what a decoder is fed first.
    pub fn to_annexb(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for n in self.sps.iter().chain(&self.pps) {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(n);
        }
        out
    }
}

pub fn nal_type(nal: &[u8]) -> u8 {
    nal.first().map(|b| b & 0x1F).unwrap_or(0)
}

/// Annex B stream → NAL units (without start codes).
pub fn split_annexb(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            starts.push((i, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::with_capacity(starts.len());
    for (k, &(_, begin)) in starts.iter().enumerate() {
        let mut end = starts.get(k + 1).map(|s| s.0).unwrap_or(data.len());
        // zero byte of a 4-byte start code and trailing zeros belong to no NAL unit
        while end > begin && data[end - 1] == 0 {
            end -= 1;
        }
        if end > begin {
            out.push(&data[begin..end]);
        }
    }
    out
}

/// Length-prefixed NAL units → NAL units.
pub fn split_prefixed(data: &[u8], len_size: usize) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos + len_size <= data.len() {
        let mut len = 0usize;
        for b in &data[pos..pos + len_size] {
            len = (len << 8) | *b as usize;
        }
        pos += len_size;
        if len == 0 || pos + len > data.len() {
            break;
        }
        out.push(&data[pos..pos + len]);
        pos += len;
    }
    out
}

/// NAL units → 4-byte length-prefixed sample.
pub fn join_prefixed<'a>(nals: impl IntoIterator<Item = &'a [u8]>) -> Vec<u8> {
    let mut out = Vec::new();
    for n in nals {
        out.extend_from_slice(&(n.len() as u32).to_be_bytes());
        out.extend_from_slice(n);
    }
    out
}

/// 4-byte length-prefixed sample → Annex B.
pub fn prefixed_to_annexb(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 16);
    for n in split_prefixed(data, 4) {
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(n);
    }
    out
}

/// Re-prefix a sample stored with a different NAL length size.
pub fn normalize_prefixed(data: Vec<u8>, len_size: usize) -> Vec<u8> {
    if len_size == 4 {
        data
    } else {
        join_prefixed(split_prefixed(&data, len_size))
    }
}

pub fn has_idr(prefixed: &[u8]) -> bool {
    split_prefixed(prefixed, 4).iter().any(|n| nal_type(n) == NAL_IDR)
}
