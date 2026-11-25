// MPEG audio (MP3, MP2) frame splitter: AVI stores the stream in arbitrary chunks, while MP4 and MKV
// need one frame per sample.

pub struct FrameInfo {
    pub len: usize,
    pub samples: u32,
    pub rate: u32,
    pub channels: u16,
}

/// Header at the start of `h` (4 bytes) → frame length and sample count.
pub fn parse_header(h: &[u8]) -> Option<FrameInfo> {
    if h.len() < 4 || h[0] != 0xFF || h[1] & 0xE0 != 0xE0 {
        return None;
    }
    let version = (h[1] >> 3) & 3; // 0 = 2.5, 2 = 2, 3 = 1
    let layer = (h[1] >> 1) & 3; // 1 = III, 2 = II, 3 = I
    let br_idx = (h[2] >> 4) as usize;
    let sr_idx = ((h[2] >> 2) & 3) as usize;
    let padding = ((h[2] >> 1) & 1) as usize;
    if version == 1 || layer == 0 || br_idx == 0 || br_idx == 15 || sr_idx == 3 {
        return None;
    }
    const BR_V1: [[u32; 15]; 3] = [
        [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384],
        [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320],
    ];
    const BR_V2: [[u32; 15]; 3] = [
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
    ];
    const SR: [u32; 3] = [44100, 48000, 32000];
    let li = (3 - layer) as usize; // 0 = I, 1 = II, 2 = III
    let kbps = if version == 3 { BR_V1[li][br_idx] } else { BR_V2[li][br_idx] };
    let rate = SR[sr_idx] >> match version { 3 => 0, 2 => 1, _ => 2 };
    let bitrate = kbps * 1000;
    let (len, samples) = match li {
        0 => ((12 * bitrate / rate) as usize * 4 + padding * 4, 384),
        1 => ((144 * bitrate / rate) as usize + padding, 1152),
        _ if version == 3 => ((144 * bitrate / rate) as usize + padding, 1152),
        _ => ((72 * bitrate / rate) as usize + padding, 576),
    };
    let channels = if h[3] >> 6 == 3 { 1 } else { 2 };
    (len >= 4).then_some(FrameInfo { len, samples, rate, channels })
}

/// Collects bytes and hands out whole frames.
#[derive(Default)]
pub struct Framer {
    buf: Vec<u8>,
}

impl Framer {
    pub fn push(&mut self, data: &[u8]) -> Vec<(Vec<u8>, FrameInfo)> {
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        let mut pos = 0;
        while pos + 4 <= self.buf.len() {
            match parse_header(&self.buf[pos..]) {
                Some(fi) => {
                    if pos + fi.len > self.buf.len() {
                        break;
                    }
                    out.push((self.buf[pos..pos + fi.len].to_vec(), fi));
                    pos += out.last().unwrap().1.len;
                }
                None => pos += 1,
            }
        }
        self.buf.drain(..pos);
        out
    }
}
