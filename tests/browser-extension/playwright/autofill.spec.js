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
      fs.readFileSync(path.join(EXTENSION_PATH, 'host-match.js'), 'utf8') +
      '\n' +
      fs.readFileSync(path.join(EXTENSION_PATH, 'content-dom.js'), 'utf8') +
      '\n' +
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
        password: 'autofill-password',
        entryHosts: [],
      });
    });

    // 4. Verify fields are filled
    const usernameValue = await page.$eval('#user-field', el => el.value);
    const passwordValue = await page.$eval('#pass-field', el => el.value);

    expect(usernameValue).toBe('autofill-user');
    expect(passwordValue).toBe('autofill-password');
  });

  test('does not fill hidden password inputs', async ({ page }) => {
    await page.setContent(`
      <form id="login-form">
        <input type="text" name="user" id="user-field">
        <input type="password" name="pass" id="pass-field">
        <input type="password" name="honeypot" id="honeypot" style="display:none">
      </form>
    `);

    const contentScript =
      fs.readFileSync(path.join(EXTENSION_PATH, 'host-match.js'), 'utf8') +
      '\n' +
      fs.readFileSync(path.join(EXTENSION_PATH, 'content-dom.js'), 'utf8') +
      '\n' +
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
        password: 'autofill-password',
        entryHosts: [],
      });
    });

    expect(await page.$eval('#pass-field', el => el.value)).toBe('autofill-password');
    expect(await page.$eval('#honeypot', el => el.value)).toBe('');
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
      fs.readFileSync(path.join(EXTENSION_PATH, 'host-match.js'), 'utf8') +
      '\n' +
      fs.readFileSync(path.join(EXTENSION_PATH, 'content-dom.js'), 'utf8') +
      '\n' +
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
        password: 'autofill-password',
        entryHosts: [],
      });
    });

    const usernameValue = await page.$eval('#identifier-field', el => el.value);
    expect(usernameValue).toBe('autofill-user');
  });

  test('does not fill a frame whose host does not match the entry', async ({ page }) => {
    // page.setContent runs on about:blank, whose hostname is empty. An entry
    // stored for a real site must not be written into a frame whose origin
    // cannot be named — the content script applies hostMatchesAny itself, so
    // the popup's check is not the only guard.
    await page.setContent(`
      <form>
        <input type="text" name="username" id="user-field">
        <input type="password" name="password" id="pass-field">
      </form>
    `);

    const contentScript =
      fs.readFileSync(path.join(EXTENSION_PATH, 'host-match.js'), 'utf8') +
      '\n' +
      fs.readFileSync(path.join(EXTENSION_PATH, 'content-dom.js'), 'utf8') +
      '\n' +
      fs.readFileSync(path.join(EXTENSION_PATH, 'content-heuristics.js'), 'utf8') +
      '\n' +
      fs.readFileSync(path.join(EXTENSION_PATH, 'content.js'), 'utf8');

    await page.evaluate((script) => {
      window.browser = {
        runtime: {
          onMessage: {
            addListener: (listener) => { window._contentScriptListener = listener; }
          }
        }
      };
      eval(script);
    }, contentScript);

    await page.evaluate(() => {
      window._contentScriptListener({
        type: 'FILL_FORM',
        username: 'autofill-user',
        password: 'autofill-password',
        entryHosts: ['example.com'],
      });
    });

    expect(await page.$eval('#pass-field', el => el.value)).toBe('');
    expect(await page.$eval('#user-field', el => el.value)).toBe('');
  });

  test('fills a login form inside an open shadow root', async ({ page }) => {
    await page.setContent('<div id="host"></div>');
    await page.evaluate(() => {
      const root = document.getElementById('host').attachShadow({ mode: 'open' });
      root.innerHTML = `
        <form>
          <input type="text" name="username" id="user-field">
          <input type="password" name="password" id="pass-field">
        </form>`;
    });

    const contentScript =
      fs.readFileSync(path.join(EXTENSION_PATH, 'host-match.js'), 'utf8') +
      '\n' +
      fs.readFileSync(path.join(EXTENSION_PATH, 'content-dom.js'), 'utf8') +
      '\n' +
      fs.readFileSync(path.join(EXTENSION_PATH, 'content-heuristics.js'), 'utf8') +
      '\n' +
      fs.readFileSync(path.join(EXTENSION_PATH, 'content.js'), 'utf8');

    await page.evaluate((script) => {
      window.browser = {
        runtime: {
          onMessage: {
            addListener: (listener) => { window._contentScriptListener = listener; }
          }
        }
      };
      eval(script);
    }, contentScript);

    await page.evaluate(() => {
      window._contentScriptListener({
        type: 'FILL_FORM',
        username: 'shadow-user',
        password: 'shadow-pass',
        entryHosts: [],
      });
    });

    const values = await page.evaluate(() => {
      const root = document.getElementById('host').shadowRoot;
      return {
        username: root.querySelector('#user-field').value,
        password: root.querySelector('#pass-field').value,
      };
    });
    expect(values).toEqual({ username: 'shadow-user', password: 'shadow-pass' });
  });

  test('fills a password field the page keeps readonly until focus', async ({ page }) => {
    await page.setContent(`
      <form>
        <input type="text" name="username" id="user-field">
        <input type="password" name="password" id="pass-field" readonly>
      </form>
    `);

    const contentScript =
      fs.readFileSync(path.join(EXTENSION_PATH, 'host-match.js'), 'utf8') +
      '\n' +
      fs.readFileSync(path.join(EXTENSION_PATH, 'content-dom.js'), 'utf8') +
      '\n' +
      fs.readFileSync(path.join(EXTENSION_PATH, 'content-heuristics.js'), 'utf8') +
      '\n' +
      fs.readFileSync(path.join(EXTENSION_PATH, 'content.js'), 'utf8');

    await page.evaluate((script) => {
      window.browser = {
        runtime: {
          onMessage: {
            addListener: (listener) => { window._contentScriptListener = listener; }
          }
        }
      };
      eval(script);
    }, contentScript);

    await page.evaluate(() => {
      window._contentScriptListener({
        type: 'FILL_FORM',
        username: 'readonly-user',
        password: 'readonly-pass',
        entryHosts: [],
      });
    });

    expect(await page.$eval('#pass-field', el => el.value)).toBe('readonly-pass');
    expect(await page.$eval('#user-field', el => el.value)).toBe('readonly-user');
  });
});
