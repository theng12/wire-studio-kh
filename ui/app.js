'use strict';

(() => {
  const APP_VERSION = '0.3.0';
  const WHATS_NEW = [
    'Two more places to post: a YouTube channel and a Facebook Page.',
    'Each place has named format sizes — YouTube landscape and Shorts, Facebook square, portrait and link card, Telegram text — with size, ratio, text limit and safe area shown before you send.',
    'One post can go to several places and several sizes; every pair is its own line in the queue.',
    'Instagram is designed and shelved: nothing is built and it never appears here.',
    'Refusals stay honest: a wrong size is refused by name, a sign-in problem goes to Needs attention instead of retrying forever.',
    'The app icon is your pick — the dispatch line.'
  ];
  const SUPPORT_URL = 'https://nowpayments.io/donation/studiokh';
  const TEST_ROUTES = {
    state: ['GET', '/api/test/state'],
    queue_post: ['POST', '/api/test/trigger/schedule'],
    cancel_delivery: ['POST', '/api/test/trigger/cancel'],
    retry_delivery: ['POST', '/api/test/trigger/retry'],
    save_telegram: ['POST', '/api/test/trigger/save-telegram']
  };
  const SECTIONS = {
    composer: { eyebrow: 'Compose', title: 'Composer', subtitle: 'Write a text post, choose where it goes, and send it now or later.' },
    queue: { eyebrow: 'Dispatch', title: 'Queue', subtitle: 'One row per target. Retry or cancel a delivery and see the reason.' },
    targets: { eyebrow: 'Connections', title: 'Targets', subtitle: 'Telegram, YouTube and Facebook Page — state comes from the core.' },
    import: { eyebrow: 'Still Posts', title: 'Import Still Posts', subtitle: 'Copies posts and images from a Story Studio KH channel into Wire. Story is never changed.' },
    settings: { eyebrow: 'Preferences', title: 'Settings', subtitle: 'App paths, connections, updates, appearance and about.' }
  };
  const STATE_LEVEL = {
    queued: 'info', sending: 'live', sent: 'ok', 'failed-retry': 'warn',
    dead: 'bad', 'needs attention': 'warn', cancelled: 'off'
  };
  const TIME_FMT = new Intl.DateTimeFormat('en-GB', {
    timeZone: 'Asia/Phnom_Penh', day: '2-digit', month: 'short',
    hour: '2-digit', minute: '2-digit', hour12: false
  });

  const $ = (selector, root = document) => root.querySelector(selector);
  const $$ = (selector, root = document) => Array.from(root.querySelectorAll(selector));
  const NS = 'http://www.w3.org/2000/svg';

  let state = emptyState();
  let connected = false;
  let coreError = null;
  let activeSection = 'composer';
  let queueLayout = 'table';
  let dialogSeq = 0;
  const dialogStack = [];
  const presetSelections = {};
  let importPreview = null;
  let importBusy = null;

  const tauriBridge = window.__TAURI__ || null;
  const tauriInvoke = tauriBridge && (tauriBridge.core && tauriBridge.core.invoke || tauriBridge.invoke);
  const testMode = !tauriBridge && location.port !== '';

  function emptyState() {
    return {
      dataDir: '', targets: [], deliveries: [], presets: [],
      telegram: { configured: false, connection: 'not_configured', chatId: '', last4: '', reason: '' }
    };
  }

  function storageGet(key, fallback) {
    try { const value = localStorage.getItem(key); return value === null ? fallback : value; }
    catch (_) { return fallback; }
  }

  function storageSet(key, value) {
    try { localStorage.setItem(key, value); } catch (_) { /* private mode: use the default */ }
  }

  function el(tag, props = {}, ...children) {
    const node = document.createElement(tag);
    for (const [key, value] of Object.entries(props)) {
      if (value === null || value === undefined || value === false) continue;
      if (key === 'text') node.textContent = String(value);
      else if (key === 'class') node.className = value;
      else if (key === 'on') for (const [event, handler] of Object.entries(value)) node.addEventListener(event, handler);
      else if (key.startsWith('aria-') || key.startsWith('data-') || ['id', 'type', 'value', 'href', 'placeholder', 'title', 'role', 'for', 'rel', 'target', 'autocomplete'].includes(key)) node.setAttribute(key, String(value));
      else if (key in node) node[key] = value;
      else node.setAttribute(key, String(value));
    }
    for (const child of children.flat()) {
      if (child === null || child === undefined) continue;
      node.append(child.nodeType ? child : document.createTextNode(String(child)));
    }
    return node;
  }

  function icon(name) {
    const svg = document.createElementNS(NS, 'svg');
    svg.setAttribute('class', 'icon');
    svg.setAttribute('aria-hidden', 'true');
    const use = document.createElementNS(NS, 'use');
    use.setAttribute('href', '#i-' + name);
    svg.append(use);
    return svg;
  }

  function message(err) { return err && err.message ? err.message : String(err || 'Unknown error'); }

  function pickField(source, ...keys) {
    for (const key of keys) if (source && source[key] !== undefined && source[key] !== null) return source[key];
    return null;
  }

  function countOf(value) {
    if (Array.isArray(value)) return value.length;
    if (typeof value === 'number' && Number.isFinite(value)) return value;
    if (typeof value === 'string' && value.trim() && Number.isFinite(Number(value))) return Number(value);
    return null;
  }

  function textOf(value) {
    return value === null || value === undefined || value === '' ? 'Not reported' : String(value);
  }

  function bytesText(value) {
    if (value === null || value === undefined) return 'Not reported';
    if (typeof value === 'number' && Number.isFinite(value)) return new Intl.NumberFormat().format(value) + ' bytes';
    return String(value);
  }

  function readImportData(raw) {
    let data = raw;
    if (typeof data === 'string') {
      try { data = JSON.parse(data); } catch (_) { throw new Error('The core did not return a readable report.'); }
    }
    if (!data || typeof data !== 'object' || Array.isArray(data)) throw new Error('The core did not return a readable report.');
    return [data.report, data.summary, data].reduce(
      (merged, part) => part && typeof part === 'object' && !Array.isArray(part) ? Object.assign(merged, part) : merged, {});
  }

  function normalizeSkipped(value) {
    if (!Array.isArray(value)) return [];
    return value.map(item => typeof item === 'string'
      ? { label: item, reason: 'Reason not reported' }
      : {
        label: textOf(pickField(item, 'path', 'file', 'name', 'id', 'item')),
        reason: textOf(pickField(item, 'reason', 'detail', 'error', 'message'))
      });
  }

  function normalizePreview(raw) {
    const data = readImportData(raw);
    const postsRaw = pickField(data, 'posts', 'items');
    const skippedRaw = pickField(data, 'skipped', 'skips');
    const posts = Array.isArray(postsRaw) ? postsRaw.map(post => {
      const row = post && typeof post === 'object' ? post : { id: post };
      return {
        id: textOf(pickField(row, 'id', 'post_id', 'postId')),
        caption: String(pickField(row, 'caption', 'text') || ''),
        aspects: textOf(pickField(row, 'aspects', 'aspect')),
        finals: countOf(pickField(row, 'finals', 'finals_count', 'finalCount'))
      };
    }) : [];
    return {
      channel: textOf(pickField(data, 'channel_name', 'channelName', 'channel', 'name')),
      posts,
      postsCount: countOf(pickField(data, 'post_count', 'postCount', 'posts_count')) ?? (Array.isArray(postsRaw) ? posts.length : null),
      fontsCount: countOf(pickField(data, 'fonts', 'font_count', 'fontCount')),
      skipped: normalizeSkipped(skippedRaw),
      skippedCount: countOf(pickField(data, 'skipped_count', 'skippedCount')) ?? (Array.isArray(skippedRaw) ? skippedRaw.length : null),
      filesRead: countOf(pickField(data, 'files_read', 'filesRead', 'read_files')),
      imageCount: countOf(pickField(data, 'images', 'image_count', 'imageCount', 'image_files', 'imageFiles', 'files_to_copy', 'filesToCopy'))
    };
  }

  function boolOf(value) {
    if (typeof value === 'boolean') return value;
    const text = value === null || value === undefined ? '' : String(value).toLowerCase().trim();
    if (['yes', 'true', '1', 'unchanged'].includes(text)) return true;
    if (['no', 'false', '0', 'changed'].includes(text)) return false;
    return null;
  }

  function normalizeImportReport(raw) {
    const data = readImportData(raw);
    return {
      imported: countOf(pickField(data, 'imported', 'imported_posts', 'posts_imported', 'postsImported')) ?? (Array.isArray(data.posts) ? data.posts.length : null),
      filesCopied: countOf(pickField(data, 'files_copied', 'copied_files', 'filesCopied')),
      bytes: pickField(data, 'bytes', 'bytes_copied', 'bytesCopied', 'total_bytes'),
      alreadyPresent: countOf(pickField(data, 'already_present', 'alreadyPresent', 'existing')),
      skipped: normalizeSkipped(pickField(data, 'skipped', 'skips')),
      unchanged: boolOf(pickField(data, 'story_files_unchanged', 'storyFilesUnchanged', 'unchanged', 'source_unchanged', 'sourceUnchanged'))
    };
  }

  /* ---------- Core contract ---------- */

  async function callCore(name, args = {}) {
    if (tauriBridge) {
      if (typeof tauriInvoke !== 'function') throw new Error('Tauri bridge has no invoke method.');
      return await tauriInvoke(name, args);
    }
    if (testMode) return await fetchTest(name, args);
    throw new Error('No core bridge and no test port are available.');
  }

  async function fetchTest(name, args) {
    const route = TEST_ROUTES[name];
    if (!route) throw new Error('No test route for ' + name + '.');
    const [method, path] = route;
    const headers = {
      'content-type': 'application/json',
      'x-wire-operator': String(window.WIRE_TEST_TOKEN || '')
    };
    const response = await fetch(path, {
      method, headers,
      body: method === 'POST' ? JSON.stringify(args) : undefined
    });
    if (!response.ok) {
      let detail = '';
      try { const body = await response.json(); detail = body && (body.error || body.message) || ''; } catch (_) { /* no JSON body */ }
      throw new Error('Core answered ' + response.status + (detail ? ': ' + detail : '') + '.');
    }
    if (response.status === 204) return {};
    try { return await response.json(); } catch (_) { return {}; }
  }

  function unwrap(raw) {
    let value = raw;
    if (typeof value === 'string') { try { value = JSON.parse(value); } catch (_) { return {}; } }
    if (!value || typeof value !== 'object') return {};
    return value.state && typeof value.state === 'object' ? value.state : value;
  }

  function normalizeConnection(value) {
    const text = String(value || '').toLowerCase().replace(/[\s-]+/g, '_');
    if (['ready', 'ok', 'connected', 'configured', 'live'].includes(text)) return 'ready';
    if (['failed', 'error', 'bad', 'off', 'unsupported'].includes(text)) return 'failed';
    return 'not_configured';
  }

  function lastFour(value) {
    const match = String(value || '').match(/([a-z0-9]{4})$/i);
    return match ? match[1] : '';
  }

  function normalizeTarget(target, index) {
    if (typeof target === 'string') {
      return { id: target, label: target, platform: target.split(':')[0].toLowerCase(), connection: 'not_configured', reason: '', enabled: true };
    }
    const platform = String(target.platform || target.kind || target.id || '').toLowerCase();
    return {
      id: String(target.id || target.name || index + 1),
      label: String(target.label || target.name || target.channel_title || target.chat_id || platform || 'Target ' + (index + 1)),
      platform,
      connection: normalizeConnection(target.connection_state || target.connectionState || target.state),
      reason: String(target.reason || target.error || target.last_error || ''),
      enabled: target.enabled !== false
    };
  }

  function normalizeState(raw) {
    const source = unwrap(raw);
    const targetsRaw = Array.isArray(source.targets) ? source.targets : Array.isArray(source.channels) ? source.channels : [];
    const telegramRaw = source.telegram && typeof source.telegram === 'object'
      ? source.telegram
      : targetsRaw.find(t => /telegram/i.test(typeof t === 'string' ? t : String(t.platform || t.id || ''))) || {};
    const configured = Boolean(telegramRaw.configured || telegramRaw.has_token || telegramRaw.token_set);
    const deliveriesRaw = Array.isArray(source.deliveries) ? source.deliveries
      : Array.isArray(source.queue) ? source.queue
      : Array.isArray(source.rows) ? source.rows : [];
    // presets only from the core; a missing list means "No presets reported", never invented.
    const presets = Array.isArray(source.presets) ? source.presets
      .filter(item => item && typeof item === 'object')
      .map(item => ({
        id: String(pickField(item, 'id', 'preset_id', 'presetId') || ''),
        platform: String(pickField(item, 'platform') || '').toLowerCase(),
        label: String(pickField(item, 'label') || ''),
        ratio: String(pickField(item, 'ratio') || ''),
        maxTextLength: countOf(pickField(item, 'max_text_length', 'maxTextLength', 'max_length'))
      }))
      .filter(preset => preset.id && preset.platform) : [];
    return {
      dataDir: String(source.data_dir || source.dataDir || ''),
      appearance: String(source.appearance || ''),
      targets: targetsRaw.map(normalizeTarget),
      presets,
      telegram: {
        configured,
        connection: normalizeConnection(telegramRaw.connection_state || telegramRaw.connectionState || telegramRaw.state),
        chatId: String(telegramRaw.chat || telegramRaw.chat_id || telegramRaw.chatId || ''),
        last4: lastFour(telegramRaw.token_masked || telegramRaw.token_last4 || telegramRaw.tokenLast4 || telegramRaw.masked_token || telegramRaw.maskedToken || telegramRaw.mask),
        reason: String(telegramRaw.reason || telegramRaw.error || (configured ? 'Token and channel saved; this build has not reported a verified connection.' : ''))
      },
      deliveries: deliveriesRaw.map(normalizeDelivery)
    };
  }

  function parseWhen(value) {
    if (value === null || value === undefined || value === '') return null;
    const date = typeof value === 'number'
      ? new Date(value < 1e12 ? value * 1000 : value)
      : new Date(value);
    return Number.isFinite(date.getTime()) ? date : null;
  }

  function shownState(status, attempts, attention, error, nextAt) {
    const text = status.toLowerCase().replace(/[\s-]+/g, '_');
    if (text === 'cancelled' || /cancelled by user/i.test(error)) return 'cancelled';
    if (['sent', 'succeeded', 'delivered'].includes(text)) return 'sent';
    if (['sending', 'in_flight', 'inflight'].includes(text)) return 'sending';
    if (attention || text === 'needs_attention' || /auth_invalid|permission_denied/.test(error)) return 'needs attention';
    if (text === 'dead' || attempts >= 5) return 'dead';
    if (text === 'failed' || text === 'failed_retry') return 'failed-retry';
    if (text === 'pending' && nextAt && nextAt.getTime() <= Date.now()) return 'failed-retry';
    return 'queued';
  }

  function normalizeDelivery(row, index) {
    const status = String(row.status || row.state || '');
    const coreShown = String(row.shown_state || row.shownState || '').toLowerCase().replace(/[\s-]+/g, ' ');
    const attempts = Number(row.attempts || 0) || 0;
    const attention = Boolean(row.attention || row.needs_attention);
    const error = String(row.last_error || row.error || '');
    const nextAt = parseWhen(row.next_attempt_at || row.nextAttemptAt || row.next_attempt);
    const known = ['queued', 'sending', 'sent', 'failed-retry', 'dead', 'needs attention', 'cancelled'];
    const shown = coreShown === 'attention' ? 'needs attention'
      : known.includes(coreShown) ? coreShown
      : shownState(status, attempts, attention, error, nextAt);
    // shortcut: the 0.1.0 core state has no post text, only a post id; upgrade when state carries text.
    const text = String(row.text || row.post_text || row.body || (row.post_id ? 'Post ' + row.post_id : 'No post text reported.'));
    return {
      id: String(row.id || index + 1), text,
      target: String(row.target || row.target_label || row.target_name || row.channel || 'Target not reported'),
      preset: String(row.preset || row.preset_id || row.presetId || ''),
      shown, nextAt, error, attempts
    };
  }

  async function refreshState() {
    setStatus(connected ? 'Refreshing…' : 'Checking core…', 'info');
    try {
      state = normalizeState(await callCore('state'));
      coreError = null;
      connected = true;
      setStatus('Core connected', 'ok');
    } catch (err) {
      coreError = message(err);
      connected = false;
      setStatus('Offline — nothing is queued', 'bad');
    }
    render();
  }

  /* ---------- Rendering ---------- */

  function setStatus(text, level) {
    const node = $('#statusLine');
    node.textContent = text;
    node.dataset.level = level;
  }

  function showComposerError(text) {
    const node = $('#composerError');
    node.textContent = text;
    node.hidden = false;
  }

  function clearComposerError() { $('#composerError').hidden = true; $('#composerError').textContent = ''; }

  function updateCount() {
    $('#charCount').textContent = $('#postText').value.length + ' characters';
  }

  function showSection(name) {
    activeSection = name;
    $$('.nav-item').forEach(button => {
      const active = button.dataset.section === name;
      button.classList.toggle('is-active', active);
      if (active) button.setAttribute('aria-current', 'page'); else button.removeAttribute('aria-current');
    });
    Object.keys(SECTIONS).forEach(section => { $('#' + section + 'View').hidden = section !== name; });
    const meta = SECTIONS[name];
    $('#pageEyebrow').textContent = meta.eyebrow;
    $('#pageTitle').textContent = meta.title;
    $('#pageSubtitle').textContent = meta.subtitle;
    if (name === 'queue') renderQueue();
  }

  function render() {
    renderTargetChecks();
    renderPresetPickers();
    renderQueue();
    renderTargets();
    renderSettings();
    $('#offlineBanner').hidden = connected;
    $('#offlineDetail').textContent = coreError || 'No core bridge or test API answered.';
    $('#queueBtn').disabled = !connected || state.targets.length === 0;
    $('#queueBtn').title = !connected ? 'Core is offline — nothing can be queued.' : '';
    updateCount();
  }

  function renderTargetChecks() {
    const host = $('#targetChecks');
    host.replaceChildren();
    if (!state.targets.length) {
      host.append(el('p', { class: 'help', text: 'No targets reported by the core. Add a Telegram token on the Targets page first.' }));
      return;
    }
    state.targets.forEach((target, index) => {
      const inputId = 'target-' + index;
      host.append(el('label', { class: 'check', for: inputId },
        el('input', { type: 'checkbox', name: 'target', id: inputId, value: target.id, disabled: !connected || !target.enabled }),
        el('span', { text: target.label }),
        el('span', { class: 'chip', 'data-level': target.connection === 'ready' ? 'ok' : target.connection === 'failed' ? 'bad' : 'off', text: target.connection.replace('_', ' ') })
      ));
    });
  }

  function presetsFor(platform) {
    return state.presets.filter(preset => preset.platform === String(platform || '').toLowerCase());
  }

  function presetHelp(preset) {
    if (!preset) return 'No presets reported by the core.';
    const parts = [];
    if (preset.ratio) parts.push('Ratio ' + preset.ratio);
    if (preset.maxTextLength) parts.push('Max ' + Number(preset.maxTextLength).toLocaleString() + ' characters');
    return parts.length ? parts.join(' · ') : 'No format details reported by the core.';
  }

  function renderPresetPickers() {
    const field = $('#presetField');
    const host = $('#presetPickers');
    host.replaceChildren();
    const selected = $$('input[name="target"]:checked')
      .map(input => state.targets.find(target => target.id === input.value))
      .filter(Boolean);
    field.hidden = selected.length === 0;
    if (!selected.length) return;
    selected.forEach((target, index) => {
      const list = presetsFor(target.platform);
      const selectId = 'preset-' + index;
      const select = el('select', {
        id: selectId, 'aria-label': 'Preset for ' + target.label,
        disabled: !connected || !list.length
      });
      if (!list.length) select.append(el('option', { value: '', text: 'No presets reported' }));
      else list.forEach(preset => select.append(el('option', { value: preset.id, text: preset.label || preset.id })));
      const saved = presetSelections[target.id];
      select.value = list.some(preset => preset.id === saved) ? saved : (list[0] ? list[0].id : '');
      presetSelections[target.id] = select.value;
      const help = el('p', { class: 'help', text: presetHelp(list.find(preset => preset.id === select.value)) });
      select.addEventListener('change', () => {
        presetSelections[target.id] = select.value;
        help.textContent = presetHelp(list.find(preset => preset.id === select.value));
      });
      host.append(el('div', { class: 'field preset-row' },
        el('label', { for: selectId, text: target.label }), select, help));
    });
  }

  function selectedPresets(targets) {
    const map = {};
    targets.forEach(id => {
      const target = state.targets.find(item => item.id === id);
      if (!target) return;
      const list = presetsFor(target.platform);
      if (!list.length) return;
      const saved = presetSelections[id];
      map[id] = list.some(preset => preset.id === saved) ? saved : list[0].id;
    });
    return map;
  }

  function presetLabel(id) {
    if (!id) return '';
    const preset = state.presets.find(item => item.id === id);
    return preset && preset.label ? preset.label : id;
  }

  function showToast(text) {
    const node = el('div', { class: 'toast', text });
    $('#toasts').append(node);
    setTimeout(() => node.remove(), 6000);
  }

  function stateBlock(kind, title, detail, actionLabel, onAction) {
    // An empty state is where the app introduces itself: the owner's mark, not a generic icon.
    const lead = kind === 'empty'
      ? el('img', { class: 'state-mark', src: 'logo.svg', alt: '', width: '44', height: '44' })
      : icon(kind === 'error' ? 'cancel' : 'update');
    const block = el('div', { class: 'state-block' },
      lead,
      el('h3', { text: title }), el('p', { text: detail })
    );
    if (actionLabel) block.append(el('button', { class: 'button subtle', type: 'button', text: actionLabel, on: { click: onAction } }));
    return block;
  }

  function excerpt(text) {
    return text.length > 120 ? text.slice(0, 117) + '…' : text;
  }

  function formatWhen(date) {
    return date ? TIME_FMT.format(date) + ' UTC+7' : 'No next attempt reported';
  }

  function filteredRows() {
    const query = $('#queueSearch').value.trim().toLowerCase();
    if (!query) return state.deliveries;
    return state.deliveries.filter(row =>
      [row.text, row.target, row.shown, row.error].some(value => String(value).toLowerCase().includes(query)));
  }

  function renderQueue() {
    const host = $('#queueContent');
    host.replaceChildren();
    if (!connected) {
      host.append(stateBlock('error', 'Queue could not be read', coreError || 'The core did not answer.', 'Retry', refreshState));
      return;
    }
    const rows = filteredRows();
    if (!rows.length) {
      host.append(stateBlock('empty',
        state.deliveries.length ? 'No matching deliveries' : 'Queue is empty',
        state.deliveries.length ? 'Try another search term.' : 'Queued posts appear here, one row per target.',
        state.deliveries.length ? 'Clear search' : null,
        () => { $('#queueSearch').value = ''; renderQueue(); }
      ));
      return;
    }
    host.append(queueLayout === 'cards' ? renderCards(rows) : renderTable(rows));
  }

  function actionButtons(row) {
    const retryable = !['sent', 'sending', 'queued'].includes(row.shown);
    const cancellable = ['queued', 'sending', 'failed-retry'].includes(row.shown);
    const retry = el('button', {
      class: 'mini-button', type: 'button', disabled: !retryable,
      title: retryable ? 'Retry this delivery' : 'Retry is not available for this state',
      'aria-label': 'Retry delivery ' + row.id,
      on: { click: () => retryDelivery(row) }
    }, icon('retry'), 'Retry');
    const cancel = el('button', {
      class: 'mini-button', type: 'button', disabled: !cancellable,
      title: cancellable ? 'Cancel this delivery' : 'Cancel is not available for this state',
      'aria-label': 'Cancel delivery ' + row.id,
      on: { click: () => confirmCancel(row) }
    }, icon('cancel'), 'Cancel');
    return el('div', { class: 'row-actions' }, retry, cancel);
  }

  function stateChip(row) {
    return el('span', { class: 'chip', 'data-level': STATE_LEVEL[row.shown] || 'info', text: row.shown });
  }

  function renderTable(rows) {
    const body = el('tbody');
    rows.forEach(row => {
      const postCell = el('td', {}, el('div', { class: 'row-title', text: excerpt(row.text) }));
      if (row.error) postCell.append(el('div', { class: 'cell-note', text: row.error }));
      const targetCell = el('td', {}, el('span', { text: row.target }));
      if (row.preset) targetCell.append(el('div', { class: 'cell-note', text: presetLabel(row.preset) }));
      body.append(el('tr', {},
        postCell,
        targetCell,
        el('td', {}, stateChip(row), el('div', { class: 'cell-note', text: 'attempt ' + (row.attempts || 0) })),
        el('td', { class: 'mono', text: row.shown === 'sent' || row.shown === 'cancelled' ? '—' : formatWhen(row.nextAt) }),
        el('td', {}, actionButtons(row))
      ));
    });
    const table = el('table', {},
      el('thead', {}, el('tr', {},
        el('th', { text: 'Post' }), el('th', { text: 'Target' }), el('th', { text: 'State' }),
        el('th', { text: 'Next attempt (UTC+7)' }), el('th', { text: 'Actions' })
      )), body);
    return el('div', { class: 'table-wrap' }, table);
  }

  function renderCards(rows) {
    const list = el('div', { class: 'card-list' });
    rows.forEach(row => {
      const targetBox = el('div', {}, el('span', { text: 'Target' }), el('strong', { text: row.target }));
      if (row.preset) targetBox.append(el('div', { class: 'cell-note', text: presetLabel(row.preset) }));
      const meta = el('div', { class: 'queue-meta' },
        targetBox,
        el('div', {}, el('span', { text: 'Next attempt (UTC+7)' }), el('strong', { class: 'mono', text: row.shown === 'sent' || row.shown === 'cancelled' ? '—' : formatWhen(row.nextAt) })),
        el('div', {}, el('span', { text: 'Attempts' }), el('strong', { text: String(row.attempts || 0) }))
      );
      list.append(el('article', { class: 'queue-card' },
        el('header', {}, el('div', { class: 'row-title', text: excerpt(row.text) }), stateChip(row)),
        el('blockquote', { text: row.text }),
        row.error ? el('p', { class: 'cell-note', text: row.error }) : null,
        meta,
        el('div', { class: 'actions' }, actionButtons(row))
      ));
    });
    return list;
  }

  function renderTargets() {
    const telegram = state.telegram;
    const chip = $('#telegramChip');
    chip.textContent = telegram.connection.replace('_', ' ');
    chip.dataset.level = telegram.connection === 'ready' ? 'ok' : telegram.connection === 'failed' ? 'bad' : 'off';
    $('#telegramReason').textContent = telegram.reason ||
      (telegram.connection === 'ready' ? 'Connection verified by the core.'
        : telegram.connection === 'failed' ? 'Connection failed; no reason reported by the core.'
          : 'No verified connection reported. Add a token and channel id to connect.');
    $('#telegramToken').textContent = telegram.last4
      ? 'Saved token •••• ' + telegram.last4
      : telegram.configured ? 'Saved token (last four not reported)' : 'No token saved';
    $('#telegramManageBtn').textContent = telegram.configured ? 'Replace token' : 'Add token';
    const chatInput = $('#telegramChatId');
    if (document.activeElement !== chatInput) chatInput.value = telegram.chatId;
    $('#settingsTelegramHelp').textContent = 'Telegram is ' + telegram.connection.replace('_', ' ') + '. Manage the target here.';
    renderPlatformCard('youtube', '#youtubeChip', '#youtubeReason');
    renderPlatformCard('facebook', '#facebookChip', '#facebookReason');
  }

  function renderPlatformCard(platform, chipSelector, reasonSelector) {
    const target = state.targets.find(item => item.platform === platform);
    // never ready unless the core said so; a missing target is honestly not configured.
    const connection = target ? target.connection : 'not_configured';
    const chip = $(chipSelector);
    chip.textContent = connection.replace('_', ' ');
    chip.dataset.level = connection === 'ready' ? 'ok' : connection === 'failed' ? 'bad' : 'off';
    $(reasonSelector).textContent = target && target.reason
      ? target.reason
      : connection === 'ready' ? 'Connection verified by the core.'
        : connection === 'failed' ? 'Connection failed; no reason reported by the core.'
          : 'No verified connection reported by the core yet.';
  }

  function renderSettings() {
    $('#dataFolder').value = state.dataDir || 'Not reported by core';
    $$('[data-theme-choice]').forEach(button => {
      button.classList.toggle('is-active', button.dataset.themeChoice === storageGet('wire.appearance', 'system'));
    });
  }

  /* ---------- Still Posts import ---------- */

  function importChip(label, value, level = 'off') {
    return el('span', { class: 'chip', 'data-level': level, text: label + ': ' + textOf(value) });
  }

  function captionExcerpt(text) { return text.length > 80 ? text.slice(0, 79) + '…' : text; }

  function skippedBlock(items) {
    if (!items.length) return null;
    const list = el('ul', { class: 'dialog-list' });
    items.forEach(item => list.append(el('li', {},
      el('span', { class: 'row-title', text: item.label }), ' — ', item.reason)));
    return el('div', { class: 'import-skipped' }, el('h3', { text: 'Skipped' }), list);
  }

  function reportStat(label, value) {
    return el('div', {}, el('span', { text: label }), el('strong', { text: textOf(value) }));
  }

  function renderImportEmpty() {
    $('#importResultsTitle').textContent = 'Preview';
    $('#importRootHint').textContent = 'No folder checked';
    $('#importResults').replaceChildren(stateBlock('empty', 'Nothing to preview', 'Choose the channel folder to see what would come in.'));
  }

  function renderImportError(title, detail, retry) {
    $('#importResultsTitle').textContent = title;
    $('#importResults').replaceChildren(stateBlock('error', title, detail, 'Retry', retry));
  }

  function renderImportPreview(preview, root) {
    $('#importResultsTitle').textContent = 'Preview';
    $('#importRootHint').textContent = root;
    const parts = [el('div', { class: 'import-chips' },
      importChip('Channel', preview.channel, 'info'),
      importChip('Posts', textOf(preview.postsCount), 'live'),
      importChip('Fonts', textOf(preview.fontsCount)),
      importChip('Skipped', textOf(preview.skippedCount)),
      importChip('Files read', textOf(preview.filesRead))
    )];
    if (preview.posts.length) {
      const body = el('tbody');
      preview.posts.forEach(post => body.append(el('tr', {},
        el('td', { class: 'mono', text: post.id }),
        el('td', { class: 'excerpt', title: post.caption, text: captionExcerpt(post.caption) }),
        el('td', { text: post.aspects }),
        el('td', { text: textOf(post.finals) })
      )));
      parts.push(el('div', { class: 'table-wrap' }, el('table', {},
        el('thead', {}, el('tr', {}, el('th', { text: 'Post' }), el('th', { text: 'Caption' }),
          el('th', { text: 'Aspects' }), el('th', { text: 'Finals' }))), body)));
    } else {
      parts.push(stateBlock('empty', 'No posts found', 'The core reported no importable Still Posts in this folder.'));
    }
    const skipped = skippedBlock(preview.skipped);
    if (skipped) parts.push(skipped);
    $('#importResults').replaceChildren(...parts);
  }

  function renderImportReport(report, root) {
    $('#importResultsTitle').textContent = 'Import report';
    $('#importRootHint').textContent = root;
    const flag = report.unchanged === false ? 'no' : report.unchanged === true ? 'yes' : 'not reported';
    const level = report.unchanged === false ? 'bad' : report.unchanged === true ? 'ok' : 'warn';
    const parts = [
      el('div', { class: 'queue-meta' },
        reportStat('Imported', textOf(report.imported)),
        reportStat('Files copied', textOf(report.filesCopied)),
        reportStat('Bytes', bytesText(report.bytes)),
        reportStat('Already present', textOf(report.alreadyPresent))
      ),
      el('p', { class: 'import-flag', 'data-level': level, text: 'Story files unchanged: ' + flag })
    ];
    const skipped = skippedBlock(report.skipped);
    if (skipped) parts.push(skipped);
    $('#importResults').replaceChildren(...parts);
  }

  function syncImportButtons() {
    const busy = importBusy !== null;
    const check = $('#importCheckBtn');
    const run = $('#importRunBtn');
    check.disabled = busy;
    check.textContent = importBusy === 'check' ? 'Checking…' : 'Check folder';
    $('#importChooseBtn').disabled = busy;
    $('#importRoot').disabled = busy;
    run.disabled = busy || !importPreview;
    run.textContent = importBusy === 'run' ? 'Importing…'
      : importPreview && importPreview.postsCount !== null ? 'Import ' + importPreview.postsCount + ' posts' : 'Import posts';
  }

  function showImportError(text) { const node = $('#importError'); node.textContent = text; node.hidden = false; }
  function clearImportError() { const node = $('#importError'); node.textContent = ''; node.hidden = true; }

  function invalidateImportPreview() {
    importPreview = null;
    renderImportEmpty();
    syncImportButtons();
  }

  function parentOf(path) {
    const clean = String(path || '').replace(/\/+$/, '') || '/';
    if (clean === '/' || !clean.includes('/')) return null;
    return clean.slice(0, clean.lastIndexOf('/')) || '/';
  }

  function normalizeDirs(raw) {
    const list = Array.isArray(raw) ? raw : pickField(raw, 'dirs', 'directories', 'folders', 'entries', 'children');
    if (!Array.isArray(list)) throw new Error('The core did not return a folder list.');
    return list.map(item => {
      if (typeof item === 'string') return { name: item.split('/').filter(Boolean).pop() || item, path: item, dir: true };
      const path = String(pickField(item, 'path', 'full_path', 'fullPath') || '');
      const name = String(pickField(item, 'name', 'label') || path.split('/').filter(Boolean).pop() || path);
      const flag = pickField(item, 'is_dir', 'isDir', 'isDirectory');
      const type = String(pickField(item, 'type', 'kind') || '').toLowerCase();
      const dir = flag !== null ? flag === true || String(flag).toLowerCase() === 'true'
        : type ? ['dir', 'directory', 'folder'].includes(type) : true;
      return { name, path: path || name, dir };
    }).filter(entry => entry.path && entry.dir);
  }

  function openFolderBrowser() {
    let current = $('#importRoot').value.trim() || '/';
    const pathNode = el('p', { class: 'mono dir-path', id: 'importDirPath' });
    const listNode = el('div', { class: 'folder-list', id: 'importDirList', tabindex: '0' });
    const dialog = openDialog({
      title: 'Choose a channel folder',
      meta: 'Browsing never changes Story. Pick a channel folder, then use it.',
      body: el('div', {}, pathNode, listNode),
      confirmLabel: 'Use this folder', cancelLabel: 'Cancel',
      initialFocus: '#importDirList',
      onConfirm: () => {
        $('#importRoot').value = current;
        clearImportError();
        invalidateImportPreview();
        setStatus('Folder selected', 'info');
      }
    });
    dialog.primary.disabled = true;

    function row(label, target, withIcon) {
      const button = el('button', { class: 'folder-row', type: 'button', on: {
        click: () => load(target),
        keydown: event => {
          if (event.key === 'Enter') { event.preventDefault(); event.stopPropagation(); load(target); }
        }
      } });
      if (withIcon) button.append(icon('folder'));
      button.append(el('span', { text: label }));
      return button;
    }

    async function load(path) {
      current = path;
      pathNode.textContent = path;
      listNode.replaceChildren(el('p', { class: 'help', text: 'Reading folders…' }));
      try {
        const dirs = normalizeDirs(await callCore('list_dirs', { path }));
        if (!dialog.overlay.isConnected) return;
        listNode.replaceChildren();
        const parent = parentOf(path);
        if (parent) listNode.append(row('Up', parent, false));
        dirs.forEach(dir => listNode.append(row(dir.name, dir.path, true)));
        if (!dirs.length && !parent) listNode.append(el('p', { class: 'help', text: 'No subfolders reported here.' }));
        dialog.primary.disabled = false;
      } catch (err) {
        if (!dialog.overlay.isConnected) return;
        listNode.replaceChildren(
          el('p', { class: 'help', text: 'Could not read folders: ' + message(err) }),
          el('button', { class: 'mini-button', type: 'button', text: 'Retry', on: { click: () => load(path) } })
        );
        dialog.primary.disabled = false;
      }
    }
    load(current);
  }

  async function checkFolder() {
    const root = $('#importRoot').value.trim();
    if (!root) { showImportError('Choose the channel folder first.'); $('#importRoot').focus(); return; }
    clearImportError();
    importBusy = 'check';
    syncImportButtons();
    setStatus('Checking Story folder…', 'info');
    try {
      importPreview = normalizePreview(await callCore('story_import_preview', { root }));
      renderImportPreview(importPreview, root);
      setStatus('Preview ready', 'ok');
    } catch (err) {
      importPreview = null;
      renderImportError('Preview failed', message(err), checkFolder);
      setStatus('Preview failed', 'bad');
    } finally {
      importBusy = null;
      syncImportButtons();
    }
  }

  function confirmImport() {
    if (!importPreview || importBusy) return;
    const posts = importPreview.postsCount;
    const images = importPreview.imageCount;
    const body = posts !== null && images !== null
      ? 'Import ' + posts + ' posts and ' + images + ' images? Story\'s files stay where they are.'
      : posts !== null
        ? 'Import ' + posts + ' posts and their images? Story\'s files stay where they are.'
        : 'Import these posts and images? Story\'s files stay where they are.';
    openDialog({ title: 'Confirm import', body, confirmLabel: 'Import', cancelLabel: 'Cancel', onConfirm: runImport });
  }

  async function runImport() {
    if (!importPreview || importBusy) return;
    const root = $('#importRoot').value.trim();
    importBusy = 'run';
    syncImportButtons();
    setStatus('Importing Still Posts…', 'info');
    try {
      renderImportReport(normalizeImportReport(await callCore('story_import_run', { root })), root);
      setStatus('Import finished', 'ok');
    } catch (err) {
      renderImportError('Import failed', message(err), confirmImport);
      setStatus('Import failed', 'bad');
    } finally {
      importBusy = null;
      syncImportButtons();
    }
  }

  /* ---------- Dialogs ---------- */

  function dialogError(panel, text) {
    const old = $('.inline-error', panel);
    if (old) old.remove();
    const footer = $('.dialog-footer', panel);
    panel.insertBefore(el('p', { class: 'inline-error', role: 'alert', text }), footer);
  }

  function openDialog(options) {
    const overlay = el('div', { class: 'dialog-overlay' });
    const titleId = 'dialog-title-' + (++dialogSeq);
    const panel = el('div', { class: 'dialog', role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': titleId, tabindex: '-1' },
      el('h2', { class: 'dialog-title', id: titleId, text: options.title }));
    if (options.meta) panel.append(el('p', { class: 'hint', text: options.meta }));
    const body = typeof options.body === 'string' ? el('p', { class: 'dialog-body', text: options.body }) : options.body;
    if (body) panel.append(body);

    const footer = el('div', { class: 'dialog-footer' });
    let safe = null;
    if (options.cancelLabel !== null) {
      safe = el('button', {
        class: 'button subtle safe', type: 'button', text: options.cancelLabel || 'Cancel',
        on: { click: () => closeDialog(dialog) }
      });
      footer.append(safe);
    }
    const primary = el('button', {
      class: 'button ' + (options.destructive ? 'danger' : 'primary'),
      type: 'button', text: options.confirmLabel || 'OK',
      on: {
        click: async () => {
          primary.disabled = true;
          try {
            if (options.onConfirm && await options.onConfirm(panel) === false) return;
            closeDialog(dialog);
          } catch (err) {
            dialogError(panel, message(err));
          } finally {
            primary.disabled = false;
          }
        }
      }
    });
    footer.append(primary);
    panel.append(footer);
    overlay.append(panel);
    overlay.addEventListener('mousedown', event => {
      if (event.target === overlay && !options.destructive) closeDialog(dialog);
    });
    panel.addEventListener('keydown', event => {
      if (event.key === 'Enter' && event.target.tagName !== 'TEXTAREA' && event.target !== safe) {
        event.preventDefault();
        primary.click();
      }
    });

    const dialog = { overlay, panel, primary, safe, options, opener: document.activeElement };
    dialogStack.push(dialog);
    $('#dialogs').append(overlay);
    requestAnimationFrame(() => {
      const target = options.initialFocus ? $(options.initialFocus, panel) : options.destructive ? safe : primary;
      (target || primary).focus();
    });
    return dialog;
  }

  function closeDialog(dialog) {
    const index = dialogStack.indexOf(dialog);
    if (index < 0) return;
    dialogStack.splice(index, 1);
    dialog.overlay.remove();
    if (dialog.opener && dialog.opener.isConnected) dialog.opener.focus();
  }

  function trapFocus(dialog, event) {
    const focusables = $$('button:not([disabled]), input:not([disabled]), select, textarea, a[href], [tabindex]:not([tabindex="-1"])', dialog.panel);
    if (!focusables.length) { event.preventDefault(); dialog.panel.focus(); return; }
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
  }

  function openWhatsNew() {
    const list = el('ul', { class: 'dialog-list' }, ...WHATS_NEW.map(text => el('li', { text })));
    const body = el('div', {}, el('p', { class: 'dialog-body', text: 'What changed in this release:' }), list);
    openDialog({ title: 'What’s New', meta: 'Version ' + APP_VERSION, body, confirmLabel: 'Done', cancelLabel: null });
  }

  function handleWhatsNewOnLaunch() {
    const last = storageGet('wire.whatsNew.lastVersion', '');
    const seen = storageGet('wire.whatsNew.seen', '');
    if (!last) {
      storageSet('wire.whatsNew.lastVersion', APP_VERSION);
      storageSet('wire.whatsNew.seen', APP_VERSION);
      return;
    }
    if (last !== APP_VERSION && seen !== APP_VERSION) {
      storageSet('wire.whatsNew.lastVersion', APP_VERSION);
      storageSet('wire.whatsNew.seen', APP_VERSION);
      openWhatsNew();
    }
  }

  function supportDialog() {
    const body = el('div', {},
      el('p', { class: 'dialog-body', text: 'Wire Studio KH is free, with every feature unlocked. If you want to help, the shared support page is:' }),
      el('p', {}, el('a', { href: SUPPORT_URL, target: '_blank', rel: 'noopener noreferrer', text: SUPPORT_URL }))
    );
    openDialog({ title: 'Support this app', body, confirmLabel: 'Done', cancelLabel: null });
  }

  function updateDialog() {
    openDialog({
      title: 'Check for updates',
      body: 'Not checked. The updater bridge is not wired in this build, so no feed was read and no update was installed.',
      confirmLabel: 'Done', cancelLabel: null
    });
  }

  function browseDialog() {
    openDialog({
      title: 'Choose a folder',
      body: 'Folder picking is not wired yet. The core owns the data folder shown in Settings; nothing was changed.',
      confirmLabel: 'Done', cancelLabel: null
    });
  }

  /* ---------- Actions ---------- */

  function readSchedule() {
    const mode = $('input[name="schedule"]:checked').value;
    if (mode === 'now') return { delayS: 0 };
    if (mode === '15') return { delayS: 900 };
    if (mode === '60') return { delayS: 3600 };
    const value = $('#schedulePick').value;
    if (!value) return { error: 'Pick a date and time, or choose Now.' };
    const when = new Date(value.length === 16 ? value + ':00+07:00' : value + '+07:00');
    if (!Number.isFinite(when.getTime())) return { error: 'That date and time could not be read.' };
    const deltaMs = when.getTime() - Date.now();
    if (deltaMs < 60000) return { error: 'Pick a time at least one minute from now (UTC+7).' };
    return { delayS: Math.round(deltaMs / 1000) };
  }

  function presetsRejected(err) {
    const msg = message(err);
    if (!/\bpresets\b/i.test(msg)) return false;
    return /(unknown|unexpected|unrecognized|unsupported|invalid|extra|not expected|not allowed|not permitted)\s+(field|argument|parameter|key|property|attribute)\b/i.test(msg)
      || /\bpresets\b[^.;]{0,80}(was|were)?\s*(unexpected|unknown|not allowed|not expected|not valid|not a valid)/i.test(msg);
  }

  async function queuePost() {
    clearComposerError();
    if (!connected) return showComposerError('Core is offline — nothing is queued.');
    const text = $('#postText').value.trim();
    if (!text) { showComposerError('Write some text before queueing.'); $('#postText').focus(); return; }
    const targets = $$('input[name="target"]:checked').map(input => input.value);
    if (!targets.length) { showComposerError('Choose at least one target.'); return; }
    const schedule = readSchedule();
    if (schedule.error) { showComposerError(schedule.error); return; }

    const button = $('#queueBtn');
    button.disabled = true;
    button.textContent = 'Queueing…';
    const presets = selectedPresets(targets);
    try {
      try {
        await callCore('queue_post', { text, targets, delayS: schedule.delayS, presets });
      } catch (err) {
        // core predates the presets field: fall back to plain queue_post, and say so.
        if (!presetsRejected(err)) throw err;
        await callCore('queue_post', { text, targets, delayS: schedule.delayS });
        showToast('The core rejected presets; queued without them.');
      }
      $('#postText').value = '';
      setStatus('Queued to ' + targets.length + ' target' + (targets.length === 1 ? '' : 's'), 'ok');
      await refreshState();
      showSection('queue');
    } catch (err) {
      showComposerError('Could not queue: ' + message(err));
      setStatus('Queue failed — nothing sent', 'bad');
    } finally {
      button.textContent = 'Queue post';
      button.disabled = !connected || state.targets.length === 0;
    }
  }

  async function runDeliveryAction(command, id, successText) {
    if (!connected) throw new Error('Core is offline — nothing was changed.');
    await callCore(command, { id });
    setStatus(successText, 'ok');
    await refreshState();
  }

  function retryDelivery(row) {
    runDeliveryAction('retry_delivery', row.id, 'Retry requested for ' + row.target)
      .catch(err => openDialog({ title: 'Retry failed', body: message(err), confirmLabel: 'Close', cancelLabel: null }));
  }

  function confirmCancel(row) {
    openDialog({
      title: 'Cancel delivery',
      meta: row.target + ' · ' + row.id,
      body: 'A queued delivery will stop. If the post has already reached the channel it may still appear — cancel never means unsent.',
      confirmLabel: 'Cancel delivery', cancelLabel: 'Keep', destructive: true,
      onConfirm: () => runDeliveryAction('cancel_delivery', row.id, 'Cancellation requested for ' + row.target)
    });
  }

  async function saveTelegram(token, chatId) {
    if (!connected) throw new Error('Core is offline — nothing was saved.');
    if (!chatId) throw new Error('Enter the target channel id.');
    // current core's Rust argument is `chat`; chatId stays in the UI contract.
    await callCore('save_telegram', { token, chatId, chat: chatId });
    setStatus('Telegram settings saved', 'ok');
    await refreshState();
  }

  function fieldBlock(labelText, input, helpText) {
    return el('div', { class: 'field' },
      el('label', { for: input.id, text: labelText }), input, el('p', { class: 'help', text: helpText }));
  }

  function openTokenDialog() {
    const tokenInput = el('input', {
      type: 'password', id: 'wire-token', autocomplete: 'off', spellcheck: 'false',
      placeholder: 'Paste bot token from @BotFather'
    });
    const chatInput = el('input', {
      type: 'text', id: 'wire-chat', autocomplete: 'off', value: state.telegram.chatId, placeholder: '-100…'
    });
    const saved = state.telegram.last4
      ? 'A token is saved (last four: ' + state.telegram.last4 + '). Leave blank to keep it.'
      : state.telegram.configured ? 'A token is saved. Leave blank to keep it.' : 'The full token is never shown again after saving.';
    const body = el('div', {},
      fieldBlock('Bot token', tokenInput, saved),
      fieldBlock('Channel id', chatInput, 'The bot must be an admin of this channel before it can post.')
    );
    openDialog({
      title: state.telegram.configured ? 'Replace Telegram token' : 'Add Telegram token',
      body, confirmLabel: 'Save', cancelLabel: 'Cancel', initialFocus: '#wire-token',
      onConfirm: async panel => {
        const token = tokenInput.value.trim();
        const chatId = chatInput.value.trim();
        if (!chatId) { dialogError(panel, 'Enter the target channel id.'); return false; }
        if (!token && !state.telegram.configured) { dialogError(panel, 'Paste the bot token from @BotFather.'); return false; }
        await saveTelegram(token, chatId);
      }
    });
  }

  function applyTheme(choice) {
    const theme = ['system', 'light', 'dark'].includes(choice) ? choice : 'system';
    if (theme === 'system') document.documentElement.removeAttribute('data-theme');
    else document.documentElement.setAttribute('data-theme', theme);
    storageSet('wire.appearance', theme);
    $$('[data-theme-choice]').forEach(button => {
      const active = button.dataset.themeChoice === theme;
      button.setAttribute('aria-pressed', String(active));
      button.classList.toggle('is-active', active);
    });
  }

  function setQueueLayout(layout) {
    queueLayout = layout === 'cards' ? 'cards' : 'table';
    storageSet('wire.queue.layout', queueLayout);
    $('#queueTableBtn').classList.toggle('is-active', queueLayout === 'table');
    $('#queueCardsBtn').classList.toggle('is-active', queueLayout === 'cards');
    $('#queueTableBtn').setAttribute('aria-pressed', String(queueLayout === 'table'));
    $('#queueCardsBtn').setAttribute('aria-pressed', String(queueLayout === 'cards'));
    renderQueue();
  }

  function wireEvents() {
    $$('.nav-item').forEach(button => button.addEventListener('click', () => showSection(button.dataset.section)));
    $('#supportBtn').addEventListener('click', supportDialog);
    $('#updateBtn').addEventListener('click', updateDialog);
    $('#whatsNewBtn').addEventListener('click', openWhatsNew);
    $('#offlineRetry').addEventListener('click', refreshState);
    $('#queueBtn').addEventListener('click', queuePost);
    $('#postText').addEventListener('input', () => { updateCount(); clearComposerError(); });
    $('#targetChecks').addEventListener('change', event => {
      if (event.target && event.target.name === 'target') { renderPresetPickers(); clearComposerError(); }
    });
    $$('input[name="schedule"]').forEach(radio => radio.addEventListener('change', () => {
      $('#schedulePick').disabled = $('input[name="schedule"]:checked').value !== 'pick';
      clearComposerError();
    }));
    $('#queueSearch').addEventListener('input', renderQueue);
    $('#queueTableBtn').addEventListener('click', () => setQueueLayout('table'));
    $('#queueCardsBtn').addEventListener('click', () => setQueueLayout('cards'));
    $('#telegramManageBtn').addEventListener('click', openTokenDialog);
    $('#importRoot').addEventListener('input', () => { clearImportError(); invalidateImportPreview(); });
    $('#importRoot').addEventListener('keydown', event => {
      if (event.key === 'Enter' && !event.metaKey && !event.ctrlKey && !event.altKey) { event.preventDefault(); checkFolder(); }
    });
    $('#importChooseBtn').addEventListener('click', openFolderBrowser);
    $('#importCheckBtn').addEventListener('click', checkFolder);
    $('#importRunBtn').addEventListener('click', confirmImport);
    $('#telegramSaveChatBtn').addEventListener('click', () => {
      saveTelegram('', $('#telegramChatId').value.trim())
        .catch(err => openDialog({ title: 'Channel not saved', body: message(err), confirmLabel: 'Close', cancelLabel: null }));
    });
    $('#settingsTargetsBtn').addEventListener('click', () => showSection('targets'));
    $('#settingsUpdateBtn').addEventListener('click', updateDialog);
    $$('[data-browse]').forEach(button => button.addEventListener('click', browseDialog));
    $('#themeGroup').addEventListener('click', event => {
      const button = event.target.closest('[data-theme-choice]');
      if (button) applyTheme(button.dataset.themeChoice);
    });
    $('#aboutWhatsNewBtn').addEventListener('click', openWhatsNew);
    $('#aboutSupportBtn').addEventListener('click', supportDialog);

    document.addEventListener('keydown', event => {
      if (dialogStack.length) {
        const top = dialogStack[dialogStack.length - 1];
        if (event.key === 'Escape') { event.preventDefault(); closeDialog(top); }
        else if (event.key === 'Tab') trapFocus(top, event);
        return;
      }
      if (event.key === 'Escape' && document.activeElement === $('#queueSearch') && $('#queueSearch').value) {
        $('#queueSearch').value = '';
        renderQueue();
        return;
      }
      if (!(event.metaKey || event.ctrlKey) || event.altKey) return;
      const key = event.key.toLowerCase();
      if (key === 'n') { event.preventDefault(); showSection('composer'); $('#postText').focus(); }
      else if (key >= '1' && key <= '4') {
        event.preventDefault();
        showSection(['composer', 'queue', 'targets', 'settings'][Number(key) - 1]);
      }
      else if (key === '5') { event.preventDefault(); showSection('import'); }
      else if (key === ',') { event.preventDefault(); showSection('settings'); }
      else if (key === 'k') { event.preventDefault(); showSection('queue'); $('#queueSearch').focus(); $('#queueSearch').select(); }
      else if (key === 'enter' && activeSection === 'composer') { event.preventDefault(); queuePost(); }
    });
  }

  function init() {
    applyTheme(storageGet('wire.appearance', 'system'));
    queueLayout = storageGet('wire.queue.layout', 'table') === 'cards' ? 'cards' : 'table';
    $('#queueTableBtn').classList.toggle('is-active', queueLayout === 'table');
    $('#queueCardsBtn').classList.toggle('is-active', queueLayout === 'cards');
    $('#queueTableBtn').setAttribute('aria-pressed', String(queueLayout === 'table'));
    $('#queueCardsBtn').setAttribute('aria-pressed', String(queueLayout === 'cards'));
    wireEvents();
    renderImportEmpty();
    syncImportButtons();
    showSection('composer');
    handleWhatsNewOnLaunch();
    refreshState();
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init, { once: true });
  else init();
})();
