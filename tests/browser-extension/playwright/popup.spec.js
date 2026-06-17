import { test, expect } from '@playwright/test';
import path from 'path';
import fs from 'fs';

const EXTENSION_PATH = path.resolve(__dirname, '..');

test.describe('Extension Popup', () => {
  test.beforeEach(async ({ page, context }) => {
    // Forward browser console logs to the test runner
    page.on('console', msg => console.log(`BROWSER: ${msg.text()}`));

    // Mock the WebExtension APIs
    await context.addInitScript(() => {
      window.browser = {
        runtime: {
          sendMessage: async (message) => {
            if (message === 'GetConfig') {
              return { Config: { is_locked: false, needs_login: false } };
            }
            if (message.GetSidebarEntries) {
              return {
                SidebarEntries: {
                  entries: [
                    { id: '1', name: 'Test Login', username: 'testuser' }
                  ]
                }
              };
            }
            if (message.GetPassword) {
              return { Password: { password: 'testpassword' } };
            }
            return { Error: { message: 'Unknown mock request' } };
          },
          connectNative: () => ({
            onMessage: { addListener: () => {} },
            onDisconnect: { addListener: () => {} },
            postMessage: () => {},
            disconnect: () => {}
          })
        },
        tabs: {
          query: async () => [{ id: 123 }],
          sendMessage: async () => {}
        }
      };
    });

    // Load the popup HTML directly (since we are mocking the environment)
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);
  });

  test('should display entries', async ({ page }) => {
    const entryName = page.locator('.entry-name');
    await expect(entryName).toHaveText('Test Login');
    
    const entryUser = page.locator('.entry-user');
    await expect(entryUser).toHaveText('testuser');
  });

  test('should copy password', async ({ page, context }) => {
    // Firefox doesn't support clipboard-read permission in Playwright
    // and navigator.clipboard.readText() might fail in headless.
    // We can mock the clipboard API in the browser context instead.
    await page.evaluate(() => {
      let clipboardText = '';
      navigator.clipboard.writeText = async (text) => {
        clipboardText = text;
      };
      navigator.clipboard.readText = async () => clipboardText;
    });
    
    const copyBtn = page.locator('button:has-text("Copy")').first();
    await copyBtn.click();
    
    // Check if the clipboard was actually updated
    const clipboardText = await page.evaluate(() => navigator.clipboard.readText());
    expect(clipboardText).toBe('testpassword');
  });
});
