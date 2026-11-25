// MP4 / MOV / 3GP reader (ISO base media file format and QuickTime): sample tables of the first video
// and first audio track, including fragmented files (moof/traf/trun), edit lists and QuickTime PCM.
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use super::{aac, h264, AudioCodec, AudioTrack, Demuxer, Kind, MediaInfo, Packet, VideoCodec, VideoTrack};

struct Sample {
    offset: u64,
    size: u32,
    dts: i64,
    cts: i32,
    key: bool,
}

#[derive(Clone, Copy)]
enum Transform {
    None,
    /// Length-prefixed NAL units with this length size → 4-byte prefixes.
    Nal(usize),
    /// PCM: bits, big-endian, float, signed 8-bit.
    Pcm { bits: u16, big: bool, float: bool, signed8: bool },
}

struct TrackRead {
    id: u32,
    kind: Kind,
    timescale: u32,
    samples: Vec<Sample>,
    next: usize,
    transform: Transform,
    /// Seconds added to every timestamp (edit list).
    shift: f64,
    /// PCM: bytes per sample frame (all channels); samples are whole chunks.
    pcm_frame: u32,
    trex_duration: u32,
    trex_size: u32,
    trex_flags: u32,
}

pub struct Mp4Demuxer {
    file: BufReader<File>,
    info: MediaInfo,
    tracks: Vec<TrackRead>,
}

// ── box walking ─────────────────────────────────────────

fn be16(d: &[u8], i: usize) -> u16 {
    d.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]])).unwrap_or(0)
}
fn be32(d: &[u8], i: usize) -> u32 {
    d.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0)
}
fn be64(d: &[u8], i: usize) -> u64 {
    d.get(i..i + 8).map(|b| u64::from_be_bytes(b.try_into().unwrap())).unwrap_or(0)
}

/// Child boxes of a payload: (type, payload).
fn boxes(data: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 8 <= data.len() {
        let mut size = be32(data, pos) as u64;
        let kind: [u8; 4] = data[pos + 4..pos + 8].try_into().unwrap();
        let mut header = 8u64;
        if size == 1 {
            size = be64(data, pos + 8);
            header = 16;
        } else if size == 0 {
            size = (data.len() - pos) as u64;
        }
        if size < header || pos as u64 + size > data.len() as u64 {
            break;
        }
        out.push((kind, &data[pos + header as usize..pos + size as usize]));
        pos += size as usize;
    }
    out
}

fn child<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(data).into_iter().find(|(k, _)| k == kind).map(|(_, p)| p)
}

/// Search a box anywhere below `data` (QuickTime hides esds inside 'wave').
fn find_deep<'a>(data: &'a [u8], kind: &[u8; 4], depth: u32) -> Option<&'a [u8]> {
    for (k, p) in boxes(data) {
        if &k == kind {
            return Some(p);
        }
        if depth > 0 {
            if let Some(f) = find_deep(p, kind, depth - 1) {
                return Some(f);
            }
        }
    }
    None
}

/// MPEG-4 descriptor walk inside 'esds': (objectTypeIndication, avg bitrate, DecoderSpecificInfo).
fn parse_esds(esds: &[u8]) -> (u8, u32, Vec<u8>) {
    // esds = version/flags (4) + ES_Descriptor
    let mut oti = 0u8;
    let mut avg = 0u32;
    let mut dsi = Vec::new();
    let d = if esds.len() > 4 { &esds[4..] } else { return (0, 0, dsi) };
    let mut pos = 0usize;
    let read_len = |pos: &mut usize| -> usize {
        let mut len = 0usize;
        for _ in 0..4 {
            let b = *d.get(*pos).unwrap_or(&0);
            *pos += 1;
            len = (len << 7) | (b & 0x7F) as usize;
            if b & 0x80 == 0 {
                break;
            }
        }
        len
    };
    while pos < d.len() {
        let tag = d[pos];
        pos += 1;
        let len = read_len(&mut pos);
        match tag {
            3 => {
                // ES_Descriptor: ES_ID(2), flags(1) [+ dependsOn(2)] [+ URL] [+ OCR(2)], then children
                let flags = *d.get(pos + 2).unwrap_or(&0);
                pos += 3;
                if flags & 0x80 != 0 {
                    pos += 2;
                }
                if flags & 0x40 != 0 {
                    pos += 1 + *d.get(pos).unwrap_or(&0) as usize;
                }
                if flags & 0x20 != 0 {
                    pos += 2;
                }
            }
            4 => {
                oti = *d.get(pos).unwrap_or(&0);
                avg = be32(d, pos + 9);
                pos += 13;
            }
            5 => {
                dsi = d.get(pos..pos + len).unwrap_or(&[]).to_vec();
                pos += len;
            }
            _ => pos += len,
        }
    }
    (oti, avg, dsi)
}

// ── opening ─────────────────────────────────────────────

impl Mp4Demuxer {
    pub fn open(path: &Path) -> Result<Self, String> {
        let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
        let mut file = BufReader::new(File::open(path).map_err(|e| format!("cannot open file: {e}"))?);

        // top-level boxes: keep moov and every moof (with its file offset) in memory
        let mut moov: Option<Vec<u8>> = None;
        let mut moofs: Vec<(u64, Vec<u8>)> = Vec::new();
        let mut pos = 0u64;
        while pos + 8 <= size {
            file.seek(SeekFrom::Start(pos)).map_err(|e| e.to_string())?;
            let mut h = [0u8; 16];
            file.read_exact(&mut h[..8]).map_err(|e| e.to_string())?;
            let mut bsize = u32::from_be_bytes(h[0..4].try_into().unwrap()) as u64;
            let kind: [u8; 4] = h[4..8].try_into().unwrap();
            let mut header = 8u64;
            if bsize == 1 {
                file.read_exact(&mut h[8..16]).map_err(|e| e.to_string())?;
                bsize = u64::from_be_bytes(h[8..16].try_into().unwrap());
                header = 16;
            } else if bsize == 0 {
                bsize = size - pos;
            }
            if bsize < header {
                break;
            }
            if &kind == b"moov" || &kind == b"moof" {
                let len = (bsize - header).min(size - pos - header) as usize;
                let mut buf = vec![0u8; len];
                file.read_exact(&mut buf).map_err(|e| e.to_string())?;
                if &kind == b"moov" {
                    moov = Some(buf);
                } else {
                    moofs.push((pos, buf));
                }
            }
            pos += bsize;
        }
        let moov = moov.ok_or("not a video file: no 'moov' box found")?;

        let movie_scale = child(&moov, b"mvhd").map(|m| if m[0] == 1 { be32(m, 20) } else { be32(m, 12) }).unwrap_or(1000).max(1);
        let mut video: Option<(VideoTrack, TrackRead)> = None;
        let mut audio: Option<(AudioTrack, TrackRead)> = None;
        let mut duration = 0f64;

        for (k, trak) in boxes(&moov) {
            if &k != b"trak" {
                continue;
            }
            let Some(parsed) = parse_trak(trak, movie_scale)? else { continue };
            duration = duration.max(parsed.duration);
            match parsed.kind {
                TrakKind::Video(v) if video.is_none() => video = Some((v, parsed.read)),
                TrakKind::Audio(a) if audio.is_none() => audio = Some((a, parsed.read)),
                _ => {}
            }
        }

        let mut tracks: Vec<TrackRead> = Vec::new();
        let (mut vinfo, mut ainfo) = (None, None);
        if let Some((v, r)) = video {
            vinfo = Some(v);
            tracks.push(r);
        }
        if let Some((a, r)) = audio {
            ainfo = Some(a);
            tracks.push(r);
        }

        // fragmented file: trex defaults from mvex, samples from every moof
        if let Some(mvex) = child(&moov, b"mvex") {
            for (k, trex) in boxes(mvex) {
                if &k == b"trex" {
                    let id = be32(trex, 4);
                    if let Some(t) = tracks.iter_mut().find(|t| t.id == id) {
                        t.trex_duration = be32(trex, 12);
                        t.trex_size = be32(trex, 16);
                        t.trex_flags = be32(trex, 20);
                    }
                }
            }
            for (moof_pos, moof) in &moofs {
                read_moof(*moof_pos, moof, &mut tracks);
            }
            for t in &tracks {
                if let Some(last) = t.samples.last() {
                    duration = duration.max(last.dts as f64 / t.timescale as f64 + t.shift);
                }
            }
        }

        if tracks.iter().all(|t| t.samples.is_empty()) {
            return Err("file has no video or audio samples".into());
        }

        // derived numbers
        for t in &tracks {
            let secs = t.samples.last().map(|s| s.dts as f64 / t.timescale as f64).unwrap_or(0.0);
            let bytes: u64 = t.samples.iter().map(|s| s.size as u64).sum();
            let span = if secs > 0.0 { secs } else { duration };
            match t.kind {
                Kind::Video => {
                    if let Some(v) = vinfo.as_mut() {
                        if span > 0.0 {
                            v.fps = (t.samples.len().saturating_sub(1)) as f64 / secs.max(1e-9);
                            if secs <= 0.0 {
                                v.fps = 0.0;
                            }
                            v.bitrate = (bytes as f64 * 8.0 / span) as u64;
                        }
                    }
                }
                Kind::Audio => {
                    if let Some(a) = ainfo.as_mut() {
                        if a.bitrate == 0 && span > 0.0 {
                            a.bitrate = (bytes as f64 * 8.0 / span) as u64;
                        }
                    }
                }
            }
        }

        let brand = {
            let mut f = File::open(path).map_err(|e| e.to_string())?;
            let mut h = [0u8; 12];
            let _ = f.read(&mut h);
            if &h[4..8] == b"ftyp" { [h[8], h[9], h[10], h[11]] } else { *b"qt  " }
        };
        let container = match &brand {
            b"qt  " => "MOV",
            b"3gp4" | b"3gp5" | b"3gp6" | b"3g2a" | b"3ge6" | b"3gg6" => "3GP",
            b"M4V " | b"M4VH" | b"M4VP" => "M4V",
            _ => "MP4",
        };

        Ok(Mp4Demuxer { file, info: MediaInfo { container, duration, size, video: vinfo, audio: ainfo }, tracks })
    }
}

enum TrakKind {
    Video(VideoTrack),
    Audio(AudioTrack),
}

struct ParsedTrak {
    kind: TrakKind,
    read: TrackRead,
    duration: f64,
}

fn parse_trak(trak: &[u8], movie_scale: u32) -> Result<Option<ParsedTrak>, String> {
    let Some(tkhd) = child(trak, b"tkhd") else { return Ok(None) };
    let v1 = tkhd[0] == 1;
    let id = be32(tkhd, if v1 { 20 } else { 12 });
    let mbase = if v1 { 52 } else { 40 };
    let matrix: [i32; 9] = std::array::from_fn(|i| be32(tkhd, mbase + i * 4) as i32);
    let tk_w = be32(tkhd, mbase + 36) >> 16;
    let tk_h = be32(tkhd, mbase + 40) >> 16;

    let Some(mdia) = child(trak, b"mdia") else { return Ok(None) };
    let Some(mdhd) = child(mdia, b"mdhd") else { return Ok(None) };
    let (timescale, media_dur) = if mdhd[0] == 1 { (be32(mdhd, 20), be64(mdhd, 24)) } else { (be32(mdhd, 12), be32(mdhd, 16) as u64) };
    let timescale = timescale.max(1);
    let handler: [u8; 4] = child(mdia, b"hdlr").and_then(|h| h.get(8..12)).map(|s| s.try_into().unwrap()).unwrap_or(*b"    ");
    let kind = match &handler {
        b"vide" => Kind::Video,
        b"soun" => Kind::Audio,
        _ => return Ok(None),
    };
    let Some(stbl) = child(mdia, b"minf").and_then(|m| child(m, b"stbl")) else { return Ok(None) };
    let Some(stsd) = child(stbl, b"stsd") else { return Ok(None) };
    let entries = boxes(stsd.get(8..).unwrap_or(&[]));
    let Some(&(fourcc, entry)) = entries.first() else { return Ok(None) };

    // edit list: leading empty edits delay the track, the first real edit skips into the media
    let mut shift = 0f64;
    if let Some(elst) = child(trak, b"edts").and_then(|e| child(e, b"elst")) {
        let v1 = elst[0] == 1;
        let n = be32(elst, 4) as usize;
        let step = if v1 { 20 } else { 12 };
        for i in 0..n {
            let base = 8 + i * step;
            let (seg_dur, media_time) = if v1 { (be64(elst, base), be64(elst, base + 8) as i64) } else { (be32(elst, base) as u64, be32(elst, base + 4) as i32 as i64) };
            if media_time == -1 {
                shift += seg_dur as f64 / movie_scale as f64;
            } else {
                shift -= media_time as f64 / timescale as f64;
                break;
            }
        }
    }

    let mut read = TrackRead {
        id,
        kind,
        timescale,
        samples: Vec::new(),
        next: 0,
        transform: Transform::None,
        shift,
        pcm_frame: 0,
        trex_duration: 0,
        trex_size: 0,
        trex_flags: 0,
    };

    let parsed = match kind {
        Kind::Video => {
            let width = be16(entry, 24) as u32;
            let height = be16(entry, 26) as u32;
            let kids = entry.get(78..).unwrap_or(&[]);
            let (mut codec, mut name) = super::video_by_fourcc(&fourcc);
            let mut avc = None;
            let mut extradata = Vec::new();
            match &fourcc {
                b"avc1" | b"avc2" | b"avc3" | b"avc4" => {
                    codec = VideoCodec::H264;
                    name = "H.264".into();
                    let nal_len = match child(kids, b"avcC").and_then(h264::AvcConfig::parse) {
                        Some((cfg, n)) => {
                            avc = Some(cfg);
                            n
                        }
                        None => 4,
                    };
                    read.transform = Transform::Nal(nal_len);
                }
                b"hvc1" | b"hev1" => {
                    codec = VideoCodec::Hevc;
                    name = "HEVC".into();
                    if let Some(hvcc) = child(kids, b"hvcC") {
                        let mut h = hvcc.to_vec();
                        let nal_len = h.get(21).map(|b| (b & 3) as usize + 1).unwrap_or(4);
                        if h.len() > 21 {
                            h[21] |= 3;
                        }
                        extradata = h;
                        read.transform = Transform::Nal(nal_len);
                    }
                }
                b"mp4v" => {
                    let (oti, _, dsi) = child(kids, b"esds").map(parse_esds).unwrap_or((0x20, 0, Vec::new()));
                    if oti == 0x6C || oti == 0x6E {
                        codec = VideoCodec::Mjpeg;
                        name = "MJPEG".into();
                    } else {
                        codec = VideoCodec::Mpeg4;
                        name = "MPEG-4 PART 2".into();
                        extradata = dsi;
                    }
                }
                b"s263" | b"h263" | b"H263" => {
                    codec = VideoCodec::Mpeg4;
                    name = "H.263".into();
                }
                b"jpeg" | b"mjpa" | b"AVDJ" | b"dmb1" => {
                    codec = VideoCodec::Mjpeg;
                    name = "MJPEG".into();
                }
                _ => {}
            }
            let w = if width > 0 { width } else { tk_w };
            let h = if height > 0 { height } else { tk_h };
            let identity = matrix == [0x10000, 0, 0, 0, 0x10000, 0, 0, 0, 0x40000000];
            TrakKind::Video(VideoTrack {
                codec,
                name,
                width: w,
                height: h,
                fps: 0.0,
                bitrate: 0,
                avc,
                extradata,
                fourcc,
                matrix: (!identity).then_some(matrix),
                mkv_codec: None,
            })
        }
        Kind::Audio => {
            let version = be16(entry, 8);
            let mut channels = be16(entry, 16);
            let mut bits = be16(entry, 18);
            let mut rate = be32(entry, 24) >> 16;
            let mut kids_at = 28usize;
            let mut lpcm_flags = 0u32;
            if version == 1 {
                kids_at = 44;
            } else if version == 2 {
                rate = f64::from_bits(be64(entry, 32)) as u32;
                channels = be32(entry, 40) as u16;
                bits = be32(entry, 48) as u16;
                lpcm_flags = be32(entry, 52);
                kids_at = 64;
            }
            let kids = entry.get(kids_at..).unwrap_or(&[]);
            let mut codec = AudioCodec::Other;
            let mut name = String::from_utf8_lossy(&fourcc).trim().to_uppercase();
            let mut asc = Vec::new();
            let mut bitrate = 0u64;
            let mut mkv_codec = None;
            let pcm = |bits: u16, big: bool, float: bool, signed8: bool| Transform::Pcm { bits, big, float, signed8 };
            match &fourcc {
                b"mp4a" => {
                    let (oti, avg, dsi) = find_deep(kids, b"esds", 2).map(parse_esds).unwrap_or((0x40, 0, Vec::new()));
                    bitrate = avg as u64;
                    match oti {
                        0x69 | 0x6B => {
                            codec = AudioCodec::Mpeg;
                            name = "MP3".into();
                        }
                        0xA5 => {
                            codec = AudioCodec::Ac3;
                            name = "AC-3".into();
                        }
                        _ => {
                            codec = AudioCodec::Aac;
                            name = "AAC".into();
                            asc = if dsi.len() >= 2 { dsi } else { aac::asc(2, rate, channels) };
                            if let Some((obj, r, ch)) = aac::parse_asc(&asc) {
                                name = if obj == 5 || obj == 29 { "HE-AAC".into() } else { "AAC".into() };
                                rate = r;
                                if ch > 0 {
                                    channels = ch;
                                }
                            }
                        }
                    }
                }
                b".mp3" | b"ms\0U" | b".mp2" => {
                    codec = AudioCodec::Mpeg;
                    name = if &fourcc == b".mp2" { "MP2".into() } else { "MP3".into() };
                }
                b"ac-3" | b"sac3" => {
                    codec = AudioCodec::Ac3;
                    name = "AC-3".into();
                }
                b"Opus" => {
                    codec = AudioCodec::Opus;
                    name = "OPUS".into();
                    if let Some(dops) = child(kids, b"dOps") {
                        mkv_codec = Some(("A_OPUS".to_string(), opus_head(dops)));
                    }
                }
                b"sowt" => { codec = AudioCodec::Pcm; read.transform = pcm(bits.max(8), false, false, false); }
                b"twos" => { codec = AudioCodec::Pcm; read.transform = pcm(bits.max(8), true, false, true); }
                b"raw " | b"NONE" => { codec = AudioCodec::Pcm; bits = 8; read.transform = pcm(8, false, false, false); }
                b"in24" => { codec = AudioCodec::Pcm; bits = 24; read.transform = pcm(24, true, false, true); }
                b"in32" => { codec = AudioCodec::Pcm; bits = 32; read.transform = pcm(32, true, false, true); }
                b"fl32" => { codec = AudioCodec::Pcm; bits = 32; read.transform = pcm(32, true, true, false); }
                b"fl64" => { codec = AudioCodec::Pcm; bits = 64; read.transform = pcm(64, true, true, false); }
                b"lpcm" => {
                    codec = AudioCodec::Pcm;
                    read.transform = pcm(bits, lpcm_flags & 2 != 0, lpcm_flags & 1 != 0, lpcm_flags & 4 != 0);
                }
                b"ipcm" | b"fpcm" => {
                    codec = AudioCodec::Pcm;
                    let little = child(kids, b"pcmC").map(|p| p.get(4).copied().unwrap_or(0) & 1 != 0).unwrap_or(false);
                    if let Some(p) = child(kids, b"pcmC") {
                        bits = *p.get(5).unwrap_or(&16) as u16;
                    }
                    read.transform = pcm(bits, !little, &fourcc == b"fpcm", true);
                }
                _ => {}
            }
            if codec == AudioCodec::Pcm {
                name = "PCM".into();
                read.pcm_frame = channels.max(1) as u32 * (bits.max(8) as u32 / 8);
                // normalized output: float and big-endian become 16/24-bit little-endian
                if let Transform::Pcm { float: true, .. } = read.transform {
                    bits = 16;
                }
                if bits > 24 {
                    bits = 24;
                }
            }
            TrakKind::Audio(AudioTrack { codec, name, sample_rate: rate, channels, bits, bitrate, asc, mkv_codec })
        }
    };

    build_samples(stbl, &mut read);
    let duration = media_dur as f64 / timescale as f64 + shift.max(0.0);
    Ok(Some(ParsedTrak { kind: parsed, read, duration }))
}

/// 'dOps' (MP4) → 'OpusHead' (Matroska CodecPrivate).
fn opus_head(dops: &[u8]) -> Vec<u8> {
    let mut h = b"OpusHead".to_vec();
    h.push(1);
    h.push(*dops.get(1).unwrap_or(&2));
    h.extend_from_slice(&be16(dops, 2).to_le_bytes());
    h.extend_from_slice(&be32(dops, 4).to_le_bytes());
    h.extend_from_slice(&(be16(dops, 8) as i16).to_le_bytes());
    h.extend_from_slice(dops.get(10..).unwrap_or(&[0]));
    h
}

/// Sample tables (stts, ctts, stss, stsc, stsz/stz2, stco/co64) → per-sample offset, size and times.
fn build_samples(stbl: &[u8], t: &mut TrackRead) {
    let mut sizes: Vec<u32> = Vec::new();
    if let Some(stsz) = child(stbl, b"stsz") {
        let fixed = be32(stsz, 4);
        let n = be32(stsz, 8) as usize;
        if fixed != 0 {
            sizes = vec![fixed; n];
        } else {
            sizes = (0..n).map(|i| be32(stsz, 12 + i * 4)).collect();
        }
    } else if let Some(stz2) = child(stbl, b"stz2") {
        let field = stz2[7];
        let n = be32(stz2, 8) as usize;
        sizes = (0..n)
            .map(|i| match field {
                4 => {
                    let b = stz2[12 + i / 2];
                    if i % 2 == 0 { (b >> 4) as u32 } else { (b & 15) as u32 }
                }
                8 => stz2[12 + i] as u32,
                _ => be16(stz2, 12 + i * 2) as u32,
            })
            .collect();
    }
    let n = sizes.len();
    if n == 0 {
        return;
    }

    let chunk_offsets: Vec<u64> = if let Some(stco) = child(stbl, b"stco") {
        (0..be32(stco, 4) as usize).map(|i| be32(stco, 8 + i * 4) as u64).collect()
    } else if let Some(co64) = child(stbl, b"co64") {
        (0..be32(co64, 4) as usize).map(|i| be64(co64, 8 + i * 8)).collect()
    } else {
        return;
    };

    // samples per chunk
    let stsc: Vec<(u32, u32)> = child(stbl, b"stsc")
        .map(|s| (0..be32(s, 4) as usize).map(|i| (be32(s, 8 + i * 12), be32(s, 12 + i * 12))).collect())
        .unwrap_or_default();
    let mut per_chunk = vec![0u32; chunk_offsets.len()];
    for (i, &(first, spc)) in stsc.iter().enumerate() {
        let end = stsc.get(i + 1).map(|e| e.0).unwrap_or(chunk_offsets.len() as u32 + 1);
        for c in first..end {
            if let Some(slot) = per_chunk.get_mut(c as usize - 1) {
                *slot = spc;
            }
        }
    }

    // decode times
    let mut dts = Vec::with_capacity(n);
    if let Some(stts) = child(stbl, b"stts") {
        let mut acc = 0i64;
        for i in 0..be32(stts, 4) as usize {
            let count = be32(stts, 8 + i * 8);
            let delta = be32(stts, 12 + i * 8) as i64;
            for _ in 0..count {
                if dts.len() < n {
                    dts.push(acc);
                }
                acc += delta;
            }
        }
    }
    while dts.len() < n {
        dts.push(dts.last().copied().unwrap_or(0));
    }

    let mut cts = vec![0i32; n];
    if let Some(ctts) = child(stbl, b"ctts") {
        let mut k = 0usize;
        for i in 0..be32(ctts, 4) as usize {
            let count = be32(ctts, 8 + i * 8);
            let off = be32(ctts, 12 + i * 8) as i32;
            for _ in 0..count {
                if k < n {
                    cts[k] = off;
                    k += 1;
                }
            }
        }
    }

    let keys: Option<Vec<u32>> = child(stbl, b"stss").map(|s| (0..be32(s, 4) as usize).map(|i| be32(s, 8 + i * 4)).collect());
    let mut is_key = vec![keys.is_none(); n];
    if let Some(k) = keys {
        for idx in k {
            if let Some(slot) = is_key.get_mut(idx as usize - 1) {
                *slot = true;
            }
        }
    }

    // QuickTime PCM: a "sample" is one sample frame — read whole chunks instead
    let pcm_chunks = t.pcm_frame > 0 && sizes[0] <= t.pcm_frame;
    let mut s = 0usize;
    for (c, &off) in chunk_offsets.iter().enumerate() {
        let count = per_chunk[c] as usize;
        if count == 0 {
            continue;
        }
        if pcm_chunks {
            if s >= n {
                break;
            }
            let take = count.min(n - s);
            t.samples.push(Sample { offset: off, size: take as u32 * t.pcm_frame, dts: dts[s], cts: 0, key: true });
            s += take;
            continue;
        }
        let mut pos = off;
        for _ in 0..count {
            if s >= n {
                break;
            }
            t.samples.push(Sample { offset: pos, size: sizes[s], dts: dts[s], cts: cts[s], key: is_key[s] });
            pos += sizes[s] as u64;
            s += 1;
        }
    }
}

/// One movie fragment: its track fragments append samples to the matching tracks.
fn read_moof(moof_pos: u64, moof: &[u8], tracks: &mut [TrackRead]) {
    for (k, traf) in boxes(moof) {
        if &k != b"traf" {
            continue;
        }
        let Some(tfhd) = child(traf, b"tfhd") else { continue };
        let flags = be32(tfhd, 0) & 0xFFFFFF;
        let id = be32(tfhd, 4);
        let Some(t) = tracks.iter_mut().find(|t| t.id == id) else { continue };
        let mut p = 8;
        let mut base = moof_pos;
        if flags & 1 != 0 {
            base = be64(tfhd, p);
            p += 8;
        }
        if flags & 2 != 0 {
            p += 4;
        }
        let mut def_dur = t.trex_duration;
        let mut def_size = t.trex_size;
        let mut def_flags = t.trex_flags;
        if flags & 8 != 0 {
            def_dur = be32(tfhd, p);
            p += 4;
        }
        if flags & 0x10 != 0 {
            def_size = be32(tfhd, p);
            p += 4;
        }
        if flags & 0x20 != 0 {
            def_flags = be32(tfhd, p);
        }
        let mut time = child(traf, b"tfdt")
            .map(|d| if d[0] == 1 { be64(d, 4) as i64 } else { be32(d, 4) as i64 })
            .unwrap_or_else(|| t.samples.last().map(|s| s.dts + def_dur as i64).unwrap_or(0));
        let mut next_offset: Option<u64> = None;
        for (k, trun) in boxes(traf) {
            if &k != b"trun" {
                continue;
            }
            let tf = be32(trun, 0);
            let version = tf >> 24;
            let count = be32(trun, 4) as usize;
            let mut q = 8;
            let mut offset = next_offset.unwrap_or(base);
            if tf & 1 != 0 {
                offset = (base as i64 + be32(trun, q) as i32 as i64) as u64;
                q += 4;
            }
            let mut first_flags = None;
            if tf & 4 != 0 {
                first_flags = Some(be32(trun, q));
                q += 4;
            }
            for i in 0..count {
                let mut dur = def_dur;
                let mut size = def_size;
                let mut sflags = if i == 0 { first_flags.unwrap_or(def_flags) } else { def_flags };
                let mut c = 0i32;
                if tf & 0x100 != 0 {
                    dur = be32(trun, q);
                    q += 4;
                }
                if tf & 0x200 != 0 {
                    size = be32(trun, q);
                    q += 4;
                }
                if tf & 0x400 != 0 {
                    sflags = be32(trun, q);
                    q += 4;
                }
                if tf & 0x800 != 0 {
                    let raw = be32(trun, q);
                    c = if version == 0 { raw as i32 } else { raw as i32 };
                    q += 4;
                }
                let key = sflags & 0x0001_0000 == 0;
                t.samples.push(Sample { offset, size, dts: time, cts: c, key: key || t.kind == Kind::Audio });
                offset += size as u64;
                time += dur as i64;
            }
            next_offset = Some(offset);
        }
    }
}

// ── reading ─────────────────────────────────────────────

fn pcm_normalize(data: Vec<u8>, bits: u16, big: bool, float: bool, signed8: bool) -> Vec<u8> {
    if float {
        let step = (bits / 8) as usize;
        return data
            .chunks_exact(step)
            .flat_map(|c| {
                let v = if step == 4 {
                    let a: [u8; 4] = c.try_into().unwrap();
                    (if big { f32::from_be_bytes(a) } else { f32::from_le_bytes(a) }) as f64
                } else {
                    let a: [u8; 8] = c.try_into().unwrap();
                    if big { f64::from_be_bytes(a) } else { f64::from_le_bytes(a) }
                };
                ((v.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes()
            })
            .collect();
    }
    match bits {
        8 => {
            if signed8 {
                data.into_iter().map(|b| b ^ 0x80).collect()
            } else {
                data
            }
        }
        32 => data
            .chunks_exact(4)
            .flat_map(|c| {
                let v = if big { [c[2], c[1], c[0]] } else { [c[1], c[2], c[3]] };
                v
            })
            .collect(),
        _ => {
            if !big {
                return data;
            }
            let step = (bits / 8) as usize;
            let mut out = data;
            for c in out.chunks_exact_mut(step) {
                c.reverse();
            }
            out
        }
    }
}

impl Demuxer for Mp4Demuxer {
    fn info(&self) -> &MediaInfo {
        &self.info
    }

    fn next(&mut self) -> Result<Option<Packet>, String> {
        // the track whose next sample comes earliest
        let mut best: Option<(usize, f64)> = None;
        for (i, t) in self.tracks.iter().enumerate() {
            if let Some(s) = t.samples.get(t.next) {
                let time = s.dts as f64 / t.timescale as f64;
                if best.map(|b| time < b.1).unwrap_or(true) {
                    best = Some((i, time));
                }
            }
        }
        let Some((ti, _)) = best else { return Ok(None) };
        let t = &mut self.tracks[ti];
        let s = &t.samples[t.next];
        let dur = t.samples.get(t.next + 1).map(|n| (n.dts - s.dts) as f64 / t.timescale as f64).unwrap_or(0.0);
        let mut data = vec![0u8; s.size as usize];
        self.file.seek(SeekFrom::Start(s.offset)).map_err(|e| format!("read error: {e}"))?;
        self.file.read_exact(&mut data).map_err(|_| "file is truncated (sample data past the end)".to_string())?;
        let data = match t.transform {
            Transform::None => data,
            Transform::Nal(n) => h264::normalize_prefixed(data, n),
            Transform::Pcm { bits, big, float, signed8 } => pcm_normalize(data, bits, big, float, signed8),
        };
        let ts = t.timescale as f64;
        let pkt = Packet {
            kind: t.kind,
            pts: (s.dts + s.cts as i64) as f64 / ts + t.shift,
            dts: Some(s.dts as f64 / ts + t.shift),
            dur,
            key: s.key,
            data,
        };
        t.next += 1;
        Ok(Some(pkt))
    }
}
