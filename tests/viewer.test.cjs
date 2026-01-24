const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const source = fs.readFileSync(require('node:path').join(__dirname, '../viewer/viewer.js'), 'utf8');

function fixture(responses) {
  const elements = new Map(), requests = [], shown = [], revoked = [];
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
    document: { getElementById: element, addEventListener() {} },
    window: { addEventListener() {} },
    AbortController, Image: class { naturalWidth = 320; naturalHeight = 240; decode() { return Promise.resolve(); } },
    URL: { createObjectURL: () => 'blob:fixture', revokeObjectURL: url => revoked.push(url) },
    performance: { now: () => (clock += 1001) },
    setTimeout: resolve => resolve(),
    fetch: async (path, options) => {
      requests.push({ path, options });
      assert.ok(responses.length, 'Unexpected request');
      return responses.shift();
    },
  });
  return { element, requests, shown, revoked };
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
  assert.equal(f.element('error').textContent, 'Pairing expired or disconnected');
});
test('failed pairing returns to the form without requesting frames', async () => {
  const f = fixture([denied]);
  await f.element('pair-form').listeners.submit({ preventDefault() {} });
  assert.equal(f.requests.length, 1);
  assert.equal(f.element('pairing').hidden, false);
  assert.equal(f.element('connect').disabled, false);
});
