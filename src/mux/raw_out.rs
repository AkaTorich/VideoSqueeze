// Plain audio files: frame streams (.mp3, .mp2, .ac3) and .wav.
use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

use super::io;

/// MPEG audio or AC-3 frames one after another — that is the whole file format.
pub struct FrameFile(BufWriter<File>);

impl FrameFile {
    pub fn create(path: &Path) -> Result<Self, String> {
        Ok(FrameFile(BufWriter::new(File::create(path).map_err(io)?)))
    }

    pub fn write(&mut self, data: &[u8]) -> Result<(), String> {
        self.0.write_all(data).map_err(io)
    }

    pub fn finish(mut self) -> Result<(), String> {
        self.0.flush().map_err(io)
    }
}

pub struct WavWriter {
    out: BufWriter<File>,
    bytes: u64,
    /// Format fields are written at the end: a decoder may correct rate and channels on the first packet.
    pub channels: u16,
    pub rate: u32,
    pub bits: u16,
}

impl WavWriter {
    pub fn create(path: &Path, rate: u32, channels: u16, bits: u16) -> Result<Self, String> {
        let mut out = BufWriter::new(File::create(path).map_err(io)?);
        out.write_all(&[0u8; 44]).map_err(io)?;
        Ok(WavWriter { out, bytes: 0, channels, rate, bits })
    }

    pub fn write(&mut self, data: &[u8]) -> Result<(), String> {
        self.bytes += data.len() as u64;
        self.out.write_all(data).map_err(io)
    }

    pub fn write_i16(&mut self, samples: &[i16]) -> Result<(), String> {
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        self.write(&bytes)
    }

    pub fn finish(mut self) -> Result<(), String> {
        if self.bytes % 2 == 1 {
            self.out.write_all(&[0]).map_err(io)?;
        }
        let data = self.bytes.min(u32::MAX as u64 - 44) as u32;
        let align = self.channels * (self.bits / 8).max(1);
        let mut h = Vec::with_capacity(44);
        h.extend_from_slice(b"RIFF");
        h.extend_from_slice(&(36 + data).to_le_bytes());
        h.extend_from_slice(b"WAVEfmt ");
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&1u16.to_le_bytes());
        h.extend_from_slice(&self.channels.to_le_bytes());
        h.extend_from_slice(&self.rate.to_le_bytes());
        h.extend_from_slice(&(self.rate * align as u32).to_le_bytes());
        h.extend_from_slice(&align.to_le_bytes());
        h.extend_from_slice(&self.bits.to_le_bytes());
        h.extend_from_slice(b"data");
        h.extend_from_slice(&data.to_le_bytes());
        self.out.seek(SeekFrom::Start(0)).map_err(io)?;
        self.out.write_all(&h).map_err(io)?;
        self.out.flush().map_err(io)
    }
}
