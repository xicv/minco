#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

plugin_dir="plugins/minco-plugin-feedback"
temporary_root="$(mktemp -d)"
startup_file="$temporary_root/startup.json"
stderr_file="$temporary_root/server.stderr"
qa_dir="${WORKBENCH_SCREENSHOT_DIR:-$temporary_root/screenshots}"
server_pid=""

cleanup() {
  if [[ -n "$server_pid" ]] && kill -0 "$server_pid" 2>/dev/null; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$temporary_root"
}
trap cleanup EXIT INT TERM

npm ci --prefix "$plugin_dir" --ignore-scripts
install_args=(--only-shell chromium)
if [[ "$(uname -s)" == "Linux" ]]; then
  install_args=(--with-deps "${install_args[@]}")
fi
"$plugin_dir/node_modules/.bin/playwright" install "${install_args[@]}"

cargo run -p cargo-minco --locked -- \
  --root "$(pwd -P)" --json workbench serve --port 0 \
  >"$startup_file" 2>"$stderr_file" &
server_pid="$!"

for _ in $(seq 1 200); do
  if [[ -s "$startup_file" ]]; then
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    cat "$stderr_file" >&2
    exit 1
  fi
  sleep 0.1
done

if [[ ! -s "$startup_file" ]]; then
  echo "workbench server did not report its origin within 20 seconds" >&2
  cat "$stderr_file" >&2
  exit 1
fi

origin="$(node -e 'const fs=require("node:fs"); const value=JSON.parse(fs.readFileSync(process.argv[1], "utf8")); process.stdout.write(value.origin)' "$startup_file")"
mkdir -p "$qa_dir"

NODE_PATH="$plugin_dir/node_modules" \
MINCO_WORKBENCH_ORIGIN="$origin" \
MINCO_WORKBENCH_SCREENSHOT_DIR="$qa_dir" \
node <<'NODE'
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('@playwright/test');

async function preparePage(context) {
  await context.addInitScript(() => {
    class TestUtterance extends EventTarget {
      constructor(text) {
        super();
        this.text = text;
      }
    }
    Object.defineProperty(window, 'SpeechSynthesisUtterance', {
      configurable: true,
      value: TestUtterance,
    });
    Object.defineProperty(window, 'speechSynthesis', {
      configurable: true,
      value: {
        speak(utterance) { window.__mincoSpokenText = utterance.text; },
        cancel() { window.__mincoSpeechCancelled = true; },
      },
    });
  });
  const page = await context.newPage();
  const pageErrors = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') pageErrors.push(message.text());
  });
  return { page, pageErrors };
}

(async () => {
  const origin = process.env.MINCO_WORKBENCH_ORIGIN;
  const screenshotDir = process.env.MINCO_WORKBENCH_SCREENSHOT_DIR;
  const browser = await chromium.launch({ headless: true });

  try {
    const desktopContext = await browser.newContext({ viewport: { width: 1536, height: 1024 } });
    const { page: desktop, pageErrors: desktopErrors } = await preparePage(desktopContext);
    await desktop.goto(origin, { waitUntil: 'networkidle' });
    await desktop.locator('#node-count').waitFor({ state: 'visible' });
    if ((await desktop.locator('#node-count').textContent()) === '—') throw new Error('summary did not load');
    if ((await desktop.locator('.evidence-lane').count()) !== 6) throw new Error('six evidence lanes were not rendered');
    if ((await desktop.locator('body').evaluate((body) => body.scrollWidth)) > 1536) throw new Error('desktop page overflows horizontally');

    await desktop.locator('[data-view="overview"]').focus();
    await desktop.keyboard.press('ArrowDown');
    if (!(await desktop.locator('[data-view="architecture"]').evaluate((node) => node === document.activeElement))) {
      throw new Error('keyboard navigation did not move focus');
    }
    await desktop.locator('[data-view="operations"]').click();
    if ((await desktop.locator('#detail-title').textContent()) !== 'Operations') throw new Error('Operations view did not render');
    await desktop.locator('[data-view="overview"]').click();

    const readAloud = desktop.locator('#read-aloud-button');
    await readAloud.click();
    if ((await readAloud.getAttribute('aria-pressed')) !== 'true') throw new Error('read-aloud state did not start');
    if (!(await desktop.evaluate(() => Boolean(window.__mincoSpokenText)))) throw new Error('read-aloud did not receive displayed text');
    await readAloud.click();
    if ((await readAloud.getAttribute('aria-pressed')) !== 'false') throw new Error('read-aloud state did not stop');

    const downloadPromise = desktop.waitForEvent('download');
    await desktop.locator('#export-button').click();
    const download = await downloadPromise;
    if (download.suggestedFilename() !== 'project-view.json') throw new Error('export download filename changed');
    await desktop.screenshot({ path: path.join(screenshotDir, 'workbench-desktop.png') });
    if (desktopErrors.length) throw new Error(`desktop browser errors: ${desktopErrors.join('; ')}`);
    await desktopContext.close();

    const mobileContext = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true });
    const { page: mobile, pageErrors: mobileErrors } = await preparePage(mobileContext);
    await mobile.goto(origin, { waitUntil: 'networkidle' });
    await mobile.locator('#node-count').waitFor({ state: 'visible' });
    if ((await mobile.locator('body').evaluate((body) => body.scrollWidth)) > 390) throw new Error('mobile page overflows horizontally');
    if ((await mobile.locator('[data-mobile-section="graph"]').getAttribute('aria-selected')) !== 'true') {
      throw new Error('mobile Graph tab is not selected initially');
    }
    await mobile.locator('[data-mobile-section="evidence"]').click();
    if (!(await mobile.locator('.evidence-region').isVisible())) throw new Error('mobile Evidence view did not render');
    if (await mobile.locator('.graph-region').isVisible()) throw new Error('mobile Evidence view left Graph visible');
    await mobile.locator('[data-mobile-section="evidence"]').focus();
    await mobile.keyboard.press('ArrowLeft');
    if ((await mobile.locator('[data-mobile-section="tasks"]').getAttribute('aria-selected')) !== 'true') {
      throw new Error('mobile tab keyboard navigation did not activate Tasks');
    }
    if (!(await mobile.locator('.task-rail').isVisible()) || await mobile.locator('.evidence-region').isVisible()) {
      throw new Error('mobile tab keyboard navigation did not switch the visible panel');
    }
    await mobile.locator('[data-mobile-section="evidence"]').click();
    if ((await mobile.locator('.evidence-lane').count()) !== 6) throw new Error('mobile evidence lanes changed');
    await mobile.locator('.evidence-region').scrollIntoViewIfNeeded();
    await mobile.screenshot({ path: path.join(screenshotDir, 'workbench-mobile.png') });
    if (mobileErrors.length) throw new Error(`mobile browser errors: ${mobileErrors.join('; ')}`);
    await mobileContext.close();

    for (const name of ['workbench-desktop.png', 'workbench-mobile.png']) {
      if (!fs.statSync(path.join(screenshotDir, name)).size) throw new Error(`${name} is empty`);
    }
    console.log('workbench browser journeys passed: desktop, keyboard, read-aloud, export, mobile');
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
NODE
