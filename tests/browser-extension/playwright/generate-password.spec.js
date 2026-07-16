import { test, expect } from '@playwright/test';
import path from 'path';
import fs from 'fs';

const EXTENSION_PATH = path.resolve(__dirname, '../../../browser-extension');
const LOGIN_FIXTURE_URL = 'file://' + path.resolve(__dirname, '../fixtures/login_page.html');
const CHANGE_PASSWORD_FIXTURE_URL =
  'file://' + path.resolve(__dirname, '../fixtures/change_password.html');

// Same stubbed-API injection technique as save-prompt.spec.js/autofill.spec.js:
// outbound messages recorded in window._sentMessages, inbound listeners
// captured in window._contentListeners, clipboard writes captured in
// window._clipboardWrites (navigator.clipboard.writeText is stubbed since
// headless browsers don't reliably grant clipboard-write permission).
// `generatedPassword`, if set, makes sendMessage resolve a GeneratePassword
// action the way the agent would — needed for the inline-fill tests below.
async function injectContentScripts(page, files, { generatedPassword } = {}) {
  const source = files
    .map((f) => fs.readFileSync(path.join(EXTENSION_PATH, f), 'utf8'))
    .join('\n');
  await page.evaluate(({ script, generatedPassword }) => {
    window._sentMessages = [];
    window._contentListeners = [];
    window._clipboardWrites = [];
    window.browser = {
      runtime: {
        sendMessage: (msg) => {
          window._sentMessages.push(msg);
          if (generatedPassword && msg && msg.GeneratePassword !== undefined) {
            return Promise.resolve({ GeneratedPassword: { password: generatedPassword } });
          }
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
  }, { script: source, generatedPassword });
}

test.describe('Generate password: clipboard relay (content-generate.js)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(LOGIN_FIXTURE_URL);
    await injectContentScripts(page, ['content-heuristics.js', 'content-generate.js']);
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

function iconNamesOn(page) {
  return page.evaluate(() =>
    Array.from(document.documentElement.querySelectorAll('[data-cosmic-bwarden-generate-icon]'))
      .map((el) => el.getAttribute('data-cosmic-bwarden-generate-icon'))
      .sort()
  );
}

test.describe('Generate password: inline icon placement', () => {
  test('shows the icon only on the registration field, never on login password fields', async ({ page }) => {
    await page.goto(LOGIN_FIXTURE_URL);
    await injectContentScripts(page, ['content-heuristics.js', 'content.js', 'content-generate.js']);

    expect(await iconNamesOn(page)).toEqual(['reg-password']);
  });

  test('shows the icon on new+confirm fields but not the current-password field', async ({ page }) => {
    await page.goto(CHANGE_PASSWORD_FIXTURE_URL);
    await injectContentScripts(page, ['content-heuristics.js', 'content.js', 'content-generate.js']);

    expect(await iconNamesOn(page)).toEqual(['confirm-password', 'new-password']);
  });
});

test.describe('Generate password: inline icon fill-on-click', () => {
  test('clicking the icon fills new+confirm together, leaves current password untouched', async ({ page }) => {
    await page.goto(CHANGE_PASSWORD_FIXTURE_URL);
    await injectContentScripts(
      page,
      ['content-heuristics.js', 'content.js', 'content-generate.js'],
      { generatedPassword: 'freshly-generated-pw' }
    );

    await page.evaluate(() => {
      document.documentElement
        .querySelector('[data-cosmic-bwarden-generate-icon="new-password"]')
        .shadowRoot.querySelector('.icon-btn')
        .click();
    });

    await expect.poll(() => page.$eval('#new-password', (el) => el.value)).toBe(
      'freshly-generated-pw'
    );
    expect(await page.$eval('#confirm-password', (el) => el.value)).toBe('freshly-generated-pw');
    expect(await page.$eval('#current-password', (el) => el.value)).toBe('');

    const sent = await page.evaluate(() => window._sentMessages);
    expect(sent).toContainEqual({ GeneratePassword: { settings: null } });
  });
});
