// File list: rows of custom shape, sorting by column, selection with mouse and arrows, Delete removes.
import { store, select, remove } from './files.js';
import { size, sizeParts, time, percentChange, esc } from './format.js';

const STATUS_ORDER = { running: 0, queued: 1, probing: 2, ready: 3, stopped: 4, done: 5, error: 6, bad: 7 };
const SORT_KEY = 'videosqueeze.sort';

let sort = { col: 'num', desc: false };
let listEl;
let headEl;
const rows = new Map();

export function initList() {
  listEl = document.getElementById('fileList');
  headEl = document.getElementById('listHead');
  try { Object.assign(sort, JSON.parse(localStorage.getItem(SORT_KEY) || '{}')); } catch { /* none saved */ }

  headEl.addEventListener('click', (e) => {
    const h = e.target.closest('[data-sort]');
    if (!h) return;
    const col = h.dataset.sort;
    sort = { col, desc: sort.col === col ? !sort.desc : false };
    try { localStorage.setItem(SORT_KEY, JSON.stringify(sort)); } catch { /* storage unavailable */ }
    renderAll();
  });

  listEl.addEventListener('mousedown', (e) => {
    const r = e.target.closest('.row');
    if (r) select(+r.dataset.id);
  });

  listEl.addEventListener('keydown', (e) => {
    const order = sorted();
    const i = order.findIndex((f) => f.id === store.selectedId);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const next = order[Math.max(0, Math.min(order.length - 1, i + (e.key === 'ArrowDown' ? 1 : -1)))];
      if (next) select(next.id);
    } else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      const next = e.key === 'Home' ? order[0] : order[order.length - 1];
      if (next) select(next.id);
    } else if (e.key === 'Delete' && i >= 0) {
      remove(order[i].id);
    }
  });
}

function resultBytes(f) {
  if (f.result) return f.result.out_size;
  if (f.plan && !f.plan.error) return f.plan.est_bytes;
  return Infinity;
}

function sortValue(f, col) {
  switch (col) {
    case 'name': return f.name.toLowerCase();
    case 'video': return f.info ? f.info.width * f.info.height : -1;
    case 'time': return f.info ? f.info.duration : -1;
    case 'size': return f.info ? f.info.size : -1;
    case 'result': return resultBytes(f);
    case 'status': return STATUS_ORDER[f.status];
    default: return f.id;
  }
}

function sorted() {
  const { col, desc } = sort;
  const list = store.files.slice();
  list.sort((a, b) => {
    const x = sortValue(a, col);
    const y = sortValue(b, col);
    const c = x < y ? -1 : x > y ? 1 : a.id - b.id;
    return desc ? -c : c;
  });
  return list;
}

function statusHtml(f) {
  switch (f.status) {
    case 'running': {
      const p = Math.round(f.progress * 100);
      return `<div class="rowprog"><div class="pbar"><i style="width:${p}%"></i></div><b>${p}%</b></div>`;
    }
    case 'probing': return 'READING';
    case 'ready': return 'READY';
    case 'queued': return 'QUEUED';
    case 'done': return 'DONE';
    case 'stopped': return 'STOPPED';
    case 'bad': return 'UNREADABLE';
    default: return 'ERROR';
  }
}

function resultHtml(f) {
  if (f.result) {
    const pc = percentChange(f.info ? f.info.size : 0, f.result.out_size);
    const pcText = pc == null ? '' : (pc > 0 ? '+' : '−') + Math.abs(pc) + '%';
    return { cls: 'got', html: `<b>${size(f.result.out_size)}</b><small class="${pc > 0 ? 'up' : ''}">${pcText}</small>` };
  }
  if (f.plan && f.plan.error) return { cls: 'bad', html: '<b>CAN\'T CONVERT</b>' };
  if (f.plan) {
    const [v, u] = sizeParts(f.plan.est_bytes);
    return { cls: 'est', html: `<b>${v} ${u}</b><small>${esc(f.plan.ext)}</small>` };
  }
  return { cls: '', html: '' };
}

function videoHtml(f) {
  const i = f.info;
  if (!i) return { cls: '', html: '' };
  if (!i.ok) return { cls: 'bad', html: '<b>—</b>' };
  if (!i.video) return { cls: '', html: `<b>${esc(i.audio || 'NO VIDEO')}</b>` };
  return { cls: i.decodable ? '' : 'bad', html: `<b>${esc(i.video)}</b><span>${i.width}×${i.height}</span>` };
}

function fill(row, f, index) {
  const res = resultHtml(f);
  const vid = videoHtml(f);
  row.className = 'row' + (f.id === store.selectedId ? ' sel' : '');
  row.innerHTML =
    `<span class="c-num">${String(index + 1).padStart(2, '0')}</span>` +
    `<span class="c-name"><b></b><em>${esc(f.ext)}</em></span>` +
    `<span class="c-video ${vid.cls}">${vid.html}</span>` +
    `<span class="c-time">${f.info && f.info.ok ? time(f.info.duration) : ''}</span>` +
    `<span class="c-size">${f.info ? size(f.info.size) : ''}</span>` +
    `<span class="c-res ${res.cls}">${res.html}</span>` +
    `<span class="c-stat ${f.status}">${statusHtml(f)}</span>`;
  row.querySelector('.c-name b').textContent = f.name;
  row.title = f.error ? `${f.path}\n${f.error}` : f.path;
}

export function renderAll() {
  headEl.querySelectorAll('[data-sort]').forEach((h) => {
    h.classList.toggle('sorted', h.dataset.sort === sort.col);
    h.classList.toggle('desc', h.dataset.sort === sort.col && sort.desc);
  });
  const frag = document.createDocumentFragment();
  rows.clear();
  sorted().forEach((f, i) => {
    const row = document.createElement('div');
    row.dataset.id = f.id;
    row.setAttribute('role', 'option');
    fill(row, f, i);
    rows.set(f.id, { row, index: i });
    frag.appendChild(row);
  });
  listEl.replaceChildren(frag);
  document.getElementById('fileCount').textContent = store.files.length;
  document.body.classList.toggle('no-files', store.files.length === 0);
}

// One row (progress) without rebuilding the list.
export function renderOne(f) {
  const r = rows.get(f.id);
  if (!r) return;
  if (f.status === 'running') {
    const bar = r.row.querySelector('.pbar i');
    if (bar) {
      const p = Math.round(f.progress * 100);
      bar.style.width = p + '%';
      r.row.querySelector('.rowprog b').textContent = p + '%';
      return;
    }
  }
  fill(r.row, f, r.index);
}

export function markSelection() {
  rows.forEach(({ row }, id) => row.classList.toggle('sel', id === store.selectedId));
  const r = rows.get(store.selectedId);
  if (r) r.row.scrollIntoView({ block: 'nearest' });
}

export function sortDependsOnResult() {
  return ['result', 'status'].includes(sort.col);
}
