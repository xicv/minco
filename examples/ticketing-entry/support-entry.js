const POSITIONS = new Set(['bottom-left', 'bottom-right', 'top-left', 'top-right']);
const SURFACES = new Set(['widget', 'portal', 'extension', 'api', 'mobile']);
const TARGETS = new Set(['modal', 'tab']);
const MAX_REFERENCES = 8;
const MAX_LAUNCH_URL_LENGTH = 4_096;
const MAX_HANDOFF_TTL_MS = 15 * 60 * 1_000;
const RFC3339_DATE_TIME = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/u;
const CONTEXT_KEYS = new Set([
  'page_title',
  'route_name',
  'release_id',
  'request_id',
  'locale',
  'timezone',
  'viewport',
  'selected_text',
]);
const CONTEXT_LIMITS = new Map([
  ['page_title', 2_000],
  ['route_name', 2_000],
  ['release_id', 2_000],
  ['request_id', 2_000],
  ['locale', 40],
  ['timezone', 100],
  ['viewport', 32],
  ['selected_text', 2_000],
]);

function plainObject(value) {
  if (!value || typeof value !== 'object') return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function text(value, maximum = 2_000) {
  if (value === undefined || value === null) return undefined;
  const normalized = String(value).trim();
  if (!normalized || /[\u0000-\u001F\u007F]/u.test(normalized)) return undefined;
  return [...normalized].slice(0, maximum).join('');
}

function localHost(hostname) {
  return hostname === 'localhost' || hostname === '127.0.0.1' || hostname === '[::1]';
}

export function parsePortalUrl(value) {
  const portal = new URL(String(value));
  if (portal.username || portal.password) throw new TypeError('Portal URL must not contain user information.');
  if (portal.protocol !== 'https:' && !(portal.protocol === 'http:' && localHost(portal.hostname))) {
    throw new TypeError('Portal URL must use HTTPS outside local development.');
  }
  portal.search = '';
  portal.hash = '';
  return portal;
}

export function sanitizePageUrl(value) {
  const page = new URL(String(value));
  if (page.protocol !== 'https:' && !(page.protocol === 'http:' && localHost(page.hostname))) {
    throw new TypeError('Page URL must use HTTPS outside local development.');
  }
  page.username = '';
  page.password = '';
  page.search = '';
  page.hash = '';
  const sanitized = page.toString();
  if ([...sanitized].length > MAX_LAUNCH_URL_LENGTH) throw new TypeError('Page URL is too long.');
  return sanitized;
}

export function normalizeResourceReferences(value) {
  if (!Array.isArray(value)) return [];
  return value.slice(0, MAX_REFERENCES).flatMap((reference) => {
    if (!plainObject(reference)) return [];
    const system = text(reference.system, 100);
    const resourceType = text(reference.resource_type, 100);
    const resourceId = text(reference.resource_id, 300);
    return system && resourceType && resourceId
      ? [{ system, resource_type: resourceType, resource_id: resourceId }]
      : [];
  });
}

export function buildSupportContext(browser, supplied = {}) {
  if (!browser?.location?.href) throw new TypeError('Browser location is required.');
  const input = plainObject(supplied) ? supplied : {};
  const context = {
    page_url: sanitizePageUrl(browser.location.href),
    locale: text(browser.navigator?.language, 40),
    timezone: text(browser.Intl?.DateTimeFormat?.().resolvedOptions?.().timeZone, 100),
    viewport:
      Number.isFinite(browser.innerWidth) && Number.isFinite(browser.innerHeight)
        ? text(`${Math.max(0, Math.trunc(browser.innerWidth))}x${Math.max(0, Math.trunc(browser.innerHeight))}`, 32)
        : undefined,
  };
  for (const key of CONTEXT_KEYS) {
    if (key in input) context[key] = text(input[key], CONTEXT_LIMITS.get(key));
  }
  if (context.viewport && !/^[0-9]{1,6}x[0-9]{1,6}$/u.test(context.viewport)) delete context.viewport;
  context.resource_references = normalizeResourceReferences(input.resource_references);
  return Object.fromEntries(
    Object.entries(context).filter(([, value]) => value !== undefined && !(Array.isArray(value) && value.length === 0)),
  );
}

export function buildPortalFallbackUrl(portalValue, projectValue, surface = 'portal') {
  const portal = parsePortalUrl(portalValue);
  const project = text(projectValue, 100);
  if (!project) throw new TypeError('Project identifier is required.');
  portal.hash = new URLSearchParams({ project, surface: text(surface, 40) || 'portal' }).toString();
  return portal.toString();
}

export function validateLaunchUrl(value, portalValue) {
  const portal = parsePortalUrl(portalValue);
  const raw = String(value);
  if ([...raw].length > MAX_LAUNCH_URL_LENGTH) throw new TypeError('Launch URL is too long.');
  const fragmentIndex = raw.indexOf('#');
  const requestTarget = fragmentIndex === -1 ? raw : raw.slice(0, fragmentIndex);
  if (requestTarget.includes('?')) throw new TypeError('Launch URL must not contain a query string.');
  const launch = new URL(raw);
  if (launch.origin !== portal.origin) throw new TypeError('Launch URL must use the configured portal origin.');
  if (launch.username || launch.password) throw new TypeError('Launch URL must not contain user information.');
  return launch.toString();
}

export function validateHandoffResponse(value, portalValue, now = Date.now()) {
  if (!plainObject(value)) throw new TypeError('Support handoff response must be an object.');
  const keys = Object.keys(value).sort();
  if (keys.length !== 2 || keys[0] !== 'expires_at' || keys[1] !== 'launch_url') {
    throw new TypeError('Support handoff response must contain exactly launch_url and expires_at.');
  }
  if (typeof value.launch_url !== 'string' || typeof value.expires_at !== 'string') {
    throw new TypeError('Support handoff response fields must be strings.');
  }
  if (!RFC3339_DATE_TIME.test(value.expires_at)) {
    throw new TypeError('Support handoff expiry must be an RFC3339 date-time.');
  }
  const expiry = Date.parse(value.expires_at);
  if (!Number.isFinite(expiry) || expiry <= now || expiry > now + MAX_HANDOFF_TTL_MS) {
    throw new TypeError('Support handoff expiry is outside the allowed lifetime.');
  }
  const launchUrl = validateLaunchUrl(value.launch_url, portalValue);
  if (!/^#handoff=[0-9a-f]{64}$/iu.test(new URL(launchUrl).hash)) {
    throw new TypeError('Support handoff launch URL must contain only a non-empty handoff fragment.');
  }
  return {
    launch_url: launchUrl,
    expires_at: value.expires_at,
  };
}

export function resolveSameOriginEndpoint(value, pageValue) {
  const page = new URL(String(pageValue));
  const endpoint = new URL(String(value), page);
  if (endpoint.origin !== page.origin) throw new TypeError('Handoff endpoint must be same-origin.');
  return endpoint.toString();
}

export function normalizePortalMessage(event, frameWindow, portalOrigin) {
  if (event.origin !== portalOrigin || event.source !== frameWindow || !plainObject(event.data)) return null;
  if (event.data.type === 'minco.support.ready' || event.data.type === 'minco.support.close') {
    return Object.keys(event.data).length === 1 ? { type: event.data.type } : null;
  }
  if (event.data.type === 'minco.support.resize') {
    const keys = Object.keys(event.data).sort();
    const height = event.data.height;
    return keys.length === 2 && keys[0] === 'height' && keys[1] === 'type' && Number.isInteger(height) && height >= 320 && height <= 900
      ? { type: event.data.type, height }
      : null;
  }
  return null;
}

export async function issueSupportHandoff(browser, options, context) {
  const request = {
    project_id: options.project,
    surface: options.surface,
    return_url: sanitizePageUrl(browser.location.href),
    context,
  };
  const callback = browser.MincoSupport?.issueHandoff;
  if (typeof callback === 'function') {
    const result = await callback(request);
    return validateHandoffResponse(result, options.portal).launch_url;
  }
  if (!options.endpoint) return buildPortalFallbackUrl(options.portal, options.project, options.surface);

  const headers = { Accept: 'application/json', 'Content-Type': 'application/json' };
  const csrf = browser.document?.querySelector?.('meta[name="csrf-token"]')?.content;
  if (csrf) headers['X-CSRF-TOKEN'] = csrf;
  const response = await browser.fetch(resolveSameOriginEndpoint(options.endpoint, browser.location.href), {
    method: 'POST',
    credentials: 'same-origin',
    headers,
    body: JSON.stringify(request),
  });
  if (!response.ok) throw new Error(`Support handoff returned ${response.status}.`);
  const result = await response.json();
  return validateHandoffResponse(result, options.portal).launch_url;
}

export function reserveSupportTab(browser) {
  let tab;
  try {
    tab = browser.open?.('about:blank', '_blank');
    if (!tab) return null;
    tab.opener = null;
    return tab;
  } catch {
    try {
      tab?.close?.();
    } catch {
      // The browser already owns cleanup for an inaccessible browsing context.
    }
    return null;
  }
}

export function navigateReservedTab(tab, value, portalValue) {
  let launchUrl;
  try {
    launchUrl = validateLaunchUrl(value, portalValue);
    if (!tab || tab.closed) return false;
    tab.location.replace(launchUrl);
    return true;
  } catch {
    try {
      tab?.close?.();
    } catch {
      // Closing a cross-origin or already-closed context may itself be denied.
    }
    return false;
  }
}

function styles(position, inline) {
  const [vertical, horizontal] = position.split('-');
  return `
    :host{all:initial;${inline ? 'position:relative;display:inline-block;' : `position:fixed;${vertical}:24px;${horizontal}:24px;z-index:2147483000;`}max-width:100%;font-family:system-ui,sans-serif}
    *{box-sizing:border-box}.launch,.close{border:1px solid #25314a;border-radius:999px;cursor:pointer;font:700 14px system-ui}
    .launch{min-height:48px;padding:0 18px;background:#172033;color:#fff;box-shadow:0 12px 32px #0f172a47}.launch[disabled]{opacity:.6;cursor:wait}
    button:focus-visible,a:focus-visible{outline:3px solid #0f766e;outline-offset:3px}
    .backdrop{position:fixed;inset:0;z-index:2147483001;display:grid;place-items:end;max-width:100vw;overflow:hidden;padding:18px;background:#0f172a8a}
    .dialog{position:relative;width:min(520px,calc(100vw - 24px));max-width:100%;height:min(760px,calc(100vh - 24px));overflow:hidden;border-radius:18px;background:#fff;box-shadow:0 28px 90px #0006}
    iframe{width:100%;height:100%;border:0}.close{position:absolute;top:10px;right:10px;z-index:2;width:38px;height:38px;background:#fff;color:#111827}.status{position:absolute;inset:0;display:grid;place-items:center;padding:40px;background:#fff;color:#475569;text-align:center}
    .status a{color:#0f766e;font-weight:700}.focus-guard{position:fixed;width:1px;height:1px;overflow:hidden;opacity:0}
    @media(max-width:640px){.backdrop{padding:0}.dialog{width:100vw;height:100dvh;max-height:100dvh;border-radius:0}}
    @media(prefers-reduced-motion:reduce){*,*::before,*::after{scroll-behavior:auto!important;animation:none!important;transition:none!important}}
  `;
}

export function defineSupportLauncher(browser = globalThis) {
  if (!browser.customElements || !browser.HTMLElement || browser.customElements.get('minco-support-launcher')) return;

  class SupportLauncher extends browser.HTMLElement {
    constructor() {
      super();
      this.attachShadow({ mode: 'open' });
      this.message = (event) => this.onMessage(event);
      this.keydown = (event) => event.key === 'Escape' && this.close();
    }

    connectedCallback() {
      if (this.shadowRoot.childElementCount) return;
      const position = POSITIONS.has(this.getAttribute('position')) ? this.getAttribute('position') : 'bottom-right';
      const style = browser.document.createElement('style');
      style.textContent = styles(position, this.getAttribute('mode') === 'inline');
      this.button = browser.document.createElement('button');
      this.button.className = 'launch';
      this.button.type = 'button';
      this.button.textContent = text(this.getAttribute('label'), 80) || 'Support';
      this.button.setAttribute('aria-haspopup', 'dialog');
      this.button.setAttribute('aria-expanded', 'false');
      this.button.addEventListener('click', () => void this.open());
      this.shadowRoot.append(style, this.button);
    }

    options() {
      const portal = this.getAttribute('portal');
      const project = text(this.getAttribute('project'), 100);
      if (!portal || !project) throw new TypeError('portal and project attributes are required.');
      const surface = SURFACES.has(this.getAttribute('surface')) ? this.getAttribute('surface') : 'widget';
      const target = TARGETS.has(this.getAttribute('target')) ? this.getAttribute('target') : 'modal';
      return {
        portal: parsePortalUrl(portal).toString(),
        project,
        surface,
        target,
        endpoint: this.getAttribute('handoff-endpoint') || undefined,
        timeout: Math.min(30_000, Math.max(1_000, Number(this.getAttribute('ready-timeout-ms')) || 8_000)),
      };
    }

    async open() {
      if (this.button.disabled) return;
      const invocationFocus = this.shadowRoot.activeElement || browser.document.activeElement;
      this.button.disabled = true;
      let options;
      let tab;
      try {
        options = this.options();
        if (options.target === 'tab') {
          tab = reserveSupportTab(browser);
        }
        const provider = browser.MincoSupport?.getContext;
        const supplied = typeof provider === 'function' ? await provider() : browser.MincoSupport?.context;
        const launchUrl = await issueSupportHandoff(browser, options, buildSupportContext(browser, supplied));
        if (options.target === 'tab') {
          if (!navigateReservedTab(tab, launchUrl, options.portal)) this.openDialog(launchUrl, options, invocationFocus);
        } else {
          this.openDialog(launchUrl, options, invocationFocus);
        }
        this.dispatchEvent(new browser.CustomEvent('minco-support-opened', { bubbles: true }));
      } catch {
        this.dispatchEvent(new browser.CustomEvent('minco-support-error', { bubbles: true, detail: { code: 'support_launch_failed' } }));
        if (options) {
          const fallback = buildPortalFallbackUrl(options.portal, options.project, options.surface);
          if (options.target !== 'tab' || !navigateReservedTab(tab, fallback, options.portal)) {
            this.openDialog(fallback, options, invocationFocus);
          }
        }
      } finally {
        this.button.disabled = false;
      }
    }

    openDialog(launchUrl, options, invocationFocus) {
      const validatedLaunchUrl = validateLaunchUrl(launchUrl, options.portal);
      this.close();
      this.previousFocus = invocationFocus || this.shadowRoot.activeElement || browser.document.activeElement;
      this.button.setAttribute('aria-expanded', 'true');
      this.backdrop = browser.document.createElement('div');
      this.backdrop.className = 'backdrop';
      this.backdrop.addEventListener('click', (event) => event.target === this.backdrop && this.close());
      this.dialog = browser.document.createElement('section');
      this.dialog.className = 'dialog';
      this.dialog.setAttribute('role', 'dialog');
      this.dialog.setAttribute('aria-modal', 'true');
      this.dialog.setAttribute('aria-label', this.button.textContent);
      this.startGuard = browser.document.createElement('span');
      this.startGuard.className = 'focus-guard';
      this.startGuard.tabIndex = 0;
      this.startGuard.setAttribute('aria-hidden', 'true');
      this.startGuard.addEventListener('focus', () => this.focusLast());
      this.endGuard = browser.document.createElement('span');
      this.endGuard.className = 'focus-guard';
      this.endGuard.tabIndex = 0;
      this.endGuard.setAttribute('aria-hidden', 'true');
      this.endGuard.addEventListener('focus', () => this.focusFirst());
      this.closeButton = browser.document.createElement('button');
      this.closeButton.className = 'close';
      this.closeButton.type = 'button';
      this.closeButton.textContent = '×';
      this.closeButton.setAttribute('aria-label', 'Close support');
      this.closeButton.addEventListener('click', () => this.close());
      this.status = browser.document.createElement('div');
      this.status.className = 'status';
      this.status.textContent = 'Opening secure support…';
      this.frame = browser.document.createElement('iframe');
      this.frame.title = this.button.textContent;
      this.frame.referrerPolicy = 'no-referrer';
      this.frame.setAttribute('sandbox', 'allow-forms allow-modals allow-popups allow-same-origin allow-scripts');
      this.portalReady = false;
      this.frame.addEventListener('load', () => {
        if (this.portalReady || this.timer || !this.status) return;
        this.timer = browser.setTimeout(() => {
          this.timer = null;
          if (this.portalReady || !this.status || !this.frame) return;
          this.status.textContent = '';
          const link = browser.document.createElement('a');
          link.href = this.frame.src;
          link.target = '_blank';
          link.rel = 'noopener noreferrer';
          link.textContent = 'Open support in a new tab';
          this.status.append('The portal did not confirm readiness. ', link);
        }, options.timeout);
      });
      this.frame.src = validatedLaunchUrl;
      this.dialog.append(this.closeButton, this.frame, this.status);
      this.backdrop.append(this.startGuard, this.dialog, this.endGuard);
      this.shadowRoot.append(this.backdrop);
      this.portalOrigin = parsePortalUrl(options.portal).origin;
      browser.addEventListener('message', this.message);
      browser.document.addEventListener('keydown', this.keydown);
      this.closeButton.focus();
    }

    focusableElements() {
      return this.dialog
        ? [...this.dialog.querySelectorAll('button:not([disabled]),a[href],iframe,[tabindex]:not([tabindex="-1"])')]
        : [];
    }

    focusFirst() {
      this.focusableElements()[0]?.focus?.();
    }

    focusLast() {
      this.focusableElements().at(-1)?.focus?.();
    }

    onMessage(event) {
      const message = normalizePortalMessage(event, this.frame?.contentWindow, this.portalOrigin);
      if (!message) return;
      if (message.type === 'minco.support.ready') {
        this.portalReady = true;
        if (this.timer) browser.clearTimeout(this.timer);
        this.timer = null;
        this.status?.remove();
        this.status = null;
      } else if (message.type === 'minco.support.close') {
        this.close();
      } else if (message.type === 'minco.support.resize' && browser.innerWidth > 640 && this.dialog) {
        this.dialog.style.height = `${message.height}px`;
      }
    }

    close() {
      const open = Boolean(this.frame || this.backdrop);
      if (this.timer) browser.clearTimeout(this.timer);
      this.timer = null;
      browser.removeEventListener?.('message', this.message);
      browser.document?.removeEventListener?.('keydown', this.keydown);
      this.backdrop?.remove();
      this.backdrop = this.dialog = this.frame = this.status = this.startGuard = this.endGuard = null;
      this.portalReady = false;
      this.button?.setAttribute?.('aria-expanded', 'false');
      this.previousFocus?.focus?.();
      this.previousFocus = null;
      if (open) this.dispatchEvent(new browser.CustomEvent('minco-support-closed', { bubbles: true }));
    }

    disconnectedCallback() {
      this.close();
    }
  }

  browser.customElements.define('minco-support-launcher', SupportLauncher);
}

if (typeof window !== 'undefined' && typeof document !== 'undefined') defineSupportLauncher(window);
