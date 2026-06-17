import { test, expect } from '@playwright/test';
import { loadFirefoxAddon, runCli } from './test-utils';
import path from 'path';

const EXTENSION_PATH = path.resolve(__dirname, '../../../browser-extension');

test.describe('Extension Full E2E', () => {
  let extensionId;
  let popupUrl;

  test.setTimeout(60000); // Increase timeout for full E2E

  test.beforeEach(async ({ page, context }) => {
    // 1. Install the extension temporarily
    // We still need this because Playwright doesn't natively support Firefox extensions via config
    await loadFirefoxAddon(12345, EXTENSION_PATH);

    // 2. Find the internal UUID of the extension dynamically
    // In MV2, we can wait for the background page
    const backgroundPage = await context.waitForEvent('backgroundpage', { timeout: 30000 });
    const url = backgroundPage.url();
    extensionId = url.split('/')[2];
    popupUrl = `moz-extension://${extensionId}/popup/popup.html`;
    
    console.log(`Detected extension internal UUID: ${extensionId}`);
  });

  test('should show login prompt when not logged in', async ({ page }) => {
    // Ensure we are locked
    try { runCli('lock'); } catch(e) {}

    await page.goto(popupUrl);
    const status = page.locator('#status');
    await expect(status).toBeVisible();
    await expect(status).toHaveText(/Not logged in|Please log in/);
  });

  test('should show vault entries after login', async ({ page }) => {
    // 1. Ensure we are unlocked for subsequent tests
    // The bash script already logged us in, but we might have locked it in the previous test
    // Actually, Playwright tests run in isolation or sequence? Here workers=1, so sequence.
    // If 'should show login prompt' runs first, it will lock the vault.
    // So we must unlock it here.
    const PASSWORD = "password123";
    try { runCli(`unlock --password ${PASSWORD}`); } catch(e) {}
    
    // 2. Add a test entry using correct syntax
    runCli('add "E2E Test Entry" username=e2e-user password=e2e-password');

    // 3. Open popup and verify entry
    await page.goto(popupUrl);
    
    const entryName = page.locator('.entry-name', { hasText: 'E2E Test Entry' });
    await expect(entryName).toBeVisible({ timeout: 10000 });
    
    const entryUser = page.locator('.entry-user', { hasText: 'e2e-user' });
    await expect(entryUser).toBeVisible();
  });

  test('should copy password to clipboard', async ({ page }) => {
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

  test('should autofill login form', async ({ page, context }) => {
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
