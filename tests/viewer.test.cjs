const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const source = fs.readFileSync(require('node:path').join(__dirname, '../viewer/viewer.js'), 'utf8');
const catalogs = JSON.parse(fs.readFileSync(require('node:path').join(__dirname, '../src/locales.json'), 'utf8').replace(/^\uFEFF/, ''));

function fixture(responses, stored = {}) {
  const elements = new Map(), requests = [], shown = [], revoked = [];
  const storage = new Map(Object.entries(stored));
  const root = { dataset: {} };
  const element = id => {
    if (!elements.has(id)) elements.set(id, {
      value: 'ABCDEFGHIJ', hidden: false, listeners: {},
      addEventListener(name, handler) { this.listeners[name] = handler; },
      removeAttribute() {},
      set src(value) { if (id === 'screen') shown.push(value); },
    });
    return elements.get(id);
  };
  let clock = 0;
  vm.runInNewContext(source, {
    document: { getElementById: element, documentElement: root, addEventListener() {} },
    window: { addEventListener() {} },
    SECOND_SCREEN_LOCALES: catalogs,
    localStorage: { getItem: key => storage.get(key), setItem: (key, value) => storage.set(key, value) },
    AbortController, Image: class { naturalWidth = 320; naturalHeight = 240; decode() { return Promise.resolve(); } },
    URL: { createObjectURL: () => 'blob:fixture', revokeObjectURL: url => revoked.push(url) },
    performance: { now: () => (clock += 1001) },
    setTimeout: resolve => resolve(),
    fetch: async (path, options) => {
      requests.push({ path, options });
      assert.ok(responses.length, 'Unexpected request');
      const response = responses.shift();
      return typeof response === 'function' ? response({ element, revoked, requests }) : response;
    },
  });
  return { element, requests, shown, revoked, storage, root };
}
const ok = value => ({ ok: true, status: 200, json: async () => value });
const denied = { ok: false, status: 401, text: async () => 'Pairing expired or disconnected' };
test('approval gates frames; token is sent in a header; revoked viewer clears image', async () => {
  const f = fixture([
    ok({ token: 'secret-fixture' }), ok({ approved: false }),
    ok({ approved: true, fps: 10, source: 'Synthetic' }),
    { ok: true, status: 200, blob: async () => ({ size: 1234 }), headers: { get: () => '1' } },
    denied,
  ]);
  await f.element('pair-form').listeners.submit({ preventDefault() {} });
  assert.deepEqual(f.requests.map(r => r.path), ['/api/pair', '/api/state', '/api/state', '/api/frame?after=0', '/api/state']);
  assert.equal(f.requests[3].options.headers.Authorization, 'Bearer secret-fixture');
  assert.ok(f.requests.every(r => !r.path.includes('secret-fixture')));
  assert.deepEqual(f.shown, ['blob:fixture']);
  assert.deepEqual(f.revoked, ['blob:fixture']);
  assert.equal(f.element('display').hidden, true);
  assert.equal(f.element('error').textContent, 'Connection failed: Pairing expired or disconnected');
});

test('language and theme change immediately, persist, and restore without pairing or network', () => {
  const f = fixture([]);
  f.element('language').value = 'ru';
  f.element('language').listeners.change();
  f.element('theme').value = 'light';
  f.element('theme').listeners.change();
  assert.equal(f.root.lang, 'ru');
  assert.equal(f.root.dataset.theme, 'light');
  assert.equal(f.element('status').textContent, 'Отключено');
  assert.equal(f.element('code').placeholder, 'Код из 10 символов');
  assert.equal(f.storage.get('SecondScreen.language'), 'ru');
  assert.equal(f.storage.get('SecondScreen.theme'), 'light');
  assert.equal(f.requests.length, 0);
  const restored = fixture([], Object.fromEntries(f.storage));
  assert.equal(restored.element('language').value, 'ru');
  assert.equal(restored.root.dataset.theme, 'light');
});

test('viewer static labels and runtime messages are covered by all six catalogs', () => {
  const html = fs.readFileSync(require('node:path').join(__dirname, '../viewer/index.html'), 'utf8');
  const keys = [...html.matchAll(/data-i18n="([^"]+)"/g)].map(match => match[1]);
  for (const language of ['en', 'ru', 'fr', 'de', 'es', 'it']) {
    for (const key of keys) assert.ok(catalogs[language][key], `${language}: ${key}`);
  }
  const f = fixture([], { 'SecondScreen.language': 'de' });
  assert.equal(f.element('status').textContent, 'Getrennt');
  f.element('language').value = 'fr';
  f.element('language').listeners.change();
  assert.equal(f.element('status').textContent, 'Déconnecté');
});
test('failed pairing returns to the form without requesting frames', async () => {
  const f = fixture([denied]);
  await f.element('pair-form').listeners.submit({ preventDefault() {} });
  assert.equal(f.requests.length, 1);
  assert.equal(f.element('pairing').hidden, false);
  assert.equal(f.element('connect').disabled, false);
});
test('inactive virtual target clears its old image before another frame is requested', async () => {
  const f = fixture([
    ok({ token: 'virtual-fixture' }),
    ok({ approved: true, fps: 10, source: 'VirtualDisplay', mode: 'Virtual display (experimental)', hasFrame: true }),
    { ok: true, status: 200, blob: async () => ({ size: 1234 }), headers: { get: () => '1' } },
    ok({ approved: true, fps: 10, source: 'VirtualDisplay', mode: 'Virtual display (experimental)', hasFrame: false }),
    ({ element, revoked }) => {
      assert.deepEqual(revoked, ['blob:fixture']);
      assert.match(element('waiting').textContent, /Enable Extend/);
      assert.equal(element('mode').textContent, 'Virtual display (experimental)');
      return { ok: true, status: 204 };
    },
    denied,
  ]);
  await f.element('pair-form').listeners.submit({ preventDefault() {} });
  assert.equal(f.requests[4].path, '/api/frame?after=1');
  assert.equal(f.element('mode').textContent, 'VIEWER');
});
