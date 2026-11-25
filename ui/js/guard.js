// The window is a program, not a page: no browser context menu and none of its keys
// (reload, print, find, view source, zoom, back/forward, caret browsing F7).

const BLOCKED_CTRL = new Set(['r', 'p', 'f', 'g', 'u', 's', 'j', 'h', 'o', 'n', 't', 'w', 'd', 'e', 'l', '+', '-', '=', '0']);
const BLOCKED_KEYS = new Set(['F1', 'F3', 'F5', 'F7', 'F11', 'F12', 'BrowserBack', 'BrowserForward', 'BrowserRefresh', 'BrowserSearch']);

export function initGuard() {
  document.addEventListener('contextmenu', (e) => e.preventDefault(), true);

  document.addEventListener('keydown', (e) => {
    const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    const editing = e.target instanceof HTMLInputElement;
    if (BLOCKED_KEYS.has(e.key)) {
      e.preventDefault();
      return;
    }
    // text fields keep the usual Ctrl+A/C/V/X/Z
    if ((e.ctrlKey || e.metaKey) && BLOCKED_CTRL.has(k) && !(editing && 'acvxz'.includes(k))) {
      e.preventDefault();
      return;
    }
    if (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight' || e.key === 'Home')) e.preventDefault();
  }, true);

  // zoom with Ctrl+wheel and pinch
  window.addEventListener('wheel', (e) => { if (e.ctrlKey) e.preventDefault(); }, { passive: false, capture: true });
  // mouse back/forward buttons
  window.addEventListener('mouseup', (e) => { if (e.button === 3 || e.button === 4) e.preventDefault(); }, true);
  // dragging of images and selected text by the page itself
  document.addEventListener('dragstart', (e) => e.preventDefault());
  document.addEventListener('selectstart', (e) => {
    if (!(e.target instanceof HTMLInputElement)) e.preventDefault();
  });
}
