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

function buildMock({ isLocked = false, tabUrl = null, entriesForQuery = null, tpmStatus = null, unlockPinResult = null, entryMetaUri = null, entryMetaName = null } = {}) {
  const entries = JSON.stringify(entriesForQuery ?? ALL_ENTRIES);
  const tpm = JSON.stringify(tpmStatus ?? { available: false, configured: false, server_credentials: false });
  const entryMeta = JSON.parse(JSON.stringify(MOCK_ENTRY_META));
  entryMeta.data.Login.uris = entryMetaUri ? [{ uri: entryMetaUri }] : [];
  if (entryMetaName) entryMeta.name = entryMetaName;
  return `
    window._sentMessages = [];
    window._locked = ${isLocked};
    // Real close would tear down the Playwright page mid-test; the extension
    // popup itself is a real window the browser lets script-close.
    window.close = () => { window._closed = true; };
    window.browser = {
      runtime: {
        sendMessage: async (message) => {
          window._sentMessages.push(JSON.parse(JSON.stringify(message)));
          if (message === 'GetConfig') return { Config: { is_locked: window._locked, needs_login: false } };
          // Response::Ack is a unit enum variant — the real agent (via serde_json)
          // serializes it as the bare string "Ack", never { Ack: true }. Mocking
          // it as an object here previously hid a real bug (popup-lock.js only
          // checked resp.Ack, so a successful UnlockWithPin fell through to
          // "Unexpected agent response." against the real agent).
          if (message === 'RequestUnlock') return 'Ack';
          if (message === 'Sync') return 'Ack';
          if (message === 'Lock') return 'Ack';
          if (message === 'CheckTpm') return { TpmStatus: ${tpm} };
          if (message === 'GetTpmDaStatus')
            return { TpmDaStatus: { status: { available: true, max_tries: 32, lockout_counter: 3, remaining: 29, in_lockout: false, recovery_interval_secs: null } } };
          if (message && message.UnlockWithPin) {
            const result = ${JSON.stringify(unlockPinResult ?? { Error: { message: 'TPM unseal failed' } })};
            if (result === 'Ack' || (result && result.Ack)) window._locked = false;
            return result;
          }
          if (message && message.GetSidebarEntries)
            return { SidebarEntries: { entries: ${entries} } };
          if (message && message.GetEntryMeta)
            return { Entry: { entry: ${JSON.stringify(entryMeta)} } };
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
        create: async (opts) => { window._tabsCreated = (window._tabsCreated || []).concat([opts]); }
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

  test('each Login entry row has a Fill, copy-dropdown, and view-details button', async ({ page }) => {
    await expect(page.locator('.entry-actions button[title="Autofill"]')).toBeVisible();
    await expect(page.locator('.entry-actions button[title="Copy username or password"]')).toBeVisible();
    await expect(page.locator('.entry-actions button[title="View details"]')).toBeVisible();
    await expect(page.locator('.entry-actions button[title="View details"]')).toHaveText('👁');
  });

  test('clicking a Login entry row opens its URL in a new tab', async ({ page }) => {
    await page.addInitScript(buildMock({ entryMetaUri: 'example.com/login' }));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);

    await page.locator('.entry-name', { hasText: 'Test Login' }).click();
    await page.waitForFunction(() => window._tabsCreated && window._tabsCreated.length > 0);

    const created = await page.evaluate(() => window._tabsCreated);
    expect(created).toEqual([{ url: 'https://example.com/login' }]);

    const msgs = await page.evaluate(() => window._sentMessages);
    expect(msgs.some(m => m && m.GetEntryMeta)).toBe(true);
    expect(msgs.some(m => m && m.GetEntry)).toBe(false);
  });

  test('clicking a Login entry row with no saved URL shows a status message instead of opening a tab', async ({ page }) => {
    await page.locator('.entry-name', { hasText: 'Test Login' }).click();
    await expect(page.locator('#status')).toBeVisible();
    await expect(page.locator('#status')).toHaveText('No website saved for this entry.');

    const created = await page.evaluate(() => window._tabsCreated || []);
    expect(created.length).toBe(0);
  });

  test('clicking a Login entry row with no saved URL but a hostname-like name opens that name as a URL', async ({ page }) => {
    const hostnameEntries = [
      { id: '1', name: 'account.facebook.com', username: 'testuser', entry_type: 'Login', is_pinned: false }
    ];
    await page.addInitScript(buildMock({ entriesForQuery: hostnameEntries, entryMetaName: 'account.facebook.com' }));
    await page.goto(`file://${path.join(EXTENSION_PATH, 'popup/popup.html')}`);

    await page.locator('.entry-name', { hasText: 'account.facebook.com' }).click();
    await page.waitForFunction(() => window._tabsCreated && window._tabsCreated.length > 0);

    const created = await page.evaluate(() => window._tabsCreated);
    expect(created).toEqual([{ url: 'https://account.facebook.com' }]);
  });

  test('copy dropdown: Copy Username writes to clipboard without calling GetPassword', async ({ page }) => {
    await page.evaluate(() => {
      let clip = '';
      navigator.clipboard.writeText = async (t) => { clip = t; };
      navigator.clipboard.readText  = async () => clip;
    });

    await page.locator('.entry-actions button[title="Copy username or password"]').click();
    await page.locator('.copy-dropdown-menu button', { hasText: 'Copy Username' }).click();

    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('testuser');
    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetPassword))).toBe(false);
  });

  test('copy dropdown: Copy Password fetches GetPassword and writes to clipboard', async ({ page }) => {
    await page.evaluate(() => {
      let clip = '';
      navigator.clipboard.writeText = async (t) => { clip = t; };
      navigator.clipboard.readText  = async () => clip;
    });

    await page.locator('.entry-actions button[title="Copy username or password"]').click();
    await page.locator('.copy-dropdown-menu button', { hasText: 'Copy Password' }).click();

    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('testpassword');
    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetPassword))).toBe(true);
  });

  test('copy dropdown closes when clicking outside', async ({ page }) => {
    await page.locator('.entry-actions button[title="Copy username or password"]').click();
    await expect(page.locator('.copy-dropdown-menu')).toBeVisible();

    await page.locator('#search').click();
    await expect(page.locator('.copy-dropdown-menu')).toHaveCount(0);
  });

  test('view-details button opens detail via GetEntryMeta, never GetEntry', async ({ page }) => {
    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    const msgs = await page.evaluate(() => window._sentMessages);
    expect(msgs.some(m => m && m.GetEntryMeta)).toBe(true);
    expect(msgs.some(m => m && m.GetEntry)).toBe(false);
  });

  test('detail view shows masked placeholder, not the real password', async ({ page }) => {
    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    const maskedText = await page.locator('.secret-text').first().textContent();
    expect(maskedText).toBe('••••••••');
    await expect(page.locator('button.reveal-btn').first()).toBeVisible();
  });

  test('reveal button triggers GetPassword and shows plaintext', async ({ page }) => {
    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    // Not yet called
    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetPassword))).toBe(false);

    await page.locator('button.reveal-btn').first().click();

    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetPassword))).toBe(true);
    expect(await page.locator('.secret-text').first().textContent()).toBe('testpassword');
  });

  test('password copy button triggers GetPassword and writes to clipboard', async ({ page }) => {
    await page.evaluate(() => {
      let clip = '';
      navigator.clipboard.writeText = async (t) => { clip = t; };
      navigator.clipboard.readText  = async () => clip;
    });

    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    // Username's copy-btn comes first in the DOM; password's is the second.
    await page.locator('button.copy-btn').nth(1).click();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('testpassword');
  });

  test('username copy button writes to clipboard without calling GetPassword', async ({ page }) => {
    await page.evaluate(() => {
      let clip = '';
      navigator.clipboard.writeText = async (t) => { clip = t; };
      navigator.clipboard.readText  = async () => clip;
    });

    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    await page.locator('button.copy-btn').first().click();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('testuser');
    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetPassword))).toBe(false);
  });

  test('detail view Edit button re-fetches the full entry via GetEntry before opening the edit form', async ({ page }) => {
    await page.locator('.entry-actions button[title="View details"]').click();
    await expect(page.locator('#view-detail')).toBeVisible({ timeout: 5000 });

    await page.locator('#edit-btn').click();
    await expect(page.locator('#view-edit')).toBeVisible({ timeout: 5000 });

    expect(await page.evaluate(() => window._sentMessages.some(m => m && m.GetEntry))).toBe(true);
    expect(await page.locator('.password-field-wrap input').inputValue()).toBe('testpassword');
  });

  test('SetTheme message is sent to background on popup init', async ({ page }) => {
    expect(await page.evaluate(() =>
      window._sentMessages.some(m => m && m.SetTheme !== undefined)
    )).toBe(true);
  });
});

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
