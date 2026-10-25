import { test, expect } from '@playwright/test';
import path from 'path';
import { buildMock, EXTENSION_PATH } from './popup-mock.js';

test.describe('Extension Popup — domain-based filtering', () => {
  // Popup receives tab URL https://example.com/login.
  // extractDomain → "example.com", which is passed as GetSidebarEntries'
  // `domain` field (query stays null). A typed search goes in `query` and
  // wins over `domain`. The mock mirrors the agent: substring name-search
  // for `query`, host equality/subdomain match for `domain`.
  test.beforeEach(async ({ page }) => {
    const domainEntries = [
      { id: '1', name: 'example.com', username: 'user@example.com', entry_type: 'Login', is_pinned: false }
    ];
    await page.addInitScript(`
      window._sentMessages = [];
      window.browser = {
        runtime: {
          sendMessage: async (message) => {
            window._sentMessages.push(JSON.parse(JSON.stringify(message)));
            if (message === 'GetConfig') return { Config: { is_locked: false, needs_login: false } };
            if (message && message.GetSidebarEntries) {
              const q = message.GetSidebarEntries.query;
              const d = message.GetSidebarEntries.domain;
              const all = ${JSON.stringify([
                { id: '1', name: 'example.com', username: 'user@example.com', entry_type: 'Login', is_pinned: false },
                { id: '2', name: 'other.com',   username: 'other@other.com',  entry_type: 'Login', is_pinned: false }
              ])};
              let filtered = all;
              if (q) filtered = all.filter(e => e.name.includes(q));
              else if (d) filtered = all.filter(e => e.name === d || d.endsWith('.' + e.name));
              return { SidebarEntries: { entries: filtered } };
            }
            if (message && message.SetTheme !== undefined) return { Ack: true };
            return { Error: { message: 'Unknown' } };
          },
          connectNative: () => ({
            onMessage: { addListener: () => {} },
            onDisconnect: { addListener: () => {} },
            postMessage: () => {},
            disconnect: () => {}
          })
        },
        tabs: {
          query: async () => [{ id: 1, url: 'https://example.com/login' }],
          sendMessage: async () => {},
          create: async () => {}
        }
      };
    `);
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);
  });

  test('passes extracted host in the domain field to GetSidebarEntries on open', async ({ page }) => {
    await page.waitForTimeout(300); // let init complete
    const msgs = await page.evaluate(() => window._sentMessages);
    const sidebarCall = msgs.find(m => m && m.GetSidebarEntries);
    expect(sidebarCall).toBeTruthy();
    expect(sidebarCall.GetSidebarEntries.domain).toBe('example.com');
    expect(sidebarCall.GetSidebarEntries.query).toBeNull();
  });

  test('shows only domain-matched entries by default', async ({ page }) => {
    await expect(page.locator('.entry-name', { hasText: 'example.com' })).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.entry-name', { hasText: 'other.com' })).not.toBeVisible();
  });

  test('typing in search overrides domain filter', async ({ page }) => {
    await page.locator('#search').fill('other');
    await expect(page.locator('.entry-name', { hasText: 'other.com' })).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.entry-name', { hasText: 'example.com' })).not.toBeVisible();
  });
});

test.describe('Extension Popup — favourites', () => {
  // Mock honours query (substring on name), only_pinned, and domain
  // (exact/subdomain on name), mirroring the agent's semantics.
  const setup = async (page, tabUrl) => {
    await page.addInitScript(`
      window._sentMessages = [];
      window.browser = {
        runtime: {
          sendMessage: async (message) => {
            window._sentMessages.push(JSON.parse(JSON.stringify(message)));
            if (message === 'GetConfig') return { Config: { is_locked: false, needs_login: false } };
            if (message && message.GetSidebarEntries) {
              const p = message.GetSidebarEntries;
              let filtered = [
                { id: '1', name: 'example.com', username: 'user@example.com', entry_type: 'Login', is_pinned: false },
                { id: '2', name: 'pinned.com',  username: 'fav@pinned.com',   entry_type: 'Login', is_pinned: true }
              ];
              if (p.only_pinned) filtered = filtered.filter(e => e.is_pinned);
              if (p.query) filtered = filtered.filter(e => e.name.includes(p.query));
              else if (p.domain) filtered = filtered.filter(e => e.name === p.domain || p.domain.endsWith('.' + e.name));
              return { SidebarEntries: { entries: filtered } };
            }
            if (message && message.SetTheme !== undefined) return { Ack: true };
            return { Error: { message: 'Unknown' } };
          },
          connectNative: () => ({
            onMessage: { addListener: () => {} },
            onDisconnect: { addListener: () => {} },
            postMessage: () => {},
            disconnect: () => {}
          })
        },
        tabs: {
          query: async () => [{ id: 1, url: '${tabUrl}' }],
          sendMessage: async () => {},
          create: async () => {}
        }
      };
    `);
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);
  };

  test('falls back to favourites when the tab domain matches nothing', async ({ page }) => {
    await setup(page, 'https://nomatch.example.org/');
    await expect(page.locator('.entry-name', { hasText: 'pinned.com' })).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.entry-name', { hasText: 'example.com' })).not.toBeVisible();
    await expect(page.locator('.list-caption')).toHaveText('Favourites');
  });

  test('domain match wins over the favourites fallback', async ({ page }) => {
    await setup(page, 'https://example.com/login');
    await expect(page.locator('.entry-name', { hasText: 'example.com' })).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.entry-name', { hasText: 'pinned.com' })).not.toBeVisible();
    await expect(page.locator('.list-caption')).not.toBeVisible();
  });

  test('star toggle restricts to favourites even with a domain match', async ({ page }) => {
    await setup(page, 'https://example.com/login');
    await expect(page.locator('.entry-name', { hasText: 'example.com' })).toBeVisible({ timeout: 5000 });

    await page.locator('#fav-btn').click();
    await expect(page.locator('.entry-name', { hasText: 'pinned.com' })).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.entry-name', { hasText: 'example.com' })).not.toBeVisible();
    await expect(page.locator('#fav-btn')).toHaveAttribute('aria-pressed', 'true');

    // Toggle off restores the domain view.
    await page.locator('#fav-btn').click();
    await expect(page.locator('.entry-name', { hasText: 'example.com' })).toBeVisible({ timeout: 5000 });
    await expect(page.locator('#fav-btn')).toHaveAttribute('aria-pressed', 'false');
  });

  test('typed search with toggle on searches favourites only', async ({ page }) => {
    await setup(page, 'https://example.com/login');
    await page.locator('#fav-btn').click();
    // "com" matches both names, but only the pinned entry may appear.
    await page.locator('#search').fill('com');
    await expect(page.locator('.entry-name', { hasText: 'pinned.com' })).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.entry-name', { hasText: 'example.com' })).not.toBeVisible();
  });
});

test.describe('Extension Popup — add entry pre-fills the current tab domain', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(buildMock({ tabUrl: 'https://example.com/login' }));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);
  });

  test('add button opens the form with the current tab domain pre-filled in Name', async ({ page }) => {
    await page.locator('#add-btn').click();
    await expect(page.locator('#view-edit')).toBeVisible({ timeout: 5000 });
    expect(await page.locator('#edit-name').inputValue()).toBe('example.com');
  });
});

test.describe('Extension Popup — click-to-fill on a matching domain', () => {
  test.beforeEach(async ({ page }) => {
    // The popup is open on https://example.com/login and the listed entry
    // belongs to example.com — a domain match, so clicking the row should
    // autofill the form on the current tab, not open the site elsewhere.
    await page.addInitScript(buildMock({
      tabUrl: 'https://example.com/login',
      entryMetaUri: 'example.com/login',
    }));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);
  });

  test('clicking a Login entry row on a matching domain fills the form instead of opening a new tab', async ({ page }) => {
    await page.locator('.entry-name', { hasText: 'Test Login' }).click();

    // The form on the current tab must receive FILL_FORM...
    await page.waitForFunction(
      () => window._sentToTab && window._sentToTab.some(m => m.msg.type === 'FILL_FORM'),
      null, { timeout: 5000 }
    );
    const filled = await page.evaluate(() => window._sentToTab.find(m => m.msg.type === 'FILL_FORM'));
    expect(filled.msg.username).toBe('testuser');
    expect(filled.msg.password).toBe('testpassword');

    // ...and no new tab may be opened.
    expect((await page.evaluate(() => window._tabsCreated || [])).length).toBe(0);

    // The full entry (with the password) is fetched via GetEntry.
    const msgs = await page.evaluate(() => window._sentMessages);
    expect(msgs.some(m => m && m.GetEntry)).toBe(true);
  });

  test('clicking a non-Login entry row on a matching domain still opens the detail view', async ({ page }) => {
    const cardEntries = [
      { id: '9', name: 'My Card', username: null, entry_type: 'Card', is_pinned: false }
    ];
    await page.addInitScript(buildMock({ tabUrl: 'https://example.com/login', entriesForQuery: cardEntries }));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);

    await page.locator('.entry-name', { hasText: 'My Card' }).click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });
    expect((await page.evaluate(() => window._tabsCreated || [])).length).toBe(0);
  });
});
