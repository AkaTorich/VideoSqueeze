// Splash: the main window builds itself hidden and sends steps (event boot); here they show as a log and a bar.
// The bar moves smoothly and takes at least MIN_MS for the whole way; at full — READY and the main window shows.
import { invoke, listen, inTauri } from './tauri.js';
import { initGuard } from './guard.js';
import { initShape } from './shape.js';

const MIN_MS = 1400;
const HOLD_MS = 320;

initGuard();
initShape({ cutBig: 24, cutSmall: 10, notch: 6, notchAt: 0.56, notchLen: 150 });

const bar = document.getElementById('spProg');
const pct = document.getElementById('spPct');
const stepEl = document.getElementById('spStep');
const log = document.getElementById('spLog');

const steps = [];          // {p, label, el}
let target = 0;
let shown = 0;
let finished = false;
const start = performance.now();

function addStep(s) {
  if (!s || !s.label) return;
  if (s.progress > target) target = s.progress;
  if (s.label === 'READY' || steps.some((x) => x.label === s.label)) return;
  steps.push({ p: s.progress, label: s.label, el: null });
}

listen('boot', (e) => addStep(e.payload));
if (inTauri) invoke('boot_status').then(addStep);

function paintLog() {
  steps.forEach((s, i) => {
    const from = i > 0 ? steps[i - 1].p : 0;
    if (shown + 1e-3 < from) return;
    if (!s.el) {
      s.el = document.createElement('div');
      s.el.className = 'sp-line';
      s.el.innerHTML = '<span class="lb"></span><span class="dots"></span><span class="st"></span>';
      s.el.querySelector('.lb').textContent = s.label;
      log.appendChild(s.el);
      while (log.children.length > 5) log.removeChild(log.firstChild);
      stepEl.textContent = s.label;
    }
    const ok = shown + 1e-3 >= s.p;
    s.el.classList.toggle('active', !ok);
    s.el.querySelector('.st').textContent = ok ? 'OK' : '··';
  });
}

function tick(now) {
  const cap = Math.min(target, (now - start) / MIN_MS);
  shown += (cap - shown) * 0.16;
  if (cap - shown < 0.002) shown = cap;
  bar.style.setProperty('--p', (shown * 100).toFixed(1));
  pct.textContent = Math.round(shown * 100) + '%';
  paintLog();
  if (!finished && target >= 1 && shown >= 0.999) {
    finished = true;
    document.body.classList.add('ready');
    stepEl.textContent = 'READY';
    const p = document.getElementById('prism');
    p.classList.add('go');
    setTimeout(() => { if (inTauri) invoke('app_ready'); }, HOLD_MS);
    return;
  }
  requestAnimationFrame(tick);
}
requestAnimationFrame(tick);
