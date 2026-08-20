import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import {
  buildPortalFallbackUrl,
  buildSupportContext,
  issueSupportHandoff,
  navigateReservedTab,
  normalizePortalMessage,
  normalizeResourceReferences,
  parsePortalUrl,
  reserveSupportTab,
  resolveSameOriginEndpoint,
  sanitizePageUrl,
  validateHandoffResponse,
  validateLaunchUrl,
} from './support-entry.js';

const HANDOFF = 'a'.repeat(64);
const HANDOFF_LAUNCH = `https://support.example.test/start#handoff=${HANDOFF}`;

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

test('launch URL is fragment-only, bounded and locked to the configured support origin', () => {
  assert.equal(
    validateLaunchUrl('https://support.example.test/start#handoff=opaque', 'https://support.example.test/portal'),
    'https://support.example.test/start#handoff=opaque',
  );
  assert.throws(
    () => validateLaunchUrl('https://attacker.example/start#handoff=opaque', 'https://support.example.test/portal'),
    /configured portal origin/,
  );
  assert.throws(
    () => validateLaunchUrl('https://support.example.test/start?handoff=opaque', 'https://support.example.test/portal'),
    /query string/,
  );
  assert.throws(
    () => validateLaunchUrl('https://person:secret@support.example.test/start#handoff=opaque', 'https://support.example.test/portal'),
    /user information/,
  );
  assert.throws(
    () => validateLaunchUrl(`https://support.example.test/start#handoff=${'x'.repeat(4_096)}`, 'https://support.example.test/portal'),
    /too long/,
  );
});

test('handoff response is exact, short-lived and uses the launch URL validator', () => {
  const now = Date.parse('2026-08-20T06:30:00Z');
  assert.deepEqual(
    validateHandoffResponse(
      {
        launch_url: HANDOFF_LAUNCH,
        expires_at: '2026-08-20T06:32:00Z',
      },
      'https://support.example.test/',
      now,
    ),
    {
      launch_url: HANDOFF_LAUNCH,
      expires_at: '2026-08-20T06:32:00Z',
    },
  );

  for (const invalid of [
    { launch_url: HANDOFF_LAUNCH },
    {
      launch_url: HANDOFF_LAUNCH,
      expires_at: '2026-08-20T06:32:00Z',
      requester: 'untrusted',
    },
    {
      launch_url: `${HANDOFF_LAUNCH}&next=untrusted`,
      expires_at: '2026-08-20T06:32:00Z',
    },
    {
      launch_url: `https://support.example.test/start#handoff=${'%61'.repeat(64)}`,
      expires_at: '2026-08-20T06:32:00Z',
    },
    {
      launch_url: 'https://support.example.test/start#project=example',
      expires_at: '2026-08-20T06:32:00Z',
    },
    {
      launch_url: `https://support.example.test/start?handoff=${HANDOFF}`,
      expires_at: '2026-08-20T06:32:00Z',
    },
    {
      launch_url: HANDOFF_LAUNCH,
      expires_at: '2026-08-20T06:29:59Z',
    },
    {
      launch_url: HANDOFF_LAUNCH,
      expires_at: '2026-08-20T06:45:00.001Z',
    },
    {
      launch_url: HANDOFF_LAUNCH,
      expires_at: 'August 20, 2026 06:32 UTC',
    },
  ]) {
    assert.throws(() => validateHandoffResponse(invalid, 'https://support.example.test/', now));
  }
});

test('callback and fetch handoff paths share the exact response validator', async () => {
  const baseBrowser = {
    location: { href: 'https://app.example.test/orders/1' },
    document: { querySelector: () => null },
  };
  const options = {
    project: 'example',
    surface: 'widget',
    portal: 'https://support.example.test/',
    endpoint: '/api/support/handoff',
  };
  const invalid = {
    launch_url: HANDOFF_LAUNCH,
    expires_at: '2999-01-01T00:00:00Z',
  };

  await assert.rejects(
    issueSupportHandoff(
      { ...baseBrowser, MincoSupport: { issueHandoff: async () => invalid } },
      options,
      { page_url: baseBrowser.location.href },
    ),
  );
  await assert.rejects(
    issueSupportHandoff(
      {
        ...baseBrowser,
        fetch: async () => ({ ok: true, json: async () => invalid }),
      },
      options,
      { page_url: baseBrowser.location.href },
    ),
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
  assert.equal(context.page_title, undefined);
  assert.equal(context.route_name, 'orders.show');
  assert.equal(context.selected_text, 'user-confirmed selection');
  assert.equal(context.ignored_secret, undefined);
  assert.deepEqual(context.resource_references, [
    { system: 'peopleplanner', resource_type: 'order', resource_id: 'opaque-1' },
  ]);

  assert.equal(buildSupportContext(browser, { page_title: 'Explicit order title' }).page_title, 'Explicit order title');
  assert.equal(buildSupportContext(browser, { locale: 'en\nAU' }).locale, undefined);
  assert.equal(buildSupportContext(browser, { viewport: 'wide' }).viewport, undefined);
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
      { origin: 'https://support.example.test', source: frame, data: { type: 'minco.support.resize', height: 640 } },
      frame,
      'https://support.example.test',
    ),
    { type: 'minco.support.resize', height: 640 },
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
  for (const data of [
    { type: 'minco.support.ready', extra: true },
    { type: 'minco.support.close', url: 'https://attacker.example' },
    { type: 'minco.support.resize' },
    { type: 'minco.support.resize', height: '640' },
    { type: 'minco.support.resize', height: Number.NaN },
    { type: 'minco.support.resize', height: 319 },
    { type: 'minco.support.resize', height: 901 },
    { type: 'minco.support.resize', height: 640.5 },
    { type: 'minco.support.resize', height: 640, html: '<script>bad()</script>' },
    { type: 'minco.support.navigate', url: 'https://attacker.example' },
  ]) {
    assert.equal(
      normalizePortalMessage(
        { origin: 'https://support.example.test', source: frame, data },
        frame,
        'https://support.example.test',
      ),
      null,
    );
  }
});

test('reserved tabs start blank, sever opener and navigate only while open', () => {
  const calls = [];
  const tab = {
    opener: { unsafe: true },
    closed: false,
    location: { replace: (value) => calls.push(value) },
  };
  const browser = {
    open: (...arguments_) => {
      calls.push(arguments_);
      return tab;
    },
  };

  assert.equal(reserveSupportTab(browser), tab);
  assert.deepEqual(calls[0], ['about:blank', '_blank']);
  assert.equal(tab.opener, null);
  assert.equal(
    navigateReservedTab(
      tab,
      'https://support.example.test/start#handoff=opaque',
      'https://support.example.test/',
    ),
    true,
  );
  assert.equal(calls[1], 'https://support.example.test/start#handoff=opaque');
  tab.closed = true;
  assert.equal(
    navigateReservedTab(tab, 'https://support.example.test/fallback', 'https://support.example.test/'),
    false,
  );
  assert.equal(reserveSupportTab({ open: () => null }), null);
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
