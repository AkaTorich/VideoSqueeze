// AC-3 (Dolby Digital) syncframes: splitting a byte stream into frames and the 'dac3' box MP4 needs.

/// Frame size in 16-bit words by frmsizecod for 48, 44.1 and 32 kHz.
const FRAME_WORDS: [[u16; 3]; 38] = [
    [64, 69, 96], [64, 70, 96], [80, 87, 120], [80, 88, 120], [96, 104, 144], [96, 105, 144], [112, 121, 168],
    [112, 122, 168], [128, 139, 192], [128, 140, 192], [160, 174, 240], [160, 175, 240], [192, 208, 288],
    [192, 209, 288], [224, 243, 336], [224, 244, 336], [256, 278, 384], [256, 279, 384], [320, 348, 480],
    [320, 349, 480], [384, 417, 576], [384, 418, 576], [448, 487, 672], [448, 488, 672], [512, 557, 768],
    [512, 558, 768], [640, 696, 960], [640, 697, 960], [768, 835, 1152], [768, 836, 1152], [896, 975, 1344],
    [896, 976, 1344], [1024, 1114, 1536], [1024, 1115, 1536], [1152, 1253, 1728], [1152, 1254, 1728],
    [1280, 1393, 1920], [1280, 1394, 1920],
];
const RATES: [u32; 3] = [48000, 44100, 32000];
const ACMOD_CHANNELS: [u16; 8] = [2, 1, 2, 3, 3, 4, 4, 5];

pub struct FrameInfo {
    pub len: usize,
    pub rate: u32,
    pub channels: u16,
    pub bitrate: u32,
}

pub fn parse_header(h: &[u8]) -> Option<FrameInfo> {
    if h.len() < 8 || h[0] != 0x0B || h[1] != 0x77 {
        return None;
    }
    let fscod = (h[4] >> 6) as usize;
    let frmsizecod = (h[4] & 0x3F) as usize;
    if fscod > 2 || frmsizecod > 37 {
        return None;
    }
    let bsid = h[5] >> 3;
    if bsid > 10 {
        return None;
    }
    let acmod = (h[6] >> 5) as usize;
    // lfeon sits after optional fields that depend on acmod
    let mut bit = 3; // bits of byte 6 consumed by acmod
    if acmod & 1 != 0 && acmod != 1 {
        bit += 2;
    }
    if acmod & 4 != 0 {
        bit += 2;
    }
    if acmod == 2 {
        bit += 2;
    }
    let byte = 6 + bit / 8;
    let lfeon = (h[byte] >> (7 - bit % 8)) & 1;
    Some(FrameInfo {
        len: FRAME_WORDS[frmsizecod][fscod] as usize * 2,
        rate: RATES[fscod],
        channels: ACMOD_CHANNELS[acmod] + lfeon as u16,
        bitrate: [32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640][frmsizecod / 2] * 1000,
    })
}

/// 'dac3' payload (3 bytes) from the first syncframe.
pub fn dac3(frame: &[u8]) -> Vec<u8> {
    if frame.len() < 8 {
        return vec![0x10, 0x3D, 0xE0];
    }
    let fscod = (frame[4] >> 6) as u32;
    let frmsizecod = (frame[4] & 0x3F) as u32;
    let bsid = (frame[5] >> 3) as u32;
    let bsmod = (frame[5] & 7) as u32;
    let acmod = (frame[6] >> 5) as u32;
    let lfeon = parse_header(frame).map(|f| (f.channels > ACMOD_CHANNELS[acmod as usize]) as u32).unwrap_or(0);
    let v: u32 = (fscod << 22) | (bsid << 17) | (bsmod << 14) | (acmod << 11) | (lfeon << 10) | ((frmsizecod >> 1) << 5);
    vec![(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

#[derive(Default)]
pub struct Framer {
    buf: Vec<u8>,
}

impl Framer {
    pub fn push(&mut self, data: &[u8]) -> Vec<(Vec<u8>, FrameInfo)> {
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        let mut pos = 0;
        while pos + 8 <= self.buf.len() {
            match parse_header(&self.buf[pos..]) {
                Some(fi) => {
                    if pos + fi.len > self.buf.len() {
                        break;
                    }
                    let len = fi.len;
                    out.push((self.buf[pos..pos + len].to_vec(), fi));
                    pos += len;
                }
                None => pos += 1,
            }
        }
        self.buf.drain(..pos);
        out
    }
}
