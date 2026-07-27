(() => {
  'use strict';

  const currentScript = document.currentScript;
  const scriptOptions = Object.freeze({ ...(currentScript?.dataset || {}) });
  const discoveredBase = currentScript
    ? new URL('.', currentScript.src).pathname.replace(/\/$/, '')
    : '/_minco/feedback';
  const DEFAULT_MAX_ATTACHMENTS = 3;

  const styles = `
    :host {
      all: initial;
      --minco-bg: #ffffff;
      --minco-bg-muted: #f1f5f9;
      --minco-bg-soft: #f8fafc;
      --minco-text: #111827;
      --minco-text-muted: #64748b;
      --minco-border: #cbd5e1;
      --minco-primary: #6655e8;
      --minco-primary-text: #ffffff;
      --minco-focus: #2dd4bf;
      --minco-danger: #b91c1c;
      --minco-developer: #e0e7ff;
      font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    :host([data-theme="dark"]) {
      --minco-bg: #111827;
      --minco-bg-muted: #1f2937;
      --minco-bg-soft: #172033;
      --minco-text: #f8fafc;
      --minco-text-muted: #cbd5e1;
      --minco-border: #475569;
      --minco-primary: #8073ff;
      --minco-primary-text: #ffffff;
      --minco-developer: #312e81;
    }
    @media (prefers-color-scheme: dark) {
      :host([data-theme="auto"]) {
        --minco-bg: #111827;
        --minco-bg-muted: #1f2937;
        --minco-bg-soft: #172033;
        --minco-text: #f8fafc;
        --minco-text-muted: #cbd5e1;
        --minco-border: #475569;
        --minco-primary: #8073ff;
        --minco-primary-text: #ffffff;
        --minco-developer: #312e81;
      }
    }
    *, *::before, *::after { box-sizing: border-box; }
    .fab {
      position: fixed;
      z-index: 2147483000;
      width: 54px;
      height: 54px;
      border: 2px solid #232348;
      border-radius: 999px;
      cursor: pointer;
      background: var(--minco-primary);
      color: var(--minco-primary-text);
      font: 800 22px/1 system-ui;
      box-shadow: 0 12px 34px rgba(15, 23, 42, .28);
      transition: transform 140ms ease, box-shadow 140ms ease;
    }
    .fab:hover { transform: translateY(-2px) scale(1.02); box-shadow: 0 16px 38px rgba(15, 23, 42, .32); }
    .fab:focus-visible, button:focus-visible, textarea:focus-visible, input:focus-visible, select:focus-visible {
      outline: 3px solid var(--minco-focus);
      outline-offset: 2px;
    }
    .backdrop {
      position: fixed;
      inset: 0;
      z-index: 2147483001;
      display: grid;
      place-items: end;
      padding: 18px;
      background: rgba(15, 23, 42, .45);
    }
    .panel {
      width: min(460px, calc(100vw - 24px));
      max-height: min(780px, calc(100vh - 24px));
      overflow: auto;
      border: 1px solid var(--minco-border);
      border-radius: 18px;
      background: var(--minco-bg);
      color: var(--minco-text);
      box-shadow: 0 24px 80px rgba(0, 0, 0, .38);
    }
    .header {
      position: sticky;
      top: 0;
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 18px 20px;
      border-bottom: 1px solid var(--minco-border);
      background: var(--minco-bg);
      z-index: 1;
    }
    h2 { margin: 0; font-size: 18px; }
    .body { padding: 18px 20px 22px; }
    label { display: grid; gap: 6px; margin: 0 0 14px; font-size: 13px; font-weight: 650; }
    input, textarea, select {
      width: 100%;
      border: 1px solid var(--minco-border);
      border-radius: 10px;
      padding: 10px 12px;
      background: var(--minco-bg);
      color: var(--minco-text);
      font: 400 14px/1.45 system-ui;
    }
    textarea { min-height: 112px; resize: vertical; }
    button {
      border: 1px solid var(--minco-border);
      border-radius: 10px;
      padding: 9px 12px;
      background: var(--minco-bg);
      color: var(--minco-text);
      cursor: pointer;
      font: 650 13px/1 system-ui;
    }
    button.primary { border-color: var(--minco-primary); background: var(--minco-primary); color: var(--minco-primary-text); }
    button.danger { color: var(--minco-danger); }
    button[disabled] { cursor: wait; opacity: .58; }
    .close { width: 34px; height: 34px; padding: 0; border-radius: 999px; font-size: 18px; }
    .actions { display: flex; flex-wrap: wrap; gap: 8px; margin: 10px 0 16px; }
    .attachments, .history { display: grid; gap: 8px; margin-bottom: 14px; }
    .attachment, .history-item {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
      padding: 9px 10px;
      border-radius: 9px;
      background: var(--minco-bg-muted);
      font-size: 12px;
    }
    .history-item button { flex: 1; text-align: left; }
    .status { min-height: 20px; margin: 10px 0; color: var(--minco-text-muted); font-size: 12px; }
    .status.error { color: var(--minco-danger); }
    .conversation { display: grid; gap: 10px; margin: 0 0 18px; }
    .message { border-radius: 11px; padding: 10px 12px; background: var(--minco-bg-muted); font-size: 13px; white-space: pre-wrap; }
    .message.developer { background: var(--minco-developer); }
    .message small { display: block; margin-bottom: 5px; color: var(--minco-text-muted); }
    .meta { padding: 9px 11px; border-radius: 9px; background: var(--minco-bg-soft); color: var(--minco-text-muted); font-size: 12px; margin-bottom: 14px; }
    .hidden { display: none !important; }
    @media (prefers-reduced-motion: reduce) {
      .fab { transition: none; }
    }
  `;

  function node(tag, attributes = {}, text = null) {
    const element = document.createElement(tag);
    for (const [name, value] of Object.entries(attributes)) {
      if (name === 'class') element.className = value;
      else if (name === 'type') element.type = value;
      else element.setAttribute(name, value);
    }
    if (text !== null) element.textContent = text;
    return element;
  }

  function normalizeChoice(value, allowed, fallback) {
    const normalized = String(value || '').trim().toLowerCase().replaceAll('_', '-');
    return allowed.includes(normalized) ? normalized : fallback;
  }

  function audioExtension(contentType) {
    const normalized = String(contentType || '').split(';', 1)[0].trim().toLowerCase();
    if (normalized === 'audio/ogg') return 'ogg';
    if (normalized === 'audio/mp4' || normalized === 'audio/x-m4a') return 'm4a';
    if (normalized === 'audio/mpeg' || normalized === 'audio/mp3') return 'mp3';
    if (normalized === 'audio/wav' || normalized === 'audio/x-wav') return 'wav';
    return 'webm';
  }

  class MincoFeedback extends HTMLElement {
    constructor() {
      super();
      this.attachShadow({ mode: 'open' });
      this.config = null;
      this.basePath = this.getAttribute('base-path') || scriptOptions.endpoint || scriptOptions.basePath || discoveredBase;
      this.files = [];
      this.audioBlob = null;
      this.audioFileName = null;
      this.mediaRecorder = null;
      this.mediaStream = null;
      this.pollTimer = null;
      this.recordingTimer = null;
      this.previewUrls = [];
      this.pollGeneration = 0;
      // Prefer tab-scoped storage unless the deployment or embed explicitly opts into
      // longer-lived local storage. The server configuration is applied after it loads.
      this.tokenStorage = sessionStorage;
    }

    async connectedCallback() {
      if (this.shadowRoot.childElementCount > 0) return;
      const style = node('style');
      style.textContent = styles;
      this.shadowRoot.append(style);
      try {
        const response = await fetch(`${this.basePath}/widget-config`, { credentials: 'same-origin' });
        if (!response.ok) throw new Error(`configuration returned ${response.status}`);
        this.config = this.applyOverrides(await response.json());
        if (!this.config.enabled) return;
        this.tokenStorage = this.config.token_storage === 'local' ? localStorage : sessionStorage;
        this.setAttribute('data-theme', this.config.theme);
        this.renderFab();
      } catch (error) {
        console.warn('Minco Feedback did not start:', error);
      }
    }

    disconnectedCallback() {
      this.stopPolling();
      this.stopMedia();
      this.clearPreviewUrls();
    }

    applyOverrides(config) {
      const position = this.getAttribute('position') || scriptOptions.position || config.position;
      const theme = this.getAttribute('theme') || scriptOptions.theme || config.theme;
      const label = this.getAttribute('label') || scriptOptions.label || config.label;
      const tokenStorage = this.getAttribute('token-storage') || scriptOptions.tokenStorage || config.token_storage;
      return {
        ...config,
        position: normalizeChoice(position, ['top-left', 'top-right', 'bottom-left', 'bottom-right'], 'bottom-right'),
        theme: normalizeChoice(theme, ['light', 'dark', 'auto'], 'auto'),
        token_storage: normalizeChoice(tokenStorage, ['session', 'local'], 'session'),
        label: String(label || 'Share feedback').slice(0, 80)
      };
    }

    storageKey() {
      return `minco-feedback:${this.config.project_id}:${location.origin}`;
    }

    storedState() {
      try {
        const parsed = JSON.parse(this.tokenStorage.getItem(this.storageKey()) || 'null');
        if (!parsed) return { version: 1, active_id: null, threads: [] };
        if (parsed.id && parsed.token) {
          return { version: 1, active_id: parsed.id, threads: [parsed] };
        }
        if (Array.isArray(parsed.threads)) return parsed;
      } catch {
        // Treat invalid browser state as empty; server-side access remains token protected.
      }
      return { version: 1, active_id: null, threads: [] };
    }

    writeState(state) {
      this.tokenStorage.setItem(this.storageKey(), JSON.stringify(state));
    }

    savedThread() {
      const state = this.storedState();
      return state.threads.find(value => value.id === state.active_id) || null;
    }

    savedThreads() {
      return this.storedState().threads;
    }

    saveThread(value) {
      const state = this.storedState();
      const record = {
        id: value.id,
        token: value.token,
        title: value.title || 'Feedback',
        updated_at: new Date().toISOString()
      };
      state.threads = [record, ...state.threads.filter(item => item.id !== record.id)].slice(0, 20);
      state.active_id = record.id;
      this.writeState(state);
    }

    activateThread(id) {
      const state = this.storedState();
      if (state.threads.some(value => value.id === id)) {
        state.active_id = id;
        this.writeState(state);
      }
    }

    clearActiveThread() {
      const state = this.storedState();
      state.active_id = null;
      this.writeState(state);
    }

    removeThread(id) {
      const state = this.storedState();
      state.threads = state.threads.filter(value => value.id !== id);
      if (state.active_id === id) state.active_id = null;
      this.writeState(state);
    }

    renderFab() {
      const fab = node('button', {
        class: 'fab',
        type: 'button',
        'aria-label': this.config.label,
        title: this.config.label
      }, '✦');
      const vertical = this.config.position.startsWith('top') ? 'top' : 'bottom';
      const horizontal = this.config.position.endsWith('left') ? 'left' : 'right';
      fab.style[vertical] = `${this.config.offset_y_px}px`;
      fab.style[horizontal] = `${this.config.offset_x_px}px`;
      fab.addEventListener('click', () => this.open());
      this.shadowRoot.append(fab);
      this.fab = fab;
    }

    async open() {
      if (this.backdrop) return;
      this.backdrop = node('div', { class: 'backdrop' });
      const panel = node('section', {
        class: 'panel',
        role: 'dialog',
        'aria-modal': 'true',
        'aria-labelledby': 'minco-feedback-title'
      });
      const header = node('header', { class: 'header' });
      header.append(node('h2', { id: 'minco-feedback-title' }, this.config.label || 'Share feedback'));
      const close = node('button', { class: 'close', type: 'button', 'aria-label': 'Close feedback' }, '×');
      close.addEventListener('click', () => this.close());
      header.append(close);
      this.body = node('div', { class: 'body' });
      panel.append(header, this.body);
      this.backdrop.append(panel);
      this.backdrop.addEventListener('click', event => {
        if (event.target === this.backdrop) this.close();
      });
      this.shadowRoot.append(this.backdrop);
      this.dialogKeyHandler = event => {
        if (event.key === 'Escape') {
          this.close();
          return;
        }
        if (event.key !== 'Tab') return;

        const focusable = [...this.backdrop.querySelectorAll(
          'a[href], button, input, select, textarea, [tabindex]:not([tabindex="-1"])'
        )].filter(element => !element.disabled && element.getClientRects().length > 0);
        if (!focusable.length) {
          event.preventDefault();
          close.focus();
          return;
        }

        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        const active = this.shadowRoot.activeElement;
        if (event.shiftKey && (active === first || !this.backdrop.contains(active))) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && (active === last || !this.backdrop.contains(active))) {
          event.preventDefault();
          first.focus();
        }
      };
      document.addEventListener('keydown', this.dialogKeyHandler);
      close.focus();
      const saved = this.savedThread();
      if (saved) await this.renderConversation(saved);
      else this.renderCreate();
    }

    close() {
      this.stopPolling();
      this.stopMedia();
      this.clearPreviewUrls();
      document.removeEventListener('keydown', this.dialogKeyHandler);
      this.backdrop?.remove();
      this.backdrop = null;
      this.body = null;
      this.fab?.focus();
    }

    status(message, error = false) {
      if (!this.statusElement) return;
      this.statusElement.textContent = message;
      this.statusElement.className = error ? 'status error' : 'status';
    }

    renderHistory() {
      const history = this.savedThreads();
      if (!history.length) return null;
      const wrapper = node('div', { class: 'history' });
      wrapper.append(node('div', { class: 'meta' }, 'Continue an earlier feedback conversation'));
      for (const item of history.slice(0, 5)) {
        const row = node('div', { class: 'history-item' });
        const open = node('button', { type: 'button' }, item.title || item.id);
        open.addEventListener('click', async () => {
          this.activateThread(item.id);
          await this.renderConversation(item);
        });
        row.append(open);
        wrapper.append(row);
      }
      return wrapper;
    }

    attachmentLimit() {
      return Number.isInteger(this.config.max_attachments)
        ? this.config.max_attachments
        : DEFAULT_MAX_ATTACHMENTS;
    }

    renderCreate() {
      this.stopPolling();
      this.files = [];
      this.audioBlob = null;
      this.audioFileName = null;
      this.body.replaceChildren();
      const history = this.renderHistory();
      if (history) this.body.append(history);

      const kind = node('select');
      for (const value of ['bug', 'feature', 'usability', 'question', 'other']) {
        kind.append(node('option', { value }, value[0].toUpperCase() + value.slice(1)));
      }
      const title = node('input', {
        type: 'text', maxlength: '300', required: 'required', placeholder: 'What should we look at?'
      });
      const description = node('textarea', {
        maxlength: '20000', required: 'required', placeholder: 'Tell us what happened or what would make this better.'
      });
      const form = node('form');
      const kindLabel = node('label', {}, 'Type'); kindLabel.append(kind);
      const titleLabel = node('label', {}, 'Title'); titleLabel.append(title);
      const descriptionLabel = node('label', {}, 'Feedback'); descriptionLabel.append(description);
      const actionRow = node('div', { class: 'actions' });
      const attachmentsEnabled = this.attachmentLimit() > 0;

      if (attachmentsEnabled && this.config.screenshot_enabled) {
        const screenshot = node('button', { type: 'button' }, 'Capture screenshot');
        screenshot.addEventListener('click', () => this.captureScreenshot());
        actionRow.append(screenshot);

        const upload = node('button', { type: 'button' }, 'Choose image');
        const input = node('input', { type: 'file', accept: 'image/*', multiple: 'multiple', class: 'hidden' });
        input.addEventListener('change', () => this.addFiles(input.files || [], 'screenshot'));
        upload.addEventListener('click', () => input.click());
        actionRow.append(upload, input);
      }

      if (attachmentsEnabled) {
        const attach = node('button', { type: 'button' }, 'Attach file');
        const fileInput = node('input', { type: 'file', multiple: 'multiple', class: 'hidden' });
        fileInput.addEventListener('change', () => this.addFiles(fileInput.files || [], 'file'));
        attach.addEventListener('click', () => fileInput.click());
        actionRow.append(attach, fileInput);
      }

      if (attachmentsEnabled && this.config.voice_enabled && window.MediaRecorder && navigator.mediaDevices?.getUserMedia) {
        this.voiceButton = node('button', { type: 'button' }, 'Record voice');
        this.voiceButton.addEventListener('click', () => this.toggleVoice(description));
        actionRow.append(this.voiceButton);
      }

      this.attachmentsElement = node('div', { class: 'attachments' });
      this.statusElement = node('div', { class: 'status', role: 'status', 'aria-live': 'polite' });
      const submit = node('button', { class: 'primary', type: 'submit' }, 'Submit feedback');
      form.addEventListener('submit', async event => {
        event.preventDefault();
        submit.disabled = true;
        this.status('Submitting feedback…');
        try {
          const payload = {
            project_id: this.config.project_id,
            kind: kind.value,
            priority: 'normal',
            title: title.value,
            description: description.value,
            context: this.pageContext(),
            tags: []
          };
          const payloadJson = JSON.stringify(payload);
          const attachmentBytes = this.files.reduce(
            (total, attachment) => total + attachment.file.size,
            this.audioBlob?.size || 0
          );
          const estimatedMultipartBytes = new Blob([payloadJson]).size
            + attachmentBytes
            + 64 * 1024
            + (this.files.length + (this.audioBlob ? 1 : 0)) * 4096;
          if (estimatedMultipartBytes > this.config.max_http_body_bytes) {
            throw new Error('The feedback and attachments exceed the configured request limit. Remove an attachment and try again.');
          }
          const data = new FormData();
          data.append('payload', payloadJson);
          for (const attachment of this.files) {
            data.append(attachment.field, attachment.file, attachment.file.name);
          }
          if (this.audioBlob) {
            data.append('audio', this.audioBlob, this.audioFileName || 'feedback-voice.webm');
          }
          const response = await fetch(`${this.basePath}/threads`, {
            method: 'POST',
            body: data,
            credentials: 'same-origin',
            headers: this.projectHeaders()
          });
          const value = await this.readResponse(response);
          this.saveThread({
            id: value.thread.id,
            token: value.client_token,
            title: value.thread.title
          });
          await this.renderConversation(this.savedThread());
        } catch (error) {
          this.status(error.message, true);
        } finally {
          submit.disabled = false;
        }
      });
      if (this.config.privacy_notice) {
        form.append(node('p', { class: 'meta' }, this.config.privacy_notice));
      }
      form.append(
        kindLabel,
        titleLabel,
        descriptionLabel,
        actionRow,
        this.attachmentsElement,
        this.statusElement,
        submit
      );
      this.body.append(form);
      title.focus();
    }

    addFiles(files, field) {
      for (const file of files) {
        const attachmentLimit = this.attachmentLimit();
        if (this.files.length + (this.audioBlob ? 1 : 0) >= attachmentLimit) {
          this.status(`No more than ${attachmentLimit} attachments are allowed.`, true);
          break;
        }
        const maximum = field === 'screenshot'
          ? this.config.max_screenshot_bytes
          : this.config.max_file_bytes;
        if (file.size === 0 || file.size > maximum) {
          this.status(`${file.name} exceeds the configured attachment limit.`, true);
          continue;
        }
        this.files.push({ file, field });
      }
      this.renderAttachments();
    }

    projectHeaders() {
      const projectKey = this.getAttribute('project-key') || scriptOptions.projectKey;
      return projectKey ? { 'X-Minco-Feedback-Project-Key': projectKey } : {};
    }

    safePageUrl() {
      const value = new URL(location.href);
      value.hash = '';
      if (!this.config.include_url_query) {
        value.search = '';
      } else {
        const redacted = new Set((this.config.redact_query_parameters || []).map(item => item.toLowerCase()));
        for (const key of [...value.searchParams.keys()]) {
          if (redacted.has(key.toLowerCase())) value.searchParams.set(key, '[REDACTED]');
        }
      }
      return value.toString();
    }

    pageContext() {
      return {
        page_url: this.safePageUrl(),
        route_name: this.getAttribute('route') || scriptOptions.route || document.body?.dataset?.route || null,
        release_id: this.getAttribute('release') || scriptOptions.release || document.documentElement.dataset.release || null,
        environment: this.getAttribute('environment') || scriptOptions.environment || document.documentElement.dataset.environment || null,
        request_id: this.getAttribute('request-id') || scriptOptions.requestId || document.documentElement.dataset.requestId || null,
        user_agent: navigator.userAgent,
        viewport: `${window.innerWidth}x${window.innerHeight}`,
        client_subject: null
      };
    }

    async captureScreenshot() {
      if (!navigator.mediaDevices?.getDisplayMedia) {
        this.status('Screen capture is not supported by this browser. Choose an image instead.', true);
        return;
      }
      const attachmentLimit = this.attachmentLimit();
      if (this.files.length + (this.audioBlob ? 1 : 0) >= attachmentLimit) {
        this.status(`No more than ${attachmentLimit} attachments are allowed.`, true);
        return;
      }
      this.status('Choose the tab or window you want to capture.');
      let stream;
      try {
        stream = await navigator.mediaDevices.getDisplayMedia({ video: true, audio: false });
        const video = document.createElement('video');
        video.srcObject = stream;
        video.muted = true;
        await video.play();
        await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        if (!video.videoWidth || !video.videoHeight) throw new Error('Captured surface has no video frame.');
        const canvas = document.createElement('canvas');
        const maximumDimension = 1600;
        const scale = Math.min(
          1,
          maximumDimension / Math.max(video.videoWidth, video.videoHeight)
        );
        canvas.width = Math.max(1, Math.round(video.videoWidth * scale));
        canvas.height = Math.max(1, Math.round(video.videoHeight * scale));
        const context = canvas.getContext('2d');
        if (!context) throw new Error('Screenshot canvas is unavailable.');
        context.drawImage(video, 0, 0, canvas.width, canvas.height);
        const encode = (contentType, quality) => new Promise(resolve => {
          canvas.toBlob(resolve, contentType, quality);
        });
        let blob = await encode('image/webp', 0.82);
        if (!blob || blob.type !== 'image/webp') blob = await encode('image/png');
        if (!blob) throw new Error('Screenshot encoding failed');
        if (blob.size > this.config.max_screenshot_bytes) {
          throw new Error('Screenshot exceeds the configured size limit.');
        }
        const extension = blob.type === 'image/webp' ? 'webp' : 'png';
        this.files.push({
          field: 'screenshot',
          file: new File(
            [blob],
            `feedback-${Date.now()}.${extension}`,
            { type: blob.type || 'image/png' }
          )
        });
        this.renderAttachments();
        this.status('Screenshot attached.');
      } catch (error) {
        const cancelled = error.name === 'NotAllowedError' || error.name === 'AbortError';
        this.status(cancelled ? 'Screenshot capture was cancelled.' : error.message, !cancelled);
      } finally {
        stream?.getTracks().forEach(track => track.stop());
      }
    }

    async toggleVoice(textarea) {
      if (this.mediaRecorder?.state === 'recording') {
        this.mediaRecorder.stop();
        return;
      }
      const attachmentLimit = this.attachmentLimit();
      if (this.files.length + (this.audioBlob ? 1 : 0) >= attachmentLimit) {
        this.status(`No more than ${attachmentLimit} attachments are allowed.`, true);
        return;
      }
      try {
        this.mediaStream = await navigator.mediaDevices.getUserMedia({ audio: true });
        const chunks = [];
        const preferredTypes = ['audio/webm;codecs=opus', 'audio/webm', 'audio/mp4'];
        const mimeType = preferredTypes.find(type => MediaRecorder.isTypeSupported?.(type));
        this.mediaRecorder = mimeType
          ? new MediaRecorder(this.mediaStream, { mimeType })
          : new MediaRecorder(this.mediaStream);
        this.mediaRecorder.addEventListener('dataavailable', event => {
          if (event.data.size) chunks.push(event.data);
        });
        this.mediaRecorder.addEventListener('stop', async () => {
          const contentType = this.mediaRecorder?.mimeType || 'audio/webm';
          this.audioBlob = new Blob(chunks, { type: contentType });
          this.audioFileName = `feedback-voice.${audioExtension(contentType)}`;
          this.stopMedia();
          this.voiceButton.textContent = 'Record voice';
          if (!this.audioBlob.size || this.audioBlob.size > this.config.max_audio_bytes) {
            this.audioBlob = null;
            this.audioFileName = null;
            this.renderAttachments();
            this.status('Voice note exceeds the configured size limit.', true);
            return;
          }
          this.renderAttachments();
          if (this.config.transcription_enabled) {
            this.status('Transcribing voice note…');
            try {
              const data = new FormData();
              data.append('audio', this.audioBlob, this.audioFileName);
              const response = await fetch(`${this.basePath}/transcriptions`, {
                method: 'POST',
                body: data,
                credentials: 'same-origin',
                headers: this.projectHeaders()
              });
              const result = await this.readResponse(response);
              textarea.value = [textarea.value.trim(), result.text.trim()].filter(Boolean).join('\n\n');
              this.status('Voice note transcribed and attached.');
            } catch (error) {
              this.status(`Voice note attached, but transcription failed: ${error.message}`, true);
            }
          } else {
            this.status('Voice note attached.');
          }
        });
        this.mediaRecorder.start();
        this.recordingTimer = window.setTimeout(() => {
          if (this.mediaRecorder?.state === 'recording') this.mediaRecorder.stop();
        }, (this.config.max_recording_seconds || 90) * 1000);
        this.voiceButton.textContent = 'Stop recording';
        this.status(`Recording voice note… maximum ${this.config.max_recording_seconds || 90} seconds.`);
      } catch (error) {
        this.stopMedia();
        this.status(`Microphone unavailable: ${error.message}`, true);
      }
    }

    stopMedia() {
      if (this.recordingTimer) window.clearTimeout(this.recordingTimer);
      this.recordingTimer = null;
      this.mediaStream?.getTracks().forEach(track => track.stop());
      this.mediaStream = null;
      this.mediaRecorder = null;
    }

    clearPreviewUrls() {
      for (const value of this.previewUrls) URL.revokeObjectURL(value);
      this.previewUrls = [];
    }

    renderAttachments() {
      if (!this.attachmentsElement) return;
      this.attachmentsElement.replaceChildren();
      this.files.forEach((attachment, index) => {
        const row = node('div', { class: 'attachment' });
        row.append(node('span', {}, `${attachment.file.name} · ${Math.ceil(attachment.file.size / 1024)} KB`));
        const remove = node('button', { type: 'button', class: 'danger' }, 'Remove');
        remove.addEventListener('click', () => {
          this.files.splice(index, 1);
          this.renderAttachments();
        });
        row.append(remove);
        this.attachmentsElement.append(row);
      });
      if (this.audioBlob) {
        const row = node('div', { class: 'attachment' });
        row.append(node('span', { 'aria-hidden': 'true' }, '🎙️'));
        row.append(node('span', {}, `Voice note · ${Math.ceil(this.audioBlob.size / 1024)} KB`));
        const remove = node('button', { type: 'button', class: 'danger' }, 'Remove');
        remove.addEventListener('click', () => {
          this.audioBlob = null;
          this.audioFileName = null;
          this.renderAttachments();
        });
        row.append(remove);
        this.attachmentsElement.append(row);
      }
    }

    async renderConversation(saved) {
      this.stopPolling();
      this.body.replaceChildren();
      this.statusElement = node('div', { class: 'status', role: 'status', 'aria-live': 'polite' });
      const meta = node('div', { class: 'meta' }, 'Loading feedback conversation…');
      const conversation = node('div', { class: 'conversation' });
      const reply = node('textarea', { maxlength: '20000', placeholder: 'Reply to the development team' });
      const send = node('button', { class: 'primary', type: 'button' }, 'Send reply');
      const newFeedback = node('button', { type: 'button' }, 'Start new feedback');
      newFeedback.addEventListener('click', () => {
        this.clearActiveThread();
        this.renderCreate();
      });
      send.addEventListener('click', async () => {
        if (!reply.value.trim()) return;
        send.disabled = true;
        this.status('Sending reply…');
        try {
          const response = await fetch(`${this.basePath}/threads/${saved.id}/messages`, {
            method: 'POST',
            body: JSON.stringify({ body: reply.value }),
            credentials: 'same-origin',
            headers: {
              'Content-Type': 'application/json',
              'X-Minco-Feedback-Token': saved.token
            }
          });
          await this.readResponse(response);
          reply.value = '';
          await load();
        } catch (error) {
          this.status(error.message, true);
        } finally {
          send.disabled = false;
        }
      });
      const actions = node('div', { class: 'actions' });
      actions.append(send, newFeedback);
      this.body.append(meta, conversation, reply, actions, this.statusElement);

      const load = async () => {
        try {
          const response = await fetch(`${this.basePath}/threads/${saved.id}`, {
            credentials: 'same-origin',
            headers: { 'X-Minco-Feedback-Token': saved.token }
          });
          const thread = await this.readResponse(response);
          this.saveThread({ id: saved.id, token: saved.token, title: thread.title });
          meta.textContent = `${thread.title} · ${thread.status.replaceAll('_', ' ')}`;
          conversation.replaceChildren();
          const original = node('div', { class: 'message' });
          original.append(
            node('small', {}, 'You · original feedback'),
            node('div', {}, thread.description)
          );
          conversation.append(original);
          for (const message of thread.messages.filter(value => value.visible_to_client)) {
            const item = node('div', {
              class: `message ${message.author_role === 'developer' ? 'developer' : ''}`
            });
            item.append(
              node('small', {}, `${message.author_role} · ${new Date(message.created_at).toLocaleString()}`),
              node('div', {}, message.body)
            );
            conversation.append(item);
          }
          this.status('Conversation is up to date.');
        } catch (error) {
          if (String(error.message).startsWith('404:')) {
            this.removeThread(saved.id);
            this.renderCreate();
            return;
          }
          this.status(error.message, true);
        }
      };

      await load();
      this.startPolling(load);
      reply.focus();
    }

    startPolling(load) {
      this.stopPolling();
      const generation = this.pollGeneration;
      const cycle = async () => {
        if (generation !== this.pollGeneration || !this.backdrop) return;
        if (!document.hidden) await load();
        if (generation === this.pollGeneration && this.backdrop) {
          this.pollTimer = window.setTimeout(cycle, this.config.poll_interval_ms || 15000);
        }
      };
      this.pollTimer = window.setTimeout(cycle, this.config.poll_interval_ms || 15000);
    }

    stopPolling() {
      this.pollGeneration += 1;
      if (this.pollTimer) window.clearTimeout(this.pollTimer);
      this.pollTimer = null;
    }

    async readResponse(response) {
      const contentType = response.headers.get('content-type') || '';
      const body = contentType.includes('json') ? await response.json() : await response.text();
      if (!response.ok) {
        const detail = typeof body === 'object' ? body.detail || body.title : body;
        throw new Error(`${response.status}: ${detail || 'request failed'}`);
      }
      return body;
    }
  }

  if (!customElements.get('minco-feedback')) {
    customElements.define('minco-feedback', MincoFeedback);
  }

  window.MincoFeedback = Object.freeze({
    mount(options = {}) {
      const element = document.createElement('minco-feedback');
      const attributes = {
        'base-path': options.basePath,
        position: options.position,
        theme: options.theme,
        label: options.label,
        'project-key': options.projectKey,
        'token-storage': options.tokenStorage,
        environment: options.environment,
        release: options.release,
        route: options.route,
        'request-id': options.requestId
      };
      for (const [name, value] of Object.entries(attributes)) {
        if (value !== undefined && value !== null && value !== '') element.setAttribute(name, value);
      }
      document.body.append(element);
      return element;
    }
  });

  if (scriptOptions.autoMount !== 'false') {
    const mount = () => {
      if (!document.querySelector('minco-feedback')) {
        window.MincoFeedback.mount({
          basePath: scriptOptions.basePath || scriptOptions.endpoint,
          position: scriptOptions.position,
          theme: scriptOptions.theme,
          label: scriptOptions.label,
          projectKey: scriptOptions.projectKey,
          tokenStorage: scriptOptions.tokenStorage,
          environment: scriptOptions.environment,
          release: scriptOptions.release,
          route: scriptOptions.route,
          requestId: scriptOptions.requestId
        });
      }
    };
    if (document.readyState === 'loading') {
      document.addEventListener('DOMContentLoaded', mount, { once: true });
    } else {
      mount();
    }
  }
})();
