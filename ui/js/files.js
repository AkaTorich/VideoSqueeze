// Files in the window and their link to Rust: description (probe_files), plan under the current settings
// (plan_files), the job queue (enqueue_jobs, cancel_queue) and its events (job-progress, job-done).
import { invoke, listen } from './tauri.js';
import { settings } from './settings.js';

export const store = {
  files: [],
  byId: new Map(),
  selectedId: null,
  nextId: 1,
};

const listeners = new Set();
export function onChange(fn) { listeners.add(fn); }
function emit(kind, file) { listeners.forEach((fn) => fn(kind, file)); }

function makeFile(path) {
  const parts = path.split(/[\\/]/);
  const name = parts.pop();
  const dot = name.lastIndexOf('.');
  return {
    id: store.nextId++,
    path,
    name: dot > 0 ? name.slice(0, dot) : name,
    ext: dot > 0 ? name.slice(dot + 1).toUpperCase() : '',
    dir: parts.join('\\'),
    status: 'probing', // probing | ready | bad | queued | running | done | error | stopped
    info: null,
    plan: null,
    stage: 0,
    progress: 0,
    speed: 0,
    result: null,
    error: null,
  };
}

const ACTIVE = new Set(['queued', 'running']);

// Files and folders → new files (repeats skipped), then their descriptions and plans.
export async function addPaths(paths) {
  if (!paths || !paths.length) return 0;
  const found = (await invoke('scan_paths', { paths })) || [];
  const have = new Set(store.files.map((f) => f.path.toLowerCase()));
  const fresh = [];
  for (const p of found) {
    const key = p.toLowerCase();
    if (have.has(key)) continue;
    have.add(key);
    const f = makeFile(p);
    store.files.push(f);
    store.byId.set(f.id, f);
    fresh.push(f);
  }
  if (!fresh.length) return 0;
  emit('add');
  if (store.selectedId == null) select(fresh[0].id);
  const infos = (await invoke('probe_files', { paths: fresh.map((f) => f.path) })) || [];
  infos.forEach((info, i) => {
    const f = fresh[i];
    if (!f || !store.byId.has(f.id)) return;
    f.info = info;
    if (f.status === 'probing') f.status = info.ok ? 'ready' : 'bad';
    if (!info.ok) f.error = info.error;
  });
  emit('probed');
  await refreshPlans(fresh);
  return fresh.length;
}

let planTimer = null;
let planGen = 0;

// Plans of the given files (all waiting ones by default) under the current settings.
export async function refreshPlans(files) {
  const list = (files || store.files).filter((f) => f.info && f.info.ok && !ACTIVE.has(f.status));
  if (!list.length) return;
  const gen = ++planGen;
  const plans = (await invoke('plan_files', { paths: list.map((f) => f.path), settings })) || [];
  if (gen !== planGen && !files) return;
  plans.forEach((p, i) => { if (list[i]) list[i].plan = p; });
  emit('plans');
}

export function settingsChanged() {
  clearTimeout(planTimer);
  planTimer = setTimeout(() => refreshPlans(), 120);
}

export function select(id) {
  if (store.selectedId === id) return;
  store.selectedId = id;
  emit('select', store.byId.get(id));
}

export function selected() {
  return store.byId.get(store.selectedId) || null;
}

export function isBusy() {
  return store.files.some((f) => ACTIVE.has(f.status));
}

// Files START will take: everything readable that isn't done; if all are done — all of them again.
export function startable() {
  const open = store.files.filter((f) => f.info && f.info.ok && ['ready', 'stopped', 'error'].includes(f.status));
  return open.length ? open : store.files.filter((f) => f.status === 'done');
}

export async function start() {
  const list = startable();
  if (!list.length) return;
  for (const f of list) {
    f.status = 'queued';
    f.progress = 0;
    f.stage = 0;
    f.error = null;
    f.result = null;
  }
  emit('queue');
  await invoke('enqueue_jobs', { jobs: list.map((f) => ({ id: f.id, path: f.path })), settings });
}

export async function stopAll() {
  await invoke('cancel_queue');
  for (const f of store.files) {
    if (ACTIVE.has(f.status)) {
      f.status = 'stopped';
      f.progress = 0;
    }
  }
  emit('stop');
  refreshPlans();
}

export async function clearAll() {
  await invoke('cancel_queue');
  store.files = [];
  store.byId.clear();
  store.selectedId = null;
  emit('clear');
}

export function remove(id) {
  const i = store.files.findIndex((f) => f.id === id);
  if (i < 0 || ACTIVE.has(store.files[i].status)) return;
  store.files.splice(i, 1);
  store.byId.delete(id);
  if (store.selectedId === id) {
    const next = store.files[Math.min(i, store.files.length - 1)];
    store.selectedId = next ? next.id : null;
  }
  emit('remove');
  emit('select', selected());
}

export function initEvents() {
  listen('job-progress', (e) => {
    const f = store.byId.get(e.payload.id);
    if (!f || !ACTIVE.has(f.status)) return;
    f.status = 'running';
    f.stage = e.payload.stage;
    f.progress = e.payload.progress;
    if (e.payload.speed > 0) f.speed = e.payload.speed;
    emit('progress', f);
  });
  listen('job-done', (e) => {
    const p = e.payload;
    const f = store.byId.get(p.id);
    if (!f) return;
    if (p.result) {
      f.result = p.result;
      f.status = 'done';
      f.progress = 1;
    } else if (p.error === 'cancelled') {
      f.status = 'stopped';
    } else {
      f.status = 'error';
      f.error = p.error;
    }
    emit('done', f);
  });
}

// For checking the window without real files.
export function inject(files) {
  for (const t of files) {
    const n = makeFile(t.path);
    Object.assign(n, t);
    store.files.push(n);
    store.byId.set(n.id, n);
  }
  emit('add');
  if (store.selectedId == null && store.files.length) select(store.files[0].id);
}
