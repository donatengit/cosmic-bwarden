// Shared stubbed browser API for the popup specs (popup.spec.js and
// popup-generator.spec.js). Both load popup/popup.html over file:// with this
// injected via addInitScript, so the popup talks to a mock instead of a real
// extension context. Per-test variations that must survive a reload or a
// re-navigation go through sessionStorage: addInitScript re-runs on every
// navigation, so anything stashed on `window` would be wiped.
import path from 'path';

export const EXTENSION_PATH = path.resolve(__dirname, '../../../browser-extension');

const MOCK_ENTRY_META = {
  id: '1', name: 'Test Login', entry_type: 'Login',
  // notes and the hidden custom field's value are redacted (null) in meta —
  // presence is signalled by `filled_secrets`, never by value.
  notes: null,
  data: { Login: { username: 'testuser', password: null, totp: null, uris: [] } },
  // FieldType serializes as the repr number: Text=0, Hidden=1.
  fields: [{ name: 'API Key', value: null, ty: 1 }]
};

const MOCK_ENTRY_FILLED_SECRETS = ['Password', 'TOTP', 'API Key'];

const MOCK_ENTRY_FULL = {
  id: '1', name: 'Test Login', entry_type: 'Login', notes: null,
  data: { Login: { username: 'testuser', password: 'testpassword', totp: 'seed', uris: [{ uri: 'https://example.com/login' }] } },
  fields: [{ name: 'API Key', value: 'sk-live-abc', ty: 1 }]
};

// Entries returned when no query (or query=null) is sent — the full vault list.
const ALL_ENTRIES = [
  { id: '1', name: 'Test Login', username: 'testuser', entry_type: 'Login', is_pinned: false }
];

// Entries returned when query is the domain — subset matching by name.
const DOMAIN_ENTRIES = [
  { id: '1', name: 'Test Login', username: 'testuser', entry_type: 'Login', is_pinned: false }
];

export function buildMock({ isLocked = false, tabUrl = null, entriesForQuery = null, tpmStatus = null, unlockPinResult = null, entryMetaUri = null, entryMetaName = null } = {}) {
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
            return { EntryMeta: { entry: ${JSON.stringify(entryMeta)}, filled_secrets: ${JSON.stringify(MOCK_ENTRY_FILLED_SECRETS)} } };
          if (message && message.GetEntry)
            return { Entry: { entry: ${JSON.stringify(MOCK_ENTRY_FULL)} } };
          if (message && message.GetPassword)
            return { Password: { password: 'testpassword' } };
          // Generator: behaviour is switched per test through sessionStorage
          // (addInitScript re-runs on navigation, so window flags would be
          // wiped by the reopen tests).
          if (message && message.GeneratePassword) {
            const mode = window.sessionStorage.getItem('cosmarden-test:generateMode') || 'ok';
            if (mode === 'throw') throw new Error('Agent disconnected');
            if (mode === 'error') return { Error: { message: 'generator rejected the settings' } };
            if (mode === 'empty') return { GeneratedPassword: { password: '' } };
            if (mode === 'bad') return {};
            return {
              GeneratedPassword: {
                password: window.sessionStorage.getItem('cosmarden-test:generatedPassword') || 'gen-pass-1',
              },
            };
          }
          if (message === 'GetPasswordHistory') {
            const mode = window.sessionStorage.getItem('cosmarden-test:historyMode') || 'ok';
            if (mode === 'throw') throw new Error('Agent disconnected');
            if (mode === 'error') return { Error: { message: 'history temporarily unavailable' } };
            if (mode === 'bad') return {};
            const raw = window.sessionStorage.getItem('cosmarden-test:historyEntries');
            if (raw !== null) return { PasswordHistory: { entries: JSON.parse(raw) } };
            const now = Math.floor(Date.now() / 1000);
            return { PasswordHistory: { entries: [
              { password: 'gen-newest', created_at: now - 120 },
              { password: 'gen-older', created_at: now - 7200 },
            ] } };
          }
          // TOTP codes rotate: each request returns a code that increments, so
          // a test can prove reveal/copy refetches rather than caching the first.
          if (message && message.GetTotp) {
            window._totpCalls = (window._totpCalls || 0) + 1;
            return { Totp: { code: String(100000 + window._totpCalls) } };
          }
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
        // The baked-in URL, unless a test overrides it to simulate the popup
        // being reopened over a different tab. The override lives in
        // sessionStorage because addInitScript re-runs on every navigation,
        // so anything set on window would be wiped by the re-open the
        // close/reopen tests perform.
        query: async () => [{
          id: 123,
          url: window.sessionStorage.getItem('cosmarden-test:tabUrlOverride')
            ?? ${tabUrl ? `'${tabUrl}'` : 'undefined'}
        }],
        sendMessage: async (tabId, msg) => {
          window._sentToTab = (window._sentToTab || []).concat([{ tabId, msg }]);
        },
        create: async (opts) => { window._tabsCreated = (window._tabsCreated || []).concat([opts]); }
      },
      storage: {
        // Backed by the page's own sessionStorage: it survives reload and
        // same-tab navigation exactly like browser.storage.session survives a
        // popup reopen / service-worker restart. Without it, a fix that
        // persists popup state to storage.session could never turn the
        // close/reopen tests green — addInitScript re-runs on every
        // navigation, so any mock-level JS object would be recreated.
        session: {
          async get(key) {
            const raw = window.sessionStorage.getItem('cosmarden-test:' + key);
            return raw !== null ? { [key]: JSON.parse(raw) } : {};
          },
          async set(obj) {
            for (const [k, v] of Object.entries(obj)) {
              window.sessionStorage.setItem('cosmarden-test:' + k, JSON.stringify(v));
            }
          },
          async remove(key) {
            window.sessionStorage.removeItem('cosmarden-test:' + key);
          },
        },
      }
    };
  `;
}
