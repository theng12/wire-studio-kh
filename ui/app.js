'use strict';

(() => {
  // Fallback only: the core reports the real version and applyVersion() uses it.
  let APP_VERSION = '0.5.0';
  const WHATS_NEW = [
    'Settings now has a Gateway card: paste its address and key, press Connect, and Wire says exactly what Gateway answered — "connected · 51 models available", an empty catalogue, or Gateway\'s own refusal.',
    'A saved Gateway key that stops working says so in Gateway\'s words and offers Check connection, instead of pretending it is connected.',
    '"Core" is gone from the last places it survived on screen.'
  ];
  const SUPPORT_URL = 'https://nowpayments.io/donation/studiokh';
  const TEST_ROUTES = {
    state: ['GET', '/api/test/state'],
    queue_post: ['POST', '/api/test/trigger/schedule'],
    cancel_delivery: ['POST', '/api/test/trigger/cancel'],
    retry_delivery: ['POST', '/api/test/trigger/retry'],
    save_telegram: ['POST', '/api/test/trigger/save-telegram'],
    telegram_connected_as: ['POST', '/api/test/trigger/telegram-connected-as'],
    telegram_discover: ['POST', '/api/test/trigger/telegram-discover'],
    telegram_use_channel: ['POST', '/api/test/trigger/telegram-use-channel'],
    save_gateway: ['POST', '/api/test/trigger/gateway-save'],
    list_dirs: ['POST', '/api/test/trigger/list-dirs'],
    media_products: ['POST', '/api/test/trigger/media-products']
  };
  const SECTIONS = {
    composer: { eyebrow: 'Compose', title: 'Composer', subtitle: 'Write a text post, choose where it goes, and send it now or later.' },
    queue: { eyebrow: 'Dispatch', title: 'Queue', subtitle: 'One row per target. Retry or cancel a delivery and see the reason.' },
    targets: { eyebrow: 'Connections', title: 'Targets', subtitle: 'Where Wire posts. Connecting YouTube and Facebook ships in a later build.' },
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
  const DAY_FMT = new Intl.DateTimeFormat('en-GB', {
    timeZone: 'Asia/Phnom_Penh', day: '2-digit', month: 'short'
  });
  const TIME_ONLY_FMT = new Intl.DateTimeFormat('en-GB', {
    timeZone: 'Asia/Phnom_Penh', hour: '2-digit', minute: '2-digit', hour12: false
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
  let productsRead = null;
  let productsError = null;
  let productsBusy = false;
  let productChoice = null;

  const tauriBridge = window.__TAURI__ || null;
  const tauriInvoke = tauriBridge && (tauriBridge.core && tauriBridge.core.invoke || tauriBridge.invoke);
  const testMode = !tauriBridge && location.port !== '';

  function emptyState() {
    return {
      dataDir: '', targets: [], deliveries: [], presets: [],
      telegram: { configured: false, connection: 'not_configured', chatId: '', last4: '', reason: '' },
      gateway: { configured: false, connection: 'not_configured', url: '', last4: '', models: null, reason: '' },
      media: { configured: false, path: '', found: false, state: '' }
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
      try { data = JSON.parse(data); } catch (_) { throw new Error('The import report could not be read.'); }
    }
    if (!data || typeof data !== 'object' || Array.isArray(data)) throw new Error('The import report could not be read.');
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
    throw new Error("Wire's own service is not available in this window.");
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
      throw new Error('The service answered ' + response.status + (detail ? ': ' + detail : '') + '.');
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
    if (['ready', 'ok', 'connected', 'live'].includes(text)) return 'ready';
    // configured = saved and never verified; the core never calls that connected.
    if (text === 'configured') return 'configured';
    if (['failed', 'error', 'bad', 'off', 'unsupported'].includes(text)) return 'failed';
    return 'not_configured';
  }

  // One word per state, the same word the core used. Saved is never shown as connected.
  function connectionLabel(connection) {
    if (connection === 'ready') return 'connected';
    if (connection === 'configured') return 'saved, not verified';
    if (connection === 'failed') return 'connection failed';
    return 'not configured';
  }

  function connectionLevel(connection) {
    return connection === 'ready' ? 'ok' : connection === 'failed' ? 'bad' : 'off';
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
        maxTextLength: countOf(pickField(item, 'max_text_length', 'maxTextLength', 'max_length')),
        media: String(pickField(item, 'media') || '').toLowerCase()
      }))
      .filter(preset => preset.id && preset.platform) : [];
    const telegram = {
      configured,
      connection: normalizeConnection(telegramRaw.connection_state || telegramRaw.connectionState || telegramRaw.state),
      chatId: String(telegramRaw.chat || telegramRaw.chat_id || telegramRaw.chatId || ''),
      chatTitle: String(telegramRaw.chat_title || telegramRaw.chatTitle || ''),
      bot: String(telegramRaw.bot || ''),
      last4: lastFour(telegramRaw.token_masked || telegramRaw.token_last4 || telegramRaw.tokenLast4 || telegramRaw.masked_token || telegramRaw.maskedToken || telegramRaw.mask),
      reason: String(telegramRaw.reason || telegramRaw.error || (configured ? 'Token and channel saved; this build has not reported a verified connection.' : ''))
    };
    // A target id is the core's key, never a name a person reads: every row is
    // labelled the way the Target page labels it.
    const targets = targetsRaw.map(normalizeTarget).map(target => {
      const named = { ...target, label: targetLabel(target.id, telegram) };
      return target.platform === 'telegram' && named.connection === 'not_configured' && telegram.connection !== 'not_configured'
        ? { ...named, connection: telegram.connection, reason: telegram.reason }
        : named;
    });
    const gatewayRaw = source.gateway && typeof source.gateway === 'object' ? source.gateway : {};
    const gateway = {
      configured: Boolean(gatewayRaw.configured),
      connection: normalizeConnection(gatewayRaw.connection_state || gatewayRaw.connectionState || gatewayRaw.state),
      url: String(gatewayRaw.url || ''),
      last4: lastFour(gatewayRaw.key_masked || gatewayRaw.keyMasked),
      // a count or nothing: "no data" must never read as zero models
      models: countOf(gatewayRaw.models),
      reason: String(gatewayRaw.reason || gatewayRaw.error || '')
    };
    const mediaRaw = source.media && typeof source.media === 'object' ? source.media : {};
    return {
      version: String(source.version || ''),
      dataDir: String(source.data_dir || source.dataDir || ''),
      appearance: String(source.appearance || ''),
      targets,
      presets,
      telegram,
      gateway,
      hold: {
        state: String((source.hold && source.hold.state) || ''),
        path: String((source.hold && source.hold.path) || ''),
        reason: String((source.hold && source.hold.reason) || '')
      },
      media: {
        configured: Boolean(mediaRaw.configured),
        path: String(mediaRaw.path || ''),
        found: Boolean(mediaRaw.found),
        state: String(mediaRaw.state || '')
      },
      deliveries: deliveriesRaw.map(normalizeDelivery).map(row => ({
        ...row,
        targetName: targetLabel(row.target, telegram)
      }))
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
    const target = String(row.target || row.target_label || row.target_name || row.channel || 'Target not reported');
    return {
      id: String(row.id || index + 1), text, target,
      preset: String(row.preset || row.preset_id || row.presetId || ''),
      shown, nextAt, error, attempts
    };
  }

  // "telegram:-100…" is the core's key, never something a person reads.
  function targetLabel(target, telegram) {
    const raw = String(target || '');
    const platform = raw.split(':')[0];
    if (platform === 'telegram') {
      const name = telegram && (telegram.chatTitle || telegram.chatId);
      return name || 'Telegram channel';
    }
    if (platform === 'youtube') return 'YouTube channel';
    if (platform === 'facebook_page') return 'Facebook page';
    return platform || raw;
  }

  // The core's version is the app's version (it comes from the build); the
  // constant above is only a fallback for a UI opened without a core.
  function applyVersion() {
    const version = state.version || APP_VERSION;
    APP_VERSION = version;
    $$('.pill-version').forEach(node => { node.textContent = version; });
    const about = $('#aboutVersion');
    if (about) about.textContent = 'Wire Studio KH ' + version;
    const hint = $('#settingsVersion');
    if (hint) hint.textContent = 'Version ' + version;
    document.title = 'Wire Studio KH ' + version;
    $('#whatsNewBtn').setAttribute('aria-label', 'Wire Studio KH ' + version + ', open What\'s New');
    return version;
  }

  async function refreshState() {
    setStatus(connected ? 'Refreshing…' : 'Connecting…', 'info');
    try {
      state = normalizeState(await callCore('state'));
      coreError = null;
      connected = true;
      setStatus('Connected', 'ok');
      applyVersion();
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
    renderProducts();
    updateProductField();
    $('#offlineBanner').hidden = connected;
    $('#offlineDetail').textContent = coreError || "Wire's own service did not answer.";
    $('#queueBtn').disabled = !connected || state.targets.length === 0;
    $('#queueBtn').title = !connected ? 'Wire is not answering — nothing can be queued.' : '';
    updateCount();
  }

  function renderTargetChecks() {
    const host = $('#targetChecks');
    host.replaceChildren();
    if (!state.targets.length) {
      host.append(el('p', { class: 'help', text: 'No targets yet. Add a Telegram token on the Targets page first.' }));
      return;
    }
    state.targets.forEach((target, index) => {
      const inputId = 'target-' + index;
      // Text posts only for now: a destination that needs a video or an image says
      // so here instead of failing after the post is queued.
      const mediaOnly = needsMedia(target.platform);
      host.append(el('label', { class: 'check', for: inputId, title: mediaOnly ? 'Needs a video or an image — ships in a later build' : '' },
        el('input', { type: 'checkbox', name: 'target', id: inputId, value: target.id, disabled: !connected || !target.enabled || mediaOnly }),
        el('span', { text: target.label }),
        el('span', { class: 'chip', 'data-level': connectionLevel(target.connection), text: connectionLabel(target.connection) }),
        mediaOnly ? el('span', { class: 'cell-note', text: 'needs a video or an image — ships in a later build' }) : null
      ));
    });
  }

  // The composer writes a caption; a size that needs a video or an image is not
  // offered until the app can attach one.
  function presetsFor(platform) {
    return state.presets.filter(preset =>
      preset.platform === String(platform || '').toLowerCase() && (preset.media === 'none' || preset.media === ''));
  }

  function needsMedia(platform) {
    const all = state.presets.filter(preset => preset.platform === String(platform || '').toLowerCase());
    return all.length > 0 && presetsFor(platform).length === 0;
  }

  function presetHelp(preset) {
    if (!preset) return 'No sizes reported yet.';
    const parts = [];
    if (preset.ratio) parts.push('Ratio ' + preset.ratio);
    if (preset.maxTextLength) parts.push('Max ' + Number(preset.maxTextLength).toLocaleString() + ' characters');
    return parts.length ? parts.join(' · ') : 'No format details reported yet.';
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

  // Two short lines, so a date never wraps into three in a narrow column.
  function nextWhen(row) {
    if (row.shown === 'sent' || row.shown === 'cancelled') return el('span', { text: '—' });
    if (!row.nextAt) return el('span', { class: 'cell-note', text: 'Not reported' });
    return el('span', { class: 'when' },
      el('span', { text: DAY_FMT.format(row.nextAt) }),
      el('span', { class: 'cell-note', text: TIME_ONLY_FMT.format(row.nextAt) + ' UTC+7' }));
  }

  function filteredRows() {
    const query = $('#queueSearch').value.trim().toLowerCase();
    if (!query) return state.deliveries;
    return state.deliveries.filter(row =>
      [row.text, row.targetName, row.shown, row.error].some(value => String(value).toLowerCase().includes(query)));
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

  // Why an action is unavailable is written on the row, not only in a tooltip.
  const ACTION_REASON = {
    sent: 'sent, nothing to retry',
    cancelled: 'cancelled, nothing to retry',
    sending: 'sending now, nothing to retry',
    queued: 'queued, nothing to retry',
    dead: 'gave up after its attempts; Cancel is closed'
  };

  function actionButtons(row) {
    const retryable = !['sent', 'sending', 'queued'].includes(row.shown);
    const cancellable = ['queued', 'sending', 'failed-retry'].includes(row.shown);
    const retry = el('button', {
      class: 'mini-button', type: 'button', disabled: !retryable,
      title: retryable ? 'Retry this delivery' : (ACTION_REASON[row.shown] || 'Retry is not available for this state'),
      'aria-label': 'Retry delivery ' + row.id,
      on: { click: () => retryDelivery(row) }
    }, icon('retry'), 'Retry');
    const cancel = el('button', {
      class: 'mini-button', type: 'button', disabled: !cancellable,
      title: cancellable ? 'Cancel this delivery' : (row.shown === 'sent' ? 'already sent; nothing to cancel' : 'Cancel is not available for this state'),
      'aria-label': 'Cancel delivery ' + row.id,
      on: { click: () => confirmCancel(row) }
    }, icon('cancel'), 'Cancel');
    const reasons = [];
    if (!retryable && !cancellable) reasons.push(ACTION_REASON[row.shown] || 'nothing to retry or cancel');
    else {
      if (!retryable) reasons.push(ACTION_REASON[row.shown] || 'retry is closed for this state');
      if (!cancellable) reasons.push('cancel is closed after the first attempt');
    }
    const reason = reasons.join(' · ');
    return el('div', { class: 'row-actions-wrap' },
      el('div', { class: 'row-actions' }, retry, cancel),
      reason ? el('div', { class: 'cell-note', text: reason }) : null);
  }

  function stateChip(row) {
    return el('span', { class: 'chip', 'data-level': STATE_LEVEL[row.shown] || 'info', text: row.shown });
  }

  function renderTable(rows) {
    const body = el('tbody');
    rows.forEach(row => {
      const postCell = el('td', {}, el('div', { class: 'row-title', text: excerpt(row.text) }));
      if (row.error) postCell.append(el('div', { class: 'cell-note', text: row.error }));
      const targetCell = el('td', {}, el('span', { text: row.targetName, title: row.target }));
      if (row.preset) targetCell.append(el('div', { class: 'cell-note', text: presetLabel(row.preset) }));
      const stateCell = el('td', {},
        stateChip(row),
        el('div', { class: 'cell-note', text: 'attempt ' + (row.attempts || 0) }),
        row.shown === 'sent' || row.shown === 'cancelled' ? null : el('div', { class: 'cell-note' }, nextWhen(row)));
      body.append(el('tr', {},
        postCell,
        targetCell,
        stateCell,
        el('td', {}, actionButtons(row))
      ));
    });
    const table = el('table', { class: 'queue-table' },
      el('thead', {}, el('tr', {},
        el('th', { text: 'Post' }), el('th', { text: 'Target' }), el('th', { text: 'State' }),
        el('th', { text: 'Actions' })
      )), body);
    return el('div', { class: 'table-wrap' }, table);
  }

  function renderCards(rows) {
    const list = el('div', { class: 'card-list' });
    rows.forEach(row => {
      const targetBox = el('div', {}, el('span', { text: 'Target' }), el('strong', { text: row.targetName, title: row.target }));
      if (row.preset) targetBox.append(el('div', { class: 'cell-note', text: presetLabel(row.preset) }));
      const meta = el('div', { class: 'queue-meta' },
        targetBox,
        el('div', {}, el('span', { text: 'Next attempt' }), el('strong', { class: 'mono' }, nextWhen(row))),
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
    renderTelegramCard();
    renderPlatformCard('youtube', '#youtubeChip', '#youtubeReason');
    renderPlatformCard('facebook', '#facebookChip', '#facebookReason');
  }

  function renderTelegramCard() {
    const telegram = state.telegram;
    const chip = $('#telegramChip');
    chip.textContent = connectionLabel(telegram.connection);
    chip.dataset.level = connectionLevel(telegram.connection);
    $('#telegramReason').textContent = telegram.reason ||
      (telegram.connection === 'ready' ? 'Connected — Telegram confirmed it.'
        : telegram.connection === 'failed' ? 'Connection failed; no reason was reported.'
          : telegram.connection === 'configured' ? 'Token and channel saved; check the connection to confirm them.'
            : 'No connection yet. Paste the bot token below and press Connect.');

    $('#telegramTokenHelp').textContent = telegram.last4
      ? 'Saved token •••• ' + telegram.last4 + '. Paste a new token only to replace it.'
      : telegram.configured ? 'A token is saved. Paste a new token only to replace it.'
        : 'No token saved yet. Paste the token @BotFather gave you.';
    $('#telegramConnectBtn').textContent = telegram.configured ? 'Check connection' : 'Connect';

    // The channel is named the way Telegram names it, never only as a number.
    $('#telegramChannelHelp').textContent = telegram.chatTitle
      ? 'Sending to ' + telegram.chatTitle + '. Paste another @username, t.me link or id to change it.'
      : telegram.chatId
        ? 'Saved channel ' + telegram.chatId + ' — no title reported yet. Press Use this channel to check it.'
        : 'No channel chosen yet. Find it below, or paste @username, a t.me link or an id.';

    // The block shows the one step the user is on, and disappears once connected.
    const step = !telegram.configured ? 1 : telegram.connection === 'ready' ? 0 : 3;
    const howto = $('#telegramHowto');
    howto.hidden = step === 0;
    if (step !== 0) {
      $('#telegramHowtoSummary').textContent = step === 1
        ? 'How to connect Telegram — 1 of 4'
        : 'How to connect Telegram — 3 of 4: add the bot as an administrator';
      $$('#telegramHowtoSteps li').forEach(item => {
        const number = Number(item.dataset.step);
        item.classList.toggle('is-current', number === step);
        item.classList.toggle('is-done', number < step);
      });
    }
    $('#howtoBotName').textContent = telegram.bot || 'your bot';
    $('#settingsTelegramHelp').textContent = 'Telegram is ' + connectionLabel(telegram.connection) + '. Manage the target here.';
  }

  function renderPlatformCard(platform, chipSelector, reasonSelector) {
    const target = state.targets.find(item => item.platform === platform);
    // never ready unless the core said so; a missing target is honestly not configured.
    const connection = target ? target.connection : 'not_configured';
    const chip = $(chipSelector);
    chip.textContent = connectionLabel(connection);
    chip.dataset.level = connectionLevel(connection);
    $(reasonSelector).textContent = target && target.reason
      ? target.reason
      : connection === 'ready' ? 'Connected and confirmed.'
        : connection === 'failed' ? 'Connection failed; no reason was reported.'
          : 'Not connected yet.';
  }

  function renderSettings() {
    const dataFolder = $('#dataFolder');
    dataFolder.value = state.dataDir || 'Not reported';
    dataFolder.title = state.dataDir || 'The app has not reported its data folder yet.';
    $$('[data-theme-choice]').forEach(button => {
      button.classList.toggle('is-active', button.dataset.themeChoice === storageGet('wire.appearance', 'system'));
    });
    renderGatewayCard();
    renderHoldCard();
  }

  // The update hold's token, in the app's own words: ready, moving on the first hold, or missing.
  function renderHoldCard() {
    const hold = state.hold || {};
    const chip = $('#holdChip');
    const words = { ready: 'ready', moves: 'moves on first hold', missing: 'token missing' };
    chip.textContent = words[hold.state] || 'not checked';
    chip.dataset.level = hold.state === 'ready' ? 'ok' : hold.state === 'missing' ? 'bad' : 'warn';
    $('#holdReason').textContent = hold.reason ||
      (hold.state === 'ready' ? `Update token at ${hold.path}.` : 'Wire pauses its queue before every update check. This says whether it can.');
  }

  // Gateway is the model source: the chip never says connected until Gateway answered a read.
  function renderGatewayCard() {
    const gateway = state.gateway;
    const chip = $('#gatewayChip');
    chip.textContent = connectionLabel(gateway.connection);
    chip.dataset.level = connectionLevel(gateway.connection);
    $('#gatewayReason').textContent = gateway.reason ||
      (gateway.connection === 'ready'
        ? (gateway.models === 0 ? 'Connected. Gateway publishes no models yet.' : 'Connected to Gateway.')
        : gateway.connection === 'failed' ? 'Gateway refused the check; no reason reported.'
          : gateway.connection === 'configured' ? 'Saved, not checked yet. Press Check connection.'
            : 'Not connected yet. Gateway is where Wire reads its models from.');

    const urlInput = $('#gatewayUrlInput');
    if (!urlInput.value && gateway.url) urlInput.value = gateway.url;
    $('#gatewayKeyHelp').textContent = gateway.last4
      ? 'Saved key •••• ' + gateway.last4 + '. Paste a new key only to replace it.'
      : gateway.configured ? 'A key is saved. Paste a new key only to replace it.'
        : 'No key saved yet. Paste the Gateway API key.';
    $('#gatewayConnectBtn').textContent = gateway.configured ? 'Check connection' : 'Connect';
  }

  // Connect or check: a blank key keeps the saved one, so the same button does both.
  async function gatewayConnect() {
    if (!connected) { setStatus('Wire is not answering — nothing was saved', 'bad'); return; }
    const url = $('#gatewayUrlInput').value.trim();
    const key = $('#gatewayKeyInput').value.trim();
    if (!url && !state.gateway.url) { setStatus('Paste the Gateway address first', 'bad'); $('#gatewayUrlInput').focus(); return; }
    if (!key && !state.gateway.configured) { setStatus('Paste the Gateway API key first', 'bad'); $('#gatewayKeyInput').focus(); return; }
    setStatus('Asking Gateway for its models…', 'info');
    let result;
    try {
      result = unwrap(await callCore('save_gateway', { url, key }));
    } catch (error) {
      setStatus('Not connected — ' + message(error), 'bad');
      return;
    }
    if (key) $('#gatewayKeyInput').value = '';
    // Say the answer after the state refresh, or the refresh's own line replaces it.
    await refreshState();
    if (result && result.connection_state === 'ready') setStatus('Connected to Gateway — ' + result.reason, 'ok');
    else setStatus('Not connected — ' + (result && result.reason ? result.reason : 'Gateway gave no answer'), 'bad');
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

  /* ---------- Products (Media catalogue, read-only) ---------- */

  function mediaStateLabel(media) {
    if (media.state === 'ready') return 'connected';
    if (media.state === 'not_found') return 'not found';
    return 'not configured';
  }

  function mediaStateLevel(media) {
    return media.state === 'ready' ? 'ok' : media.state === 'not_found' ? 'warn' : 'off';
  }

  function priceText(price) {
    return price === null || price === undefined ? 'Not reported' : String(price);
  }

  function asOfText(iso) {
    const when = parseWhen(iso);
    return when ? TIME_ONLY_FMT.format(when) : 'an unknown time';
  }

  function normalizeProducts(raw) {
    const data = unwrap(raw);
    if (data.configured === false) return { blocked: "Set Media's data folder in Settings." };
    if (data.found === false) return { blocked: 'That folder has no database.sqlite.' };
    const list = Array.isArray(data.products) ? data.products : [];
    return {
      asOf: String(data.as_of || ''),
      products: list.map(item => ({
        id: String(item.id || ''),
        sku: String(item.sku || ''),
        name: String(item.name || ''),
        price: item.price_retail === null || item.price_retail === undefined ? null : Number(item.price_retail)
      })).filter(product => product.id)
    };
  }

  async function readProducts() {
    if (!connected) { setStatus('Wire is not answering — nothing was read', 'bad'); return; }
    productsBusy = true;
    renderProducts();
    setStatus("Reading Media's products…", 'info');
    try {
      productsRead = normalizeProducts(await callCore('media_products'));
      productsError = null;
      const count = productsRead.products.length;
      setStatus(count + ' product' + (count === 1 ? '' : 's') + ' read', 'ok');
    } catch (err) {
      productsRead = null;
      productsError = message(err);
      setStatus('Products could not be read', 'bad');
    } finally {
      productsBusy = false;
      renderProducts();
      updateProductField();
    }
  }

  function renderProducts() {
    const media = state.media;
    const chip = $('#productsChip');
    chip.textContent = mediaStateLabel(media);
    chip.dataset.level = mediaStateLevel(media);
    $('#productsPath').textContent = media.path || 'Not reported';
    const button = $('#productsReadBtn');
    button.disabled = !connected || productsBusy;
    button.textContent = productsBusy ? 'Reading…' : 'Read products';
    const host = $('#productsList');
    const asOf = $('#productsAsOf');
    host.replaceChildren();
    if (productsError) {
      asOf.hidden = true;
      host.append(stateBlock('error', 'Products could not be read',
        "Media's store could not be read: " + productsError, 'Try again', readProducts));
      return;
    }
    if (productsRead && productsRead.blocked) {
      asOf.hidden = true;
      host.append(stateBlock('empty', 'Catalogue not available', productsRead.blocked));
      return;
    }
    if (!productsRead) {
      asOf.hidden = true;
      host.append(stateBlock('empty', 'Not read yet',
        media.state === 'ready'
          ? 'Press Read products to list the catalogue.'
          : "Media is not connected yet. Press Read products to check, or set Media's data folder in Settings."));
      return;
    }
    const query = $('#productsSearch').value.trim().toLowerCase();
    const rows = !query ? productsRead.products
      : productsRead.products.filter(p => [p.name, p.sku].some(v => v.toLowerCase().includes(query)));
    if (!rows.length) {
      host.append(stateBlock('empty',
        productsRead.products.length ? 'No matching products' : 'No products yet',
        productsRead.products.length ? 'Try another search term.' : "Media's catalogue has no products yet."));
    } else {
      const body = el('tbody');
      rows.forEach(p => body.append(el('tr', {},
        el('td', { text: p.name || p.id }),
        el('td', { class: 'mono', text: p.sku || 'Not reported' }),
        el('td', { text: priceText(p.price) }))));
      host.append(el('div', { class: 'table-wrap' }, el('table', {},
        el('thead', {}, el('tr', {},
          el('th', { text: 'Name' }), el('th', { text: 'SKU' }), el('th', { text: 'Price' }))), body)));
    }
    asOf.hidden = false;
    asOf.textContent = 'price as of ' + asOfText(productsRead.asOf);
  }

  function productLine(choice) {
    return choice.name + ' · ' + (choice.price === null ? 'no price' : String(choice.price));
  }

  function updateProductField() {
    const choice = productChoice;
    $('#productClearBtn').hidden = !choice;
    const help = $('#productHelp');
    if (choice) help.textContent = productLine(choice) + ' — as of ' + asOfText(choice.asOf);
    else if (productsRead && productsRead.blocked || productsError) help.textContent = "Media's catalogue is not reachable — the post can still be queued without a product.";
    else help.textContent = 'No product chosen.';
    const preview = $('#productPreview');
    preview.hidden = !choice;
    preview.textContent = choice ? '— ' + productLine(choice) + ' (as of ' + asOfText(choice.asOf) + ')' : '';
  }

  function openProductDialog() {
    const search = el('input', { type: 'search', id: 'productDialogSearch', placeholder: 'Search by name or SKU', autocomplete: 'off' });
    const list = el('div', { class: 'folder-list', id: 'productDialogList', tabindex: '0' });
    const dialog = openDialog({
      title: 'Choose product',
      meta: "Read-only from Media's catalogue. Pick one to attach to this post.",
      body: el('div', { class: 'field compact' }, search, list),
      confirmLabel: 'Done', cancelLabel: null,
      initialFocus: '#productDialogSearch'
    });

    function paint() {
      if (!productsRead || productsRead.blocked) {
        list.replaceChildren(el('p', { class: 'help', text: "Media's catalogue could not be read. Close this dialog, press Read products on the Import page, then try again." }));
        return;
      }
      const query = search.value.trim().toLowerCase();
      const found = !query ? productsRead.products
        : productsRead.products.filter(p => [p.name, p.sku].some(v => v.toLowerCase().includes(query)));
      list.replaceChildren();
      if (!found.length) {
        list.append(el('p', { class: 'help', text: productsRead.products.length ? 'No matching products.' : "Media's catalogue has no products yet." }));
        return;
      }
      found.forEach(p => list.append(el('button', { class: 'folder-row', type: 'button', on: { click: () => {
        productChoice = { id: p.id, name: p.name || p.id, price: p.price, asOf: productsRead.asOf };
        closeDialog(dialog);
        updateProductField();
        setStatus('Product attached: ' + (p.name || p.id), 'ok');
      } } },
        el('span', { text: p.name || p.id }),
        el('span', { class: 'cell-note', text: (p.sku || 'no SKU') + ' · ' + (p.price === null ? 'no price' : String(p.price)) }))));
    }

    async function load() {
      if (!productsRead && connected) await readProducts();
      if (!dialog.overlay.isConnected) return;
      paint();
    }
    search.addEventListener('input', paint);
    load();
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
    if (!connected) return showComposerError('Wire is not answering — nothing was queued.');
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
    const args = { text, targets, delayS: schedule.delayS, presets };
    if (productChoice) args.product_id = productChoice.id;
    try {
      try {
        await callCore('queue_post', args);
      } catch (err) {
        // core predates the presets field: fall back to plain queue_post, and say so.
        if (!presetsRejected(err)) throw err;
        const plain = { text, targets, delayS: schedule.delayS };
        if (productChoice) plain.product_id = productChoice.id;
        await callCore('queue_post', plain);
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
    if (!connected) throw new Error('Wire is not answering — nothing was changed.');
    await callCore(command, { id });
    setStatus(successText, 'ok');
    await refreshState();
  }

  function retryDelivery(row) {
    runDeliveryAction('retry_delivery', row.id, 'Retry requested for ' + row.targetName)
      .catch(err => openDialog({ title: 'Retry failed', body: message(err), confirmLabel: 'Close', cancelLabel: null }));
  }

  function confirmCancel(row) {
    openDialog({
      title: 'Cancel delivery',
      meta: row.targetName + ' · ' + row.id,
      body: 'A queued delivery will stop. If the post has already reached the channel it may still appear — cancel never means unsent.',
      confirmLabel: 'Cancel delivery', cancelLabel: 'Keep', destructive: true,
      onConfirm: () => runDeliveryAction('cancel_delivery', row.id, 'Cancellation requested for ' + row.targetName)
    });
  }

  function fieldBlock(labelText, input, helpText) {
    return el('div', { class: 'field' },
      el('label', { for: input.id, text: labelText }), input, el('p', { class: 'help', text: helpText }));
  }

  function chatKind(kind) {
    return ({ channel: 'Channel', group: 'Group', supergroup: 'Group', private: 'Private chat' })[kind] || 'Chat';
  }

  // Connect: the token is checked by Telegram itself before anything is saved as "connected".
  async function telegramConnect() {
    if (!connected) { setStatus('Wire is not answering — nothing was saved', 'bad'); return null; }
    const input = $('#telegramTokenInput');
    const token = input.value.trim();
    if (!token && !state.telegram.configured) {
      setStatus('Paste the bot token from @BotFather first', 'bad');
      return null;
    }
    setStatus('Checking the token with Telegram…', 'info');
    let result;
    try {
      result = unwrap(await callCore('telegram_connected_as', { token }));
    } catch (error) {
      setStatus('Not connected — ' + message(error), 'bad');
      return null;
    }
    if (result && result.ok) input.value = '';
    // Say the answer after the state refresh, or the refresh's own line replaces it.
    await refreshState();
    if (result && result.ok) setStatus('Connected as ' + (result.bot || 'your bot'), 'ok');
    else setStatus('Not connected — ' + (result && result.error ? result.error : 'Telegram gave no answer'), 'bad');
    return result;
  }

  // Discover: every channel the bot can see, by name; an empty answer says what to do.
  async function telegramFind() {
    if (!connected) { setStatus('Wire is not answering — nothing was read', 'bad'); return; }
    const list = $('#telegramChatList');
    const help = $('#telegramFindHelp');
    setStatus('Asking Telegram which channels the bot can see…', 'info');
    let result;
    try {
      result = unwrap(await callCore('telegram_discover'));
    } catch (error) {
      help.hidden = false;
      help.textContent = 'Could not ask Telegram — ' + message(error);
      return;
    }
    const chats = result && Array.isArray(result.chats) ? result.chats : [];
    list.replaceChildren();
    list.hidden = false;
    if (!chats.length) {
      help.hidden = false;
      help.textContent = 'Nothing yet. Add ' + (state.telegram.bot || 'your bot') +
        ' as an administrator of your channel, post any message in it, then press Check again.';
      list.append(el('li', { class: 'chat-row' },
        el('button', { class: 'button subtle', type: 'button', text: 'Check again', on: { click: telegramFind } })));
      setStatus('No channel found yet', 'info');
      return;
    }
    help.hidden = true;
    chats.forEach(chat => {
      list.append(el('li', { class: 'chat-row' },
        el('div', {},
          el('strong', { text: chat.title || chat.id }),
          el('span', { class: 'chat-kind', text: chatKind(chat.kind) })),
        el('button', {
          class: 'button subtle', type: 'button', text: 'Use this channel',
          on: { click: () => telegramUse(chat.id) }
        })));
    });
    setStatus(chats.length + ' channel' + (chats.length === 1 ? '' : 's') + ' found', 'ok');
  }

  // Use a channel by @username, t.me link or id; the title Telegram gives back is saved with it.
  async function telegramUse(reference) {
    if (!connected) { setStatus('Wire is not answering — nothing was saved', 'bad'); return null; }
    const text = String(reference || $('#telegramChannel').value || '').trim();
    if (!text) { setStatus('Choose a channel from the list or paste one first', 'bad'); return null; }
    setStatus('Checking the channel with Telegram…', 'info');
    let result;
    try {
      result = unwrap(await callCore('telegram_use_channel', { text }));
    } catch (error) {
      setStatus('Channel not set — ' + message(error), 'bad');
      return null;
    }
    const used = Boolean(result && result.ok && result.chat);
    if (used) {
      $('#telegramChannel').value = '';
      $('#telegramChatList').hidden = true;
    }
    await refreshState();
    if (used) setStatus('Sending to ' + (result.chat.title || result.chat.id), 'ok');
    else setStatus('Channel not set — ' + (result && result.error ? result.error : 'Telegram gave no answer'), 'bad');
    return result;
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
    $('#telegramConnectBtn').addEventListener('click', telegramConnect);
    $('#telegramTokenInput').addEventListener('keydown', event => {
      if (event.key === 'Enter') { event.preventDefault(); telegramConnect(); }
    });
    $('#telegramFindBtn').addEventListener('click', telegramFind);
    $('#telegramUseBtn').addEventListener('click', () => telegramUse(''));
    $('#importRoot').addEventListener('input', () => { clearImportError(); invalidateImportPreview(); });
    $('#importRoot').addEventListener('keydown', event => {
      if (event.key === 'Enter' && !event.metaKey && !event.ctrlKey && !event.altKey) { event.preventDefault(); checkFolder(); }
    });
    $('#importChooseBtn').addEventListener('click', openFolderBrowser);
    $('#importCheckBtn').addEventListener('click', checkFolder);
    $('#importRunBtn').addEventListener('click', confirmImport);
    $('#productsReadBtn').addEventListener('click', readProducts);
    $('#productsSearch').addEventListener('input', renderProducts);
    $('#productChooseBtn').addEventListener('click', openProductDialog);
    $('#productClearBtn').addEventListener('click', () => { productChoice = null; updateProductField(); clearComposerError(); });
    $('#settingsTargetsBtn').addEventListener('click', () => showSection('targets'));
    $('#settingsUpdateBtn').addEventListener('click', updateDialog);
    $('#gatewayConnectBtn').addEventListener('click', gatewayConnect);
    $('#gatewayKeyInput').addEventListener('keydown', event => {
      if (event.key === 'Enter') { event.preventDefault(); gatewayConnect(); }
    });
    $('#gatewayUrlInput').addEventListener('keydown', event => {
      if (event.key === 'Enter') { event.preventDefault(); gatewayConnect(); }
    });
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
