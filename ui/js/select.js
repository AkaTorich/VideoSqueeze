// Custom-shaped drop-down list: a face with a cut and an arrow, a menu with cut corners.
// Mouse and keyboard: Enter/Space — open or pick, arrows — move, Esc — close.

export function makeSelect(root, items, value, onChange) {
  root.classList.add('cselect');
  root.tabIndex = 0;
  root.setAttribute('role', 'combobox');
  root.setAttribute('aria-expanded', 'false');
  root.innerHTML = '<div class="cs-face"><span class="cs-label"></span></div><span class="cs-arrow"></span>';
  const label = root.querySelector('.cs-label');

  const menu = document.createElement('div');
  menu.className = 'cs-menu';
  menu.setAttribute('role', 'listbox');
  menu.innerHTML = '<div class="cs-shape"><div class="cs-list">' +
    items.map((it, i) => `<div class="cs-opt" role="option" data-i="${i}"><span class="cs-mark"></span><span>${it.label}</span></div>`).join('') +
    '</div></div>';
  document.body.appendChild(menu);
  const opts = [...menu.querySelectorAll('.cs-opt')];

  let cur = Math.max(0, items.findIndex((it) => it.value === value));
  let hot = cur;
  let open = false;

  function paint() {
    label.textContent = items[cur].short || items[cur].label;
    opts.forEach((o, i) => {
      o.classList.toggle('on', i === cur);
      o.classList.toggle('hot', i === hot);
      o.setAttribute('aria-selected', String(i === cur));
    });
  }

  function place() {
    const r = root.getBoundingClientRect();
    menu.style.minWidth = r.width + 'px';
    menu.style.left = r.left + 'px';
    menu.style.top = r.bottom + 4 + 'px';
    const mr = menu.getBoundingClientRect();
    if (mr.bottom > window.innerHeight - 14) menu.style.top = r.top - 4 - mr.height + 'px';
    if (mr.right > window.innerWidth - 14) menu.style.left = r.right - mr.width + 'px';
  }

  function setOpen(v) {
    open = v;
    menu.classList.toggle('open', v);
    root.classList.toggle('open', v);
    root.setAttribute('aria-expanded', String(v));
    if (v) {
      hot = cur;
      paint();
      place();
    }
  }

  function pick(i) {
    const changed = i !== cur;
    cur = i;
    paint();
    setOpen(false);
    if (changed) onChange(items[i].value);
  }

  root.addEventListener('mousedown', (e) => {
    e.preventDefault();
    root.focus();
    setOpen(!open);
  });
  menu.addEventListener('mousedown', (e) => e.preventDefault());
  menu.addEventListener('click', (e) => {
    const o = e.target.closest('.cs-opt');
    if (o) pick(+o.dataset.i);
  });
  menu.addEventListener('mousemove', (e) => {
    const o = e.target.closest('.cs-opt');
    if (o && +o.dataset.i !== hot) {
      hot = +o.dataset.i;
      paint();
    }
  });
  root.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      if (open) pick(hot);
      else setOpen(true);
    } else if (e.key === 'Escape') {
      setOpen(false);
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const d = e.key === 'ArrowDown' ? 1 : -1;
      if (open) {
        hot = (hot + d + items.length) % items.length;
        paint();
      } else {
        pick((cur + d + items.length) % items.length);
      }
    }
  });
  root.addEventListener('blur', () => setOpen(false));
  window.addEventListener('resize', () => setOpen(false));
  paint();

  return {
    get value() { return items[cur].value; },
    set value(v) {
      const i = items.findIndex((it) => it.value === v);
      if (i >= 0) { cur = i; paint(); }
    },
  };
}
