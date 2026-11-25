// Codecs: video decoders (H.264, MPEG-4 Part 2, MS-MPEG4, MJPEG), H.264 encoder, scaling, colour
// conversion; audio decoders (AAC, MPEG audio, AC-3, PCM) and the AAC encoder.
pub mod audio;
pub mod color;
pub mod encode;
pub mod scale;
pub mod video;

/// A picture in planar YUV 4:2:0 (limited range), planes packed without padding.
#[derive(Clone)]
pub struct Frame {
    pub w: usize,
    pub h: usize,
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
    /// Presentation time, seconds.
    pub pts: f64,
}

impl Frame {
    pub fn cw(&self) -> usize {
        self.w.div_ceil(2)
    }
    pub fn ch(&self) -> usize {
        self.h.div_ceil(2)
    }

    /// Copy planes given with strides into a packed frame.
    #[allow(clippy::too_many_arguments)]
    pub fn from_planes(w: usize, h: usize, y: &[u8], ys: usize, u: &[u8], us: usize, v: &[u8], vs: usize) -> Frame {
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        let copy = |src: &[u8], stride: usize, pw: usize, ph: usize| -> Vec<u8> {
            let mut out = vec![128u8; pw * ph];
            for row in 0..ph {
                let s = row * stride;
                if s >= src.len() {
                    break;
                }
                let n = pw.min(src.len() - s);
                out[row * pw..row * pw + n].copy_from_slice(&src[s..s + n]);
            }
            out
        };
        Frame { w, h, y: copy(y, ys, w, h), u: copy(u, us, cw, ch), v: copy(v, vs, cw, ch), pts: 0.0 }
    }
}
