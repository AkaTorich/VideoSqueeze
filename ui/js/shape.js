// Window shape: a polygon with cut corners and a notch at the bottom — that is the window region (command
// set_window_shape): outside the outline there is no window. Inside, everything is painted opaque (page and
// frame background); WebView2 transparency is not needed, so no white can appear with dialogs or minimizing.
// The neon edge is drawn inside the outline. Shared by the main window and the splash; ready — region applied.
import { invoke, appWindow, inTauri } from './tauri.js';

const DEFAULTS = { cutBig: 28, cutSmall: 12, notch: 7, notchAt: 0.3, notchLen: 240 };
// how far the edge and accents sit inside the outline (half a line width)
const EDGE_INSET = 1;
const ACCENT_INSET = 2.5;

// Outline vertices of the rectangle (x0, y0)–(x1, y1); the bottom notch between n1 and n2.
function outline(x0, y0, x1, y1, big, small, n1, n2, depth) {
  return [
    [x0 + big, y0], [x1 - small, y0], [x1, y0 + small],
    [x1, y1 - big], [x1 - big, y1],
    [n2 + depth, y1], [n2, y1 - depth], [n1, y1 - depth], [n1 - depth, y1],
    [x0 + small, y1], [x0, y1 - small], [x0, y0 + big],
  ];
}

const svgPoints = (pts) => pts.map(([x, y]) => `${x},${y}`).join(' ');

export function initShape(options = {}) {
  const o = { ...DEFAULTS, ...options };
  const edge = document.getElementById('edge');
  const polys = edge.querySelectorAll('polygon');
  const accP = document.getElementById('accP');
  const accC = document.getElementById('accC');
  const accY = document.getElementById('accY');
  let maximized = false;
  let pending = false;
  let lastRegion = '';
  let resolveReady;
  const ready = new Promise((r) => { resolveReady = r; });

  async function update() {
    pending = false;
    if (inTauri) {
      try { maximized = await appWindow.isMaximized(); } catch { /* the window is closing */ }
    }
    document.body.classList.toggle('maximized', maximized);
    const W = window.innerWidth;
    const H = window.innerHeight;
    const n1 = Math.round(W * o.notchAt);
    const n2 = n1 + Math.round(Math.min(o.notchLen, W * 0.2));
    const big = o.cutBig;
    const small = o.cutSmall;
    // notch edges (with its slopes) — the status bar is laid out around it
    const rootStyle = document.documentElement.style;
    rootStyle.setProperty('--notch-l', n1 - o.notch + 'px');
    rootStyle.setProperty('--notch-r', n2 + o.notch + 'px');
    // outline moved in by i: sides shift by i, a 45° cut gets shorter by i·(2−√2)
    const inner = (i) => outline(i, i, W - i, H - i, big - 0.586 * i, small - 0.586 * i, n1, n2, o.notch);

    edge.setAttribute('viewBox', `0 0 ${W} ${H}`);
    const e = inner(EDGE_INSET);
    polys.forEach((p) => p.setAttribute('points', svgPoints(e)));

    // accents at the cuts and on the notch
    const a = ACCENT_INSET;
    const ab = big - 0.586 * a;
    if (accP) accP.setAttribute('points', svgPoints([[a, a + ab + 46], [a, a + ab], [a + ab, a], [a + ab + 46, a]]));
    if (accC) accC.setAttribute('points', svgPoints([[W - a, H - a - ab - 46], [W - a, H - a - ab], [W - a - ab, H - a], [W - a - ab - 46, H - a]]));
    if (accY) accY.setAttribute('points', svgPoints([[n1 - o.notch, H - a], [n1, H - a - o.notch], [n2, H - a - o.notch], [n2 + o.notch, H - a]]));

    const region = outline(0, 0, W, H, big, small, n1, n2, o.notch);
    const key = W + 'x' + H + (maximized ? 'M' : '');
    if (!inTauri) {
      resolveReady();
    } else if (key !== lastRegion) {
      lastRegion = key;
      invoke('set_window_shape', { points: region }).catch(() => {}).finally(resolveReady);
    }
  }

  function schedule() {
    if (!pending) {
      pending = true;
      requestAnimationFrame(update);
    }
  }

  window.addEventListener('resize', schedule);
  update();

  // resize handles in the glow strip (main window only)
  document.querySelectorAll('.rz').forEach((h) => {
    h.addEventListener('mousedown', (e) => {
      if (e.button === 0 && inTauri) appWindow.startResizeDragging(h.dataset.dir);
    });
  });
  const grip = document.getElementById('grip');
  if (grip) {
    grip.addEventListener('mousedown', (e) => {
      if (e.button === 0 && inTauri && !maximized) appWindow.startResizeDragging('SouthEast');
    });
  }

  return { refresh: schedule, ready };
}
