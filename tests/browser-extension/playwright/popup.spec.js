import { test, expect } from '@playwright/test';
import path from 'path';

const EXTENSION_PATH = path.resolve(__dirname, '../../../browser-extension');

const MOCK_ENTRY_META = {
  id: '1', name: 'Test Login', entry_type: 'Login', notes: null,
  data: { Login: { username: 'testuser', password: null, totp: null, uris: [] } }
};

const MOCK_ENTRY_FULL = {
  id: '1', name: 'Test Login', entry_type: 'Login', notes: null,
  data: { Login: { username: 'testuser', password: 'testpassword', totp: null, uris: [] } }
};

// Entries returned when no query (or query=null) is sent — the full vault list.
const ALL_ENTRIES = [
  { id: '1', name: 'Test Login', username: 'testuser', entry_type: 'Login', is_pinned: false }
];

// Entries returned when query is the domain — subset matching by name.
const DOMAIN_ENTRIES = [
  { id: '1', name: 'Test Login', username: 'testuser', entry_type: 'Login', is_pinned: false }
];

function buildMock({ isLocked = false, tabUrl = null, entriesForQuery = null } = {}) {
  const entries = JSON.stringify(entriesForQuery ?? ALL_ENTRIES);
  return `
    window._sentMessages = [];
    window.browser = {
      runtime: {
        sendMessage: async (message) => {
          window._sentMessages.push(JSON.parse(JSON.stringify(message)));
          if (message === 'GetConfig') return { Config: { is_locked: ${isLocked}, needs_login: false } };
          if (message === 'RequestUnlock') return { Ack: true };
          if (message === 'Sync') return { Ack: true };
          if (message === 'Lock') return { Ack: true };
          if (message && message.GetSidebarEntries)
            return { SidebarEntries: { entries: ${entries} } };
          if (message && message.GetEntryMeta)
            return { Entry: { entry: ${JSON.stringify(MOCK_ENTRY_META)} } };
          if (message && message.GetEntry)
            return { Entry: { entry: ${JSON.stringify(MOCK_ENTRY_FULL)} } };
          if (message && message.GetPassword)
            return { Password: { password: 'testpassword' } };
          if (message && message.SetTheme !== undefined) return { Ack: true };
          return { Error: { message: 'Unknown mock: ' + JSON.stringify(message) } };
        },
        connectNative: () => ({
          onMessage: { addListener: () => {} },
          onDisconnect: { addListener: () => {} },
          postMessage: () => {},
          disconnect: () => {}
        })
      },
      tabs: {
        query: async () => [{ id: 123, url: ${tabUrl ? `'${tabUrl}'` : 'undefined'} }],
        sendMessage: async () => {},
        create: async () => {}
      }
    };
  `;
}

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

  test('each entry row has a Fill and an Edit button', async ({ page }) => {
    await expect(page.locator('.entry-actions button[title="Autofill"]')).toBeVisible();
    await expect(page.locator('.entry-actions button[title="Edit"]')).toBeVisible();
    await expect(page.locator('.entry-actions button[title="Edit"]')).toHaveText('✏');
  });

  test('clicking entry name opens detail via GetEntryMeta, never GetEntry', async ({ page }) => {
    await page.locator('.entry-name', { hasText: 'Test Login' }).click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    const msgs = await page.evaluate(() => window._sentMessages);
    expect(msgs.some(m => m && m.GetEntryMeta)).toBe(true);
    expect(msgs.some(m => m && m.GetEntry)).toBe(false);
  });

  test('detail view shows masked placeholder, not the real password', async ({ page }) => {
    await page.locator('.entry-name', { hasText: 'Test Login' }).click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    const maskedText = await page.locator('.secret-text').first().textContent();
    expect(maskedText).toBe('••••••••');
    await expect(page.locator('button.reveal-btn').first()).toBeVisible();
  });

  test('reveal button triggers GetPassword and shows plaintext', async ({ page }) => {
    await page.locator('.entry-name', { hasText: 'Test Login' }).click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    // Not yet called
    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetPassword))).toBe(false);

    await page.locator('button.reveal-btn').first().click();

    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetPassword))).toBe(true);
    expect(await page.locator('.secret-text').first().textContent()).toBe('testpassword');
  });

  test('copy button triggers GetPassword and writes to clipboard', async ({ page }) => {
    await page.evaluate(() => {
      let clip = '';
      navigator.clipboard.writeText = async (t) => { clip = t; };
      navigator.clipboard.readText  = async () => clip;
    });

    await page.locator('.entry-name', { hasText: 'Test Login' }).click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    await page.locator('button.copy-btn').first().click();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('testpassword');
  });

  test('edit icon in list opens edit form via GetEntry', async ({ page }) => {
    await page.locator('.entry-actions button[title="Edit"]').click();
    await expect(page.locator('#view-edit')).toBeVisible({ timeout: 5000 });

    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetEntry))).toBe(true);
  });

  test('SetTheme message is sent to background on popup init', async ({ page }) => {
    expect(await page.evaluate(() =>
      window._sentMessages.some(m => m && m.SetTheme !== undefined)
    )).toBe(true);
  });
});

test.describe('Extension Popup — locked vault', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(buildMock({ isLocked: true }));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);
  });

  test('shows locked status and sends RequestUnlock to agent', async ({ page }) => {
    await expect(page.locator('#status')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('#status')).toContainText('locked');

    expect(await page.evaluate(() =>
      window._sentMessages.some(m => m === 'RequestUnlock')
    )).toBe(true);
  });
});

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
    await expect(page.locator('.list-caption')).toHaveText('★ Favourites');
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
    await expect(page.locator('#fav-btn')).toHaveText('★');

    // Toggle off restores the domain view.
    await page.locator('#fav-btn').click();
    await expect(page.locator('.entry-name', { hasText: 'example.com' })).toBeVisible({ timeout: 5000 });
    await expect(page.locator('#fav-btn')).toHaveText('☆');
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
