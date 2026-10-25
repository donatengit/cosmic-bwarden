import { test, expect } from '@playwright/test';
import path from 'path';
import { buildMock, EXTENSION_PATH } from './popup-mock.js';

test.describe('Extension Popup — locked vault, no PIN configured', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(buildMock({ isLocked: true }));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);
  });

  test('shows locked message (no PIN input) and sends RequestUnlock to agent', async ({ page }) => {
    await expect(page.locator('#view-locked')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('#locked-message')).toContainText('locked');
    await expect(page.locator('#locked-pin-group')).not.toBeVisible();

    expect(await page.evaluate(() =>
      window._sentMessages.some(m => m === 'RequestUnlock')
    )).toBe(true);
    expect(await page.evaluate(() =>
      window._sentMessages.some(m => m === 'CheckTpm')
    )).toBe(true);
  });
});

test.describe('Extension Popup — locked vault, PIN unlock configured', () => {
  const pinTpmStatus = { available: true, configured: true, server_credentials: false };

  test('shows the PIN input', async ({ page }) => {
    await page.addInitScript(buildMock({ isLocked: true, tpmStatus: pinTpmStatus }));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);

    await expect(page.locator('#locked-pin-group')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('#locked-fallback-btn')).toBeVisible();
  });

  test('correct PIN unlocks and returns to the entry list', async ({ page }) => {
    await page.addInitScript(buildMock({
      isLocked: true, tpmStatus: pinTpmStatus, unlockPinResult: 'Ack',
    }));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);

    await page.locator('#locked-pin-input').fill('1234');
    await page.locator('#locked-unlock-btn').click();

    await expect(page.locator('#view-list')).toBeVisible({ timeout: 5000 });
    const msgs = await page.evaluate(() => window._sentMessages);
    expect(msgs.some(m => m && m.UnlockWithPin && m.UnlockWithPin.pin === '1234')).toBe(true);
  });

  test('wrong PIN shows attempts-remaining feedback and stays locked', async ({ page }) => {
    await page.addInitScript(buildMock({
      isLocked: true, tpmStatus: pinTpmStatus,
      unlockPinResult: { Error: { message: 'TPM unseal failed' } },
    }));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);

    await page.locator('#locked-pin-input').fill('0000');
    await page.locator('#locked-unlock-btn').click();

    await expect(page.locator('#locked-feedback')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('#locked-feedback')).toContainText('29 of 32 attempts remaining');
    await expect(page.locator('#view-locked')).toBeVisible();
    // The PIN field is cleared after a failed attempt.
    expect(await page.locator('#locked-pin-input').inputValue()).toBe('');

    const msgs = await page.evaluate(() => window._sentMessages);
    expect(msgs.some(m => m === 'GetTpmDaStatus')).toBe(true);
  });

  test('an environmental error is shown as-is, not as an incorrect PIN', async ({ page }) => {
    await page.addInitScript(buildMock({
      isLocked: true, tpmStatus: pinTpmStatus,
      unlockPinResult: { Error: { message: 'no account configured — please login first' } },
    }));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);

    await page.locator('#locked-pin-input').fill('1234');
    await page.locator('#locked-unlock-btn').click();

    await expect(page.locator('#locked-feedback')).toContainText('no account configured');
  });

  test('fallback button switches to the master-password-elsewhere message', async ({ page }) => {
    await page.addInitScript(buildMock({ isLocked: true, tpmStatus: pinTpmStatus }));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);

    await expect(page.locator('#locked-pin-group')).toBeVisible({ timeout: 5000 });
    await page.locator('#locked-fallback-btn').click();

    await expect(page.locator('#locked-pin-group')).not.toBeVisible();
    await expect(page.locator('#locked-fallback-btn')).not.toBeVisible();
    await expect(page.locator('#locked-message')).toContainText('COSMIC app or applet');
  });
});
