// Numbers for people: sizes, lengths, bitrates.

export function sizeParts(bytes) {
  if (!(bytes >= 0)) return ['—', ''];
  const units = [['GB', 1024 ** 3], ['MB', 1024 ** 2], ['KB', 1024]];
  for (const [u, k] of units) {
    if (bytes >= k) {
      const v = bytes / k;
      return [v >= 100 ? v.toFixed(0) : v >= 10 ? v.toFixed(1) : v.toFixed(2), u];
    }
  }
  return [String(Math.round(bytes)), 'B'];
}

export function size(bytes) {
  const [v, u] = sizeParts(bytes);
  return u ? `${v} ${u}` : v;
}

export function time(sec) {
  if (!(sec > 0)) return '—';
  const s = Math.round(sec);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  return h ? `${h}:${String(m).padStart(2, '0')}:${String(r).padStart(2, '0')}` : `${m}:${String(r).padStart(2, '0')}`;
}

export function bitrate(bps) {
  if (!(bps > 0)) return '—';
  if (bps >= 1e6) return (bps / 1e6).toFixed(bps >= 1e7 ? 0 : 1) + ' MBIT/S';
  return Math.round(bps / 1000) + ' KBIT/S';
}

export function fps(v) {
  if (!(v > 0)) return '—';
  return (Math.abs(v - Math.round(v)) < 0.01 ? Math.round(v) : v.toFixed(2)) + ' FPS';
}

export function percentChange(from, to) {
  if (!(from > 0) || !(to >= 0)) return null;
  return Math.round((to / from - 1) * 100);
}

export function esc(s) {
  return String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
}
