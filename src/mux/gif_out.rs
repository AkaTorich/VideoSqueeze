// Animated GIF: each frame gets its own 256-colour palette (NeuQuant); a frame's delay is the time to the
// next one, so the frame is written one step late.
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use crate::codec::{color, Frame};

pub struct GifWriter {
    enc: gif::Encoder<BufWriter<File>>,
    held: Option<(Vec<u8>, f64)>,
    w: u16,
    h: u16,
    last_delay: u16,
    /// Remainder of rounded-off delays, carried so the total time stays right.
    carry: f64,
}

impl GifWriter {
    pub fn create(path: &Path, w: usize, h: usize) -> Result<Self, String> {
        let file = BufWriter::new(File::create(path).map_err(super::io)?);
        let mut enc = gif::Encoder::new(file, w as u16, h as u16, &[]).map_err(|e| format!("GIF: {e}"))?;
        enc.set_repeat(gif::Repeat::Infinite).map_err(|e| format!("GIF: {e}"))?;
        Ok(GifWriter { enc, held: None, w: w as u16, h: h as u16, last_delay: 4, carry: 0.0 })
    }

    pub fn push(&mut self, f: &Frame) -> Result<(), String> {
        let rgba = color::frame_to_rgba(f);
        if let Some((prev, t)) = self.held.take() {
            let exact = (f.pts - t) * 100.0 + self.carry;
            let delay = exact.round().max(2.0);
            self.carry = exact - delay;
            self.last_delay = delay as u16;
            self.emit(prev, delay as u16)?;
        }
        self.held = Some((rgba, f.pts));
        Ok(())
    }

    fn emit(&mut self, mut rgba: Vec<u8>, delay: u16) -> Result<(), String> {
        let mut frame = gif::Frame::from_rgba_speed(self.w, self.h, &mut rgba, 10);
        frame.delay = delay;
        self.enc.write_frame(&frame).map_err(|e| format!("GIF: {e}"))
    }

    pub fn finish(mut self) -> Result<(), String> {
        if let Some((prev, _)) = self.held.take() {
            let d = self.last_delay;
            self.emit(prev, d)?;
        }
        Ok(())
    }
}
