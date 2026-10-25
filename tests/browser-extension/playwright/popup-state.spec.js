import { test, expect } from '@playwright/test';
import path from 'path';
import { buildMock, EXTENSION_PATH } from './popup-mock.js';

test.describe('Extension Popup — state survives close/reopen (window switching)', () => {
  // A browser popup is torn down the moment it loses focus (e.g. the
  // context menu closes or the user switches windows); reopening it is a
  // fresh document. Re-navigating to popup.html is the same thing from the
  // script's point of view, so these tests use page.goto() rather than
  // trying to close/reopen a real browser popup.
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(buildMock());
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);
  });

  test('restores the detail view after the popup is closed and reopened', async ({ page }) => {
    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    // Full re-navigation (not reload), see note on the search test below.
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('#detail-content')).toContainText('Test Login');
  });

  test('restores an in-progress edit draft after the popup is closed and reopened', async ({ page }) => {
    await page.locator('#add-btn').click();
    await page.locator('#edit-name').fill('draft name');
    await page.locator('#edit-notes').fill('draft notes');

    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);
    await expect(page.locator('#view-edit')).toBeVisible({ timeout: 5000 });
    expect(await page.locator('#edit-name').inputValue()).toBe('draft name');
    expect(await page.locator('#edit-notes').inputValue()).toBe('draft notes');
  });

  test('does not restore state captured on a different tab', async ({ page }) => {
    // Restored state is scoped to the domain it was captured on. Otherwise one
    // detail view left open would reopen on every unrelated site for the rest
    // of the browser session, hiding the current tab's matches.
    await page.evaluate(() => sessionStorage.setItem('cosmarden-test:tabUrlOverride', 'https://example.com/login'));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);
    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    await page.evaluate(() => sessionStorage.setItem('cosmarden-test:tabUrlOverride', 'https://unrelated.test/'));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);

    await expect(page.locator('#view-list')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('#view-detail')).toBeHidden();
  });

  test('restores the typed search query after the popup is closed and reopened', async ({ page }) => {
    await page.locator('#search').fill('facebook');

    // All three tests use full re-navigation rather than reload(): reload()
    // would let Firefox restore input values from its form state, which could
    // mask the lost module-level state these tests are about — a reopened
    // popup is a brand-new document.
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);
    expect(await page.locator('#search').inputValue()).toBe('facebook');
  });
});
