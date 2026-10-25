import { test, expect } from '@playwright/test';
import path from 'path';
import { buildMock, EXTENSION_PATH } from './popup-mock.js';

// Popup generator menu (popup-generator.js): Generate password round-trips
// through the agent's GeneratePassword action and copies the result; Show
// previous renders the agent's 7-day history. The shared mock
// (popup-mock.js) switches agent behaviour per test through sessionStorage.
const POPUP_URL = `file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`;

async function openPopup(page, options = {}) {
  page.on('console', msg => {
    if (msg.type() === 'error') console.log(`BROWSER ERROR: ${msg.text()}`);
  });
  await page.addInitScript(buildMock(options));
  await page.goto(POPUP_URL);
}

// Headless Firefox does not grant real clipboard access, so record writes in a
// variable the test can read back — the same technique popup.spec.js uses.
async function stubClipboard(page) {
  await page.evaluate(() => {
    let clip = '';
    navigator.clipboard.writeText = async (t) => { clip = t; };
    navigator.clipboard.readText = async () => clip;
  });
}

async function failClipboardWrites(page) {
  await page.evaluate(() => {
    navigator.clipboard.readText = async () => '';
    navigator.clipboard.writeText = async () => { throw new Error('denied'); };
  });
}

async function setMockMode(page, key, value) {
  await page.evaluate(([k, v]) => sessionStorage.setItem(`cosmarden-test:${k}`, v), [key, value]);
}

async function openGeneratorMenu(page) {
  await page.locator('#gen-btn').click();
  await expect(page.locator('.gen-menu')).toBeVisible();
}

async function clickMenuItem(page, label) {
  await page.locator('.gen-menu button', { hasText: label }).click();
}

test.describe('Popup generator menu', () => {
  test.beforeEach(async ({ page }) => {
    await openPopup(page);
  });

  test('sits in the header next to sync and lock', async ({ page }) => {
    await expect(page.locator('#gen-btn')).toBeVisible();
    const ids = await page.locator('#header button').evaluateAll(btns => btns.map(b => b.id));
    expect(ids).toContain('gen-btn');
    // The user asked for it near the vault-state controls: generate comes
    // before sync, and sync before lock.
    expect(ids.indexOf('gen-btn')).toBeLessThan(ids.indexOf('sync-btn'));
    expect(ids.indexOf('sync-btn')).toBeLessThan(ids.indexOf('lock-btn'));

    // The inline SVG actually renders (a malformed path would leave a
    // zero-size icon inside a visible button).
    const box = await page.locator('#gen-btn svg').boundingBox();
    expect(box).not.toBeNull();
    expect(box.width).toBeGreaterThan(8);
    expect(box.height).toBeGreaterThan(8);
  });

  test('opens exactly Generate password and Show previous, and closes again', async ({ page }) => {
    await openGeneratorMenu(page);
    const labels = await page.locator('.gen-menu button').allTextContents();
    expect(labels).toEqual(['Generate password', 'Show previous']);
    expect(await page.locator('#gen-btn').getAttribute('aria-expanded')).toBe('true');

    // Second click on the anchor toggles it shut.
    await page.locator('#gen-btn').click();
    await expect(page.locator('.gen-menu')).toHaveCount(0);
    expect(await page.locator('#gen-btn').getAttribute('aria-expanded')).toBe('false');
  });

  test('closes on Escape and on an outside click', async ({ page }) => {
    await openGeneratorMenu(page);
    await page.keyboard.press('Escape');
    await expect(page.locator('.gen-menu')).toHaveCount(0);

    await openGeneratorMenu(page);
    await page.locator('#search').click();
    await expect(page.locator('.gen-menu')).toHaveCount(0);
  });

  test('is a keyboard ARIA menu: Enter opens, arrows move, Escape returns focus', async ({ page }) => {
    await page.locator('#gen-btn').focus();
    await page.keyboard.press('Enter');

    const first = page.getByRole('menuitem', { name: 'Generate password' });
    const second = page.getByRole('menuitem', { name: 'Show previous' });
    await expect(first).toBeFocused();

    await page.keyboard.press('ArrowDown');
    await expect(second).toBeFocused();
    await page.keyboard.press('ArrowDown'); // wraps to the first item
    await expect(first).toBeFocused();
    await page.keyboard.press('Home');
    await expect(first).toBeFocused();
    await page.keyboard.press('End');
    await expect(second).toBeFocused();

    await page.keyboard.press('Escape');
    await expect(page.locator('.gen-menu')).toHaveCount(0);
    await expect(page.locator('#gen-btn')).toBeFocused();
    expect(await page.locator('#gen-btn').getAttribute('aria-expanded')).toBe('false');
  });

  test('Tab closes the menu and continues from the trigger', async ({ page }) => {
    await page.locator('#gen-btn').focus();
    await page.keyboard.press('Enter');
    await expect(page.getByRole('menuitem', { name: 'Generate password' })).toBeFocused();

    await page.keyboard.press('Tab');

    await expect(page.locator('.gen-menu')).toHaveCount(0);
    // The next focusable control after the generate button is Sync.
    await expect(page.locator('#sync-btn')).toBeFocused();
  });

  test('Generate sends the reuse-last-settings request and copies what the agent returned', async ({ page }) => {
    await stubClipboard(page);
    await setMockMode(page, 'generatedPassword', 'Xy9!generated');
    await page.evaluate(() => { window._sentMessages.length = 0; });

    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Generate password');

    await expect(page.locator('#status')).toHaveText('Password generated and copied to clipboard.');
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('Xy9!generated');
    // settings:null is the "reuse whatever the user last configured" request.
    expect(await page.evaluate(() => window._sentMessages)).toEqual([
      { GeneratePassword: { settings: null } },
    ]);
    // The popup stays open on the list: generating is not a navigation.
    await expect(page.locator('#view-list')).toBeVisible();
    await expect(page.locator('.gen-menu')).toHaveCount(0);
  });

  test('Generate surfaces an agent error and copies nothing', async ({ page }) => {
    await stubClipboard(page);
    await setMockMode(page, 'generateMode', 'error');

    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Generate password');

    await expect(page.locator('#status')).toHaveText('generator rejected the settings');
    await expect(page.locator('#status')).toHaveClass(/status-error/);
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('');
  });

  test('Generate rejects an empty password instead of copying it', async ({ page }) => {
    await stubClipboard(page);
    await setMockMode(page, 'generateMode', 'empty');

    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Generate password');

    await expect(page.locator('#status')).toHaveText('Unexpected agent response.');
    await expect(page.locator('#status')).toHaveClass(/status-error/);
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('');
  });

  test('Generate rejects a malformed agent response', async ({ page }) => {
    await stubClipboard(page);
    await setMockMode(page, 'generateMode', 'bad');

    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Generate password');

    await expect(page.locator('#status')).toHaveText('Unexpected agent response.');
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('');
  });

  test('Generate reports an unreachable agent', async ({ page }) => {
    await stubClipboard(page);
    await setMockMode(page, 'generateMode', 'throw');

    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Generate password');

    await expect(page.locator('#status')).toHaveText('Agent disconnected');
    await expect(page.locator('#status')).toHaveClass(/status-error/);
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('');
  });

  test('Generate says the password was generated when only the clipboard fails', async ({ page }) => {
    await failClipboardWrites(page);

    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Generate password');

    await expect(page.locator('#status'))
      .toHaveText('Password generated, but copying to the clipboard failed.');
    await expect(page.locator('#status')).toHaveClass(/status-error/);
    // The agent call still happened (and therefore recorded the password in
    // history) — the message must not read as "nothing happened".
    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GeneratePassword))).toBe(true);
  });

  test('is offered on the list and locked views but not in detail or edit', async ({ page }) => {
    await expect(page.locator('#gen-btn')).toBeVisible();

    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('#gen-btn')).toBeHidden();

    await page.locator('#edit-btn').click();
    await expect(page.locator('#view-edit')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('#gen-btn')).toBeHidden();

    await page.locator('#cancel-btn').click();
    // Cancel returns to the detail view (the entry is still open), which also
    // hides the generator.
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('#gen-btn')).toBeHidden();

    await page.locator('#back-btn').click();
    await expect(page.locator('#view-list')).toBeVisible();
    await expect(page.locator('#gen-btn')).toBeVisible();
  });

  test('generates while the vault is locked (no unlock needed)', async ({ page }) => {
    // Re-opens the page with a locked mock: addInitScript scripts accumulate,
    // and the one added last assigns the browser mock that wins on this
    // navigation (the outer beforeEach already added the unlocked one).
    await openPopup(page, { isLocked: true });
    await expect(page.locator('#view-locked')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('#gen-btn')).toBeVisible();

    await stubClipboard(page);
    await setMockMode(page, 'generatedPassword', 'locked-but-generated');

    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Generate password');

    await expect(page.locator('#status')).toHaveText('Password generated and copied to clipboard.');
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('locked-but-generated');
    await expect(page.locator('#view-locked')).toBeVisible();
  });
});

test.describe('Popup generated-password history', () => {
  test.beforeEach(async ({ page }) => {
    await openPopup(page);
  });

  test('lists the last 7 days newest first, masked', async ({ page }) => {
    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Show previous');

    await expect(page.locator('#view-history')).toBeVisible();
    const rows = page.locator('#history-content .history-entry');
    await expect(rows).toHaveCount(2);
    expect(await page.locator('#history-content .list-caption').textContent()).toBe('Last 7 days');

    // Agent order (newest first) is rendered as received.
    const ages = await page.locator('#history-content .history-age').allTextContents();
    expect(ages[0]).toBe('2m ago');
    expect(ages[1]).toBe('2h ago');

    // Passive view: no plaintext until a reveal click.
    expect(await page.locator('#history-content .history-password').allTextContents())
      .toEqual(['••••••••', '••••••••']);
    expect(await page.locator('#history-content').textContent()).not.toContain('gen-newest');

    // The exact action the agent exposes, not a bespoke message.
    expect(await page.evaluate(() => window._sentMessages.some(m => m === 'GetPasswordHistory'))).toBe(true);
  });

  test('row icons actually paint (SVG namespace regression guard)', async ({ page }) => {
    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Show previous');

    // A namespace-less <svg> keeps its CSS-sized 16px box but paints nothing,
    // so measure a shape inside it — the outer box alone proves nothing.
    const eyeShape = await page.locator('#history-content .history-reveal-btn svg g > *').first().boundingBox();
    expect(eyeShape.width).toBeGreaterThan(0);
    expect(eyeShape.height).toBeGreaterThan(0);

    const copyShape = await page.locator('#history-content .history-copy-btn svg g > *').first().boundingBox();
    expect(copyShape.width).toBeGreaterThan(0);

    await page.locator('#history-content .history-reveal-btn').first().click();
    const eyeOffShape = await page.locator('#history-content .history-reveal-btn svg g > *').last().boundingBox();
    expect(eyeOffShape.width).toBeGreaterThan(0);

    // List-row icons go through the same setIcon path.
    await page.locator('#back-btn').click();
    const rowShape = await page.locator('.entry-actions button[title="View details"] svg g > *').first().boundingBox();
    expect(rowShape.width).toBeGreaterThan(0);
  });

  test('reveal shows one password and hides it again', async ({ page }) => {
    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Show previous');
    const firstValue = page.locator('#history-content .history-password').first();
    const reveal = page.locator('#history-content .history-reveal-btn').first();

    await expect(firstValue).toHaveText('••••••••');
    await reveal.click();
    await expect(firstValue).toHaveText('gen-newest');
    expect(await reveal.getAttribute('aria-pressed')).toBe('true');

    await reveal.click();
    await expect(firstValue).toHaveText('••••••••');
    expect(await reveal.getAttribute('aria-pressed')).toBe('false');
  });

  test('copy writes that row password to the clipboard', async ({ page }) => {
    await stubClipboard(page);
    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Show previous');

    await page.locator('#history-content .history-copy-btn').nth(1).click();

    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('gen-older');
    await expect(page.locator('#status')).toHaveText('Password copied to clipboard.');
  });

  test('reports a clipboard failure instead of silently doing nothing', async ({ page }) => {
    await failClipboardWrites(page);
    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Show previous');

    await page.locator('#history-content .history-copy-btn').first().click();

    await expect(page.locator('#status'))
      .toHaveText('Failed to copy the password to the clipboard.');
    await expect(page.locator('#status')).toHaveClass(/status-error/);
  });

  test('shows the empty state when nothing was generated recently', async ({ page }) => {
    await page.evaluate(() => sessionStorage.setItem('cosmarden-test:historyEntries', '[]'));
    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Show previous');

    await expect(page.locator('#history-content')).toContainText('No passwords generated in the last 7 days.');
    await expect(page.locator('#history-content .history-entry')).toHaveCount(0);
  });

  test('surfaces an agent error', async ({ page }) => {
    await setMockMode(page, 'historyMode', 'error');
    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Show previous');

    await expect(page.locator('#status')).toHaveText('history temporarily unavailable');
    await expect(page.locator('#status')).toHaveClass(/status-error/);
    await expect(page.locator('#history-content .history-entry')).toHaveCount(0);
  });

  test('surfaces a malformed history response', async ({ page }) => {
    await setMockMode(page, 'historyMode', 'bad');
    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Show previous');

    await expect(page.locator('#status')).toHaveText('Unexpected agent response.');
    await expect(page.locator('#history-content .history-entry')).toHaveCount(0);
  });

  test('surfaces an unreachable agent', async ({ page }) => {
    await setMockMode(page, 'historyMode', 'throw');
    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Show previous');

    await expect(page.locator('#status')).toHaveText('Agent disconnected');
    await expect(page.locator('#history-content .history-entry')).toHaveCount(0);
  });

  test('back returns to the vault list and drops the rows', async ({ page }) => {
    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Show previous');
    await expect(page.locator('#history-content .history-entry')).toHaveCount(2);

    await page.locator('#back-btn').click();

    await expect(page.locator('#view-list')).toBeVisible();
    await expect(page.locator('#view-history')).toBeHidden();
    await expect(page.locator('#history-content .history-entry')).toHaveCount(0);
  });

  test('reopening the popup restores the list, never the history view', async ({ page }) => {
    await openGeneratorMenu(page);
    await clickMenuItem(page, 'Show previous');
    await expect(page.locator('#view-history')).toBeVisible();

    // A fresh popup document, exactly like a close+reopen.
    await page.goto(POPUP_URL);

    await expect(page.locator('#view-list')).toBeVisible();
    await expect(page.locator('#view-history')).toBeHidden();
    await expect(page.locator('#history-content .history-entry')).toHaveCount(0);
    const stored = await page.evaluate(() => sessionStorage.getItem('cosmarden-test:popupState'));
    expect(JSON.parse(stored).view).toBe('list');
    expect(stored).not.toContain('gen-newest');
  });
});
