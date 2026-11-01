import { test, expect } from '@playwright/test';
import path from 'path';
import fs from 'fs';

const EXTENSION_PATH = path.resolve(__dirname, '../../../browser-extension');
const LOGIN_FIXTURE_URL = 'file://' + path.resolve(__dirname, '../fixtures/login_page.html');

// Same stubbed-API injection technique as save-prompt.spec.js/autofill.spec.js:
// outbound messages recorded in window._sentMessages, inbound listeners
// captured in window._contentListeners, clipboard writes captured in
// window._clipboardWrites (navigator.clipboard.writeText is stubbed since
// headless browsers don't reliably grant clipboard-write permission).
async function injectContentScripts(page, files) {
  const source = files
    .map((f) => fs.readFileSync(path.join(EXTENSION_PATH, f), 'utf8'))
    .join('\n');
  await page.evaluate(({ script }) => {
    window._sentMessages = [];
    window._contentListeners = [];
    window._clipboardWrites = [];
    window.browser = {
      runtime: {
        sendMessage: (msg) => {
          window._sentMessages.push(msg);
          return Promise.resolve({ Ack: true });
        },
        onMessage: {
          addListener: (listener) => {
            window._contentListeners.push(listener);
          },
        },
      },
    };
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: (text) => {
          window._clipboardWrites.push(text);
          return Promise.resolve();
        },
      },
    });
    eval(script);
  }, { script: source });
}

test.describe('Generate password: clipboard relay (content-generate.js)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(LOGIN_FIXTURE_URL);
    await injectContentScripts(page, ['clipboard-clear.js', 'content-generate.js']);
  });

  test('GENERATE_COPY_TO_CLIPBOARD message writes the password to the clipboard', async ({ page }) => {
    await page.evaluate(() => {
      for (const listener of window._contentListeners) {
        listener({ type: 'GENERATE_COPY_TO_CLIPBOARD', password: 'hunter2-generated' });
      }
    });
    const writes = await page.evaluate(() => window._clipboardWrites);
    expect(writes).toEqual(['hunter2-generated']);
  });

  test('unrelated messages are ignored (no clipboard write)', async ({ page }) => {
    await page.evaluate(() => {
      for (const listener of window._contentListeners) {
        listener({ type: 'SAVE_BAR_ACTION', action: 'save' });
      }
    });
    const writes = await page.evaluate(() => window._clipboardWrites);
    expect(writes).toEqual([]);
  });
});
