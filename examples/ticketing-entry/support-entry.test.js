import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import {
  buildPortalFallbackUrl,
  buildSupportContext,
  normalizePortalMessage,
  normalizeResourceReferences,
  parsePortalUrl,
  resolveSameOriginEndpoint,
  sanitizePageUrl,
  validateLaunchUrl,
} from './support-entry.js';

test('page context strips credentials, query and fragment', () => {
  assert.equal(
    sanitizePageUrl('https://person:secret@example.test/orders/42?token=secret#panel'),
    'https://example.test/orders/42',
  );
});

test('portal URL requires HTTPS outside loopback and strips caller state', () => {
  assert.throws(() => parsePortalUrl('http://support.example.test/path'), /HTTPS/);
  assert.equal(parsePortalUrl('http://127.0.0.1:3000/portal?x=1#y').toString(), 'http://127.0.0.1:3000/portal');
});

test('fallback URL contains no page context or handoff credential', () => {
  const value = new URL(buildPortalFallbackUrl('https://support.example.test/portal', 'peopleplanner', 'widget'));
  const fragment = new URLSearchParams(value.hash.slice(1));
  assert.equal(value.search, '');
  assert.deepEqual(Object.fromEntries(fragment), { project: 'peopleplanner', surface: 'widget' });
});

test('launch URL cannot escape the configured support origin', () => {
  assert.equal(
    validateLaunchUrl('https://support.example.test/start#handoff=opaque', 'https://support.example.test/portal'),
    'https://support.example.test/start#handoff=opaque',
  );
  assert.throws(
    () => validateLaunchUrl('https://attacker.example/start#handoff=opaque', 'https://support.example.test/portal'),
    /configured portal origin/,
  );
});

test('browser handoff endpoint is same-origin', () => {
  assert.equal(
    resolveSameOriginEndpoint('/api/support/handoff', 'https://app.example.test/orders/1'),
    'https://app.example.test/api/support/handoff',
  );
  assert.throws(
    () => resolveSameOriginEndpoint('https://support.example.test/handoff', 'https://app.example.test/'),
    /same-origin/,
  );
});

test('support context captures only bounded explicit rich context', () => {
  const browser = {
    location: { href: 'https://app.example.test/orders/1?access_token=secret#notes' },
    document: { title: 'Order 1' },
    navigator: { language: 'en-AU' },
    Intl,
    innerWidth: 390,
    innerHeight: 844,
  };
  const context = buildSupportContext(browser, {
    route_name: 'orders.show',
    selected_text: 'user-confirmed selection',
    ignored_secret: 'must not be copied',
    resource_references: [
      { system: 'peopleplanner', resource_type: 'order', resource_id: 'opaque-1' },
      { system: '', resource_type: 'bad', resource_id: 'bad' },
    ],
  });
  assert.equal(context.page_url, 'https://app.example.test/orders/1');
  assert.equal(context.route_name, 'orders.show');
  assert.equal(context.selected_text, 'user-confirmed selection');
  assert.equal(context.ignored_secret, undefined);
  assert.deepEqual(context.resource_references, [
    { system: 'peopleplanner', resource_type: 'order', resource_id: 'opaque-1' },
  ]);
});

test('resource references are bounded and structurally validated', () => {
  const references = Array.from({ length: 12 }, (_, index) => ({
    system: 'example',
    resource_type: 'record',
    resource_id: `opaque-${index}`,
  }));
  assert.equal(normalizeResourceReferences(references).length, 8);
});

test('postMessage accepts only exact portal origin, frame and schema', () => {
  const frame = {};
  assert.deepEqual(
    normalizePortalMessage(
      { origin: 'https://support.example.test', source: frame, data: { type: 'minco.support.resize', height: 2_000 } },
      frame,
      'https://support.example.test',
    ),
    { type: 'minco.support.resize', height: 900 },
  );
  assert.equal(
    normalizePortalMessage(
      { origin: 'https://attacker.example', source: frame, data: { type: 'minco.support.close' } },
      frame,
      'https://support.example.test',
    ),
    null,
  );
  assert.equal(
    normalizePortalMessage(
      { origin: 'https://support.example.test', source: {}, data: { type: 'minco.support.close' } },
      frame,
      'https://support.example.test',
    ),
    null,
  );
  assert.equal(
    normalizePortalMessage(
      { origin: 'https://support.example.test', source: frame, data: { type: 'unknown' } },
      frame,
      'https://support.example.test',
    ),
    null,
  );
});

test('handoff JSON schema is committed valid JSON with closed request and response shapes', async () => {
  const schema = JSON.parse(await readFile(new URL('./handoff-contract.schema.json', import.meta.url), 'utf8'));
  assert.equal(schema.$schema, 'https://json-schema.org/draft/2020-12/schema');
  assert.equal(schema.$defs.SupportHandoffRequest.additionalProperties, false);
  assert.equal(schema.$defs.SupportHandoffResponse.additionalProperties, false);
  assert.deepEqual(schema.$defs.SupportHandoffRequest.properties.surface.enum, [
    'widget',
    'portal',
    'extension',
    'api',
    'mobile',
  ]);
});
