// Window assembly: window buttons, toolbar, settings, file drops, status bar, start-up steps for the splash.
import { appWindow, inTauri, invoke, listen, pickFiles, pickFolders } from './tauri.js';
import { initGuard } from './guard.js';
import { initShape } from './shape.js';
import { settings, loadSettings, initSettings } from './settings.js';
import { store, onChange, addPaths, start, stopAll, clearAll, isBusy, startable, initEvents, selected, settingsChanged } from './files.js';
import { initList, renderAll, renderOne, markSelection, sortDependsOnResult } from './list.js';
import { initDetail, renderDetail, refreshProgress } from './detail.js';
import { size } from './format.js';
import { demo } from './demo.js';

const $ = (id) => document.getElementById(id);

initGuard();
loadSettings();
const shape = initShape();
initList();
initDetail();
initSettings(() => {
  settingsChanged();
  updateChrome();
});

// ── window buttons ──
$('btnMin').addEventListener('click', () => inTauri && appWindow.minimize());
$('btnMax').addEventListener('click', () => inTauri && appWindow.toggleMaximize());
$('btnClose').addEventListener('click', () => inTauri && appWindow.close());

// ── toolbar ──
$('btnFiles').addEventListener('click', async () => addPaths(await pickFiles()));
$('btnFolder').addEventListener('click', async () => addPaths(await pickFolders()));
$('btnStart').addEventListener('click', () => start());
$('btnStop').addEventListener('click', () => stopAll());
$('btnClear').addEventListener('click', () => clearAll());

// ── dropping files and folders onto the window ──
listen('tauri://drag-enter', () => document.body.classList.add('dragging'));
listen('tauri://drag-leave', () => document.body.classList.remove('dragging'));
listen('tauri://drag-drop', (e) => {
  document.body.classList.remove('dragging');
  addPaths(e.payload.paths || []);
});

// ── status bar, totals and button states ──
function updateChrome() {
  const files = store.files;
  const total = files.length;
  const busy = isBusy();
  const jobs = files.filter((f) => ['queued', 'running', 'done', 'error', 'stopped'].includes(f.status) && (f.status !== 'stopped' || f.result));
  const running = files.find((f) => f.status === 'running');
  const done = files.filter((f) => f.status === 'done');
  const errors = files.filter((f) => f.status === 'error').length;

  $('btnStart').disabled = busy || startable().length === 0;
  $('btnStop').disabled = !busy;
  $('btnClear').disabled = total === 0;

  // totals of the list: source size → expected or reached size
  const src = files.reduce((s, f) => s + (f.info && f.info.ok ? f.info.size : 0), 0);
  const out = files.reduce((s, f) => s + (f.result ? f.result.out_size : f.plan && !f.plan.error ? f.plan.est_bytes : 0), 0);
  $('totals').innerHTML = total ? `<b>${size(src)}</b> → <em>${size(out)}</em>${src > 0 && out > 0 && out < src ? ` · <i>−${Math.round((1 - out / src) * 100)}%</i>` : ''}` : '';

  // whole-queue progress: finished jobs plus the share of the running one
  const inQueue = jobs.length;
  const finished = jobs.filter((f) => f.status !== 'queued' && f.status !== 'running').length;
  const part = busy && inQueue ? (finished + (running ? running.progress : 0)) / inQueue : done.length && !busy ? 1 : 0;
  const prog = $('sbProg');
  prog.style.setProperty('--p', Math.round(part * 100));
  $('sbPct').textContent = Math.round(part * 100) + '%';
  prog.classList.toggle('active', busy);
  const err = errors ? ` · <em>${errors} ERROR${errors > 1 ? 'S' : ''}</em>` : '';
  let text;
  if (!total) text = 'IDLE · ADD VIDEOS TO START';
  else if (busy) text = `CONVERTING <b>${Math.min(finished + 1, inQueue)}</b> / ${inQueue} · <i></i>${err}`;
  else if (done.length) {
    const saved = done.reduce((s, f) => s + (f.info ? f.info.size : 0) - f.result.out_size, 0);
    text = `DONE · <b>${done.length}</b> FILE${done.length > 1 ? 'S' : ''}${saved > 0 ? ` · SAVED <b>${size(saved)}</b>` : ''}${err}`;
  } else text = `READY · <b>${total}</b> FILE${total > 1 ? 'S' : ''} · PRESS START${err}`;
  $('sbText').innerHTML = text;
  const name = $('sbText').querySelector('i');
  if (name) name.textContent = running ? running.name : '';
}

onChange((kind, f) => {
  switch (kind) {
    case 'add':
    case 'clear':
    case 'remove':
    case 'probed':
    case 'plans':
    case 'queue':
    case 'stop':
      renderAll();
      renderDetail(kind === 'add' || kind === 'plans');
      break;
    case 'progress':
      renderOne(f);
      if (f === selected()) refreshProgress();
      updateChrome();
      return;
    case 'done':
      if (sortDependsOnResult()) renderAll(); else renderOne(f);
      if (f === selected()) renderDetail(true);
      break;
    case 'select':
      markSelection();
      renderDetail(true);
      break;
  }
  updateChrome();
});

initEvents();

// The window builds itself hidden and reports steps to the splash (promises only, no timers — a hidden
// window throttles them).
async function boot() {
  const step = (progress, label) => (inTauri ? invoke('boot_step', { progress, label }).catch(() => {}) : Promise.resolve());
  await step(0.12, 'INITIALIZING CORE');
  await step(0.3, 'LOADING FONT');
  try {
    await Promise.all([document.fonts.load('400 16px VideoSqueeze'), document.fonts.load('700 16px VideoSqueeze')]);
  } catch { /* the fallback font stays */ }
  await document.fonts.ready;
  await step(0.5, 'LOADING CODECS');
  await step(0.66, 'BUILDING INTERFACE');
  renderAll();
  updateChrome();
  await step(0.82, 'DRAWING PANELS');
  renderDetail(false);
  await step(0.92, 'SHAPING WINDOW');
  await shape.ready;
  await step(1, 'READY');
  $('fileList').focus();
}
boot();

// for checking the window through the WebView2 debug port (tools/check)
window.App = { demo, addPaths, store, settings };
