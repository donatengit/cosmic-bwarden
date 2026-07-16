// Save-prompt state machine: holds just-submitted credentials per tab until
// the post-login page settles, asks the agent whether to offer Save or
// Update, and performs the chosen action. Loaded before background.js
// (Firefox manifest "scripts" order / Chrome importScripts); it only calls
// background.js globals (sendToAgent, extractDomain, updateBadge) at event
// time, when both scripts are loaded.

const PENDING_TTL_MS = 90 * 1000;
const SPA_FALLBACK_MS = 2000;

// Pending-save state lives in browser.storage.session — in-memory only
// (never written to disk, cleared when the browser session ends), but
// unlike a plain module-level Map it survives a Chrome MV3 service-worker
// restart. The bar sits waiting on the user for as long as they take to
// read it; that routinely exceeds the ~30s SW inactivity timeout, and a
// plain Map lost the pending credential across that restart — the user's
// "Save" click would land on a fresh, empty Map and silently do nothing.
// TTL expiry uses browser.alarms rather than setTimeout for the same
// reason: a setTimeout is destroyed along with a killed service worker.
function pendingKey(tabId) {
    return `pendingSave:${tabId}`;
}

function alarmName(tabId) {
    return `pendingSaveExpire:${tabId}`;
}

async function getPendingSave(tabId) {
    const key = pendingKey(tabId);
    const store = await browser.storage.session.get(key);
    return store[key] ?? null;
}

async function setPendingSave(tabId, pending) {
    await browser.storage.session.set({ [pendingKey(tabId)]: pending });
}

async function clearPendingSave(tabId) {
    await browser.storage.session.remove(pendingKey(tabId));
    await browser.alarms.clear(alarmName(tabId));
}

// Pure decision helpers (unit-tested via savePrompt).
function vaultUsable(configResp) {
    return !!(configResp && configResp.Config
        && !configResp.Config.is_locked && !configResp.Config.needs_login);
}

function decideBarMode(matchResp) {
    if (!matchResp || !matchResp.LoginMatch) return null;
    if (matchResp.LoginMatch.password_matches) return null; // already stored as-is
    return matchResp.LoginMatch.entry_id ? 'update' : 'save';
}

async function onLoginSubmitted(tabId, message) {
    if (typeof tabId !== 'number' || tabId < 0) return;
    const domain = extractDomain(message.url);
    if (!domain || !message.username || !message.password) return;

    await clearPendingSave(tabId);
    await setPendingSave(tabId, {
        username: message.username,
        password: message.password,
        url: message.url,
        domain,
        entryId: null,
        mode: null,
        evaluated: false,
    });
    browser.alarms.create(alarmName(tabId), { delayInMinutes: PENDING_TTL_MS / 60000 });
    // AJAX/SPA logins never fire a page load — evaluate anyway shortly after.
    // A short setTimeout is fine here (unlike the TTL above): it fires a
    // couple of seconds after an event that just woke the service worker,
    // long before any inactivity eviction could occur.
    setTimeout(() => evaluatePendingSave(tabId), SPA_FALLBACK_MS);
}

async function evaluatePendingSave(tabId) {
    const pending = await getPendingSave(tabId);
    if (!pending || pending.evaluated) return;
    pending.evaluated = true;
    await setPendingSave(tabId, pending);

    try {
        const configResp = await sendToAgent("GetConfig");
        if (!vaultUsable(configResp)) {
            // v1: locked or logged-out vault silently drops the prompt.
            await clearPendingSave(tabId);
            return;
        }

        const matchResp = await sendToAgent({
            CheckLoginMatch: {
                domain: pending.domain,
                username: pending.username,
                password: pending.password,
            },
        });
        const mode = decideBarMode(matchResp);
        if (!mode) {
            await clearPendingSave(tabId);
            return;
        }

        pending.mode = mode;
        pending.entryId = matchResp.LoginMatch.entry_id;
        await setPendingSave(tabId, pending);
        // Display labels only — the password never goes back to the page.
        await browser.tabs.sendMessage(tabId, {
            type: 'SHOW_SAVE_BAR',
            mode,
            domain: pending.domain,
            username: pending.username,
            entryName: matchResp.LoginMatch.name,
        });
    } catch {
        // Agent unreachable or tab gone; drop the credential.
        await clearPendingSave(tabId);
    }
}

// Returns the pending promise (harmlessly ignored by the tabs.onUpdated
// listener) so tests can await evaluation instead of racing it.
function onTabComplete(tabId) {
    return evaluatePendingSave(tabId);
}

function onTabRemoved(tabId) {
    return clearPendingSave(tabId);
}

async function onBarAction(tabId, action) {
    const pending = await getPendingSave(tabId);
    if (!pending) {
        // The bar is showing (the user just clicked it) but we have no
        // pending credential for this tab — it was lost or already cleared
        // out from under a bar the user is still looking at. Surface this
        // rather than leaving the bar stuck on "Saving…"/"Updating…" until
        // its own 30s auto-dismiss silently removes it with nothing saved.
        if (action !== 'dismiss') {
            console.warn(`cosmic-bwarden: no pending save for tab ${tabId} on action "${action}"`);
            browser.tabs.sendMessage(tabId, { type: 'SAVE_BAR_ERROR' }).catch(() => {});
        }
        return { Ack: true };
    }

    let agentAction = null;
    if (action === 'save' && pending.mode === 'save') {
        agentAction = {
            AddEntry: {
                name: pending.domain,
                entry_type: 'Login',
                username: pending.username,
                password: pending.password,
                notes: null,
                fields: [],
                totp: null,
                uris: [{ uri: originOf(pending.url) || pending.url, match_type: null }],
            },
        };
    } else if (action === 'update' && pending.mode === 'update' && pending.entryId) {
        agentAction = { UpdateLoginPassword: { id: pending.entryId, password: pending.password } };
    }

    if (!agentAction) {
        await clearPendingSave(tabId);
        if (action !== 'dismiss') {
            // Clicked action doesn't match what we evaluated (stale pending
            // state, e.g. overwritten by a second LOGIN_SUBMITTED before the
            // user acted) — the click did not save/update anything, so say so
            // rather than quietly clearing state with no feedback at all.
            console.warn(
                `cosmic-bwarden: action "${action}" did not match pending mode ` +
                `"${pending.mode}" for tab ${tabId}; dropping without saving`
            );
            browser.tabs.sendMessage(tabId, { type: 'SAVE_BAR_ERROR' }).catch(() => {});
        }
        return { Ack: true };
    }

    try {
        const response = await sendToAgent(agentAction);
        await clearPendingSave(tabId);
        if (response === 'Ack' || (response && response.Ack !== undefined)) {
            browser.tabs.sendMessage(tabId, { type: 'HIDE_SAVE_BAR' }).catch(() => {});
            updateBadge(tabId, pending.url);
            return { Ack: true };
        }
        console.warn(`cosmic-bwarden: ${action} rejected by agent:`, response);
        browser.tabs.sendMessage(tabId, { type: 'SAVE_BAR_ERROR' }).catch(() => {});
        return response;
    } catch (e) {
        console.warn(`cosmic-bwarden: ${action} failed:`, e);
        await clearPendingSave(tabId);
        browser.tabs.sendMessage(tabId, { type: 'SAVE_BAR_ERROR' }).catch(() => {});
        return { Error: { message: String(e) } };
    }
}

function originOf(url) {
    try { return new URL(url).origin; } catch { return null; }
}

// Fires even if the service worker was killed and restarted since the alarm
// was scheduled — unlike setTimeout, browser.alarms survives that.
browser.alarms.onAlarm.addListener((alarm) => {
    const m = /^pendingSaveExpire:(\d+)$/.exec(alarm.name);
    if (m) clearPendingSave(Number(m[1]));
});

globalThis.savePrompt = {
    onLoginSubmitted,
    onTabComplete,
    onBarAction,
    onTabRemoved,
    // exposed for unit tests
    decideBarMode,
    vaultUsable,
    getPendingSave,
};
