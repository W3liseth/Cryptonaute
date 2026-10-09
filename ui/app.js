'use strict';

/* ============================================================
   Cryptonaute — logique de l'interface
   ============================================================ */

const TAURI = window.__TAURI__;
const invoke = TAURI ? TAURI.core.invoke : demoInvoke;

/* Plateforme : adapte la barre de titre (macOS) et les raccourcis (⌘ au lieu de Ctrl). */
const PLATFORM = /Mac/i.test(navigator.platform || navigator.userAgent) ? "macos"
  : /Linux/i.test(navigator.userAgent) ? "linux" : "windows";
document.documentElement.dataset.platform = PLATFORM;
if (PLATFORM === 'macos') {
  document.querySelectorAll('[title*="Ctrl+"]').forEach((e) => { e.title = e.title.replace('Ctrl+', '⌘'); });
}

const $ = (sel, root = document) => root.querySelector(sel);
const el = (tag, cls, html) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (html != null) e.innerHTML = html;
  return e;
};
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

const ICONS = {
  check: '<svg viewBox="0 0 24 24"><path d="M5 12l5 5L20 7"/></svg>',
  x: '<svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18"/></svg>',
  info: '<svg viewBox="0 0 24 24"><path d="M12 8h.01M11 12h1v5h1"/></svg>',
  copy: '<svg viewBox="0 0 24 24"><rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V5a2 2 0 012-2h8"/></svg>',
  globe: '<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.7 3.8 5.7 3.8 9s-1.3 6.3-3.8 9c-2.5-2.7-3.8-5.7-3.8-9S9.5 5.7 12 3z"/></svg>',
  split: '<svg viewBox="0 0 24 24"><path d="M6 3v6a3 3 0 003 3h6a3 3 0 013 3v6M6 21v-6"/></svg>',
  server: '<svg viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="7" rx="2"/><rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 7.5h.01M7 16.5h.01"/></svg>',
  warn: '<svg viewBox="0 0 24 24"><path d="M12 9v4M12 17h.01M10.3 3.9L1.8 18a2 2 0 001.7 3h17a2 2 0 001.7-3L13.7 3.9a2 2 0 00-3.4 0z"/></svg>',
};

/* ---------- État ---------- */
const state = {
  tunnels: [],
  selected: null,
  filter: '',
  service: { available: null, error: null },
  status: null,          // ServiceStatus renvoyé par le service : { tunnels: [...] }
  pending: null,         // { kind: 'connect'|'disconnect', name }
  lastError: null,       // { name, message }
  traffic: new Map(),    // nom → { since, prev: { t, rx, tx }, samples: [{ t, rx, tx }] } (octets/s)
};

/* ---------- Formatage ---------- */
const nf1 = new Intl.NumberFormat('fr-FR', { maximumFractionDigits: 1 });
function fmtBytes(n) {
  const u = ['o', 'Ko', 'Mo', 'Go', 'To'];
  let i = 0;
  while (n >= 1000 && i < u.length - 1) { n /= 1000; i++; }
  return `${i === 0 ? Math.round(n) : nf1.format(n)} ${u[i]}`;
}
const fmtRate = (n) => `${fmtBytes(n)}/s`;
function fmtDuration(sec) {
  sec = Math.max(0, Math.floor(sec));
  const h = String(Math.floor(sec / 3600)).padStart(2, '0');
  const m = String(Math.floor((sec % 3600) / 60)).padStart(2, '0');
  const s = String(sec % 60).padStart(2, '0');
  return `${h}:${m}:${s}`;
}
function fmtAgo(sec) {
  if (sec < 5) return "à l'instant";
  if (sec < 60) return `il y a ${sec} s`;
  if (sec < 3600) return `il y a ${Math.floor(sec / 60)} min`;
  return `il y a ${Math.floor(sec / 3600)} h`;
}
const now = () => Date.now() / 1000;
const hue = (name) => [...name].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7);

/* ---------- Notifications ---------- */
function toast(message, kind = 'info', ms = 4200) {
  const t = el('div', `toast ${kind}`);
  const icon = kind === 'success' ? ICONS.check : kind === 'error' ? ICONS.x : ICONS.info;
  t.innerHTML = `<span class="ti">${icon}</span><span>${esc(message)}</span>`;
  $('#toasts').appendChild(t);
  setTimeout(() => {
    t.classList.add('out');
    t.addEventListener('animationend', () => t.remove(), { once: true });
  }, ms);
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    toast('Copié dans le presse-papiers', 'success', 1800);
  } catch {
    toast('Impossible de copier', 'error');
  }
}

/* ---------- Données ---------- */
async function loadTunnels(selectName) {
  try {
    state.tunnels = await invoke('list_tunnels');
  } catch (e) {
    toast(String(e), 'error');
    state.tunnels = [];
  }
  if (selectName !== undefined) state.selected = selectName;
  if (!state.tunnels.some((t) => t.name === state.selected)) {
    state.selected = state.tunnels.find((t) => isActive(t.name))?.name ?? state.tunnels[0]?.name ?? null;
  }
  renderList();
  renderDetail();
}

const current = () => state.tunnels.find((t) => t.name === state.selected) ?? null;
/* Tunnels actifs (plusieurs possibles), dans l'ordre de leur activation. */
const activeList = () => state.status?.tunnels ?? [];
const statusOf = (name) => activeList().find((s) => s.name === name) ?? null;
const isActive = (name) => !!statusOf(name);
const quoteList = (names) => names.map((n) => `« ${n} »`).join(', ');

function uiState() {
  const t = current();
  if (t && state.pending?.name === t.name) return state.pending.kind === 'connect' ? 'connecting' : 'disconnecting';
  if (t && isActive(t.name)) return 'connected';
  if (t && state.lastError?.name === t.name) return 'error';
  return 'disconnected';
}

/* ---------- Liste ---------- */
let listSignature = null;

/* Reconstruit la liste (avec animation d'entrée) uniquement si son contenu change ;
   sinon met simplement à jour l'état de chaque élément. */
function renderList() {
  const list = $('#tunnel-list');
  const q = state.filter.toLowerCase();
  const items = state.tunnels.filter((t) => t.name.toLowerCase().includes(q));
  $('#tunnel-count').textContent = state.tunnels.length;

  const signature = items.map((t) => `${t.name}:${t.error ? 1 : 0}`).join('|') || `empty:${state.tunnels.length}`;
  if (signature !== listSignature) {
    listSignature = signature;
    list.innerHTML = '';
    if (!items.length) {
      list.appendChild(el('li', 'list-empty', state.tunnels.length ? 'Aucun résultat' : 'Aucun tunnel pour le moment'));
      return;
    }
    items.forEach((t, i) => {
      const li = el('li', 'tunnel-item');
      li.style.animationDelay = `${i * 35}ms`;
      li.setAttribute('role', 'option');
      li.dataset.name = t.name;
      li.innerHTML = `
        <div class="t-avatar" style="--h:${hue(t.name)}">${esc(t.name[0])}</div>
        <div class="t-text">
          <div class="t-name">${esc(t.name)}</div>
          <div class="t-sub"></div>
        </div>
        <span class="status-dot"></span>`;
      li.addEventListener('click', () => select(t.name));
      li.addEventListener('dblclick', () => { select(t.name); togglePower(); });
      list.appendChild(li);
    });
  }

  for (const li of list.querySelectorAll('.tunnel-item')) {
    const t = state.tunnels.find((x) => x.name === li.dataset.name);
    if (!t) continue;
    const active = isActive(t.name);
    const busy = state.pending?.name === t.name;
    li.classList.toggle('selected', t.name === state.selected);
    li.setAttribute('aria-selected', t.name === state.selected);
    li.classList.toggle('active', active);
    li.classList.toggle('busy', busy);
    li.classList.toggle('invalid', !!t.error);
    li.querySelector('.t-sub').textContent = t.error
      ? 'Configuration invalide'
      : busy
        ? (state.pending.kind === 'connect' ? 'Connexion…' : 'Déconnexion…')
        : active
          ? `Connecté · ${fmtRate(lastRate('rx', t.name))} ↓`
          : t.summary.peers[0]?.endpoint ?? '—';
  }
}

function select(name) {
  if (state.selected === name) return;
  state.selected = name;
  renderList();
  renderDetail(true);
}

/* ---------- Détail ---------- */
function renderDetail(animate = false) {
  const t = current();
  $('#empty-view').hidden = state.tunnels.length > 0;
  $('#tunnel-view').hidden = !t;
  if (!t) { applyState(); return; }

  const view = $('#tunnel-view');
  if (animate) { view.style.animation = 'none'; void view.offsetWidth; view.style.animation = ''; }

  $('#t-name').textContent = t.name;
  const chips = $('#t-chips');
  chips.innerHTML = '';
  if (t.error) {
    chips.innerHTML = `<span class="chip warn">${ICONS.warn}${esc(t.error)}</span>`;
  } else {
    const s = t.summary;
    chips.innerHTML = [
      s.fullTunnel
        ? `<span class="chip accent">${ICONS.globe}Tout le trafic</span>`
        : `<span class="chip">${ICONS.split}Tunnel partiel</span>`,
      `<span class="chip">${ICONS.server}${s.peers.length} pair${s.peers.length > 1 ? 's' : ''}</span>`,
      s.warnings.length ? `<span class="chip warn" title="${esc(s.warnings.join('\n'))}">${ICONS.warn}${s.warnings.length} avertissement${s.warnings.length > 1 ? 's' : ''}</span>` : '',
    ].join('');

    $('#q-endpoint').textContent = s.peers[0]?.endpoint ?? '—';
    $('#q-address').textContent = s.addresses.join(', ');
    $('#s-mode').textContent = s.fullTunnel ? 'Complet' : 'Partiel';
    $('#s-mode-sub').textContent = s.fullTunnel
      ? 'Tout le trafic passe par le VPN'
      : `${s.peers.reduce((n, p) => n + p.allowedIps.length, 0)} réseau(x) routé(s)`;

    const row = (k, v) => `<dt>${k}</dt><dd>${v}</dd>`;
    const tags = (arr) => arr.length ? arr.map((x) => `<span class="tag">${esc(x)}</span>`).join('') : '<span class="tag">—</span>';
    const copyBtn = (v) => `<button class="copy" data-copy="${esc(v)}" title="Copier">${ICONS.copy}</button>`;
    $('#d-interface').innerHTML =
      row('Clé publique', `<span class="tag">${esc(s.publicKey)}</span>${copyBtn(s.publicKey)}`) +
      row('Adresses', tags(s.addresses)) +
      row('DNS', tags(s.dns)) +
      row('MTU', `<span class="tag">${s.mtu ?? 'auto (1420)'}</span>`) +
      (s.listenPort ? row('Port d’écoute', `<span class="tag">${s.listenPort}</span>`) : '');
    $('#d-peers').innerHTML = s.peers.map((p) => `
      <div class="peer"><dl>
        ${row('Clé publique', `<span class="tag">${esc(p.publicKey)}</span>${copyBtn(p.publicKey)}`)}
        ${row('Endpoint', `<span class="tag">${esc(p.endpoint ?? '—')}</span>`)}
        ${row('IP autorisées', tags(p.allowedIps))}
        ${row('Keepalive', `<span class="tag">${p.persistentKeepalive ? p.persistentKeepalive + ' s' : 'désactivé'}</span>`)}
        ${p.hasPresharedKey ? row('Clé pré-partagée', '<span class="tag">activée</span>') : ''}
      </dl></div>`).join('');
  }
  applyState();
}

/* Met à jour tout ce qui dépend de l'état de connexion (appelé chaque seconde). */
function applyState() {
  const ui = uiState();
  document.body.dataset.state = state.tunnels.length ? ui : 'disconnected';
  const t = current();
  if (!t) return;

  const st = statusOf(t.name);
  const active = !!st;
  const others = activeList().filter((s) => s.name !== t.name);
  const otherFull = others.find((s) => s.fullTunnel);
  const othersSplit = others.filter((s) => !s.fullTunnel).map((s) => s.name);
  const label = $('#state-label');
  const sub = $('#state-sub');
  const power = $('#power');
  sub.classList.remove('warn');
  power.disabled = !!t.error || !!state.pending || state.service.available === false;

  switch (ui) {
    case 'connecting':
      label.textContent = 'Connexion…';
      sub.textContent = 'Création de l’interface et négociation avec le serveur';
      break;
    case 'disconnecting':
      label.textContent = 'Déconnexion…';
      sub.textContent = 'Fermeture du tunnel';
      break;
    case 'connected': {
      label.textContent = 'Connecté';
      const hs = st.lastHandshake;
      const since = now() - st.connectedSince;
      if (!hs && since > 8) {
        sub.textContent = 'En attente de la poignée de main… vérifiez l’endpoint et les clés';
        sub.classList.add('warn');
      } else if (hs && now() - hs > 180) {
        sub.textContent = 'Le serveur ne répond plus depuis quelques minutes';
        sub.classList.add('warn');
      } else if (!t.summary.fullTunnel && otherFull) {
        sub.textContent = `Ses réseaux passent par ce tunnel, le reste du trafic par « ${otherFull.name} »`;
      } else if (t.summary.fullTunnel && othersSplit.length) {
        sub.textContent = `Tout le trafic passe par ce tunnel, sauf les réseaux de ${quoteList(othersSplit)}`;
      } else {
        sub.textContent = `Trafic chiffré via ${st.endpoint ?? 'le serveur'}`;
      }
      break;
    }
    case 'error':
      label.textContent = 'Échec de connexion';
      sub.textContent = state.lastError.message;
      break;
    default:
      label.textContent = 'Déconnecté';
      if (state.service.available === false) sub.textContent = 'Le service Cryptonaute est requis pour se connecter';
      else if (t.error || !others.length) sub.textContent = 'Cliquez sur le bouton pour vous connecter';
      else if (t.summary.fullTunnel && otherFull) sub.textContent = `Remplacera « ${otherFull.name} » : un seul tunnel complet à la fois`;
      else if (t.summary.fullTunnel) sub.textContent = `S’ajoutera à ${quoteList(othersSplit)}, qui gardera ses réseaux`;
      else if (otherFull) sub.textContent = `S’ajoutera à « ${otherFull.name} » : ses réseaux passeront par ce tunnel`;
      else sub.textContent = `S’ajoutera à ${quoteList(others.map((s) => s.name))}`;
  }
  power.title = active ? 'Se déconnecter' : 'Se connecter';

  $('#timer').textContent = active && st.connectedSince ? fmtDuration(now() - st.connectedSince) : '00:00:00';
  $('#q-handshake').textContent = active
    ? (st.lastHandshake ? fmtAgo(Math.floor(now() - st.lastHandshake)) : 'en attente…')
    : '—';
  if (active && st.endpoint) $('#q-endpoint').textContent = st.endpoint;

  setValue('#s-rx', fmtBytes(active ? st.rxBytes : 0));
  setValue('#s-tx', fmtBytes(active ? st.txBytes : 0));
  $('#s-rx-rate').textContent = fmtRate(active ? lastRate('rx', t.name) : 0);
  $('#s-tx-rate').textContent = fmtRate(active ? lastRate('tx', t.name) : 0);
}

function setValue(sel, text) {
  const e = $(sel);
  if (e.textContent !== text) {
    e.textContent = text;
    e.classList.remove('pulse'); void e.offsetWidth; e.classList.add('pulse');
  }
}

const samplesOf = (name) => state.traffic.get(name)?.samples ?? [];
const lastRate = (k, name) => samplesOf(name).at(-1)?.[k] ?? 0;

/* ---------- Service & sondage ---------- */
function renderService() {
  const pill = $('#service-pill');
  const ok = state.service.available;
  pill.classList.toggle('ok', ok === true);
  pill.classList.toggle('down', ok === false);
  $('#service-label').textContent = ok === null
    ? 'Connexion au service…'
    : ok ? `Service actif${state.service.version ? ' · v' + state.service.version : ''}` : 'Service indisponible';
  pill.title = state.service.error ?? '';
  $('#service-banner').hidden = ok !== false;
  if (ok === false) $('#service-banner-text').textContent = state.service.error ?? '';
}

async function poll() {
  let view;
  try { view = await invoke('service_status'); } catch (e) { view = { available: false, error: String(e) }; }
  const wasAvailable = state.service.available;
  state.service.available = view.available;
  state.service.error = view.error ?? null;
  if (view.available && !state.service.version) {
    invoke('service_info').then((i) => { state.service.version = i.version; renderService(); }).catch(() => {});
  }
  if (wasAvailable !== view.available) renderService();

  const prevActive = activeList().map((s) => s.name);
  if (!state.pending) state.status = view.status ?? null;
  sampleTraffic();

  if (!state.pending) {
    for (const name of prevActive.filter((n) => !isActive(n))) toast(`Tunnel « ${name} » déconnecté`, 'info');
  }
  renderList();
  applyState();
}

/* Débits instantanés de chaque tunnel actif, à partir des compteurs cumulés. */
function sampleTraffic() {
  const t = now();
  for (const st of activeList()) {
    let tr = state.traffic.get(st.name);
    if (!tr || tr.since !== st.connectedSince) {
      tr = { since: st.connectedSince, prev: null, samples: [] };
      state.traffic.set(st.name, tr);
    }
    const p = tr.prev;
    const dt = p ? t - p.t : 0;
    tr.samples.push(dt > 0
      ? { t, rx: Math.max(0, (st.rxBytes - p.rx) / dt), tx: Math.max(0, (st.txBytes - p.tx) / dt) }
      : { t, rx: 0, tx: 0 });
    tr.prev = { t, rx: st.rxBytes, tx: st.txBytes };
    tr.samples = tr.samples.filter((s) => t - s.t < 75);
  }
  for (const name of [...state.traffic.keys()]) {
    if (!isActive(name)) state.traffic.delete(name);
  }
}

/* ---------- Connexion ---------- */
async function togglePower() {
  const t = current();
  if (!t || t.error || state.pending) return;
  const disconnecting = isActive(t.name);
  const before = activeList().map((s) => s.name);
  state.pending = { kind: disconnecting ? 'disconnect' : 'connect', name: t.name };
  state.lastError = null;
  renderList(); applyState();
  try {
    state.status = await invoke(disconnecting ? 'disconnect' : 'connect', { name: t.name });
    state.traffic.delete(t.name);
    sampleTraffic();
    toast(disconnecting ? `Déconnecté de « ${t.name} »` : `Connecté à « ${t.name} »`, 'success');
    // Un tunnel complet en remplace un autre : on le signale.
    const replaced = before.filter((n) => n !== t.name && !isActive(n));
    if (replaced.length) toast(`${quoteList(replaced)} déconnecté : un seul tunnel complet à la fois`, 'info', 5500);
  } catch (e) {
    if (!disconnecting) state.lastError = { name: t.name, message: String(e) };
    toast(String(e), 'error', 6500);
  } finally {
    state.pending = null;
    renderList(); applyState();
  }
}

/* ---------- Modales ---------- */
function openModal(id) {
  const m = $(id);
  m.hidden = false;
  m.classList.remove('closing');
}
function closeModal(id) {
  const m = $(id);
  if (m.hidden) return;
  m.classList.add('closing');
  setTimeout(() => { m.hidden = true; m.classList.remove('closing'); }, 190);
}
document.querySelectorAll('.modal-backdrop').forEach((b) => {
  b.addEventListener('mousedown', (e) => { if (e.target === b) closeModal('#' + b.id); });
  b.querySelectorAll('[data-close]').forEach((btn) => btn.addEventListener('click', () => closeModal('#' + b.id)));
});

/* ---------- Éditeur ---------- */
const editor = { previous: null, timer: null };
const edName = $('#ed-name');
const edConfig = $('#ed-config');
const edHl = $('#ed-hl');

function highlight(text) {
  return text.split('\n').map((line) => {
    const m = line.match(/^(\s*)([^#;]*?)(\s*[#;].*)?$/);
    const [, indent, body, comment = ''] = m;
    let html = esc(indent);
    if (/^\[.*\]\s*$/.test(body)) {
      html += `<span class="hl-section">${esc(body)}</span>`;
    } else if (body.includes('=')) {
      const i = body.indexOf('=');
      const key = body.slice(0, i);
      const val = body.slice(i + 1);
      const ignored = /^\s*(pre|post)(up|down)\s*$/i.test(key);
      const valCls = /^\s*[\d.,\s]+\s*$/.test(val) ? 'hl-num' : 'hl-val';
      html += ignored
        ? `<span class="hl-danger">${esc(body)}</span>`
        : `<span class="hl-key">${esc(key)}</span><span class="hl-eq">=</span><span class="${valCls}">${esc(val)}</span>`;
    } else {
      html += esc(body);
    }
    if (comment) html += `<span class="hl-comment">${esc(comment)}</span>`;
    return html;
  }).join('\n') + '\n';
}

function syncEditor() {
  edHl.innerHTML = highlight(edConfig.value);
  edHl.scrollTop = edConfig.scrollTop;
  edHl.scrollLeft = edConfig.scrollLeft;
}
edConfig.addEventListener('input', () => { syncEditor(); scheduleValidate(); });
edConfig.addEventListener('scroll', () => { edHl.scrollTop = edConfig.scrollTop; edHl.scrollLeft = edConfig.scrollLeft; });
edConfig.addEventListener('keydown', (e) => {
  if (e.key === 'Tab') {
    e.preventDefault();
    document.execCommand('insertText', false, '    ');
  }
});
edName.addEventListener('input', scheduleValidate);

function scheduleValidate() {
  clearTimeout(editor.timer);
  editor.timer = setTimeout(validateEditor, 180);
}

async function validateEditor() {
  const box = $('#ed-validation');
  let ok = true;
  const name = edName.value.trim();
  try {
    await invoke('validate_name', { name });
    if (!editor.previous || editor.previous.toLowerCase() !== name.toLowerCase()) {
      if (state.tunnels.some((t) => t.name.toLowerCase() === name.toLowerCase())) throw 'un tunnel porte déjà ce nom';
    }
    $('#ed-name-err').textContent = '';
    edName.classList.remove('invalid');
  } catch (e) {
    ok = false;
    $('#ed-name-err').textContent = name ? String(e) : '';
    edName.classList.toggle('invalid', !!name);
  }
  if (!edConfig.value.trim()) {
    box.className = 'validation';
    box.innerHTML = '<span class="v-meta">Collez ou chargez une configuration WireGuard.</span>';
    $('#ed-save').disabled = true;
    return;
  }
  try {
    const s = await invoke('validate_config', { config: edConfig.value });
    box.className = 'validation ok';
    box.innerHTML = `<div class="v-title">${ICONS.check}Configuration valide</div>
      <div class="v-meta">${esc(s.addresses.join(', '))} → ${esc(s.peers.map((p) => p.endpoint).join(', '))} · ${s.fullTunnel ? 'tout le trafic' : 'tunnel partiel'}</div>
      ${s.warnings.length ? `<ul>${s.warnings.map((w) => `<li>${esc(w)}</li>`).join('')}</ul>` : ''}`;
  } catch (e) {
    ok = false;
    box.className = 'validation err';
    box.innerHTML = `<div class="v-title">${ICONS.x}${esc(e)}</div>`;
  }
  $('#ed-save').disabled = !ok;
}

function openEditor({ title, name = '', config = '', previous = null }) {
  editor.previous = previous;
  $('#editor-title').textContent = title;
  edName.value = name;
  edConfig.value = config;
  $('#ed-save').textContent = previous ? 'Enregistrer' : 'Ajouter le tunnel';
  syncEditor();
  validateEditor();
  openModal('#editor');
  setTimeout(() => (name ? edConfig : edName).focus(), 60);
}

async function newTunnel() {
  const config = await invoke('new_template');
  openEditor({ title: 'Nouveau tunnel', name: uniqueName('wg'), config });
}

async function editTunnel() {
  const t = current();
  if (!t) return;
  try {
    const config = await invoke('get_tunnel_config', { name: t.name });
    openEditor({ title: `Modifier « ${t.name} »`, name: t.name, config, previous: t.name });
  } catch (e) { toast(String(e), 'error'); }
}

async function saveEditor() {
  const name = edName.value.trim();
  const btn = $('#ed-save');
  btn.disabled = true;
  try {
    const wasActive = editor.previous && isActive(editor.previous);
    await invoke('save_tunnel', { name, config: edConfig.value, previous: editor.previous });
    closeModal('#editor');
    toast(editor.previous ? `« ${name} » mis à jour` : `Tunnel « ${name} » ajouté`, 'success');
    if (wasActive) toast('Reconnectez le tunnel pour appliquer les modifications', 'info', 5500);
    await loadTunnels(name);
  } catch (e) {
    toast(String(e), 'error');
    btn.disabled = false;
  }
}

$('#ed-keys').addEventListener('click', async () => {
  const tpl = await invoke('new_template');
  const key = tpl.match(/PrivateKey = (.+)/)[1];
  const pub = tpl.match(/Clé publique : (.+)/)[1];
  let text = edConfig.value;
  if (/^\s*PrivateKey\s*=.*$/im.test(text)) {
    text = text.replace(/^(\s*PrivateKey\s*=).*$/im, `$1 ${key}`);
    text = text.replace(/^#\s*Clé publique :.*$/im, `# Clé publique : ${pub}`);
  } else {
    text = tpl;
  }
  edConfig.value = text;
  syncEditor(); validateEditor();
  toast('Nouvelle paire de clés générée — communiquez la clé publique à votre serveur', 'info', 5000);
});

/* ---------- Import de fichiers ---------- */
function nameFromFile(fileName, reserved) {
  const base = fileName.replace(/\.(conf|txt)$/i, '').replace(/[^A-Za-z0-9_=+.-]/g, '-').replace(/^[.-]+|[.]+$/g, '').slice(0, 32);
  return uniqueName(base || 'tunnel', reserved);
}
function uniqueName(base, reserved = []) {
  const taken = new Set([...state.tunnels.map((t) => t.name), ...reserved].map((n) => n.toLowerCase()));
  if (!taken.has(base.toLowerCase())) return base;
  for (let i = 2; ; i++) {
    const n = `${base.slice(0, 29)}-${i}`;
    if (!taken.has(n.toLowerCase())) return n;
  }
}

async function importFiles(files) {
  files = [...files].filter((f) => f.size < 64 * 1024);
  if (!files.length) return;
  if (files.length === 1) {
    const f = files[0];
    openEditor({ title: 'Importer une configuration', name: nameFromFile(f.name), config: await f.text() });
    return;
  }
  const added = [];
  for (const f of files) {
    const name = nameFromFile(f.name, added);
    try {
      await invoke('save_tunnel', { name, config: await f.text(), previous: null });
      added.push(name);
    } catch (e) {
      toast(`${f.name} : ${e}`, 'error', 6000);
    }
  }
  const n = added.length;
  if (n) toast(`${n} tunnel${n > 1 ? 's' : ''} importé${n > 1 ? 's' : ''}`, 'success');
  await loadTunnels(added[n - 1] ?? state.selected);
}

const fileInput = $('#file-input');
fileInput.addEventListener('change', () => { importFiles(fileInput.files); fileInput.value = ''; });
$('#ed-file').addEventListener('click', () => {
  const pick = el('input');
  pick.type = 'file'; pick.accept = '.conf,.txt';
  pick.addEventListener('change', async () => {
    const f = pick.files[0];
    if (!f) return;
    edConfig.value = await f.text();
    if (!edName.value.trim() || !editor.previous) edName.value = nameFromFile(f.name);
    syncEditor(); validateEditor();
  });
  pick.click();
});

let dragDepth = 0;
window.addEventListener('dragenter', (e) => {
  if (![...(e.dataTransfer?.types ?? [])].includes('Files')) return;
  e.preventDefault();
  if (dragDepth++ === 0) $('#drop-overlay').hidden = false;
});
window.addEventListener('dragover', (e) => e.preventDefault());
window.addEventListener('dragleave', () => { if (--dragDepth <= 0) { dragDepth = 0; $('#drop-overlay').hidden = true; } });
window.addEventListener('drop', (e) => {
  e.preventDefault();
  dragDepth = 0;
  $('#drop-overlay').hidden = true;
  if (e.dataTransfer?.files?.length) importFiles(e.dataTransfer.files);
});

/* ---------- Suppression ---------- */
function askDelete() {
  const t = current();
  if (!t) return;
  $('#confirm-text').innerHTML = `Le tunnel <strong>${esc(t.name)}</strong> et sa clé privée seront définitivement supprimés de ce poste.`
    + (isActive(t.name) ? '<br>Il sera d’abord déconnecté.' : '');
  openModal('#confirm');
  $('#confirm-ok').focus();
}
$('#confirm-ok').addEventListener('click', async () => {
  const t = current();
  closeModal('#confirm');
  if (!t) return;
  try {
    if (isActive(t.name)) {
      state.status = await invoke('disconnect', { name: t.name });
    }
    await invoke('delete_tunnel', { name: t.name });
    toast(`« ${t.name} » supprimé`, 'success');
    state.selected = null;
    await loadTunnels();
  } catch (e) { toast(String(e), 'error'); }
});

/* ---------- Graphique temps réel ---------- */
const chart = { canvas: $('#chart'), max: 1024 };
function drawChart() {
  const c = chart.canvas;
  if (c.offsetParent !== null) {
    const dpr = window.devicePixelRatio || 1;
    const w = c.clientWidth, h = c.clientHeight;
    if (c.width !== Math.round(w * dpr) || c.height !== Math.round(h * dpr)) {
      c.width = Math.round(w * dpr); c.height = Math.round(h * dpr);
    }
    const ctx = c.getContext('2d');
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);

    const WINDOW = 60;
    const t = now() - 1; // léger retard pour un défilement fluide entre deux mesures
    const pts = samplesOf(state.selected);
    const peak = Math.max(1024, ...pts.map((p) => Math.max(p.rx, p.tx)));
    chart.max += (peak * 1.25 - chart.max) * 0.08; // mise à l'échelle amortie
    const pad = { t: 10, b: 20, l: 4, r: 56 };
    const iw = w - pad.l - pad.r, ih = h - pad.t - pad.b;
    const x = (ts) => pad.l + iw - ((t - ts) / WINDOW) * iw;
    const y = (v) => pad.t + ih - (v / chart.max) * ih;

    // Grille et échelle
    ctx.font = '11px "Segoe UI Variable Text", "Segoe UI", sans-serif';
    ctx.textBaseline = 'middle';
    for (let i = 0; i <= 3; i++) {
      const v = (chart.max / 3) * i;
      const yy = Math.round(y(v)) + 0.5;
      ctx.strokeStyle = 'rgba(255,255,255,0.06)';
      ctx.setLineDash(i === 0 ? [] : [3, 5]);
      ctx.beginPath(); ctx.moveTo(pad.l, yy); ctx.lineTo(pad.l + iw, yy); ctx.stroke();
      ctx.fillStyle = 'rgba(184,188,217,0.55)';
      ctx.fillText(fmtRate(v), pad.l + iw + 8, yy);
    }
    ctx.setLineDash([]);
    ctx.fillStyle = 'rgba(127,132,166,0.6)';
    ctx.fillText('-60 s', pad.l, h - 8);
    ctx.textAlign = 'right';
    ctx.fillText('maintenant', pad.l + iw, h - 8);
    ctx.textAlign = 'left';

    const series = [
      { key: 'tx', color: [167, 139, 250] },
      { key: 'rx', color: [34, 211, 238] },
    ];
    const visible = pts.filter((p) => t - p.t <= WINDOW + 2);
    ctx.save();
    ctx.beginPath(); ctx.rect(pad.l, 0, iw, h); ctx.clip();
    for (const s of series) {
      if (visible.length < 2) break;
      const [r, g, b] = s.color;
      const path = new Path2D();
      visible.forEach((p, i) => {
        const px = x(p.t), py = y(p[s.key]);
        if (i === 0) { path.moveTo(px, py); return; }
        const prev = visible[i - 1];
        const cx = (x(prev.t) + px) / 2;
        path.bezierCurveTo(cx, y(prev[s.key]), cx, py, px, py);
      });
      const area = new Path2D(path);
      area.lineTo(x(visible[visible.length - 1].t), pad.t + ih);
      area.lineTo(x(visible[0].t), pad.t + ih);
      area.closePath();
      const grad = ctx.createLinearGradient(0, pad.t, 0, pad.t + ih);
      grad.addColorStop(0, `rgba(${r},${g},${b},0.35)`);
      grad.addColorStop(1, `rgba(${r},${g},${b},0)`);
      ctx.fillStyle = grad;
      ctx.fill(area);
      ctx.shadowColor = `rgba(${r},${g},${b},0.8)`;
      ctx.shadowBlur = 10;
      ctx.strokeStyle = `rgb(${r},${g},${b})`;
      ctx.lineWidth = 2;
      ctx.stroke(path);
      ctx.shadowBlur = 0;
      const last = visible[visible.length - 1];
      ctx.fillStyle = `rgb(${r},${g},${b})`;
      ctx.beginPath(); ctx.arc(x(last.t), y(last[s.key]), 3.5, 0, Math.PI * 2); ctx.fill();
    }
    ctx.restore();
  }
  requestAnimationFrame(drawChart);
}

/* ---------- Fenêtre & raccourcis ---------- */
if (TAURI) {
  const win = TAURI.window.getCurrentWindow();
  $('#win-min').addEventListener('click', () => win.minimize());
  $('#win-max').addEventListener('click', () => win.toggleMaximize());
  $('#win-close').addEventListener('click', () => win.close()); // masque dans la zone de notification
  // Résultat des actions lancées depuis l'icône de la zone de notification
  TAURI.event.listen('tray-action', ({ payload }) => {
    toast(payload.message, payload.ok ? 'success' : 'error', payload.ok ? 4200 : 6500);
    poll();
  });
} else {
  $('.win-controls').style.visibility = 'hidden';
}

$('#search').addEventListener('input', (e) => { state.filter = e.target.value; renderList(); });
$('#btn-new').addEventListener('click', newTunnel);
$('#btn-import').addEventListener('click', () => fileInput.click());
document.querySelector('[data-action="import"]').addEventListener('click', () => fileInput.click());
document.querySelector('[data-action="new"]').addEventListener('click', newTunnel);
$('#btn-edit').addEventListener('click', editTunnel);
$('#btn-delete').addEventListener('click', askDelete);
$('#power').addEventListener('click', togglePower);
$('#ed-save').addEventListener('click', saveEditor);
document.addEventListener('click', (e) => {
  const b = e.target.closest('[data-copy]');
  if (b) copyText(b.dataset.copy);
});
document.addEventListener('keydown', (e) => {
  const modalOpen = !$('#editor').hidden || !$('#confirm').hidden || !$('#options').hidden;
  if (e.key === 'Escape') { closeModal('#editor'); closeModal('#confirm'); closeModal('#options'); return; }
  if ((e.ctrlKey || e.metaKey) && e.key === ',' && !modalOpen) { e.preventDefault(); openOptions(); return; }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's' && !$('#editor').hidden) { e.preventDefault(); if (!$('#ed-save').disabled) saveEditor(); return; }
  if (modalOpen || e.target.matches('input, textarea')) return;
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n') { e.preventDefault(); newTunnel(); }
  else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'o') { e.preventDefault(); fileInput.click(); }
  else if (e.key === 'Delete') askDelete();
  else if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); togglePower(); }
  else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault();
    const i = state.tunnels.findIndex((t) => t.name === state.selected);
    const next = state.tunnels[Math.min(state.tunnels.length - 1, Math.max(0, i + (e.key === 'ArrowDown' ? 1 : -1)))];
    if (next) select(next.name);
  }
});
if (TAURI) document.addEventListener('contextmenu', (e) => { if (!e.target.matches('input, textarea')) e.preventDefault(); });

/* ---------- Options ---------- */
const optTray = $('#opt-tray');
const optBoot = $('#opt-boot');

function renderOptions(o) {
  optTray.checked = o.showTray;
  optBoot.checked = o.autostart;
  $('#opt-boot-desc').textContent = o.showTray
    ? 'Démarre Cryptonaute discrètement dans la zone de notification à l’ouverture de votre session.'
    : 'Ouvre la fenêtre de Cryptonaute à l’ouverture de votre session.';
  $('#win-close').title = o.showTray ? 'Fermer (Cryptonaute reste dans la zone de notification)' : 'Quitter';
}

async function openOptions() {
  try { renderOptions(await invoke('get_options')); } catch (e) { toast(String(e), 'error'); return; }
  const appVersion = TAURI ? await TAURI.app.getVersion().catch(() => null) : '0.4.0 (démo)';
  $('#about').textContent = `Cryptonaute ${appVersion ? 'v' + appVersion : ''}`;
  openModal('#options');
}

async function changeOption(box, key, [onMsg, offMsg]) {
  const wanted = box.checked;
  box.disabled = true;
  try {
    renderOptions(await invoke('set_options', { [key]: wanted }));
    toast(wanted ? onMsg : offMsg, 'success');
  } catch (e) {
    box.checked = !wanted;
    toast(String(e), 'error');
  } finally {
    box.disabled = false;
  }
}

optTray.addEventListener('change', () => changeOption(optTray, 'showTray', [
  'Icône affichée dans la zone de notification',
  'Icône retirée : fermer la fenêtre quittera Cryptonaute',
]));
optBoot.addEventListener('change', () => changeOption(optBoot, 'autostart', [
  'Cryptonaute démarrera à l’ouverture de votre session',
  'Lancement au démarrage désactivé',
]));
$('#btn-options').addEventListener('click', openOptions);
if (TAURI) TAURI.event.listen('open-options', openOptions);

/* ---------- Démarrage ---------- */
(async function init() {
  renderService();
  invoke('get_options').then(renderOptions).catch(() => {});
  await poll();
  await loadTunnels();
  setInterval(poll, 1000);
  requestAnimationFrame(drawChart);
})();

/* ============================================================
   Mode démo : utilisé uniquement hors de Tauri (aperçu navigateur)
   ============================================================ */
function demoInvoke(cmd, args = {}) {
  const D = (window.__demo ??= {
    configs: {
      'bureau-paris': '[Interface]\nPrivateKey = yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=\nAddress = 10.8.0.2/32\nDNS = 10.8.0.1\n\n[Peer]\nPublicKey = xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=\nEndpoint = vpn.paris.example:51820\nAllowedIPs = 0.0.0.0/0, ::/0\nPersistentKeepalive = 25\n',
      'homelab': '[Interface]\nPrivateKey = yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=\nAddress = 192.168.50.7/24\n\n[Peer]\nPublicKey = xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=\nEndpoint = 82.64.12.9:51820\nAllowedIPs = 192.168.1.0/24, 192.168.50.0/24\n',
    },
    active: [], // { name, full, since, rx, tx }
  });
  const summary = (text) => {
    const get = (k) => [...text.matchAll(new RegExp(`^\\s*${k}\\s*=\\s*(.+)$`, 'gim'))].flatMap((m) => m[1].split(',').map((s) => s.trim()));
    if (!/\[Interface\]/i.test(text)) throw 'section [Interface] manquante';
    if (!get('PrivateKey').length) throw 'PrivateKey manquante dans [Interface]';
    if (!get('Endpoint').length) throw 'Endpoint manquant dans [Peer] (requis pour un client)';
    const allowed = get('AllowedIPs');
    return {
      publicKey: 'HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=', addresses: get('Address'), dns: get('DNS'),
      listenPort: null, mtu: null, fullTunnel: allowed.some((a) => a.endsWith('/0')), warnings: [],
      peers: [{ publicKey: get('PublicKey')[0] ?? '', endpoint: get('Endpoint')[0], allowedIps: allowed, persistentKeepalive: +get('PersistentKeepalive')[0] || null, hasPresharedKey: false }],
    };
  };
  const status = () => ({
    tunnels: D.active.map((a) => {
      a.rx += Math.random() * 900000 * (1 + Math.sin(Date.now() / 4000)); a.tx += Math.random() * 160000;
      return { name: a.name, fullTunnel: a.full, connectedSince: a.since, rxBytes: Math.round(a.rx), txBytes: Math.round(a.tx), lastHandshake: now() - 12, endpoint: '203.0.113.10:51820' };
    }),
  });
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  return (async () => {
    switch (cmd) {
      case 'list_tunnels': return Object.keys(D.configs).sort().map((name) => { try { return { name, summary: summary(D.configs[name]) }; } catch (e) { return { name, error: String(e) }; } });
      case 'get_tunnel_config': return D.configs[args.name];
      case 'validate_config': return summary(args.config);
      case 'validate_name': if (!/^[A-Za-z0-9_=+.-]{1,32}$/.test(args.name)) throw 'caractères autorisés : lettres, chiffres et _ = + . -'; return null;
      case 'new_template': return '[Interface]\n# Clé publique : HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=\nPrivateKey = yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=\nAddress = 10.0.0.2/32\nDNS = 1.1.1.1\n\n[Peer]\nPublicKey = \nAllowedIPs = 0.0.0.0/0, ::/0\nEndpoint = vpn.exemple.fr:51820\nPersistentKeepalive = 25\n';
      case 'save_tunnel': summary(args.config); if (args.previous && args.previous !== args.name) delete D.configs[args.previous]; D.configs[args.name] = args.config; return { name: args.name, summary: summary(args.config) };
      case 'delete_tunnel': delete D.configs[args.name]; return null;
      case 'service_status': return { available: true, status: status() };
      case 'service_info': return { available: true, version: '0.4.0 (démo)' };
      case 'connect': {
        await wait(1400);
        const full = summary(D.configs[args.name]).fullTunnel;
        D.active = D.active.filter((a) => a.name !== args.name && !(full && a.full));
        D.active.push({ name: args.name, full, since: now(), rx: 0, tx: 0 });
        return status();
      }
      case 'disconnect': await wait(600); D.active = D.active.filter((a) => args.name != null && a.name !== args.name); return status();
      case 'get_options': return { showTray: D.showTray ?? true, autostart: !!D.autostart };
      case 'set_options':
        await wait(150);
        if (args.showTray !== undefined) D.showTray = args.showTray;
        if (args.autostart !== undefined) D.autostart = args.autostart;
        return { showTray: D.showTray ?? true, autostart: !!D.autostart };
      default: throw `commande inconnue : ${cmd}`;
    }
  })();
}
