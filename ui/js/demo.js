// Plausible files for checking the window layout without real videos (tools/check, browser preview).
import { inject } from './files.js';

const MB = 1024 * 1024;

function info(container, video, w, h, fps, vbr, audio, dur, sizeMb, rotation = 0) {
  return { ok: true, container, video, width: w, height: h, fps, video_bitrate: vbr, decodable: true, audio, audio_bitrate: 160000, sample_rate: 48000, channels: 2, duration: dur, size: sizeMb * MB, rotation };
}

function plan(ext, name, w, h, fps, estMb, video, audio, notes = []) {
  return { out_name: name, out_dir: 'C:\\Users\\Me\\Videos', ext, video, audio, width: w, height: h, fps, est_bytes: estMb * MB, notes, error: null };
}

export function demo() {
  inject([
    {
      path: 'C:\\Users\\Me\\Videos\\Holiday_2024_beach.mp4', status: 'done',
      info: info('MP4', 'H.264', 3840, 2160, 30, 42e6, 'AAC', 214, 1100),
      plan: plan('MP4', 'Holiday_2024_beach_squeezed.mp4', 1920, 1080, 30, 168, 'H.264 · 4.6 MBIT/S', 'COPY · AAC'),
      result: { out_path: 'C:\\Users\\Me\\Videos\\Holiday_2024_beach_squeezed.mp4', out_size: 161 * MB, in_size: 1100 * MB, seconds: 96.4, frames: 6420, notes: [] },
    },
    {
      path: 'C:\\Users\\Me\\Videos\\old_movies\\Terminator2.avi', status: 'running', stage: 1, progress: 0.43, speed: 212,
      info: info('AVI', 'XVID', 720, 304, 23.976, 1.2e6, 'MP3', 8220, 1400),
      plan: plan('MP4', 'Terminator2_squeezed.mp4', 720, 304, 23.976, 690, 'H.264 · 600 KBIT/S', 'COPY · MP3'),
    },
    {
      path: 'C:\\Users\\Me\\Videos\\old_movies\\Matrix.divx', status: 'queued',
      info: info('AVI', 'DIVX 3 (DIV3)', 640, 272, 25, 0.9e6, 'AC-3', 8160, 1050),
      plan: plan('MP4', 'Matrix_squeezed.mp4', 640, 272, 25, 610, 'H.264 · 500 KBIT/S', 'COPY · AC-3'),
    },
    {
      path: 'C:\\Users\\Me\\Phone\\VID_20241012_181502.mp4', status: 'ready',
      info: info('MP4', 'H.264', 1920, 1080, 29.97, 17e6, 'AAC', 48, 104, 90),
      plan: plan('MP4', 'VID_20241012_181502_squeezed.mp4', 1080, 1920, 29.97, 28, 'H.264 · 4.6 MBIT/S', 'COPY · AAC'),
    },
    {
      path: 'C:\\Users\\Me\\Downloads\\concert.mkv', status: 'ready',
      info: info('MKV', 'H.264', 1280, 720, 25, 3.1e6, 'AC-3', 1830, 760),
      plan: plan('MP4', 'concert_squeezed.mp4', 1280, 720, 25, 402, 'H.264 · 1.7 MBIT/S', 'COPY · AC-3'),
    },
    {
      path: 'C:\\Users\\Me\\Downloads\\screen_capture.webm', status: 'ready',
      info: { ...info('WEBM', 'VP9', 2560, 1440, 60, 6e6, 'OPUS', 310, 240), decodable: false },
      plan: plan('MKV', 'screen_capture_squeezed.mkv', 2560, 1440, 60, 240, 'COPY · VP9', 'COPY · OPUS', ['SAVED AS MKV — THE ONLY CONTAINER FOR THIS CODEC', 'VP9 CAN\'T BE RE-ENCODED — COPIED AS IS']),
    },
  ]);
}
