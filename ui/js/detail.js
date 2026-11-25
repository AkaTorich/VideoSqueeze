// Selected file: source and result sizes as large readouts, the size ratio, job stages and progress,
// notes of the plan, details of source and output.
import { selected } from './files.js';
import { invoke } from './tauri.js';
import { size, sizeParts, time, bitrate, fps, percentChange, esc } from './format.js';

const $ = (id) => document.getElementById(id);
let shownId = null;
let shownOut = '';

function setDisp(el, text) {
  el.textContent = text;
  el.dataset.text = text;
}

function burst(el) {
  el.classList.remove('burst');
  void el.offsetWidth;
  el.classList.add('burst');
}

function sweep() {
  const p = $('prism');
  p.classList.remove('go');
  void p.offsetWidth;
  p.classList.add('go');
}

export function initDetail() {
  $('btnReveal').addEventListener('click', () => {
    const f = selected();
    if (f && f.result) invoke('reveal_path', { path: f.result.out_path });
  });
}

function sec(title) {
  return `<dt class="sec">${title}</dt>`;
}

function row(k, v, hl) {
  return `<dt>${k}</dt><dd${hl ? ' class="hl"' : ''}>${esc(v)}</dd>`;
}

export function renderDetail(animate) {
  const f = selected();
  const panel = $('detail');
  panel.classList.toggle('has-file', !!f);
  if (!f) {
    shownId = null;
    return;
  }
  const fresh = f.id !== shownId;
  shownId = f.id;
  const i = f.info;
  const p = f.plan;
  panel.classList.toggle('running', f.status === 'running' || f.status === 'queued');
  panel.classList.toggle('done', f.status === 'done');

  $('dName').textContent = f.name + (f.ext ? '.' + f.ext.toLowerCase() : '');
  $('dPath').textContent = f.path;

  // readouts
  const [sv, su] = i ? sizeParts(i.size) : ['—', ''];
  setDisp($('dSrc'), sv);
  $('dSrcUnit').textContent = su;
  $('dSrcSub').innerHTML = i && i.ok ? `<b>${esc(i.container)}</b> · ${i.width ? i.width + '×' + i.height : 'AUDIO'} · ${time(i.duration)}` : '';

  const outBytes = f.result ? f.result.out_size : p && !p.error ? p.est_bytes : null;
  const [ov, ou] = outBytes != null ? sizeParts(outBytes) : ['—', ''];
  setDisp($('dOut'), ov);
  $('dOutUnit').textContent = ou;
  $('dOutCap').textContent = f.result ? 'RESULT' : 'ESTIMATE';
  $('dOutSub').innerHTML = p && !p.error ? `<b>${esc(p.ext)}</b> · ${p.width ? p.width + '×' + p.height : 'AUDIO'}${p.fps ? ' · ' + fps(p.fps) : ''}` : '';
  const outKey = f.id + ':' + ov + ou;
  if (animate && outKey !== shownOut) burst($('dOut'));
  shownOut = outKey;

  // ratio
  const src = i ? i.size : 0;
  const share = src > 0 && outBytes != null ? outBytes / src : null;
  const big = share != null && share > 1;
  $('dRatioSrc').style.width = big ? (100 / share).toFixed(1) + '%' : '100%';
  $('dRatioOut').style.width = share == null ? '0%' : Math.min(100, share * 100).toFixed(1) + '%';
  const pc = percentChange(src, outBytes);
  $('dRatioTxt').innerHTML = pc == null ? '' : pc <= 0
    ? `${f.result ? 'SAVED' : 'SAVES ABOUT'} <b>${Math.abs(pc)}%</b> · ${size(Math.max(0, src - outBytes))}`
    : `<em>LARGER BY ${pc}%</em>`;

  // progress
  renderProcess(f);

  // notes
  const notes = [];
  if (f.error) notes.push(`<div class="note err">${esc(f.error)}</div>`);
  if (p && p.error && !f.error) notes.push(`<div class="note err">${esc(p.error)}</div>`);
  const list = (f.result && f.result.notes) || (p && p.notes) || [];
  for (const n of list) notes.push(`<div class="note">${esc(n)}</div>`);
  $('dNotes').innerHTML = notes.join('');

  // details
  let meta = '';
  if (i && i.ok) {
    meta += sec('SOURCE');
    meta += row('CONTAINER', i.container);
    if (i.video) {
      meta += row('VIDEO', `${i.video} · ${i.width}×${i.height}${i.rotation ? ' · TURNED ' + i.rotation + '°' : ''}`);
      meta += row('FRAME RATE', fps(i.fps));
      meta += row('VIDEO BITRATE', bitrate(i.video_bitrate));
    }
    meta += row('AUDIO', i.audio ? `${i.audio} · ${(i.sample_rate / 1000).toFixed(1)} KHZ · ${i.channels === 1 ? 'MONO' : i.channels === 2 ? 'STEREO' : i.channels + ' CH'}` : 'NONE');
    meta += row('LENGTH', time(i.duration));
  }
  if (p && !p.error) {
    meta += sec('OUTPUT');
    meta += row('FILE', f.result ? f.result.out_path.split(/[\\/]/).pop() : p.out_name, true);
    meta += row('FOLDER', p.out_dir);
    meta += row('VIDEO', p.video);
    meta += row('AUDIO', p.audio);
  }
  if (f.result) {
    meta += sec('DONE');
    meta += row('TIME', time(f.result.seconds) + (f.result.seconds < 60 ? ` (${f.result.seconds.toFixed(1)} S)` : ''));
    if (f.result.frames && f.result.seconds > 0) meta += row('SPEED', fps(f.result.frames / f.result.seconds));
  }
  $('dMeta').innerHTML = meta;

  if (fresh && animate) sweep();
}

const STAGES = ['OPENING FILE', 'CONVERTING', 'WRITING FILE'];

function renderProcess(f) {
  const running = f.status === 'running';
  const stage = running ? f.stage : -1;
  document.querySelectorAll('#detail .stage').forEach((el, k) => {
    el.classList.toggle('done', running && k < stage);
    el.classList.toggle('active', running && k === stage);
    el.classList.remove('failed');
  });
  const bar = $('dProg');
  const pct = Math.round((running ? f.progress : 0) * 100);
  bar.style.setProperty('--p', pct);
  bar.querySelector('.pct').textContent = pct + '%';
  bar.classList.toggle('active', running);
  const msg = $('dProcMsg');
  const speed = f.speed > 0 ? ` · <b>${Math.round(f.speed)}</b> FPS` : '';
  const realtime = f.speed > 0 && f.info && f.info.fps > 0 ? ` · ${(f.speed / f.info.fps).toFixed(1)}× REALTIME` : '';
  msg.innerHTML = f.status === 'queued' ? 'IN QUEUE' : running ? `${STAGES[stage] || ''}${stage === 1 ? speed + realtime : ''}` : '';
}

export function refreshProgress() {
  const f = selected();
  if (f) renderProcess(f);
}
