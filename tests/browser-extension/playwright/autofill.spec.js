import { test, expect } from '@playwright/test';
import path from 'path';
import fs from 'fs';

const EXTENSION_PATH = path.resolve(__dirname, '../../../browser-extension');

test.describe('Autofill Content Script', () => {
  test('should fill login form', async ({ page, context }) => {
    // 1. Setup a mock login page
    await page.setContent(`
      <form id="login-form">
        <input type="text" name="user" id="user-field">
        <input type="password" name="pass" id="pass-field">
        <button type="submit">Login</button>
      </form>
    `);

    // 2. Inject the content scripts (heuristics first, mirroring manifest order)
    const contentScript =
      fs.readFileSync(path.join(EXTENSION_PATH, 'content-heuristics.js'), 'utf8') +
      '\n' +
      fs.readFileSync(path.join(EXTENSION_PATH, 'content.js'), 'utf8');

    // Wrap content script to capture the listener
    await page.evaluate((script) => {
      window.browser = {
        runtime: {
          onMessage: {
            addListener: (listener) => {
              window._contentScriptListener = listener;
            }
          }
        }
      };
      eval(script);
    }, contentScript);

    // 3. Simulate receiving a FILL_FORM message (new format: flat username/password)
    await page.evaluate(() => {
      window._contentScriptListener({
        type: 'FILL_FORM',
        username: 'autofill-user',
        password: 'autofill-password'
      });
    });

    // 4. Verify fields are filled
    const usernameValue = await page.$eval('#user-field', el => el.value);
    const passwordValue = await page.$eval('#pass-field', el => el.value);

    expect(usernameValue).toBe('autofill-user');
    expect(passwordValue).toBe('autofill-password');
  });

  test('fills the username on a multi-step login page with no password field yet', async ({ page }) => {
    // Google/Microsoft/SSO-style flow: only an email/username field is
    // present on the first screen; the password field appears after "Next".
    await page.setContent(`
      <form id="login-form">
        <input type="email" name="identifier" id="identifier-field" autocomplete="username">
        <button type="submit">Next</button>
      </form>
    `);

    const contentScript =
      fs.readFileSync(path.join(EXTENSION_PATH, 'content-heuristics.js'), 'utf8') +
      '\n' +
      fs.readFileSync(path.join(EXTENSION_PATH, 'content.js'), 'utf8');

    await page.evaluate((script) => {
      window.browser = {
        runtime: {
          onMessage: {
            addListener: (listener) => {
              window._contentScriptListener = listener;
            }
          }
        }
      };
      eval(script);
    }, contentScript);

    await page.evaluate(() => {
      window._contentScriptListener({
        type: 'FILL_FORM',
        username: 'autofill-user',
        password: 'autofill-password'
      });
    });

    const usernameValue = await page.$eval('#identifier-field', el => el.value);
    expect(usernameValue).toBe('autofill-user');
  });
});
