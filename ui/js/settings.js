// Conversion settings: the values sent to Rust (job/settings.rs), their lists, saving between runs,
// and the settings block — rows that don't apply to the chosen format are hidden.
import { makeSelect } from './select.js';
import { pickOutputFolder } from './tauri.js';

const KEY = 'videosqueeze.settings';

export const FORMATS = [
  { value: 'mp4', label: 'MP4 · H.264', short: 'MP4 · H.264' },
  { value: 'mkv', label: 'MKV · H.264', short: 'MKV · H.264' },
  { value: 'mov', label: 'MOV · H.264', short: 'MOV · H.264' },
  { value: 'avi', label: 'AVI · H.264', short: 'AVI · H.264' },
  { value: 'gif', label: 'GIF · ANIMATION', short: 'GIF' },
  { value: 'audio', label: 'AUDIO · AS IS', short: 'AUDIO · AS IS' },
  { value: 'm4a', label: 'M4A · AAC AUDIO', short: 'M4A · AAC' },
  { value: 'wav', label: 'WAV · PCM AUDIO', short: 'WAV · PCM' },
];

const VIDEO_MODES = [
  { value: 'encode', label: 'RE-ENCODE · COMPRESS', short: 'RE-ENCODE' },
  { value: 'copy', label: 'COPY · CHANGE CONTAINER ONLY', short: 'COPY AS IS' },
];

const SIZE_MODES = [
  { value: 'quality', label: 'QUALITY' },
  { value: 'target', label: 'FILE SIZE' },
  { value: 'bitrate', label: 'BITRATE' },
];

// bits per pixel per frame
const QUALITIES = [
  { value: 0.16, label: 'BEST' },
  { value: 0.11, label: 'HIGH' },
  { value: 0.075, label: 'BALANCED' },
  { value: 0.05, label: 'SMALL' },
  { value: 0.03, label: 'TINY' },
];

const HEIGHTS = [0, 2160, 1440, 1080, 720, 576, 480, 360, 240, 144].map((h) => ({ value: h, label: h ? h + 'P' : 'ORIGINAL' }));
const RATES = [0, 60, 50, 30, 25, 24, 20, 15, 12, 10, 5].map((f) => ({ value: f, label: f ? f + ' FPS' : 'ORIGINAL' }));
const AUDIO = [
  { value: 'auto', label: 'AUTO · KEEP IF POSSIBLE', short: 'AUTO' },
  { value: 'aac', label: 'AAC · RE-ENCODE', short: 'AAC' },
  { value: 'remove', label: 'REMOVE', short: 'REMOVE' },
];
const AUDIO_M4A = AUDIO.slice(0, 2);
const AUDIO_KBPS = [320, 256, 192, 160, 128, 96, 64, 48].map((k) => ({ value: k, label: k + ' KBIT/S' }));

export const settings = {
  format: 'mp4',
  videoMode: 'encode',
  sizeMode: 'quality',
  qualityBpp: 0.075,
  targetMb: 25,
  bitrateKbps: 2000,
  height: 0,
  fps: 0,
  audio: 'auto',
  audioKbps: 128,
  outDir: '',
  suffix: '_squeezed',
};

export function loadSettings() {
  try {
    Object.assign(settings, JSON.parse(localStorage.getItem(KEY) || '{}'));
  } catch { /* nothing saved yet */ }
}

function save() {
  try { localStorage.setItem(KEY, JSON.stringify(settings)); } catch { /* storage unavailable */ }
}

const $ = (id) => document.getElementById(id);
let changed = () => {};

function applies(row) {
  const f = settings.format;
  const video = ['mp4', 'mkv', 'mov', 'avi'].includes(f);
  const encode = video && settings.videoMode === 'encode';
  switch (row) {
    case 'video': return video;
    case 'size': return encode;
    case 'quality': return encode && settings.sizeMode === 'quality';
    case 'target': return encode && settings.sizeMode === 'target';
    case 'bitrate': return encode && settings.sizeMode === 'bitrate';
    case 'height':
    case 'fps': return encode || f === 'gif';
    case 'audio': return video || f === 'm4a';
    case 'audioKbps': return (video || f === 'm4a') && settings.audio === 'aac';
    default: return true;
  }
}

function layout() {
  document.querySelectorAll('#settings .srow[data-for]').forEach((r) => r.classList.toggle('off', !applies(r.dataset.for)));
  const out = $('outPath');
  out.classList.toggle('src', !settings.outDir);
  out.textContent = settings.outDir || 'NEXT TO EACH SOURCE FILE';
  out.title = settings.outDir;
}

function update(patch) {
  Object.assign(settings, patch);
  save();
  layout();
  changed();
}

export function initSettings(onChange) {
  changed = onChange;
  makeSelect($('selFormat'), FORMATS, settings.format, (v) => {
    const patch = { format: v };
    // M4A has no REMOVE
    if (v === 'm4a' && settings.audio === 'remove') patch.audio = 'auto';
    update(patch);
    rebuildAudio();
  });
  makeSelect($('selVideo'), VIDEO_MODES, settings.videoMode, (v) => update({ videoMode: v }));
  makeSelect($('selSize'), SIZE_MODES, settings.sizeMode, (v) => update({ sizeMode: v }));
  makeSelect($('selQuality'), QUALITIES, settings.qualityBpp, (v) => update({ qualityBpp: v }));
  makeSelect($('selHeight'), HEIGHTS, settings.height, (v) => update({ height: v }));
  makeSelect($('selFps'), RATES, settings.fps, (v) => update({ fps: v }));
  makeSelect($('selAudioKbps'), AUDIO_KBPS, settings.audioKbps, (v) => update({ audioKbps: v }));
  rebuildAudio();

  numberField($('fTarget'), 'targetMb', 0.1, 100000, 1);
  numberField($('fBitrate'), 'bitrateKbps', 50, 200000, 100);

  const suffix = $('fSuffix');
  suffix.value = settings.suffix;
  suffix.addEventListener('input', () => update({ suffix: suffix.value.replace(/[\\/:*?"<>|]/g, '') }));
  suffix.addEventListener('keydown', (e) => { if (e.key === 'Enter') suffix.blur(); });

  $('btnOutChoose').addEventListener('click', async () => {
    const dir = await pickOutputFolder();
    if (dir) update({ outDir: dir });
  });
  $('btnOutSource').addEventListener('click', () => update({ outDir: '' }));
  layout();
}

function rebuildAudio() {
  const host = $('selAudio');
  const fresh = host.cloneNode(false);
  host.replaceWith(fresh);
  document.querySelectorAll('.cs-menu.audio-menu').forEach((m) => m.remove());
  const list = settings.format === 'm4a' ? AUDIO_M4A : AUDIO;
  makeSelect(fresh, list, settings.audio, (v) => update({ audio: v }));
  const menus = document.querySelectorAll('.cs-menu');
  menus[menus.length - 1].classList.add('audio-menu');
}

function numberField(input, key, min, max, step) {
  input.value = settings[key];
  const apply = () => {
    const v = Number(input.value);
    const ok = Number.isFinite(v) && v >= min && v <= max;
    input.parentElement.classList.toggle('bad', !ok);
    if (ok && v !== settings[key]) update({ [key]: v });
  };
  input.addEventListener('input', apply);
  input.addEventListener('wheel', (e) => {
    e.preventDefault();
    const v = Math.max(min, Math.min(max, Number(input.value || 0) + (e.deltaY < 0 ? step : -step)));
    input.value = Math.round(v * 100) / 100;
    apply();
  }, { passive: false });
  input.addEventListener('keydown', (e) => { if (e.key === 'Enter') input.blur(); });
}
