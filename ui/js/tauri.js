// Bridge to Tauri. In a plain browser (layout preview) the calls do nothing.
const T = window.__TAURI__;

export const inTauri = !!T;
export const appWindow = T ? T.window.getCurrentWindow() : null;

export function invoke(cmd, args) {
  return T ? T.core.invoke(cmd, args) : Promise.resolve(null);
}

export function listen(name, cb) {
  return T ? T.event.listen(name, cb) : Promise.resolve(() => {});
}

const VIDEO = ['mp4', 'm4v', 'mov', '3gp', '3g2', 'mkv', 'webm', 'avi', 'divx', 'xvid'];

const asList = (r) => (r ? (Array.isArray(r) ? r : [r]) : []);

export async function pickFiles() {
  if (!T) return [];
  return asList(await T.dialog.open({ multiple: true, title: 'Add videos', filters: [{ name: 'Video', extensions: VIDEO }] }));
}

export async function pickFolders() {
  if (!T) return [];
  return asList(await T.dialog.open({ directory: true, multiple: true, title: 'Add folders' }));
}

export async function pickOutputFolder() {
  if (!T) return null;
  const r = await T.dialog.open({ directory: true, multiple: false, title: 'Save converted files to' });
  return Array.isArray(r) ? r[0] || null : r;
}
