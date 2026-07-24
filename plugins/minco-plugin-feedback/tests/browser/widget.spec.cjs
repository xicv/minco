const fs = require('node:fs');
const path = require('node:path');
const { test, expect } = require('@playwright/test');

const widgetSource = fs.readFileSync(
  path.resolve(__dirname, '../../assets/widget.js'),
  'utf8',
);

const defaultConfig = Object.freeze({
  enabled: true,
  project_id: 'orders-review',
  label: 'Share feedback',
  position: 'bottom-right',
  offset_x_px: 24,
  offset_y_px: 24,
  theme: 'light',
  token_storage: 'session',
  screenshot_enabled: true,
  voice_enabled: true,
  transcription_enabled: false,
  max_http_body_bytes: 7 * 1024 * 1024,
  max_screenshot_bytes: 4 * 1024 * 1024,
  max_audio_bytes: 2 * 1024 * 1024,
  max_file_bytes: 2 * 1024 * 1024,
  max_attachments: 3,
  max_recording_seconds: 90,
  include_url_query: false,
  redact_query_parameters: ['token', 'access_token', 'password'],
  poll_interval_ms: 60_000,
  privacy_notice: 'Only share information that is safe for the review team.',
});

function multipartPayload(request) {
  const body = request.postDataBuffer()?.toString('utf8') || '';
  const match = body.match(
    /name="payload"\r\n(?:Content-Type: [^\r]+\r\n)?\r\n([\s\S]*?)\r\n--/,
  );
  if (!match) throw new Error('feedback payload field was absent from multipart body');
  return JSON.parse(match[1]);
}

function multipartFieldNames(request) {
  const body = request.postDataBuffer()?.toString('utf8') || '';
  return [...body.matchAll(/;\sname="([^"]+)"/g)].map(match => match[1]);
}

async function installSuccessfulMediaMocks(
  page,
  { microphone = false, screen = false } = {},
) {
  await page.addInitScript(
    ({ enableMicrophone, enableScreen }) => {
      window.__mincoMedia = {
        displayRequests: 0,
        microphoneRequests: 0,
        microphoneTrackStops: 0,
        recorderStarts: 0,
        recorderStops: 0,
        screenTrackStops: 0,
      };

      const mediaDevices = {};
      if (enableScreen) {
        mediaDevices.getDisplayMedia = async () => {
          window.__mincoMedia.displayRequests += 1;
          return {
            getTracks: () => [
              {
                stop() {
                  window.__mincoMedia.screenTrackStops += 1;
                },
              },
            ],
          };
        };

        const createElement = Document.prototype.createElement;
        Document.prototype.createElement = function createMediaElement(tagName, options) {
          const element = createElement.call(this, tagName, options);
          if (String(tagName).toLowerCase() === 'video') {
            Object.defineProperties(element, {
              srcObject: { configurable: true, writable: true, value: null },
              videoHeight: { configurable: true, value: 600 },
              videoWidth: { configurable: true, value: 800 },
            });
            element.play = async () => {};
          }
          if (String(tagName).toLowerCase() === 'canvas') {
            element.getContext = () => ({ drawImage() {} });
            element.toBlob = callback => {
              callback(new Blob(['test-image'], { type: 'image/webp' }));
            };
          }
          return element;
        };
      }

      if (enableMicrophone) {
        mediaDevices.getUserMedia = async () => {
          window.__mincoMedia.microphoneRequests += 1;
          return {
            getTracks: () => [
              {
                stop() {
                  window.__mincoMedia.microphoneTrackStops += 1;
                },
              },
            ],
          };
        };

        class TestMediaRecorder extends EventTarget {
          static isTypeSupported(type) {
            return type.startsWith('audio/webm');
          }

          constructor(_stream, options = {}) {
            super();
            this.mimeType = options.mimeType || 'audio/webm';
            this.state = 'inactive';
          }

          start() {
            this.state = 'recording';
            window.__mincoMedia.recorderStarts += 1;
          }

          stop() {
            this.state = 'inactive';
            window.__mincoMedia.recorderStops += 1;
            const dataEvent = new Event('dataavailable');
            Object.defineProperty(dataEvent, 'data', {
              value: new Blob(['test-voice'], { type: this.mimeType }),
            });
            this.dispatchEvent(dataEvent);
            this.dispatchEvent(new Event('stop'));
          }
        }

        Object.defineProperty(window, 'MediaRecorder', {
          configurable: true,
          value: TestMediaRecorder,
        });
      }

      Object.defineProperty(navigator, 'mediaDevices', {
        configurable: true,
        value: mediaDevices,
      });
    },
    {
      enableMicrophone: microphone,
      enableScreen: screen,
    },
  );
}

async function loadWidget(
  page,
  {
    config = {},
    hostCss = '',
    initialThread = {},
    url = 'https://widget.test/review?token=secret#account',
    scriptAttributes = {},
    submissionResponse = null,
    transcriptionResponse = null,
  } = {},
) {
  const runtime = {
    config: { ...defaultConfig, ...config },
    requests: [],
    submissions: [],
    replies: [],
    transcriptions: [],
    clientToken: '00000000-0000-0000-0000-000000000001',
    thread: {
      id: '00000000-0000-0000-0000-000000000002',
      title: 'Checkout total is unclear',
      description: 'The tax-inclusive total is difficult to find.',
      status: 'new',
      messages: [],
      ...initialThread,
    },
  };

  const dataAttributes = {
    endpoint: '/api',
    projectKey: 'review-project-key',
    environment: 'review',
    release: '2026-07-24.abc123',
    route: 'checkout',
    requestId: 'request-browser-test',
    ...scriptAttributes,
  };
  const serializedAttributes = Object.entries(dataAttributes)
    .map(([name, value]) => {
      const dataName = name.replace(/[A-Z]/g, letter => `-${letter.toLowerCase()}`);
      return `data-${dataName}="${String(value)}"`;
    })
    .join(' ');

  await page.route(/^https:\/\/widget\.test\//, async route => {
    const request = route.request();
    const requestUrl = new URL(request.url());
    runtime.requests.push({
      method: request.method(),
      pathname: requestUrl.pathname,
      headers: request.headers(),
    });

    if (request.isNavigationRequest()) {
      await route.fulfill({
        contentType: 'text/html',
        body: `<!doctype html>
          <html data-environment="document-environment">
            <head>
              <title>Widget browser fixture</title>
              <style>${hostCss}</style>
            </head>
            <body data-route="document-route">
              <a href="#outside">Outside link</a>
              <button id="host-button" type="button">Host button</button>
              <main><h1>Review checkout</h1></main>
              <script src="/widget.js" ${serializedAttributes}></script>
            </body>
          </html>`,
      });
      return;
    }

    if (requestUrl.pathname === '/widget.js') {
      await route.fulfill({
        contentType: 'application/javascript; charset=utf-8',
        body: widgetSource,
      });
      return;
    }

    if (requestUrl.pathname === '/api/widget-config') {
      await route.fulfill({ json: runtime.config });
      return;
    }

    if (requestUrl.pathname === '/api/threads' && request.method() === 'POST') {
      const payload = multipartPayload(request);
      runtime.submissions.push({
        payload,
        fields: multipartFieldNames(request),
        headers: request.headers(),
      });
      if (submissionResponse) {
        await route.fulfill(submissionResponse);
        return;
      }
      runtime.thread = {
        ...runtime.thread,
        title: payload.title,
        description: payload.description,
      };
      await route.fulfill({
        status: 201,
        json: {
          thread: runtime.thread,
          client_token: runtime.clientToken,
          warnings: [],
        },
      });
      return;
    }

    if (
      requestUrl.pathname === '/api/transcriptions'
      && request.method() === 'POST'
    ) {
      runtime.transcriptions.push({
        fields: multipartFieldNames(request),
        headers: request.headers(),
      });
      if (transcriptionResponse) {
        await route.fulfill(transcriptionResponse);
      } else {
        await route.fulfill({ json: { text: 'Recorded checkout feedback.' } });
      }
      return;
    }

    if (
      requestUrl.pathname === `/api/threads/${runtime.thread.id}/messages`
      && request.method() === 'POST'
    ) {
      const body = request.postDataJSON();
      runtime.replies.push({
        body,
        headers: request.headers(),
      });
      runtime.thread.messages.push({
        id: `message-${runtime.thread.messages.length + 1}`,
        author_role: 'client',
        source: 'text',
        body: body.body,
        visible_to_client: true,
        created_at: '2026-07-24T08:30:00Z',
      });
      await route.fulfill({
        json: {
          thread: runtime.thread,
          warnings: [],
        },
      });
      return;
    }

    if (
      requestUrl.pathname === `/api/threads/${runtime.thread.id}`
      && request.method() === 'GET'
    ) {
      await route.fulfill({ json: runtime.thread });
      return;
    }

    await route.fulfill({
      status: 404,
      json: { title: 'Not found', detail: requestUrl.pathname },
    });
  });

  await page.goto(url);
  const widget = page.locator('minco-feedback');
  await expect(widget.getByRole('button', { name: runtime.config.label })).toBeVisible();
  return { runtime, widget };
}

async function submitFeedback(
  dialog,
  {
    description = 'The checkout needs attention.',
    title = 'Checkout feedback',
  } = {},
) {
  await dialog
    .getByRole('textbox', { name: 'Title', exact: true })
    .fill(title);
  await dialog
    .getByRole('textbox', { name: 'Feedback', exact: true })
    .fill(description);
  await dialog.getByRole('button', { name: 'Submit feedback' }).click();
}

for (const position of [
  'top-left',
  'top-right',
  'bottom-left',
  'bottom-right',
]) {
  test(`places the launcher at ${position}`, async ({ page }) => {
    const { widget } = await loadWidget(page, {
      config: { position },
    });
    const placement = await widget
      .getByRole('button', { name: 'Share feedback' })
      .evaluate(element => ({
        bottom: element.style.bottom,
        left: element.style.left,
        right: element.style.right,
        top: element.style.top,
      }));
    const [vertical, horizontal] = position.split('-');
    expect(placement[vertical]).toBe('24px');
    expect(placement[horizontal]).toBe('24px');
    expect(placement[vertical === 'top' ? 'bottom' : 'top']).toBe('');
    expect(placement[horizontal === 'left' ? 'right' : 'left']).toBe('');
  });
}

test('honors reduced motion and isolates Shadow DOM styles from the host page', async ({
  page,
}) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  const { widget } = await loadWidget(page, {
    hostCss: `
      button { display: none !important; font-size: 1px !important; transition: all 30s !important; }
      h1 { color: rgb(255, 0, 0); }
    `,
  });
  const launcher = widget.getByRole('button', { name: 'Share feedback' });
  await expect(page.locator('#host-button')).toBeHidden();
  await expect(launcher).toBeVisible();

  const styles = await launcher.evaluate(element => {
    const computed = getComputedStyle(element);
    return {
      display: computed.display,
      fontSize: computed.fontSize,
      transitionDuration: computed.transitionDuration,
      usesOpenShadowRoot: Boolean(element.getRootNode().host?.shadowRoot),
    };
  });
  expect(styles).toMatchObject({
    display: 'block',
    fontSize: '22px',
    transitionDuration: '0s',
    usesOpenShadowRoot: true,
  });
});

test('opens and closes from keyboard with labelled dialog and restored focus', async ({
  page,
}, testInfo) => {
  const { widget } = await loadWidget(page);
  const launcher = widget.getByRole('button', { name: 'Share feedback' });

  await launcher.click();
  const dialog = widget.getByRole('dialog', { name: 'Share feedback' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('heading', { name: 'Share feedback', level: 2 })).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Close feedback' })).toBeVisible();
  await expect(
    dialog.getByRole('combobox', { name: 'Type', exact: true }),
  ).toHaveValue('bug');
  await expect(
    dialog.getByRole('textbox', { name: 'Title', exact: true }),
  ).toBeFocused();
  await expect(
    dialog.getByRole('textbox', { name: 'Feedback', exact: true }),
  ).toBeEditable();
  await expect(dialog.getByRole('status')).toHaveAttribute('aria-live', 'polite');

  await page.keyboard.press('Tab');
  await expect(
    dialog.getByRole('textbox', { name: 'Feedback', exact: true }),
  ).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(
    dialog.getByRole('textbox', { name: 'Title', exact: true }),
  ).toBeFocused();

  const submit = dialog.getByRole('button', { name: 'Submit feedback' });
  await submit.focus();
  await page.keyboard.press('Tab');
  await expect(
    dialog.getByRole('button', { name: 'Close feedback' }),
  ).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(submit).toBeFocused();

  await page.screenshot({
    path: testInfo.outputPath('feedback-dialog.png'),
    fullPage: true,
  });

  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(launcher).toBeFocused();

  await launcher.click();
  await widget.getByRole('button', { name: 'Close feedback' }).click();
  await expect(widget.getByRole('dialog')).toHaveCount(0);
  await expect(launcher).toBeFocused();
});

test('submits feedback, defaults to tab-scoped tokens, and resumes the conversation', async ({
  page,
}) => {
  const { runtime, widget } = await loadWidget(page);
  await widget.getByRole('button', { name: 'Share feedback' }).click();
  const dialog = widget.getByRole('dialog');

  await dialog
    .getByRole('combobox', { name: 'Type', exact: true })
    .selectOption('usability');
  await dialog
    .getByRole('textbox', { name: 'Title', exact: true })
    .fill('Payment total needs stronger emphasis');
  await dialog
    .getByRole('textbox', { name: 'Feedback', exact: true })
    .fill('The total is visually lost between the subtotal and tax rows.');
  await dialog.getByRole('button', { name: 'Submit feedback' }).click();

  await expect(dialog.getByText('Payment total needs stronger emphasis · new')).toBeVisible();
  await expect(dialog.getByText('The total is visually lost between the subtotal and tax rows.')).toBeVisible();
  expect(runtime.submissions).toHaveLength(1);
  expect(runtime.submissions[0].payload).toMatchObject({
    project_id: 'orders-review',
    kind: 'usability',
    priority: 'normal',
    title: 'Payment total needs stronger emphasis',
    context: {
      page_url: 'https://widget.test/review',
      route_name: 'checkout',
      release_id: '2026-07-24.abc123',
      environment: 'review',
      request_id: 'request-browser-test',
    },
  });
  expect(runtime.submissions[0].headers['x-minco-feedback-project-key']).toBe(
    'review-project-key',
  );

  const browserState = await page.evaluate(() => ({
    session: sessionStorage.getItem(
      'minco-feedback:orders-review:https://widget.test',
    ),
    localLength: localStorage.length,
  }));
  expect(browserState.localLength).toBe(0);
  expect(JSON.parse(browserState.session)).toMatchObject({
    version: 1,
    active_id: runtime.thread.id,
    threads: [
      {
        id: runtime.thread.id,
        token: runtime.clientToken,
        title: 'Payment total needs stronger emphasis',
      },
    ],
  });

  await page.reload();
  const reloadedWidget = page.locator('minco-feedback');
  await reloadedWidget.getByRole('button', { name: 'Share feedback' }).click();
  const resumedDialog = reloadedWidget.getByRole('dialog');
  await expect(
    resumedDialog.getByText('Payment total needs stronger emphasis · new'),
  ).toBeVisible();
  await expect(
    resumedDialog.getByPlaceholder('Reply to the development team'),
  ).toBeFocused();

  const threadGet = runtime.requests
    .filter(request => request.pathname === `/api/threads/${runtime.thread.id}`)
    .at(-1);
  expect(threadGet.headers['x-minco-feedback-token']).toBe(runtime.clientToken);

  await resumedDialog
    .getByPlaceholder('Reply to the development team')
    .fill('A bold final-total row would solve it.');
  await resumedDialog.getByRole('button', { name: 'Send reply' }).click();
  await expect(resumedDialog.getByText('A bold final-total row would solve it.')).toBeVisible();
  expect(runtime.replies).toHaveLength(1);
  expect(runtime.replies[0].headers['x-minco-feedback-token']).toBe(runtime.clientToken);
  expect(runtime.replies[0].body).toEqual({
    body: 'A bold final-total row would solve it.',
  });
});

test('supports an explicit localStorage token policy from the embed', async ({
  page,
}) => {
  const { runtime, widget } = await loadWidget(page, {
    scriptAttributes: { tokenStorage: 'local' },
  });
  await widget.getByRole('button', { name: 'Share feedback' }).click();
  const dialog = widget.getByRole('dialog');
  await submitFeedback(dialog, { title: 'Persistent review thread' });
  await expect(dialog.getByText('Persistent review thread · new')).toBeVisible();

  const browserState = await page.evaluate(() => ({
    local: localStorage.getItem(
      'minco-feedback:orders-review:https://widget.test',
    ),
    sessionLength: sessionStorage.length,
  }));
  expect(browserState.sessionLength).toBe(0);
  expect(JSON.parse(browserState.local)).toMatchObject({
    active_id: runtime.thread.id,
    threads: [
      {
        id: runtime.thread.id,
        token: runtime.clientToken,
      },
    ],
  });
});

test('shows developer clarification, hides private notes, and sends a client reply', async ({
  page,
}) => {
  const { runtime, widget } = await loadWidget(page);
  await widget.getByRole('button', { name: 'Share feedback' }).click();
  let dialog = widget.getByRole('dialog');
  await submitFeedback(dialog, { title: 'Clarify the payment state' });
  await expect(dialog.getByText('Clarify the payment state · new')).toBeVisible();

  runtime.thread.status = 'needs_clarification';
  runtime.thread.messages.push(
    {
      id: 'message-visible',
      author_role: 'developer',
      source: 'text',
      body: 'Does this happen before or after payment confirmation?',
      visible_to_client: true,
      created_at: '2026-07-24T08:35:00Z',
    },
    {
      id: 'message-private',
      author_role: 'developer',
      source: 'text',
      body: 'Private triage note: check the payment provider logs.',
      visible_to_client: false,
      created_at: '2026-07-24T08:36:00Z',
    },
  );

  await widget.getByRole('button', { name: 'Close feedback' }).click();
  await widget.getByRole('button', { name: 'Share feedback' }).click();
  dialog = widget.getByRole('dialog');
  await expect(
    dialog.getByText('Clarify the payment state · needs clarification'),
  ).toBeVisible();
  await expect(
    dialog.getByText('Does this happen before or after payment confirmation?'),
  ).toBeVisible();
  await expect(
    dialog.getByText('Private triage note: check the payment provider logs.'),
  ).toHaveCount(0);

  await dialog
    .getByPlaceholder('Reply to the development team')
    .fill('It happens immediately after confirmation.');
  await dialog.getByRole('button', { name: 'Send reply' }).click();
  await expect(
    dialog.getByText('It happens immediately after confirmation.'),
  ).toBeVisible();
  expect(runtime.replies.at(-1)).toMatchObject({
    body: { body: 'It happens immediately after confirmation.' },
    headers: { 'x-minco-feedback-token': runtime.clientToken },
  });
  expect(
    runtime.requests.some(request =>
      request.pathname.startsWith('/api/developer/'),
    ),
  ).toBe(false);
});

test('redacts configured URL query secrets when query capture is enabled', async ({
  page,
}) => {
  const { runtime, widget } = await loadWidget(page, {
    config: {
      include_url_query: true,
      redact_query_parameters: ['access_token', 'PASSWORD'],
    },
    url: 'https://widget.test/review?share=order-123&access_token=s3cr3t&Password=hunter2#billing',
  });
  await widget.getByRole('button', { name: 'Share feedback' }).click();
  const dialog = widget.getByRole('dialog');
  await dialog
    .getByRole('textbox', { name: 'Title', exact: true })
    .fill('Redaction check');
  await dialog
    .getByRole('textbox', { name: 'Feedback', exact: true })
    .fill('Validate the captured browser context.');
  await dialog.getByRole('button', { name: 'Submit feedback' }).click();
  await expect(dialog.getByText('Redaction check · new')).toBeVisible();

  const capturedUrl = new URL(runtime.submissions[0].payload.context.page_url);
  expect(capturedUrl.hash).toBe('');
  expect(capturedUrl.searchParams.get('share')).toBe('order-123');
  expect(capturedUrl.searchParams.get('access_token')).toBe('[REDACTED]');
  expect(capturedUrl.searchParams.get('Password')).toBe('[REDACTED]');
  expect(capturedUrl.toString()).not.toContain('s3cr3t');
  expect(capturedUrl.toString()).not.toContain('hunter2');
});

test('captures a screenshot only after an explicit click and stops its track', async ({
  page,
}) => {
  await installSuccessfulMediaMocks(page, { screen: true });
  const { widget } = await loadWidget(page);
  await widget.getByRole('button', { name: 'Share feedback' }).click();
  const dialog = widget.getByRole('dialog');

  await expect(dialog.locator('.attachment')).toHaveCount(0);
  expect(
    await page.evaluate(() => window.__mincoMedia.displayRequests),
  ).toBe(0);
  await dialog.getByRole('button', { name: 'Capture screenshot' }).click();
  await expect(dialog.getByRole('status')).toHaveText('Screenshot attached.');
  await expect(dialog.locator('.attachment')).toContainText(/feedback-\d+\.webp/);
  await expect
    .poll(() => page.evaluate(() => window.__mincoMedia))
    .toMatchObject({
      displayRequests: 1,
      screenTrackStops: 1,
    });
});

test('records and stops a voice note without transcription when disabled', async ({
  page,
}) => {
  await installSuccessfulMediaMocks(page, { microphone: true });
  const { runtime, widget } = await loadWidget(page, {
    config: { transcription_enabled: false },
  });
  await widget.getByRole('button', { name: 'Share feedback' }).click();
  const dialog = widget.getByRole('dialog');

  await expect(dialog.locator('.attachment')).toHaveCount(0);
  expect(
    await page.evaluate(() => window.__mincoMedia.microphoneRequests),
  ).toBe(0);
  await dialog.getByRole('button', { name: 'Record voice' }).click();
  await expect(dialog.getByRole('button', { name: 'Stop recording' })).toBeVisible();
  await expect(dialog.getByRole('status')).toContainText('Recording voice note');
  await dialog.getByRole('button', { name: 'Stop recording' }).click();
  await expect(dialog.getByRole('button', { name: 'Record voice' })).toBeVisible();
  await expect(dialog.getByRole('status')).toHaveText('Voice note attached.');
  await expect(dialog.locator('.attachment')).toContainText('Voice note');
  expect(runtime.transcriptions).toHaveLength(0);
  await expect
    .poll(() => page.evaluate(() => window.__mincoMedia))
    .toMatchObject({
      microphoneRequests: 1,
      microphoneTrackStops: 1,
      recorderStarts: 1,
      recorderStops: 1,
    });
});

test('transcribes a voice note when the capability is enabled', async ({
  page,
}) => {
  await installSuccessfulMediaMocks(page, { microphone: true });
  const { runtime, widget } = await loadWidget(page, {
    config: { transcription_enabled: true },
  });
  await widget.getByRole('button', { name: 'Share feedback' }).click();
  const dialog = widget.getByRole('dialog');

  await dialog.getByRole('button', { name: 'Record voice' }).click();
  await dialog.getByRole('button', { name: 'Stop recording' }).click();
  await expect(dialog.getByRole('status')).toHaveText(
    'Voice note transcribed and attached.',
  );
  await expect(
    dialog.getByRole('textbox', { name: 'Feedback', exact: true }),
  ).toHaveValue('Recorded checkout feedback.');
  expect(runtime.transcriptions).toHaveLength(1);
  expect(runtime.transcriptions[0]).toMatchObject({
    fields: ['audio'],
    headers: {
      'x-minco-feedback-project-key': 'review-project-key',
    },
  });
});

test('keeps a voice attachment when the transcription provider fails', async ({
  page,
}) => {
  await installSuccessfulMediaMocks(page, { microphone: true });
  const { runtime, widget } = await loadWidget(page, {
    config: { transcription_enabled: true },
    transcriptionResponse: {
      status: 502,
      json: {
        title: 'Voice transcription failed',
        detail: 'The transcription provider did not complete the request.',
      },
    },
  });
  await widget.getByRole('button', { name: 'Share feedback' }).click();
  const dialog = widget.getByRole('dialog');

  await dialog.getByRole('button', { name: 'Record voice' }).click();
  await dialog.getByRole('button', { name: 'Stop recording' }).click();
  await expect(dialog.getByRole('status')).toContainText(
    'Voice note attached, but transcription failed: 502:',
  );
  await expect(dialog.locator('.attachment')).toContainText('Voice note');
  await expect(
    dialog.getByRole('textbox', { name: 'Feedback', exact: true }),
  ).toHaveValue('');
  expect(runtime.transcriptions).toHaveLength(1);
});

test('offers safe fallbacks when screenshot and voice browser APIs are unavailable', async ({
  page,
}) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'mediaDevices', {
      configurable: true,
      value: {},
    });
    Object.defineProperty(window, 'MediaRecorder', {
      configurable: true,
      value: undefined,
    });
  });
  const { widget } = await loadWidget(page);
  await widget.getByRole('button', { name: 'Share feedback' }).click();
  const dialog = widget.getByRole('dialog');

  await expect(dialog.getByRole('button', { name: 'Choose image' })).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Record voice' })).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Capture screenshot' }).click();
  await expect(dialog.getByRole('status')).toHaveText(
    'Screen capture is not supported by this browser. Choose an image instead.',
  );
  await expect(dialog.getByRole('status')).toHaveClass(/error/);
});

test('reports browser permission denials without creating media attachments', async ({
  page,
}) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'mediaDevices', {
      configurable: true,
      value: {
        getDisplayMedia: async () => {
          throw new DOMException('Screen permission denied', 'NotAllowedError');
        },
        getUserMedia: async () => {
          throw new DOMException('Microphone permission denied', 'NotAllowedError');
        },
      },
    });
    Object.defineProperty(window, 'MediaRecorder', {
      configurable: true,
      value: function TestMediaRecorder() {},
    });
  });
  const { widget } = await loadWidget(page);
  await widget.getByRole('button', { name: 'Share feedback' }).click();
  const dialog = widget.getByRole('dialog');

  await dialog.getByRole('button', { name: 'Capture screenshot' }).click();
  await expect(dialog.getByRole('status')).toHaveText(
    'Screenshot capture was cancelled.',
  );
  await expect(dialog.getByRole('status')).not.toHaveClass(/error/);

  await dialog.getByRole('button', { name: 'Record voice' }).click();
  await expect(dialog.getByRole('status')).toHaveText(
    'Microphone unavailable: Microphone permission denied',
  );
  await expect(dialog.getByRole('status')).toHaveClass(/error/);
  await expect(dialog.locator('.attachment')).toHaveCount(0);
});

test('rejects attachments beyond the configured count before submission', async ({
  page,
}) => {
  const { runtime, widget } = await loadWidget(page, {
    config: { max_attachments: 1 },
  });
  await widget.getByRole('button', { name: 'Share feedback' }).click();
  const dialog = widget.getByRole('dialog');
  const fileInput = dialog.locator('input[type="file"]:not([accept])');

  await fileInput.setInputFiles([
    {
      name: 'first.txt',
      mimeType: 'text/plain',
      buffer: Buffer.from('first'),
    },
    {
      name: 'second.txt',
      mimeType: 'text/plain',
      buffer: Buffer.from('second'),
    },
  ]);
  await expect(dialog.locator('.attachment')).toHaveCount(1);
  await expect(dialog.locator('.attachment')).toContainText('first.txt');
  await expect(dialog.getByRole('status')).toHaveText(
    'No more than 1 attachments are allowed.',
  );
  expect(runtime.submissions).toHaveLength(0);
});

test('rejects an oversized attachment before submission', async ({ page }) => {
  const { runtime, widget } = await loadWidget(page, {
    config: { max_file_bytes: 4 },
  });
  await widget.getByRole('button', { name: 'Share feedback' }).click();
  const dialog = widget.getByRole('dialog');

  await dialog
    .locator('input[type="file"]:not([accept])')
    .setInputFiles({
      name: 'oversized.txt',
      mimeType: 'text/plain',
      buffer: Buffer.from('12345'),
    });
  await expect(dialog.locator('.attachment')).toHaveCount(0);
  await expect(dialog.getByRole('status')).toHaveText(
    'oversized.txt exceeds the configured attachment limit.',
  );
  expect(runtime.submissions).toHaveLength(0);
});

test('surfaces screenshot type rejection from the API without persisting a token', async ({
  page,
}) => {
  const { runtime, widget } = await loadWidget(page, {
    submissionResponse: {
      status: 422,
      json: {
        title: 'Invalid attachment',
        detail: 'screenshot content type must be image/*',
      },
    },
  });
  await widget.getByRole('button', { name: 'Share feedback' }).click();
  const dialog = widget.getByRole('dialog');
  await dialog.locator('input[type="file"][accept="image/*"]').setInputFiles({
    name: 'not-an-image.pdf',
    mimeType: 'application/pdf',
    buffer: Buffer.from('pdf'),
  });
  await submitFeedback(dialog, { title: 'Screenshot type validation' });

  await expect(dialog.getByRole('status')).toHaveText(
    '422: screenshot content type must be image/*',
  );
  await expect(dialog.getByRole('status')).toHaveClass(/error/);
  expect(runtime.submissions).toHaveLength(1);
  expect(runtime.submissions[0].fields).toEqual(['payload', 'screenshot']);
  expect(
    await page.evaluate(
      () =>
        sessionStorage.getItem(
          'minco-feedback:orders-review:https://widget.test',
        ),
    ),
  ).toBeNull();
});
