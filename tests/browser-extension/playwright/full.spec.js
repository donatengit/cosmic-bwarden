import { test, expect } from '@playwright/test';
import { loadFirefoxAddon, runCli } from './test-utils';
import path from 'path';

const EXTENSION_PATH = path.resolve(__dirname, '../../../browser-extension');

// Install once per worker (browser-level, persists across contexts) and let
// the addon settle before any test navigates to the popup.
//
// The four tests below are SKIPPED: Playwright's Firefox (juggler) build
// cannot navigate moz-extension:// pages at all — every page.goto to an
// extension origin times out, even for a bogus UUID (verified empirically;
// the same limitation is tracked upstream in puppeteer#6616 and
// microsoft/playwright#ca49d50). The popup logic is covered in Firefox by
// the firefox-mock project (file:// + mocked chrome APIs) and by the
// chrome-full project against the real extension.
let popupUrl;

test.describe('Extension Full E2E', () => {
  test.setTimeout(60000); // Increase timeout for full E2E

  test.beforeAll(async ({ browser }) => {
    // Destructuring `browser` is load-bearing: it forces the worker browser
    // (with --start-debugger-server 12345) to launch before we connect to it.
    void browser;
    const uuid = await loadFirefoxAddon(12345, EXTENSION_PATH);
    popupUrl = `moz-extension://${uuid}/popup/popup.html`;
    console.log(`Detected extension internal UUID: ${uuid}`);
    await new Promise((resolve) => setTimeout(resolve, 3000));
  });

  test.skip('should show login prompt when not logged in', async ({ page }) => {
    // Ensure we are locked
    try { runCli('lock'); } catch (e) {}

    await page.goto(popupUrl);
    const status = page.locator('#status');
    await expect(status).toBeVisible();
    await expect(status).toHaveText(/Not logged in|Please log in/);
  });

  test.skip('should show vault entries after login', async ({ page }) => {
    // 1. Ensure we are unlocked for subsequent tests
    // The bash script already logged us in, but the previous test locked it.
    // workers=1, so tests run in sequence.
    const PASSWORD = 'password123';
    try { runCli(`unlock --password ${PASSWORD}`); } catch (e) {}

    // 2. Add a test entry using correct syntax
    runCli('add "E2E Test Entry" username=e2e-user password=e2e-password');

    // 3. Open popup and verify entry
    await page.goto(popupUrl);

    const entryName = page.locator('.entry-name', { hasText: 'E2E Test Entry' });
    await expect(entryName).toBeVisible({ timeout: 10000 });

    const entryUser = page.locator('.entry-user', { hasText: 'e2e-user' });
    await expect(entryUser).toBeVisible();
  });

  test.skip('should copy password to clipboard', async ({ page }) => {
    // Mock clipboard since Firefox in Playwright has issues with real clipboard
    await page.evaluate(() => {
      let clipboardText = '';
      navigator.clipboard.writeText = async (text) => {
        clipboardText = text;
      };
      navigator.clipboard.readText = async () => clipboardText;
    });

    await page.goto(popupUrl);

    const copyBtn = page.locator('.entry:has-text("E2E Test Entry") button:has-text("Copy")');
    await copyBtn.click();

    // Check if the clipboard was actually updated
    const clipboardText = await page.evaluate(() => navigator.clipboard.readText());
    expect(clipboardText).toBe('e2e-password');
  });

  test.skip('should autofill login form', async ({ page, context }) => {
    // 1. Create a dummy login page
    const testPage = await context.newPage();
    await testPage.setContent(`
      <form>
        <input type="text" id="username" placeholder="Username">
        <input type="password" id="password" placeholder="Password">
      </form>
    `);

    // 2. Open popup and click fill
    await page.goto(popupUrl);
    const fillBtn = page.locator('.entry:has-text("E2E Test Entry") button:has-text("Fill")');
    await fillBtn.click();

    // 3. Verify fields on the test page
    // Content script should have received the message and filled the form
    const usernameValue = await testPage.locator('#username').inputValue();
    const passwordValue = await testPage.locator('#password').inputValue();

    expect(usernameValue).toBe('e2e-user');
    expect(passwordValue).toBe('e2e-password');
  });
});
