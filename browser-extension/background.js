if (typeof globalThis.browser === 'undefined') { globalThis.browser = globalThis.chrome; }

// Chrome MV3 service worker loads only this file; pull in the save-prompt
// state machine. Firefox loads it via the manifest "scripts" array instead.
if (typeof importScripts === 'function') { importScripts('background-save.js'); }

let port = null;
const queue = [];
let isProcessing = false;

function connect() {
    port = browser.runtime.connectNative("com.8bit.cosmic_bwarden");

    port.onDisconnect.addListener((p) => {
        const lastErr = chrome.runtime.lastError;
        if (p.error || lastErr) {
            console.error("Disconnected from agent:", p.error || (lastErr && lastErr.message));
        }
        port = null;
        while (queue.length > 0) {
            const req = queue.shift();
            req.reject(new Error("Agent disconnected"));
        }
        isProcessing = false;
    });

    port.onMessage.addListener((response) => {
        const req = queue.shift();
        if (req) req.resolve(response);
        isProcessing = false;
        processQueue();
    });
}

function processQueue() {
    if (isProcessing || queue.length === 0) return;
    if (!port) connect();
    isProcessing = true;
    const { action, reject } = queue[0];
    try {
        port.postMessage(action);
    } catch (e) {
        queue.shift();
        isProcessing = false;
        reject(e);
        processQueue();
    }
}

function sendToAgent(action) {
    return new Promise((resolve, reject) => {
        queue.push({ action, resolve, reject });
        processQueue();
    });
}

// Full host (only a leading "www." is stripped). Never collapse labels here:
// deriving a registrable domain is the agent's job, where the Public Suffix
// List lives (see docs/public_suffix_list.md). The full host goes in the
// `domain` field of GetSidebarEntries and the agent applies exact /
// boundary-subdomain / eTLD+1 matching against each entry's stored URIs.
function extractDomain(url) {
    try {
        return new URL(url).hostname.toLowerCase().replace(/^www\./, '');
    } catch { return null; }
}

async function updateBadge(tabId, tabUrl) {
    if (!tabUrl || tabUrl.startsWith('about:') || tabUrl.startsWith('chrome:') || tabUrl.startsWith('moz-extension:')) {
        browser.action.setBadgeText({ text: '', tabId });
        return;
    }
    const domain = extractDomain(tabUrl);
    if (!domain) { browser.action.setBadgeText({ text: '', tabId }); return; }
    try {
        const response = await sendToAgent({ GetSidebarEntries: { query: null, entry_type: null, only_pinned: false, domain } });
        if (!response || !response.SidebarEntries) { browser.action.setBadgeText({ text: '', tabId }); return; }
        const count = response.SidebarEntries.entries.length;
        browser.action.setBadgeText({ text: count > 0 ? String(count) : '', tabId });
        if (count > 0) browser.action.setBadgeBackgroundColor({ color: '#175DDC', tabId });
    } catch {
        browser.action.setBadgeText({ text: '', tabId });
    }
}

browser.tabs.onActivated.addListener(async ({ tabId }) => {
    try {
        const tab = await browser.tabs.get(tabId);
        updateBadge(tabId, tab.url);
    } catch { /* tab may not exist */ }
});

browser.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
    if (changeInfo.status === 'complete') {
        updateBadge(tabId, tab.url);
        savePrompt.onTabComplete(tabId);
    }
});

browser.tabs.onRemoved.addListener((tabId) => {
    savePrompt.onTabRemoved(tabId);
});

// Initialize badge for already-loaded tabs when the background script starts.
// onActivated/onUpdated only fire for future navigation, not the current state.
(async () => {
    try {
        const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
        if (tab) updateBadge(tab.id, tab.url);
    } catch { /* no tabs permission or no active window */ }
})();

// ── Theme-aware icon ──────────────────────────────────────────────────────────
function setThemeIcon(isDark) {
    const v = isDark ? 'white' : 'black';
    browser.action.setIcon({ path: { 16: `icons/${v}16.png`, 32: `icons/${v}32.png`, 64: `icons/${v}64.png`, 128: `icons/${v}128.png` } });
}

// Firefox background page has matchMedia; Chrome MV3 service worker does not.
try {
    const mq = self.matchMedia('(prefers-color-scheme: dark)');
    setThemeIcon(mq.matches);
    mq.addEventListener('change', e => setThemeIcon(e.matches));
} catch { /* Chrome SW: icon updated via SetTheme message from popup */ }

// ── Generate password (context menu) ─────────────────────────────────────────
// Registered at every background-script load. Chrome MV3 persists context
// menu items across service-worker restarts, so re-creating on an already-live
// id throws/sets lastError — the callback here just swallows that instead of
// letting it surface as an unhandled rejection or console error.
try {
    browser.contextMenus.create(
        {
            id: 'cosmic-bwarden-generate-password',
            title: 'Generate password (COSMIC BWarden)',
            contexts: ['editable'],
        },
        () => { void chrome.runtime.lastError; }
    );
} catch { /* already registered */ }

browser.contextMenus.onClicked.addListener(async (info, tab) => {
    if (info.menuItemId !== 'cosmic-bwarden-generate-password') return;
    try {
        const response = await sendToAgent({ GeneratePassword: { settings: null } });
        if (response && response.GeneratedPassword && tab && typeof tab.id === 'number') {
            // Chrome MV3 service workers have no clipboard/DOM access; relay the
            // actual write through the target tab's content script, which does
            // (Firefox's persistent background page could write directly, but
            // one code path for both browsers is simpler).
            browser.tabs.sendMessage(tab.id, {
                type: 'GENERATE_COPY_TO_CLIPBOARD',
                password: response.GeneratedPassword.password,
            });
        }
    } catch (e) {
        console.error('generate-password context menu failed:', e);
    }
});

browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (message && message.SetTheme !== undefined) {
        setThemeIcon(message.SetTheme.dark);
        return Promise.resolve({ Ack: true });
    }
    // Save-prompt messages from content scripts are handled locally — they
    // are not agent actions and must not fall through to the native host.
    if (message && message.type === 'LOGIN_SUBMITTED') {
        if (sender.tab && typeof sender.tab.id === 'number') {
            savePrompt.onLoginSubmitted(sender.tab.id, message);
        }
        return Promise.resolve({ Ack: true });
    }
    if (message && message.type === 'SAVE_BAR_ACTION') {
        if (sender.tab && typeof sender.tab.id === 'number') {
            return savePrompt.onBarAction(sender.tab.id, message.action);
        }
        return Promise.resolve({ Ack: true });
    }
    return sendToAgent(message);
});
