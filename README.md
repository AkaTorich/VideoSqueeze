# VideoSqueeze — video compressor and converter

A Rust + Tauri 2 application that compresses and converts video **without FFmpeg**: every container reader,
writer and codec is Rust code or a library built from source with the program. Files are processed in batches:
add files or whole folders, choose one set of settings, press START — the queue converts them one by one.
The interface is in English, cyberpunk-styled (pink, cyan, yellow, the TempoKey look) with a custom window
shape, its own font, colour dispersion effects and a splash screen.

## What it reads and writes

| | |
|---|---|
| **Containers in** | MP4, M4V, MOV, 3GP (including fragmented MP4), MKV, WebM, AVI (with OpenDML parts over 1 GB), .divx/.xvid |
| **Video decoded** | H.264 (with B-frames), MPEG-4 Part 2 — XviD, DivX 4/5/6, MP4V, H.263 (with B-frames and packed bitstream), Microsoft MPEG-4 v1/v2/v3 — DivX 3, MJPEG (interlaced AVI MJPEG too) |
| **Video copied as is** | the above plus HEVC, VP8, VP9, AV1 and any AVI FourCC (into MKV) |
| **Audio decoded** | AAC (LC, HE-AAC, HE-AAC v2), MP3/MP2/MP1, AC-3, PCM (8/16/24/32-bit, float, big-endian) |
| **Formats out** | MP4 · MKV · MOV · AVI with H.264 video; animated GIF; audio as is (.m4a/.mp3/.ac3/.wav/.mka); M4A (AAC); WAV (PCM) |

## Settings (one set for the whole list)

- **Video**: RE-ENCODE (H.264) or COPY (change the container only — seconds instead of minutes).
- **Size by**: QUALITY (BEST … TINY = bits per pixel per frame; never more than the source spent on the
  same picture), FILE SIZE (target megabytes — the bitrate is computed from the length minus the audio),
  BITRATE (kbit/s).
- **Resolution** (ORIGINAL, 2160P … 144P, only downscaling), **frame rate** (ORIGINAL or a lower rate;
  AVI gets a constant rate).
- **Audio**: AUTO (kept when the container can hold it; PCM always becomes AAC), AAC at a chosen bitrate,
  REMOVE.
- **Save to**: next to each source or one folder; **name suffix** (default `_squeezed`); existing files are
  never overwritten (`name (2).mp4`).

When a file can't go where the settings say, the plan adapts and says so in the file's notes: a codec that
can't be copied into the container is re-encoded; an undecodable one (VP9, HEVC…) is copied into MKV;
audio that a container can't hold is re-encoded to AAC.

## How it works

### Reading — `src/media/`
- `mod.rs` → `open` (container by its first bytes, not the extension), `probe`, `MediaInfo`, `Packet`,
  `video_by_fourcc` (FourCC → codec); packets are normalized: H.264/HEVC as 4-byte length-prefixed NAL units,
  MPEG audio and AC-3 one frame per packet, PCM signed little-endian.
- `mp4_in.rs` → `Mp4Demuxer`: own ISO BMFF/QuickTime parser — sample tables (stts, ctts, stss, stsc,
  stsz/stz2, stco/co64), fragments (moof/traf/trun with trex defaults), edit lists, rotation matrix,
  avcC/hvcC/esds, QuickTime PCM (sowt, twos, in24, in32, fl32, fl64, lpcm) read by whole chunks.
- `mkv_in.rs` → `MkvDemuxer` (matroska-demuxer): codec IDs, VfW-in-Matroska, laced MPEG audio/AC-3 re-split.
- `avi_in.rs` → `AviDemuxer`: own RIFF parser, AVIX continuations, idx1 keyframes, time from frame numbers
  (video) and bytes or chunk counts (audio); in-band SPS/PPS and VOL headers picked from the first frame.
- `h264.rs` (Annex B ↔ length-prefixed, avcC), `aac.rs` (AudioSpecificConfig, ADTS), `mp3.rs` and `ac3.rs`
  (frame splitters, 'dac3').

### Codecs — `src/codec/`
- `video.rs` → `VideoDecoder`: OpenH264 (built from source by the `openh264` crate), `oxideav-mpeg4video`
  (MPEG-4 Part 2, ecosystem-compat mode), `oxideav-msmpeg4` (DivX 3; the real codec is sniffed from the first
  packet — FourCCs lie), `jpeg-decoder` (MJPEG, two fields woven back). Display-order times come from the
  sorted packet times.
- `encode.rs` → `H264Encoder` (OpenH264, bitrate control, no frame skipping, keyframe every 5 s).
- `scale.rs` → `scale_frame` (separable triangle filter, widened when downscaling — no aliasing),
  `rotate_frame`, `matrix_rotation`; `color.rs` → RGB ↔ YUV 4:2:0 (BT.601/BT.709).
- `audio.rs` → `AudioDecoder` (FDK AAC from `fdk-aac-rs` — a pure-Rust port, Symphonia for MPEG audio,
  `oxideav-ac3`, PCM), `AacEncoder` (FDK AAC-LC, CBR + afterburner, linear resampling to an AAC rate,
  encoder delay kept for the edit list).

### Writing — `src/mux/`
- `mp4_out.rs` → `Mp4Muxer` (MP4/MOV/M4A): data into a side file, then `moov` before `mdat` ("fast start"),
  edit lists put every track on the source timeline (B-frame offsets, AAC priming), co64 over 4 GB.
- `mkv_out.rs` → `MkvMuxer` (MKV/MKA): clusters of SimpleBlocks, cues, seek head and duration patched at the end.
- `avi_out.rs` → `AviMuxer`: hdrl/movi/idx1, AVIX parts past 1 GB, empty chunks for missing frames, SPS/PPS
  before every H.264 keyframe; `bitmap_info` (also used for V_MS/VFW/FOURCC in MKV).
- `gif_out.rs` → `GifWriter` (per-frame NeuQuant palette, delays from frame times);
  `raw_out.rs` → `FrameFile` (.mp3/.ac3), `WavWriter`.

### One conversion — `src/job/`
- `settings.rs` → `Settings` (what the window sends).
- `plan.rs` → `make`: copy or re-encode per track, output size/rate/bitrate, file name, expected size and
  notes; `view` — what the window shows; `unique` — free output name.
- `gate.rs` → `Gate`: frame-rate limit (variable: drop; constant for AVI: repeat over gaps).
- `run.rs` → `run`: the pipeline (decode → turn → scale → gate → encode → mux), stages and progress,
  removes a half-written file on error or stop.

### Application — `src/`
- `lib.rs` — the core above as a library (the program and the test generator use it).
- `main.rs` — Tauri window and commands; `cli.rs` — work without a window:
  `VideoSqueeze.exe --probe <file> --out <json>`, `--convert <file> --settings <json> --out <json>`.
- `commands.rs` → `probe_files`, `plan_files` (in parallel), `enqueue_jobs`, `cancel_queue`, `scan_paths`
  (folders with subfolders), `reveal_path` (Explorer).
- `worker.rs` → queue thread, one file at a time, `job-progress` / `job-done` events, stop by generation.
- `shape.rs` → window region along the outline (SetWindowRgn); `boot.rs` → splash steps and reveal.

## Window — `ui/`
- `index.html` — main window, `splash.html` — splash screen.
- `css/base.css` (palette, own font), `frame.css` (window frame, title bar, status bar around the notch),
  `controls.css` (cut-corner buttons, fields, lists, progress), `panels.css` (list, side panel),
  `settings.css` (settings rows), `effects.css` (dispersion, prism sweep, drop overlay), `splash.css`.
- `js/app.js` — assembly, status bar, totals, start-up steps; `files.js` — files, plans, queue events;
  `list.js` — the file list; `detail.js` — selected file; `settings.js` — settings and their rows;
  `select.js` — custom drop-down; `shape.js` — window outline; `guard.js` — no browser menu/keys;
  `format.js` — sizes, times, bitrates; `tauri.js` — bridge and dialogs; `demo.js` — sample files for checks.
- `fonts/VideoSqueeze-*.ttf` — own font drawn by `tools/font` (`python tools/font/make_font.py`).

## Tools
- `icons/` made by `python tools/icon/make_icon.py` (icon, NSIS pictures).
- `examples/make_tests.rs` — test videos with known content into `target/testfiles`
  (`cargo run --release --example make_tests`): H.264+AAC MP4 and MKV, XviD with B-frames, DivX 3 and MJPEG
  AVIs, a phone video turned 90°.
- `tools/testgen/convert_all.py` — every test file through every output format (windowless mode) and the
  results read back.
- `tools/check/` — window checks through WebView2's debug port (`run-debug.ps1`, `cdp.ps1`,
  `desktop-shot.ps1`, `region-info.ps1`, `shots.ps1`, `splash-shots.ps1`).

## Build
```
cargo tauri build
```
Result: `target\release\VideoSqueeze.exe` and `target\release\bundle\nsis\VideoSqueeze_1.0.0_x64-setup.exe`.
OpenH264 is compiled from its bundled C++ source (needs the MSVC build tools).
