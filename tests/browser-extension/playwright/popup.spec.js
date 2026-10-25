import { test, expect } from '@playwright/test';
import path from 'path';
import { buildMock, EXTENSION_PATH } from './popup-mock.js';

test.describe('Extension Popup', () => {
  test.beforeEach(async ({ page }) => {
    page.on('console', msg => {
      if (msg.type() === 'error') console.log(`BROWSER ERROR: ${msg.text()}`);
    });
    await page.addInitScript(buildMock());
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);
  });

  test('displays entries from agent', async ({ page }) => {
    await expect(page.locator('.entry-name')).toHaveText('Test Login');
    await expect(page.locator('.entry-user')).toHaveText('testuser');
  });

  test('lock button is in the header row, footer is absent', async ({ page }) => {
    const lockBtn = page.locator('#lock-btn');
    await expect(lockBtn).toBeVisible();
    const isInHeader = await page.evaluate(() =>
      document.getElementById('header').contains(document.getElementById('lock-btn'))
    );
    expect(isInHeader).toBe(true);
    expect(await page.locator('#footer').count()).toBe(0);
  });

  test('icon-only buttons center their icon (regression: icon sat flush left)', async ({ page }) => {
    // Every .btn-icon carries its own flex centering; the SVG's center must
    // match the button's center, not hug the left edge.
    for (const id of ['fav-btn', 'add-btn', 'sync-btn', 'lock-btn']) {
      const btn = page.locator(`#${id}`);
      await expect(btn).toBeVisible();
      const btnBox = await btn.boundingBox();
      const iconBox = await btn.locator('svg').boundingBox();
      expect(btnBox).not.toBeNull();
      expect(iconBox).not.toBeNull();
      const delta = Math.abs(
        (iconBox.x + iconBox.width / 2) - (btnBox.x + btnBox.width / 2)
      );
      expect(delta, `#${id} icon not centered (off by ${delta}px)`).toBeLessThanOrEqual(1);
    }
  });

  test('each Login entry row has a Fill, copy-dropdown, and view-details button', async ({ page }) => {
    await expect(page.locator('.entry-actions button[title="Autofill"]')).toBeVisible();
    await expect(page.locator('.entry-actions button[title="Copy username or password"]')).toBeVisible();
    await expect(page.locator('.entry-actions button[title="View details"]')).toBeVisible();
    await expect(page.locator('.entry-actions button[title="View details"]')).toHaveAttribute('aria-label', 'View details');
  });

  test('clicking a Login entry row opens its URL in a new tab', async ({ page }) => {
    await page.addInitScript(buildMock({ entryMetaUri: 'example.com/login' }));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);

    await page.locator('.entry-name', { hasText: 'Test Login' }).click();
    await page.waitForFunction(() => window._tabsCreated && window._tabsCreated.length > 0);

    const created = await page.evaluate(() => window._tabsCreated);
    expect(created).toEqual([{ url: 'https://example.com/login' }]);

    const msgs = await page.evaluate(() => window._sentMessages);
    expect(msgs.some(m => m && m.GetEntryMeta)).toBe(true);
    expect(msgs.some(m => m && m.GetEntry)).toBe(false);
  });

  test('clicking a Login entry row with no saved URL shows a status message instead of opening a tab', async ({ page }) => {
    await page.locator('.entry-name', { hasText: 'Test Login' }).click();
    await expect(page.locator('#status')).toBeVisible();
    await expect(page.locator('#status')).toHaveText('No website saved for this entry.');

    const created = await page.evaluate(() => window._tabsCreated || []);
    expect(created.length).toBe(0);
  });

  test('clicking a Login entry row with no saved URL but a hostname-like name opens that name as a URL', async ({ page }) => {
    const hostnameEntries = [
      { id: '1', name: 'account.facebook.com', username: 'testuser', entry_type: 'Login', is_pinned: false }
    ];
    await page.addInitScript(buildMock({ entriesForQuery: hostnameEntries, entryMetaName: 'account.facebook.com' }));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);

    await page.locator('.entry-name', { hasText: 'account.facebook.com' }).click();
    await page.waitForFunction(() => window._tabsCreated && window._tabsCreated.length > 0);

    const created = await page.evaluate(() => window._tabsCreated);
    expect(created).toEqual([{ url: 'https://account.facebook.com' }]);
  });

  test('copy dropdown: Copy Username writes to clipboard without calling GetPassword', async ({ page }) => {
    await page.evaluate(() => {
      let clip = '';
      navigator.clipboard.writeText = async (t) => { clip = t; };
      navigator.clipboard.readText  = async () => clip;
    });

    await page.locator('.entry-actions button[title="Copy username or password"]').click();
    await page.locator('.copy-dropdown-menu button', { hasText: 'Copy Username' }).click();

    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('testuser');
    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetPassword))).toBe(false);
  });

  test('copy dropdown: Copy Password fetches GetPassword and writes to clipboard', async ({ page }) => {
    await page.evaluate(() => {
      let clip = '';
      navigator.clipboard.writeText = async (t) => { clip = t; };
      navigator.clipboard.readText  = async () => clip;
    });

    await page.locator('.entry-actions button[title="Copy username or password"]').click();
    await page.locator('.copy-dropdown-menu button', { hasText: 'Copy Password' }).click();

    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('testpassword');
    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetPassword))).toBe(true);
  });

  test('copy dropdown closes when clicking outside', async ({ page }) => {
    await page.locator('.entry-actions button[title="Copy username or password"]').click();
    await expect(page.locator('.copy-dropdown-menu')).toBeVisible();

    await page.locator('#search').click();
    await expect(page.locator('.copy-dropdown-menu')).toHaveCount(0);
  });

  test('view-details button opens detail via GetEntryMeta, never GetEntry', async ({ page }) => {
    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    const msgs = await page.evaluate(() => window._sentMessages);
    expect(msgs.some(m => m && m.GetEntryMeta)).toBe(true);
    expect(msgs.some(m => m && m.GetEntry)).toBe(false);
  });

  test('detail view shows masked placeholder, not the real password', async ({ page }) => {
    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    const maskedText = await page.locator('.secret-text').first().textContent();
    expect(maskedText).toBe('••••••••');
    await expect(page.locator('button.reveal-btn').first()).toBeVisible();
  });

  test('reveal button triggers GetPassword and shows plaintext', async ({ page }) => {
    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    // Not yet called
    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetPassword))).toBe(false);

    await page.locator('button.reveal-btn').first().click();

    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetPassword))).toBe(true);
    expect(await page.locator('.secret-text').first().textContent()).toBe('testpassword');
  });

  test('password copy button triggers GetPassword and writes to clipboard', async ({ page }) => {
    await page.evaluate(() => {
      let clip = '';
      navigator.clipboard.writeText = async (t) => { clip = t; };
      navigator.clipboard.readText  = async () => clip;
    });

    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    // Username's copy-btn comes first in the DOM; password's is the second.
    await page.locator('button.copy-btn').nth(1).click();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('testpassword');
  });

  test('username copy button writes to clipboard without calling GetPassword', async ({ page }) => {
    await page.evaluate(() => {
      let clip = '';
      navigator.clipboard.writeText = async (t) => { clip = t; };
      navigator.clipboard.readText  = async () => clip;
    });

    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    await page.locator('button.copy-btn').first().click();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('testuser');
    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetPassword))).toBe(false);
  });

  test('detail view shows a masked row for every filled secret (incl. hidden custom fields) and none for empty ones', async ({ page }) => {
    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    // Filled secrets (Password, TOTP, hidden 'API Key' custom field) each get a row.
    const labels = await page.locator('.detail-label').allTextContents();
    expect(labels).toContain('Password');
    expect(labels).toContain('TOTP');
    expect(labels).toContain('API Key');

    // All masked — plaintext never shown on a passive detail read.
    const masked = await page.locator('.secret-text').count();
    expect(masked).toBeGreaterThanOrEqual(3);
    for (const el of await page.locator('.secret-text').all()) {
      expect(await el.textContent()).toBe('••••••••');
    }

    // The hidden custom field value did not leak into detail.
    const detailText = await page.locator('#detail-content').innerText();
    expect(detailText).not.toContain('sk-live-abc');
  });

  test('multi-line notes keep their line breaks in the detail view', async ({ page }) => {
    // A SecureNote's body is its notes, fetched in full by showDetail. Its
    // newlines are content: the value must lay out on separate lines rather
    // than collapse into one run. `.detail-value` is also the container of the
    // revealed `.secret-text` span, which is how a non-SecureNote entry's notes
    // appear, so one rule covers both paths.
    const meta = { id: 'n1', name: 'Note', data: 'SecureNote', notes: null, fields: [] };
    const full = {
      id: 'n1',
      name: 'Note',
      data: 'SecureNote',
      notes: 'first line\nsecond line\nthird line',
      fields: [],
    };
    await page.addInitScript(
      ([note, body]) => {
        const original = window.browser.runtime.sendMessage;
        window.browser.runtime.sendMessage = async (message) => {
          if (message && message.GetEntryMeta) {
            return { EntryMeta: { entry: note, filled_secrets: [] } };
          }
          if (message && message.GetEntry) return { Entry: { entry: body } };
          return original(message);
        };
      },
      [meta, full]
    );
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);
    await page.evaluate(() => showDetail('n1'));
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    const notes = page
      .locator('.detail-item', { hasText: 'Notes' })
      .locator('.detail-value')
      .first();
    const box = await notes.evaluate((el) => ({
      whiteSpace: getComputedStyle(el).whiteSpace,
      height: el.getBoundingClientRect().height,
      lineHeight: parseFloat(getComputedStyle(el).lineHeight),
      text: el.textContent,
    }));
    expect(box.whiteSpace).toBe('pre-wrap');
    expect(box.text).toContain('first line\nsecond line\nthird line');
    // Three lines tall. A collapsed newline draws one line; the copy button is
    // shorter than a line of text, so it cannot reach two lines by itself.
    expect(box.height).toBeGreaterThan(box.lineHeight * 2);
  });

  test('hidden custom field reveal fetches the full entry (GetEntry) and shows its value', async ({ page }) => {
    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    // Reveal rows are ordered: Password, TOTP, then the hidden 'API Key' field.
    await page.locator('button.reveal-btn').nth(2).click();

    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetEntry))).toBe(true);
    // Reveal rows ordered Password(0), TOTP(1), API Key(2); the API Key reveal
    // filled slot 2 with the fetched value.
    expect(await page.locator('.secret-text').nth(2).textContent()).toBe('sk-live-abc');
  });

  test('TOTP reveal/copy always refetches the live code (never a cached/stale one)', async ({ page }) => {
    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    // Reveal rows: Password(0), TOTP(1), API Key(2). Reveal TOTP.
    await page.locator('button.reveal-btn').nth(1).click();
    expect(await page.locator('.secret-text').nth(1).textContent()).toBe('100001');

    // Hide and reveal again — the code has rotated to 100002, proving a fresh
    // GetTotp round-trip rather than a memoized value.
    await page.locator('button.reveal-btn').nth(1).click();
    await page.locator('button.reveal-btn').nth(1).click();
    expect(await page.locator('.secret-text').nth(1).textContent()).toBe('100002');

    // Copy also refetches, not a cached code. Write to a local so we can read it back.
    await page.evaluate(() => {
      let clip = '';
      navigator.clipboard.writeText = async (t) => { clip = t; };
      navigator.clipboard.readText  = async () => clip;
    });
    await page.locator('.detail-item').filter({ hasText: 'TOTP' }).locator('button.copy-btn').click();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('100003');
  });

  test('detail view Edit button re-fetches the full entry via GetEntry before opening the edit form', async ({ page }) => {
    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    await page.locator('#edit-btn').click();
    await expect(page.locator('#view-edit')).toBeVisible({ timeout: 5000 });

    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetEntry))).toBe(true);
    expect(await page.locator('.password-field-wrap input').inputValue()).toBe('testpassword');
  });

  test('SetTheme message is sent to background on popup init', async ({ page }) => {
    expect(await page.evaluate(() =>
      window._sentMessages.some(m => m && m.SetTheme !== undefined)
    )).toBe(true);
  });
});
