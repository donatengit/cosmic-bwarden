if (typeof globalThis.browser === 'undefined') { globalThis.browser = globalThis.chrome; }

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

// Full registrable host (only a leading "www." is stripped). We deliberately do
// NOT collapse to the last two labels: for "victim.co.uk" that yields "co.uk", so
// the agent's substring search would surface every ".co.uk" entry. A proper fix
// would consult the Public Suffix List; matching the full host is the safe choice.
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
        const response = await sendToAgent({ GetSidebarEntries: { query: domain, entry_type: null, only_pinned: false } });
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
    if (changeInfo.status === 'complete') updateBadge(tabId, tab.url);
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

browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (message && message.SetTheme !== undefined) {
        setThemeIcon(message.SetTheme.dark);
        return Promise.resolve({ Ack: true });
    }
    return sendToAgent(message);
});
