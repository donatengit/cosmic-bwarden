import { test, expect } from '@playwright/test';
import path from 'path';
import fs from 'fs';

const EXTENSION_PATH = path.resolve(__dirname, '../../../browser-extension');
const FIXTURE_URL = 'file://' + path.resolve(__dirname, '../fixtures/login_page.html');

// Inject content scripts with a stubbed WebExtension API: outbound messages
// are recorded in window._sentMessages, inbound listeners captured in
// window._contentListeners (mirrors the autofill.spec.js pattern).
async function injectContentScripts(page, files) {
  const source = files
    .map((f) => fs.readFileSync(path.join(EXTENSION_PATH, f), 'utf8'))
    .join('\n');
  await page.evaluate((script) => {
    window._sentMessages = [];
    window._contentListeners = [];
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
    eval(script);
  }, source);
}

test.describe('Submit capture (content-submit.js)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(FIXTURE_URL);
    await injectContentScripts(page, ['content-heuristics.js', 'content-submit.js']);
  });

  test('clicking submit reports LOGIN_SUBMITTED exactly once (click+submit de-dupe)', async ({ page }) => {
    await page.fill('#ajax-user', 'alice');
    await page.fill('#ajax-password', 'secret1');
    await page.click('#ajax-submit');

    const messages = await page.evaluate(() => window._sentMessages);
    expect(messages).toHaveLength(1);
    expect(messages[0].type).toBe('LOGIN_SUBMITTED');
    expect(messages[0].username).toBe('alice');
    expect(messages[0].password).toBe('secret1');
    expect(messages[0].url).toContain('login_page.html');
  });

  test('pressing Enter in the password field reports the submission once', async ({ page }) => {
    await page.fill('#ajax-user', 'bob');
    await page.fill('#ajax-password', 'secret2');
    await page.press('#ajax-password', 'Enter');

    const messages = await page.evaluate(() => window._sentMessages);
    expect(messages).toHaveLength(1);
    expect(messages[0]).toMatchObject({
      type: 'LOGIN_SUBMITTED',
      username: 'bob',
      password: 'secret2',
    });
  });

  test('captures the classic login form via its submit event', async ({ page }) => {
    await page.fill('#username', 'carol');
    await page.fill('#password', 'secret3');
    await page.evaluate(() => {
      // Fire a real submit (covers form.requestSubmit()) but cancel the
      // navigation at bubble phase — the capture listener has already run.
      const form = document.getElementById('login-form');
      form.addEventListener('submit', (e) => e.preventDefault());
      form.requestSubmit();
    });

    const messages = await page.evaluate(() => window._sentMessages);
    expect(messages).toHaveLength(1);
    expect(messages[0]).toMatchObject({ username: 'carol', password: 'secret3' });
  });

  test('captures registration forms (email field, differently named password)', async ({ page }) => {
    await page.fill('#email', 'dave@example.com');
    await page.fill('#reg-password', 'chosen-pw');
    await page.evaluate(() => {
      const form = document.getElementById('register-form');
      form.addEventListener('submit', (e) => e.preventDefault());
      form.requestSubmit();
    });

    const messages = await page.evaluate(() => window._sentMessages);
    expect(messages).toHaveLength(1);
    expect(messages[0]).toMatchObject({ username: 'dave@example.com', password: 'chosen-pw' });
  });

  test('does not report when the password is empty', async ({ page }) => {
    await page.fill('#ajax-user', 'alice');
    await page.click('#ajax-submit');

    const messages = await page.evaluate(() => window._sentMessages);
    expect(messages).toHaveLength(0);
  });
});

test.describe('Save bar (content-bar.js)', () => {
  async function showBar(page, message) {
    await page.evaluate((msg) => {
      window._contentListeners.forEach((l) => l(msg));
    }, message);
  }

  test.beforeEach(async ({ page }) => {
    await page.setContent('<h1>Some page</h1>');
    await injectContentScripts(page, ['content-bar.js']);
  });

  test('renders the save bar and reports the save action', async ({ page }) => {
    await showBar(page, { type: 'SHOW_SAVE_BAR', mode: 'save', domain: 'example.com', username: 'alice' });

    const bar = page.locator('#cosmic-bwarden-save-bar');
    await expect(bar.locator('.text')).toHaveText('Save password for example.com in COSMIC BWarden?');
    await expect(bar.locator('button.primary')).toHaveText('Save');

    await bar.locator('button.primary').click();
    await expect(bar.locator('.text')).toHaveText('Saving…');
    await expect(bar.locator('button.primary')).toBeDisabled();

    const messages = await page.evaluate(() => window._sentMessages);
    expect(messages).toEqual([{ type: 'SAVE_BAR_ACTION', action: 'save' }]);

    // Background confirms success by hiding the bar.
    await showBar(page, { type: 'HIDE_SAVE_BAR' });
    await expect(bar).toHaveCount(0);
  });

  test('renders the update bar with the entry name', async ({ page }) => {
    await showBar(page, {
      type: 'SHOW_SAVE_BAR', mode: 'update', domain: 'example.com',
      username: 'alice', entryName: 'My example.com login',
    });

    const bar = page.locator('#cosmic-bwarden-save-bar');
    await expect(bar.locator('.text')).toHaveText('Update password for "My example.com login" in COSMIC BWarden?');
    await expect(bar.locator('button.primary')).toHaveText('Update');

    await bar.locator('button.primary').click();
    const messages = await page.evaluate(() => window._sentMessages);
    expect(messages).toEqual([{ type: 'SAVE_BAR_ACTION', action: 'update' }]);
  });

  test('renders the locked bar and reports the unlock action', async ({ page }) => {
    await showBar(page, { type: 'SHOW_SAVE_BAR', mode: 'locked', domain: 'example.com', username: 'alice' });

    const bar = page.locator('#cosmic-bwarden-save-bar');
    await expect(bar.locator('.text')).toHaveText('Unlock COSMIC BWarden to save this password for example.com');
    await expect(bar.locator('button.primary')).toHaveText('Unlock');

    await bar.locator('button.primary').click();
    const messages = await page.evaluate(() => window._sentMessages);
    expect(messages).toEqual([{ type: 'SAVE_BAR_ACTION', action: 'unlock' }]);

    // Opening the popup needs a user gesture the background doesn't always
    // have, and the failure is silent. Both buttons must stay live so the user
    // can retry or dismiss instead of being left with a dead bar.
    await expect(bar.locator('button.primary')).toBeEnabled();
    await expect(bar.locator('button.secondary')).toBeEnabled();
  });

  test('dismiss removes the bar and notifies background', async ({ page }) => {
    await showBar(page, { type: 'SHOW_SAVE_BAR', mode: 'save', domain: 'example.com', username: 'alice' });

    await page.locator('#cosmic-bwarden-save-bar button.secondary').click();
    await expect(page.locator('#cosmic-bwarden-save-bar')).toHaveCount(0);

    const messages = await page.evaluate(() => window._sentMessages);
    expect(messages).toEqual([{ type: 'SAVE_BAR_ACTION', action: 'dismiss' }]);
  });

  test('only one bar exists at a time', async ({ page }) => {
    await showBar(page, { type: 'SHOW_SAVE_BAR', mode: 'save', domain: 'first.com', username: 'a' });
    await showBar(page, { type: 'SHOW_SAVE_BAR', mode: 'save', domain: 'second.com', username: 'b' });

    const bar = page.locator('#cosmic-bwarden-save-bar');
    await expect(bar).toHaveCount(1);
    await expect(bar.locator('.text')).toContainText('second.com');
  });

  test('shows a failure message on SAVE_BAR_ERROR', async ({ page }) => {
    await showBar(page, { type: 'SHOW_SAVE_BAR', mode: 'save', domain: 'example.com', username: 'alice' });
    await showBar(page, { type: 'SAVE_BAR_ERROR' });

    await expect(page.locator('#cosmic-bwarden-save-bar .text'))
      .toContainText('Saving failed');
  });

  test('page text never receives the password', async ({ page }) => {
    // The SHOW_SAVE_BAR contract carries labels only; guard against regressions
    // that would render arbitrary message fields.
    await showBar(page, {
      type: 'SHOW_SAVE_BAR', mode: 'save', domain: 'example.com',
      username: 'alice', password: 'must-never-render',
    });
    const html = await page.evaluate(
      () => document.getElementById('cosmic-bwarden-save-bar').shadowRoot.innerHTML
    );
    expect(html).not.toContain('must-never-render');
  });
});
