// MP4 / MOV / M4A writer. Sample data goes to a side file while encoding; at the end the movie header
// (moov) is built and placed before the data ("fast start": playback begins before the whole file loads).
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use super::{io, AudioConfig, AudioOut, Muxer, OutPacket, VideoConfig, VideoOut};
use crate::media::ac3;

#[derive(Clone, Copy, PartialEq)]
pub enum Brand {
    Mp4,
    Mov,
    M4a,
}

struct Sample {
    off: u64,
    size: u32,
    pts: i64,
    dts: Option<i64>,
    key: bool,
}

enum Kind {
    Video(VideoOut),
    Audio(AudioOut),
}

struct Track {
    kind: Kind,
    timescale: u32,
    samples: Vec<Sample>,
    last_dur: u32,
    first_frame: Vec<u8>,
    max_size: u32,
    bytes: u64,
}

pub struct Mp4Muxer {
    path: PathBuf,
    part: PathBuf,
    data: BufWriter<File>,
    written: u64,
    brand: Brand,
    video: Option<Track>,
    audio: Option<Track>,
}

impl Mp4Muxer {
    pub fn create(path: &Path, brand: Brand, video: Option<VideoOut>, audio: Option<AudioOut>) -> Result<Self, String> {
        let part = path.with_extension("vsq-part");
        let data = BufWriter::with_capacity(1 << 20, File::create(&part).map_err(io)?);
        let mk = |kind: Kind, timescale: u32| Track { kind, timescale, samples: Vec::new(), last_dur: 0, first_frame: Vec::new(), max_size: 0, bytes: 0 };
        Ok(Mp4Muxer {
            path: path.to_path_buf(),
            part,
            data,
            written: 0,
            brand,
            video: video.map(|v| mk(Kind::Video(v), 90000)),
            audio: audio.map(|a| {
                let ts = a.rate.max(1);
                mk(Kind::Audio(a), ts)
            }),
        })
    }

    fn add(&mut self, video: bool, p: &OutPacket) -> Result<(), String> {
        let off = self.written;
        self.data.write_all(&p.data).map_err(io)?;
        self.written += p.data.len() as u64;
        let t = if video { self.video.as_mut() } else { self.audio.as_mut() };
        let Some(t) = t else { return Ok(()) };
        let ts = t.timescale as f64;
        if t.first_frame.is_empty() {
            t.first_frame = p.data.iter().take(16).copied().collect();
        }
        t.max_size = t.max_size.max(p.data.len() as u32);
        t.bytes += p.data.len() as u64;
        t.last_dur = (p.dur * ts).round().max(0.0) as u32;
        t.samples.push(Sample {
            off,
            size: p.data.len() as u32,
            pts: (p.pts * ts).round() as i64,
            dts: if video { p.dts.map(|d| (d * ts).round() as i64) } else { Some((p.pts * ts).round() as i64) },
            key: p.key || !video,
        });
        Ok(())
    }
}

impl Muxer for Mp4Muxer {
    fn write_video(&mut self, p: &OutPacket) -> Result<(), String> {
        self.add(true, p)
    }

    fn write_audio(&mut self, p: &OutPacket) -> Result<(), String> {
        self.add(false, p)
    }

    fn finish(mut self: Box<Self>, progress: &dyn Fn(f64)) -> Result<(), String> {
        self.data.flush().map_err(io)?;
        let payload = self.written;
        let ftyp = ftyp(self.brand);
        let tracks: Vec<&Track> = self.video.iter().chain(self.audio.iter()).filter(|t| !t.samples.is_empty()).collect();
        if tracks.is_empty() {
            let _ = std::fs::remove_file(&self.part);
            return Err("nothing to write: no video or audio came out".into());
        }
        let large = payload + 1_000_000_000 > u32::MAX as u64;
        let probe = moov(&tracks, 0, large, self.brand);
        let base = ftyp.len() as u64 + probe.len() as u64 + 16;
        let moov = moov(&tracks, base, large, self.brand);
        debug_assert_eq!(moov.len(), probe.len());

        let mut out = BufWriter::with_capacity(1 << 20, File::create(&self.path).map_err(io)?);
        out.write_all(&ftyp).map_err(io)?;
        out.write_all(&moov).map_err(io)?;
        out.write_all(&1u32.to_be_bytes()).map_err(io)?;
        out.write_all(b"mdat").map_err(io)?;
        out.write_all(&(payload + 16).to_be_bytes()).map_err(io)?;
        let mut src = File::open(&self.part).map_err(io)?;
        let mut buf = vec![0u8; 1 << 20];
        let mut done = 0u64;
        loop {
            let n = src.read(&mut buf).map_err(io)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(io)?;
            done += n as u64;
            progress(done as f64 / payload.max(1) as f64);
        }
        out.flush().map_err(io)?;
        drop(src);
        let _ = std::fs::remove_file(&self.part);
        Ok(())
    }
}

impl Drop for Mp4Muxer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.part);
    }
}

// ── boxes ───────────────────────────────────────────────

fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(body.len() + 8);
    v.extend_from_slice(&(body.len() as u32 + 8).to_be_bytes());
    v.extend_from_slice(kind);
    v.extend_from_slice(body);
    v
}

fn full(kind: &[u8; 4], version: u8, flags: u32, body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(body.len() + 4);
    b.push(version);
    b.extend_from_slice(&flags.to_be_bytes()[1..]);
    b.extend_from_slice(body);
    bx(kind, &b)
}

trait Put {
    fn u8(&mut self, v: u8) -> &mut Self;
    fn u16(&mut self, v: u16) -> &mut Self;
    fn u32(&mut self, v: u32) -> &mut Self;
    fn u64(&mut self, v: u64) -> &mut Self;
    fn raw(&mut self, v: &[u8]) -> &mut Self;
}

impl Put for Vec<u8> {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.extend_from_slice(&v.to_be_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.extend_from_slice(&v.to_be_bytes());
        self
    }
    fn u64(&mut self, v: u64) -> &mut Self {
        self.extend_from_slice(&v.to_be_bytes());
        self
    }
    fn raw(&mut self, v: &[u8]) -> &mut Self {
        self.extend_from_slice(v);
        self
    }
}

const IDENTITY: [i32; 9] = [0x10000, 0, 0, 0, 0x10000, 0, 0, 0, 0x40000000];
const MOVIE_SCALE: u32 = 1000;

fn ftyp(brand: Brand) -> Vec<u8> {
    let mut b = Vec::new();
    match brand {
        Brand::Mp4 => b.raw(b"isom").u32(512).raw(b"isomiso2avc1mp41"),
        Brand::Mov => b.raw(b"qt  ").u32(0x2005_0300).raw(b"qt  "),
        Brand::M4a => b.raw(b"M4A ").u32(0).raw(b"M4A mp42isom"),
    };
    bx(b"ftyp", &b)
}

/// Decode times (derived from the presentation order when the source had none), composition offsets
/// and the source time of the first decode time (the media timeline starts there).
fn timing(t: &Track) -> (Vec<i64>, Vec<i32>, i64) {
    let n = t.samples.len();
    let have_all = t.samples.iter().all(|s| s.dts.is_some());
    let mut dts: Vec<i64> = if have_all {
        t.samples.iter().map(|s| s.dts.unwrap()).collect()
    } else {
        let mut p: Vec<i64> = t.samples.iter().map(|s| s.pts).collect();
        p.sort_unstable();
        p
    };
    // strictly increasing
    for i in 1..n {
        if dts[i] <= dts[i - 1] {
            dts[i] = dts[i - 1] + 1;
        }
    }
    // composition offsets must not be negative: move decode times earlier
    let worst = t.samples.iter().zip(&dts).map(|(s, d)| d - s.pts).max().unwrap_or(0).max(0);
    if worst > 0 {
        dts.iter_mut().for_each(|d| *d -= worst);
    }
    let cts: Vec<i32> = t.samples.iter().zip(&dts).map(|(s, d)| (s.pts - d) as i32).collect();
    let dts0 = dts[0];
    (dts, cts, dts0)
}

fn moov(tracks: &[&Track], base: u64, large: bool, brand: Brand) -> Vec<u8> {
    // the earliest picture or sound is movie time zero (sources may start at any timestamp)
    let origin = tracks
        .iter()
        .filter_map(|t| t.samples.iter().map(|s| s.pts).min().map(|p| p as f64 / t.timescale as f64))
        .fold(f64::INFINITY, f64::min)
        .max(0.0);
    let mut traks = Vec::new();
    let mut movie_dur = 0u64;
    for (i, t) in tracks.iter().enumerate() {
        let (trak, dur) = trak(t, i as u32 + 1, base, large, brand, origin);
        movie_dur = movie_dur.max(dur);
        traks.extend_from_slice(&trak);
    }
    let mut mvhd = Vec::new();
    mvhd.u32(0).u32(0).u32(MOVIE_SCALE).u32(movie_dur as u32).u32(0x0001_0000).u16(0x0100).raw(&[0; 10]);
    for m in IDENTITY {
        mvhd.u32(m as u32);
    }
    mvhd.raw(&[0; 24]).u32(tracks.len() as u32 + 1);
    let mut body = full(b"mvhd", 0, 0, &mvhd);
    body.extend_from_slice(&traks);
    bx(b"moov", &body)
}

fn trak(t: &Track, id: u32, base: u64, large: bool, brand: Brand, origin: f64) -> (Vec<u8>, u64) {
    let (dts, cts, dts0) = timing(t);
    let dts0 = dts0 - (origin * t.timescale as f64).round() as i64;
    let n = t.samples.len();
    let mut durs: Vec<u32> = (0..n).map(|i| if i + 1 < n { (dts[i + 1] - dts[i]) as u32 } else { 0 }).collect();
    if n > 0 {
        durs[n - 1] = if t.last_dur > 0 { t.last_dur } else if n > 1 { durs[n - 2] } else { t.timescale / 25 };
    }
    let media_dur: u64 = durs.iter().map(|&d| d as u64).sum::<u64>();
    // the track sits on the source timeline: a late start becomes an empty edit, an early one
    // (B-frame reorder, AAC priming) is skipped
    let to_movie = |ticks: u64| ticks * MOVIE_SCALE as u64 / t.timescale as u64;
    let skip = (-dts0).max(0) as u64;
    let empty = to_movie(dts0.max(0) as u64);
    let shown = to_movie(media_dur.saturating_sub(skip));
    let movie_dur = empty + shown;
    let is_video = matches!(t.kind, Kind::Video(_));

    // tkhd
    let (w, h, matrix) = match &t.kind {
        Kind::Video(v) => (v.width, v.height, v.matrix.unwrap_or(IDENTITY)),
        Kind::Audio(_) => (0, 0, IDENTITY),
    };
    let mut tkhd = Vec::new();
    tkhd.u32(0).u32(0).u32(id).u32(0).u32(movie_dur as u32).raw(&[0; 8]).u16(0).u16(0).u16(if is_video { 0 } else { 0x0100 }).u16(0);
    for m in matrix {
        tkhd.u32(m as u32);
    }
    tkhd.u32(w << 16).u32(h << 16);
    let mut body = full(b"tkhd", 0, 3, &tkhd);

    if empty > 0 || skip > 0 {
        let mut elst = Vec::new();
        elst.u32(if empty > 0 { 2 } else { 1 });
        if empty > 0 {
            elst.u32(empty as u32).u32(u32::MAX).u32(0x0001_0000);
        }
        elst.u32(shown as u32).u32(skip as u32).u32(0x0001_0000);
        body.extend_from_slice(&bx(b"edts", &full(b"elst", 0, 0, &elst)));
    }

    // mdia
    let mut mdhd = Vec::new();
    mdhd.u32(0).u32(0).u32(t.timescale).u32(media_dur as u32).u16(0x55C4).u16(0);
    let mut hdlr = Vec::new();
    hdlr.u32(0).raw(if is_video { b"vide" } else { b"soun" }).raw(&[0; 12]).raw(if is_video { b"VideoHandler\0" } else { b"SoundHandler\0" });
    let mhd = if is_video { full(b"vmhd", 0, 1, &[0; 8]) } else { full(b"smhd", 0, 0, &[0; 4]) };
    let dref = full(b"dref", 0, 0, &[&1u32.to_be_bytes()[..], &full(b"url ", 0, 1, &[])].concat());
    let dinf = bx(b"dinf", &dref);

    let mut stbl = full(b"stsd", 0, 0, &[&1u32.to_be_bytes()[..], &sample_entry(t, brand)].concat());
    // stts
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for &d in &durs {
        match runs.last_mut() {
            Some((c, v)) if *v == d => *c += 1,
            _ => runs.push((1, d)),
        }
    }
    let mut stts = Vec::new();
    stts.u32(runs.len() as u32);
    for (c, v) in &runs {
        stts.u32(*c).u32(*v);
    }
    stbl.extend_from_slice(&full(b"stts", 0, 0, &stts));
    if cts.iter().any(|&c| c != 0) {
        let mut runs: Vec<(u32, i32)> = Vec::new();
        for &c in &cts {
            match runs.last_mut() {
                Some((n, v)) if *v == c => *n += 1,
                _ => runs.push((1, c)),
            }
        }
        let mut ctts = Vec::new();
        ctts.u32(runs.len() as u32);
        for (n, v) in &runs {
            ctts.u32(*n).u32(*v as u32);
        }
        stbl.extend_from_slice(&full(b"ctts", 0, 0, &ctts));
    }
    if is_video && t.samples.iter().any(|s| !s.key) {
        let keys: Vec<u32> = t.samples.iter().enumerate().filter(|(_, s)| s.key).map(|(i, _)| i as u32 + 1).collect();
        let mut stss = Vec::new();
        stss.u32(keys.len() as u32);
        for k in keys {
            stss.u32(k);
        }
        stbl.extend_from_slice(&full(b"stss", 0, 0, &stss));
    }
    let mut stsc = Vec::new();
    stsc.u32(1).u32(1).u32(1).u32(1);
    stbl.extend_from_slice(&full(b"stsc", 0, 0, &stsc));
    let mut stsz = Vec::new();
    stsz.u32(0).u32(n as u32);
    for s in &t.samples {
        stsz.u32(s.size);
    }
    stbl.extend_from_slice(&full(b"stsz", 0, 0, &stsz));
    let mut co = Vec::new();
    co.u32(n as u32);
    for s in &t.samples {
        if large {
            co.u64(base + s.off);
        } else {
            co.u32((base + s.off) as u32);
        }
    }
    stbl.extend_from_slice(&full(if large { b"co64" } else { b"stco" }, 0, 0, &co));

    let minf = bx(b"minf", &[mhd, dinf, bx(b"stbl", &stbl)].concat());
    let mdia = bx(b"mdia", &[full(b"mdhd", 0, 0, &mdhd), full(b"hdlr", 0, 0, &hdlr), minf].concat());
    body.extend_from_slice(&mdia);
    (bx(b"trak", &body), movie_dur)
}

/// ES descriptor for 'esds': object type, stream type, buffer/bitrates, optional decoder setup.
fn esds(oti: u8, stream_type: u8, max_size: u32, bitrate: u32, dsi: &[u8]) -> Vec<u8> {
    let desc = |tag: u8, body: &[u8]| -> Vec<u8> {
        let mut v = vec![tag];
        let len = body.len() as u32;
        v.extend_from_slice(&[0x80 | ((len >> 21) & 0x7F) as u8, 0x80 | ((len >> 14) & 0x7F) as u8, 0x80 | ((len >> 7) & 0x7F) as u8, (len & 0x7F) as u8]);
        v.extend_from_slice(body);
        v
    };
    let mut dc = Vec::new();
    dc.u8(oti).u8((stream_type << 2) | 1);
    dc.raw(&max_size.to_be_bytes()[1..]).u32(bitrate).u32(bitrate);
    if !dsi.is_empty() {
        dc.extend_from_slice(&desc(5, dsi));
    }
    let mut es = Vec::new();
    es.u16(1).u8(0);
    es.extend_from_slice(&desc(4, &dc));
    es.extend_from_slice(&desc(6, &[2]));
    full(b"esds", 0, 0, &desc(3, &es))
}

fn sample_entry(t: &Track, brand: Brand) -> Vec<u8> {
    let secs = t.samples.last().map(|s| s.pts as f64 / t.timescale as f64).unwrap_or(1.0).max(0.1);
    let bitrate = (t.bytes as f64 * 8.0 / secs) as u32;
    match &t.kind {
        Kind::Video(v) => {
            let (kind, extra): (&[u8; 4], Vec<u8>) = match &v.config {
                VideoConfig::Avc(c) => (b"avc1", bx(b"avcC", &c.to_avcc())),
                VideoConfig::Hevc(h) => (b"hvc1", bx(b"hvcC", h)),
                VideoConfig::Mpeg4(dsi) => (b"mp4v", esds(0x20, 4, t.max_size, bitrate, dsi)),
                VideoConfig::Mjpeg if brand == Brand::Mov => (b"jpeg", Vec::new()),
                VideoConfig::Mjpeg => (b"mp4v", esds(0x6C, 4, t.max_size, bitrate, &[])),
                _ => (b"avc1", Vec::new()),
            };
            let mut e = Vec::new();
            e.raw(&[0; 6]).u16(1).u16(0).u16(0).raw(&[0; 12]).u16(v.width as u16).u16(v.height as u16);
            e.u32(0x0048_0000).u32(0x0048_0000).u32(0).u16(1);
            let mut name = [0u8; 32];
            let label = b"VideoSqueeze";
            name[0] = label.len() as u8;
            name[1..1 + label.len()].copy_from_slice(label);
            e.raw(&name).u16(0x0018).u16(0xFFFF).raw(&extra);
            bx(kind, &e)
        }
        Kind::Audio(a) => {
            let (kind, extra): (&[u8; 4], Vec<u8>) = match &a.config {
                AudioConfig::Aac(asc) => (b"mp4a", esds(0x40, 5, t.max_size, bitrate, asc)),
                AudioConfig::Mpeg(_) => (b"mp4a", esds(if a.rate < 32000 { 0x69 } else { 0x6B }, 5, t.max_size, bitrate, &[])),
                AudioConfig::Ac3 => (b"ac-3", bx(b"dac3", &ac3::dac3(&t.first_frame))),
                _ => (b"mp4a", esds(0x40, 5, t.max_size, bitrate, &[])),
            };
            let mut e = Vec::new();
            e.raw(&[0; 6]).u16(1).raw(&[0; 8]).u16(a.channels.min(8)).u16(16).u16(0).u16(0).u32(a.rate.min(65535) << 16).raw(&extra);
            bx(kind, &e)
        }
    }
}
