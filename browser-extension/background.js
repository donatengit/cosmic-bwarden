if (typeof globalThis.browser === 'undefined') { globalThis.browser = globalThis.chrome; }

// Chrome MV3 service worker loads only this file; pull in the save-prompt
// state machine. Firefox loads it via the manifest "scripts" array instead.
if (typeof importScripts === 'function') { importScripts('background-save.js'); }

let port = null;
const queue = [];
let isProcessing = false;

function connect() {
    port = browser.runtime.connectNative("com.enikeev.cosmarden");

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

// Neutral grey, matching theme.js's light `text-secondary`. The count is
// informational (how many entries match this host), not an alert, so it must
// not borrow the accent blue. White badge text stays legible in both themes.
const BADGE_BACKGROUND = '#5c5f66';

async function updateBadge(tabId, tabUrl) {
    // Keep the lock-state icon in sync on every tab activation/update.
    await updateLockState();
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
        if (count > 0) browser.action.setBadgeBackgroundColor({ color: BADGE_BACKGROUND, tabId });
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

// Recompute the active tab's badge after a lock-state change, and once at
// startup: onActivated/onUpdated only fire for future navigation, so the
// current tab would otherwise keep a stale (or empty) count.
async function refreshActiveBadge() {
    try {
        const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
        if (tab) await updateBadge(tab.id, tab.url);
    } catch { /* no tabs permission or no active window */ }
}

refreshActiveBadge();

// ── Theme-aware + lock-state icon ─────────────────────────────────────────────
let currentThemeDark = false;

function iconPaths(variant) {
    return {
        16: `icons/${variant}16.png`,
        32: `icons/${variant}32.png`,
        64: `icons/${variant}64.png`,
        128: `icons/${variant}128.png`,
    };
}

function setThemeIcon(isDark) {
    currentThemeDark = isDark;
    const v = isDark ? 'white' : 'black';
    browser.action.setIcon({ path: iconPaths(v) });
}

function setLockIcon(isDark) {
    const v = isDark ? 'white' : 'black';
    browser.action.setIcon({ path: iconPaths(`${v}_locked`) });
}

// Query the agent for lock state and reflect it in the toolbar icon, so the
// icon changes shape when the vault is locked/unlocked (not just color theme).
async function updateLockState() {
    try {
        const configResp = await sendToAgent("GetConfig");
        // Same test as background-save.js's vaultUsable() (inlined: that file
        // isn't loaded in background.js's own unit-test scope). Logged out is
        // shown as locked too — in both states there is no vault to reach.
        const usable = !!(configResp && configResp.Config
            && !configResp.Config.is_locked && !configResp.Config.needs_login);
        if (usable) setThemeIcon(currentThemeDark);
        else setLockIcon(currentThemeDark);
    } catch {
        // Agent unreachable — leave the current icon alone.
    }
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
            id: 'cosmarden-generate-password',
            title: 'Generate password (Cosmarden)',
            contexts: ['editable'],
        },
        () => { void chrome.runtime.lastError; }
    );
} catch { /* already registered */ }

browser.contextMenus.onClicked.addListener(async (info, tab) => {
    if (info.menuItemId !== 'cosmarden-generate-password') return;
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
            }, { frameId: info.frameId ?? 0 }).catch(() => {
                // The frame closed while the agent was generating; there is no
                // page left to paste into.
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
            // frameId is 0 for the top frame; the content scripts run in every
            // frame, so the submission must stay attached to the frame it came
            // from (the bar is shown there, and two frames of one tab must not
            // overwrite each other's pending credential).
            savePrompt.onLoginSubmitted(sender.tab.id, sender.frameId ?? 0, message);
        }
        return Promise.resolve({ Ack: true });
    }
    if (message && message.type === 'SAVE_BAR_ACTION') {
        if (sender.tab && typeof sender.tab.id === 'number') {
            return savePrompt.onBarAction(
                sender.tab.id,
                sender.frameId ?? 0,
                message.action,
                message.pendingKey,
            );
        }
        return Promise.resolve({ Ack: true });
    }
    if (message === 'Lock' || message === 'Logout') {
        // Counterpart to VAULT_UNLOCKED: flip the icon and clear the match
        // count as soon as the agent confirms, rather than leaving both stale
        // until the next tab event. Fire-and-forget so the reply is not held
        // up by the extra agent round trips.
        return sendToAgent(message).then((resp) => {
            refreshActiveBadge();
            return resp;
        });
    }
    if (message && message.type === 'VAULT_UNLOCKED') {
        // The popup/applet unlocked the vault: re-evaluate any save prompts
        // that were deferred while locked, then refresh the icon and count.
        savePrompt.onVaultUnlocked();
        refreshActiveBadge();
        return Promise.resolve({ Ack: true });
    }
    // Content scripts share this listener but have no agent actions of their
    // own: fill and save are handled locally above, and the inline generate
    // icon that used to proxy GeneratePassword was removed. The popup page is
    // still allowed even when rendered in a tab (specs, manual debugging):
    // its sender.url is the extension's own popup/ origin, which no web-page
    // content script can forge.
    if (sender && sender.tab && !isPopupSender(sender)) {
        return Promise.resolve({ Error: { message: 'action not allowed from content script' } });
    }
    return sendToAgent(message);
});

function isPopupSender(sender) {
    if (!sender || typeof sender.url !== 'string') return false;
    return sender.url.startsWith(browser.runtime.getURL('popup/'));
}
