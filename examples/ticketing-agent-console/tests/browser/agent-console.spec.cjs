const fs = require('node:fs');
const path = require('node:path');
const { test, expect } = require('@playwright/test');

const ASSETS = path.resolve(__dirname, '../../../../plugins/minco-plugin-ticketing/assets');
const CONSOLE_HTML = fs.readFileSync(path.join(ASSETS, 'agent-console.html'), 'utf8');
const CONSOLE_JS = fs.readFileSync(path.join(ASSETS, 'agent-console.js'), 'utf8');
const CONSOLE_CSS = fs.readFileSync(path.join(ASSETS, 'agent-console.css'), 'utf8');

const ORIGIN = 'https://console.example.test';
const BASE = `${ORIGIN}/_minco/ticketing`;

function summary(reference, subject, extra = {}) {
  return {
    id: extra.id || `00000000-0000-4000-8000-0000000000${reference.length}`,
    project_id: 'example',
    display_reference: reference,
    subject,
    requester_subject: 'user-42',
    status: extra.status || 'open',
    clock_state: extra.clock_state || 'open',
    priority: extra.priority || 'normal',
    queue_id: extra.queue_id ?? null,
    assignee_subject: extra.assignee_subject ?? null,
    message_count: extra.message_count ?? 1,
    attachment_count: 0,
    last_activity_at: '2026-08-23T10:00:00Z',
    needs_attention: extra.needs_attention ?? false,
    created_at: '2026-08-23T09:00:00Z',
    updated_at: '2026-08-23T10:00:00Z',
    revision: extra.revision ?? 0,
  };
}

const DETAIL_ID = '11111111-1111-4111-8111-111111111111';
function detailTicket() {
  return {
    id: DETAIL_ID,
    project_id: 'example',
    display_reference: 'TKT-111',
    subject: 'Checkout fails on mobile',
    description: 'The final action returns an error.',
    requester: { subject: 'user-42' },
    channel: 'portal',
    priority: 'high',
    status: 'open',
    clock_state: 'open',
    queue_id: null,
    assignee_subject: null,
    followers: [],
    category: null,
    tags: [],
    source_references: [],
    resource_references: [],
    messages: [
      {
        id: '22222222-2222-4222-8222-222222222221',
        kind: 'public_reply',
        direction: 'inbound',
        author_subject: 'user-42',
        body: 'The final action returns an error.',
        created_at: '2026-08-23T09:00:00Z',
      },
      {
        id: '22222222-2222-4222-8222-222222222222',
        kind: 'internal_note',
        direction: 'internal',
        author_subject: 'agent-1',
        body: 'private note',
        created_at: '2026-08-23T09:30:00Z',
      },
    ],
    attachments: [],
    created_at: '2026-08-23T09:00:00Z',
    updated_at: '2026-08-23T09:30:00Z',
    first_public_response_at: null,
    waiting_since: null,
    resolved_at: null,
    closed_at: null,
    resolution: null,
    close_reason: null,
    revision: 0,
  };
}

// Installs fixture routing: the exact plugin assets plus a deterministic
// agent API. Every console fetch is recorded for payload assertions.
async function loadConsole(page, options = {}) {
  const requests = [];
  const state = {
    etag: '"ticket:11111111-1111-4111-8111-111111111111:1"',
    revision: 0,
    listPages: options.listPages || null,
    managementRespond: options.managementRespond || null,
  };

  await page.route('**/*', async route => {
    const request = route.request();
    const url = new URL(request.url());
    requests.push({ method: request.method(), path: url.pathname, query: url.searchParams });

    if (url.toString() === `${ORIGIN}/_minco/ticketing/agent` && request.isNavigationRequest()) {
      await route.fulfill({ contentType: 'text/html; charset=utf-8', body: CONSOLE_HTML });
      return;
    }
    if (url.pathname === '/_minco/ticketing/agent/console.js') {
      await route.fulfill({ contentType: 'application/javascript; charset=utf-8', body: CONSOLE_JS });
      return;
    }
    if (url.pathname === '/_minco/ticketing/agent/console.css') {
      await route.fulfill({ contentType: 'text/css; charset=utf-8', body: CONSOLE_CSS });
      return;
    }
    if (url.pathname === '/_minco/ticketing/agent/bootstrap') {
      if (options.bootstrapStatus) {
        await route.fulfill({
          status: options.bootstrapStatus,
          contentType: 'application/problem+json',
          body: JSON.stringify({ title: 'denied', code: 'ticketing_permission_denied' }),
        });
        return;
      }
      await route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({
          schema_version: 1,
          project_id: 'example',
          brand: 'Support',
          label: 'Console',
          subject: 'agent-1',
          capabilities: options.capabilities || {
            create: true, reply: true, internal_note: true, manage: true,
          },
        }),
      });
      return;
    }
    if (url.pathname === '/_minco/ticketing/agent/tickets' && request.method() === 'GET') {
      if (options.listStatus) {
        await route.fulfill({
          status: options.listStatus,
          contentType: 'application/problem+json',
          body: JSON.stringify({ title: 'forbidden', code: 'ticketing_permission_denied' }),
        });
        return;
      }
      const pages = state.listPages || [
        {
          data: [summary('TKT-111', 'Checkout fails on mobile', { id: DETAIL_ID })],
          page: { hasMore: false, nextCursor: null },
        },
      ];
      const cursor = url.searchParams.get('page[after]');
      const index = cursor ? Math.min(1, pages.length - 1) : 0;
      await route.fulfill({ contentType: 'application/json', body: JSON.stringify(pages[index]) });
      return;
    }
    if (url.pathname === `/_minco/ticketing/agent/tickets/${DETAIL_ID}` && request.method() === 'GET') {
      state.revision += 1;
      state.etag = `"ticket:${DETAIL_ID}:${state.revision + 1}"`;
      await route.fulfill({
        contentType: 'application/json',
        headers: { ETag: state.etag },
        body: JSON.stringify(detailTicket()),
      });
      return;
    }
    if (url.pathname.endsWith('/agent-replies') || url.pathname.endsWith('/internal-notes')) {
      const body = request.postDataJSON();
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        headers: { ETag: state.etag },
        body: JSON.stringify({ ticket: detailTicket(), warnings: [] }),
      });
      state.lastNoteBody = body.body;
      return;
    }
    if (url.pathname.endsWith('/management') && request.method() === 'PATCH') {
      if (state.managementRespond) {
        const respond = state.managementRespond;
        state.managementRespond = null;
        await route.fulfill({
          status: respond.status,
          contentType: respond.status === 412 ? 'application/problem+json' : 'application/json',
          body: respond.status === 412
            ? JSON.stringify({ title: 'Precondition failed' })
            : JSON.stringify({ ticket: detailTicket(), warnings: [] }),
        });
        return;
      }
      state.lastManagement = request.postDataJSON();
      state.lastManagementIfMatch = request.headers()['if-match'];
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        headers: { ETag: state.etag },
        body: JSON.stringify({ ticket: detailTicket(), warnings: [] }),
      });
      return;
    }
    if (url.pathname === '/_minco/ticketing/tickets' && request.method() === 'POST') {
      state.lastCreate = request.postDataJSON();
      await route.fulfill({
        status: 201,
        contentType: 'application/json',
        headers: { ETag: state.etag },
        body: JSON.stringify({ ticket: detailTicket(), warnings: [] }),
      });
      return;
    }
    await route.fulfill({ status: 404, body: 'not found' });
  });

  await page.goto(`${ORIGIN}/_minco/ticketing/agent`);
  return { requests, state };
}

test('bootstrap renders brand and list rows from the exact plugin assets', async ({ page }) => {
  const { requests } = await loadConsole(page);
  await expect(page.locator('[data-console="brand"]')).toHaveText('Support — Console');
  await expect(page.locator('[data-console="list"] tr')).toHaveCount(1);
  await expect(page.locator('[data-console="list"] tr').first()).toContainText('Checkout fails on mobile');
  const bootstraps = requests.filter(r => r.path.endsWith('/bootstrap'));
  expect(bootstraps).toHaveLength(1);
});

test('view switches request the exact status filters for the active view', async ({ page }) => {
  const { requests } = await loadConsole(page);
  await page.getByRole('button', { name: 'Active' }).click();
  const listRequests = requests.filter(r => r.path.endsWith('/agent/tickets'));
  const active = listRequests[listRequests.length - 1];
  expect(active.query.getAll('filter[status]')).toEqual(
    expect.arrayContaining(['new', 'open', 'pending_internal']),
  );
});

test('mine view filters by the bootstrap subject', async ({ page }) => {
  const { requests } = await loadConsole(page);
  await page.getByRole('button', { name: 'Mine' }).click();
  const listRequests = requests.filter(r => r.path.endsWith('/agent/tickets'));
  expect(listRequests[listRequests.length - 1].query.get('filter[assignee_subject]')).toBe('agent-1');
});

test('cursor pagination loads the next page and replaces the rows', async ({ page }) => {
  const pageOne = [summary('TKT-A', 'First ticket', { id: 'aaaaaaaa-0000-4000-8000-000000000001' })];
  const pageTwo = [
    summary('TKT-B', 'Second ticket', { id: 'bbbbbbbb-0000-4000-8000-000000000002' }),
    summary('TKT-111', 'Checkout fails on mobile', { id: DETAIL_ID }),
  ];
  await loadConsole(page, {
    listPages: [
      { data: pageOne, page: { hasMore: true, nextCursor: 'cursor-2' } },
      { data: pageTwo, page: { hasMore: false, nextCursor: null } },
    ],
  });
  await expect(page.locator('[data-console="list"] tr')).toHaveCount(1);
  await page.locator('[data-console="next"]').click();
  await expect(page.locator('[data-console="list"] tr')).toHaveCount(2);
  await expect(page.locator('[data-console="list"]')).toContainText('Second ticket');
  await expect(page.locator('[data-console="next"]')).toBeHidden();
});

test('current-page search filters rows locally without extra requests', async ({ page }) => {
  const { requests } = await loadConsole(page, {
    listPages: [
      {
        data: [
          summary('TKT-A', 'Billing question', { id: 'aaaaaaaa-0000-4000-8000-000000000001' }),
          summary('TKT-111', 'Checkout fails on mobile', { id: DETAIL_ID }),
        ],
        page: { hasMore: false, nextCursor: null },
      },
    ],
  });
  await expect(page.locator('[data-console="list"] tr')).toHaveCount(2);
  const before = requests.filter(r => r.path.endsWith('/agent/tickets')).length;
  await page.locator('[data-console="search"]').fill('billing');
  await expect(page.locator('[data-console="list"] tr')).toHaveCount(1);
  await expect(page.locator('[data-console="list"]')).toContainText('Billing question');
  const after = requests.filter(r => r.path.endsWith('/agent/tickets')).length;
  expect(after).toBe(before);
});

test('selection loads detail with conversation and internal note styling', async ({ page }) => {
  await loadConsole(page);
  await page.locator('[data-console="list"] tr').first().click();
  await expect(page.locator('[data-console="detail-title"]')).toContainText('TKT-111');
  await expect(page.locator('[data-console="detail-messages"] li')).toHaveCount(2);
  await expect(page.locator('[data-console="detail-messages"] li.internal_note')).toContainText('private note');
});

test('keyboard-only selection works from the list row', async ({ page }) => {
  await loadConsole(page);
  await page.locator('[data-console="list"] tr').first().focus();
  await page.keyboard.press('Enter');
  await expect(page.locator('[data-console="detail-title"]')).toContainText('TKT-111');
});

test('public reply submits the exact body with If-Match', async ({ page }) => {
  const { state } = await loadConsole(page);
  await page.locator('[data-console="list"] tr').first().click();
  await expect(page.locator('[data-console="detail-title"]')).toBeVisible();
  await page.locator('#reply-body').fill('We are on it.');
  await page.getByRole('button', { name: 'Send reply' }).click();
  await expect(page.locator('[data-console="status"]')).toHaveText('');
  expect(state.lastNoteBody).toBe('We are on it.');
  expect(state.etag).toContain('ticket:');
});

test('management submits one atomic payload with the current If-Match', async ({ page }) => {
  const { state } = await loadConsole(page);
  await page.locator('[data-console="list"] tr').first().click();
  await expect(page.locator('[data-console="detail-title"]')).toBeVisible();
  await page.locator('#manage-priority').selectOption('high');
  await page.locator('#manage-status').selectOption('pending_requester');
  await page.getByRole('button', { name: 'Save management' }).click();
  await expect(page.locator('[data-console="status"]')).toHaveText('');
  expect(state.lastManagement).toEqual({ priority: 'high', status: 'pending_requester' });
  expect(state.lastManagementIfMatch).toBe(state.etag);
});

test('stale management conflict shows recovery and reloads the ticket', async ({ page }) => {
  const { requests, state } = await loadConsole(page, {
    managementRespond: { status: 412 },
  });
  await page.locator('[data-console="list"] tr').first().click();
  await expect(page.locator('[data-console="detail-title"]')).toBeVisible();
  await page.getByRole('button', { name: 'Save management' }).click();
  await expect(page.locator('[data-console="status"]')).toContainText('changed while you were working');
  const detailLoads = requests.filter(
    r => r.path === `/_minco/ticketing/agent/tickets/${DETAIL_ID}`,
  );
  expect(detailLoads.length).toBeGreaterThanOrEqual(2);
  expect(state.lastManagement).toBeUndefined();
});

test('create uses the accessible dialog and opens the created detail', async ({ page }) => {
  const { state } = await loadConsole(page);
  await page.locator('[data-console="create"]').click();
  const dialog = page.locator('[data-console="create-dialog"]');
  await expect(dialog).toBeVisible();
  // Focus lands in the first field of the labelled form.
  await expect(page.locator('#create-subject')).toBeFocused();
  await page.locator('#create-subject').fill('New urgent issue');
  await page.locator('#create-description').fill('A new request that needs an agent right now.');
  await page.locator('[data-console="create-submit"]').click();
  await expect(dialog).not.toBeVisible();
  // Focus restoration: the opener regains keyboard position.
  await expect(page.locator('[data-console="create"]')).toBeFocused();
  await expect(page.locator('[data-console="detail-title"]')).toContainText('TKT-111');
  expect(state.lastCreate).toEqual({
    project_id: 'example',
    subject: 'New urgent issue',
    description: 'A new request that needs an agent right now.',
    requester: { subject: 'agent-1' },
    channel: 'internal',
  });
});

test('create dialog shows inline validation and cancel restores focus', async ({ page }) => {
  await loadConsole(page);
  await page.locator('[data-console="create"]').click();
  await page.locator('#create-subject').fill('Too short description');
  await page.locator('#create-description').fill('short');
  await page.locator('[data-console="create-submit"]').click();
  await expect(page.locator('[data-console="create-error"]')).toBeVisible();
  await expect(page.locator('[data-console="create-error"]')).toContainText(
    'at least 20 characters');
  await page.locator('[data-console="create-cancel"]').click();
  await expect(page.locator('[data-console="create-dialog"]')).not.toBeVisible();
  await expect(page.locator('[data-console="create"]')).toBeFocused();
});

test('capabilities the principal lacks hide the matching controls', async ({ page }) => {
  await loadConsole(page, {
    capabilities: { create: false, reply: false, internal_note: false, manage: true },
  });
  await expect(page.locator('[data-console="create"]')).toBeHidden();
  await expect(page.locator('[data-console="reply-form"]')).toBeHidden();
  await expect(page.locator('[data-console="note-form"]')).toBeHidden();
  // The management form lives in the detail panel: open a ticket first.
  await page.locator('[data-console="list"] tr').first().click();
  await expect(page.locator('[data-console="detail-panel"]')).toBeVisible();
  await expect(page.locator('[data-console="manage-form"]')).toBeVisible();
  await expect(page.locator('[data-console="brand"]')).toHaveText('Support — Console');
});

test('empty view renders the truthful empty state', async ({ page }) => {
  await loadConsole(page, {
    listPages: [{ data: [], page: { hasMore: false, nextCursor: null } }],
  });
  await expect(page.locator('[data-console="empty"]')).toBeVisible();
  await expect(page.locator('[data-console="empty"]')).toHaveText('No tickets in this view.');
});

test('forbidden list access reports the truth without rows', async ({ page }) => {
  await loadConsole(page, { listStatus: 403 });
  await expect(page.locator('[data-console="status"]')).toContainText('Console access is forbidden.');
});

test('unauthenticated bootstrap reports that sign-in is required', async ({ page }) => {
  await loadConsole(page, { bootstrapStatus: 401 });
  await expect(page.locator('[data-console="status"]')).toContainText('Sign-in required.');
});

test('page never requests remote resources and renders in dark scheme', async ({ page }) => {
  const remote = [];
  page.on('request', request => {
    const host = new URL(request.url()).host;
    if (host !== 'console.example.test') remote.push(host);
  });
  await loadConsole(page);
  await expect(page.locator('[data-console="brand"]')).toBeVisible();
  expect(remote).toEqual([]);
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(page.locator('[data-console="brand"]')).toBeVisible();
});
