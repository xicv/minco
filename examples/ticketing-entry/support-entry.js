const POSITIONS = new Set(['bottom-left', 'bottom-right', 'top-left', 'top-right']);
const SURFACES = new Set(['widget', 'portal', 'extension', 'api', 'mobile']);
const TARGETS = new Set(['modal', 'tab']);
const MAX_REFERENCES = 8;
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

function plainObject(value) {
  if (!value || typeof value !== 'object') return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function text(value, maximum = 2_000) {
  if (value === undefined || value === null) return undefined;
  const normalized = String(value).trim();
  if (!normalized || /[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F]/u.test(normalized)) return undefined;
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
  if (!['http:', 'https:'].includes(page.protocol)) throw new TypeError('Page URL must use HTTP or HTTPS.');
  page.username = '';
  page.password = '';
  page.search = '';
  page.hash = '';
  return page.toString();
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
    page_title: text(browser.document?.title, 300),
    locale: text(browser.navigator?.language, 40),
    timezone: text(browser.Intl?.DateTimeFormat?.().resolvedOptions?.().timeZone, 100),
    viewport:
      Number.isFinite(browser.innerWidth) && Number.isFinite(browser.innerHeight)
        ? `${Math.max(0, Math.trunc(browser.innerWidth))}x${Math.max(0, Math.trunc(browser.innerHeight))}`
        : undefined,
  };
  for (const key of CONTEXT_KEYS) {
    if (key in input) context[key] = text(input[key], key === 'selected_text' ? 2_000 : 2_000);
  }
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
  const launch = new URL(String(value));
  if (launch.origin !== portal.origin) throw new TypeError('Launch URL must use the configured portal origin.');
  if (launch.username || launch.password) throw new TypeError('Launch URL must not contain user information.');
  return launch.toString();
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
    return { type: event.data.type };
  }
  if (event.data.type === 'minco.support.resize') {
    const height = Number(event.data.height);
    return Number.isFinite(height)
      ? { type: event.data.type, height: Math.min(900, Math.max(320, Math.trunc(height))) }
      : null;
  }
  return null;
}

async function handoff(browser, options, context) {
  const request = {
    project_id: options.project,
    surface: options.surface,
    return_url: sanitizePageUrl(browser.location.href),
    context,
  };
  const callback = browser.MincoSupport?.issueHandoff;
  if (typeof callback === 'function') {
    const result = await callback(request);
    return validateLaunchUrl(typeof result === 'string' ? result : result?.launch_url, options.portal);
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
  if (!plainObject(result) || typeof result.launch_url !== 'string') throw new Error('Missing launch_url.');
  return validateLaunchUrl(result.launch_url, options.portal);
}

function styles(position, inline) {
  const [vertical, horizontal] = position.split('-');
  return `
    :host{all:initial;${inline ? 'position:relative;display:inline-block;' : `position:fixed;${vertical}:24px;${horizontal}:24px;z-index:2147483000;`}font-family:system-ui,sans-serif}
    *{box-sizing:border-box}.launch,.close{border:1px solid #25314a;border-radius:999px;cursor:pointer;font:700 14px system-ui}
    .launch{min-height:48px;padding:0 18px;background:#172033;color:#fff;box-shadow:0 12px 32px #0f172a47}.launch[disabled]{opacity:.6;cursor:wait}
    button:focus-visible,a:focus-visible{outline:3px solid #0f766e;outline-offset:3px}
    .backdrop{position:fixed;inset:0;z-index:2147483001;display:grid;place-items:end;padding:18px;background:#0f172a8a}
    .dialog{position:relative;width:min(520px,calc(100vw - 24px));height:min(760px,calc(100vh - 24px));overflow:hidden;border-radius:18px;background:#fff;box-shadow:0 28px 90px #0006}
    iframe{width:100%;height:100%;border:0}.close{position:absolute;top:10px;right:10px;z-index:2;width:38px;height:38px;background:#fff;color:#111827}.status{position:absolute;inset:0;display:grid;place-items:center;padding:40px;background:#fff;color:#475569;text-align:center}
    .status a{color:#0f766e;font-weight:700}@media(max-width:640px){.backdrop{padding:0}.dialog{width:100vw;height:100dvh;border-radius:0}}
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
      this.button.disabled = true;
      let options;
      let tab;
      try {
        options = this.options();
        if (options.target === 'tab') {
          tab = browser.open(buildPortalFallbackUrl(options.portal, options.project, options.surface), '_blank');
          if (tab) tab.opener = null;
        }
        const provider = browser.MincoSupport?.getContext;
        const supplied = typeof provider === 'function' ? await provider() : browser.MincoSupport?.context;
        const launchUrl = await handoff(browser, options, buildSupportContext(browser, supplied));
        if (options.target === 'tab') {
          if (tab && !tab.closed) tab.location.replace(launchUrl);
        } else {
          this.openDialog(launchUrl, options);
        }
        this.dispatchEvent(new browser.CustomEvent('minco-support-opened', { bubbles: true }));
      } catch {
        this.dispatchEvent(new browser.CustomEvent('minco-support-error', { bubbles: true, detail: { code: 'support_launch_failed' } }));
        if (!tab && options) browser.open(buildPortalFallbackUrl(options.portal, options.project, options.surface), '_blank', 'noopener,noreferrer');
      } finally {
        this.button.disabled = false;
      }
    }

    openDialog(launchUrl, options) {
      this.close();
      this.previousFocus = browser.document.activeElement;
      this.backdrop = browser.document.createElement('div');
      this.backdrop.className = 'backdrop';
      this.backdrop.addEventListener('click', (event) => event.target === this.backdrop && this.close());
      this.dialog = browser.document.createElement('section');
      this.dialog.className = 'dialog';
      this.dialog.setAttribute('role', 'dialog');
      this.dialog.setAttribute('aria-modal', 'true');
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
      this.frame.src = validateLaunchUrl(launchUrl, options.portal);
      this.frame.referrerPolicy = 'no-referrer';
      this.frame.setAttribute('sandbox', 'allow-forms allow-modals allow-popups allow-same-origin allow-scripts allow-top-navigation-by-user-activation');
      this.frame.addEventListener('load', () => {
        this.timer = browser.setTimeout(() => {
          this.status.textContent = '';
          const link = browser.document.createElement('a');
          link.href = this.frame.src;
          link.target = '_blank';
          link.rel = 'noopener noreferrer';
          link.textContent = 'Open support in a new tab';
          this.status.append('The portal did not confirm readiness. ', link);
        }, options.timeout);
      });
      this.dialog.append(this.closeButton, this.frame, this.status);
      this.backdrop.append(this.dialog);
      this.shadowRoot.append(this.backdrop);
      this.portalOrigin = parsePortalUrl(options.portal).origin;
      browser.addEventListener('message', this.message);
      browser.document.addEventListener('keydown', this.keydown);
      this.closeButton.focus();
    }

    onMessage(event) {
      const message = normalizePortalMessage(event, this.frame?.contentWindow, this.portalOrigin);
      if (!message) return;
      if (message.type === 'minco.support.ready') {
        if (this.timer) browser.clearTimeout(this.timer);
        this.timer = null;
        this.status?.remove();
        this.status = null;
      } else if (message.type === 'minco.support.close') {
        this.close();
      } else if (message.type === 'minco.support.resize' && browser.innerWidth > 640) {
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
      this.backdrop = this.dialog = this.frame = this.status = null;
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
