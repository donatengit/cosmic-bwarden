// Save-prompt state machine: holds just-submitted credentials per tab+frame
// until the post-login page settles, asks the agent whether to offer Save or
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
//
// The key carries the frame ID because the content scripts run in every frame
// (manifest all_frames). Keying by tab alone made two submissions in one tab
// — an embedded login widget and the page's own form — overwrite each other,
// and the bar for the first would then save the second's password.
function pendingKey(tabId, frameId) {
    return `pendingSave:${tabId}:${frameId}`;
}

function alarmName(tabId, frameId) {
    return `pendingSaveExpire:${tabId}:${frameId}`;
}

async function getPendingSave(tabId, frameId) {
    const key = pendingKey(tabId, frameId);
    const store = await browser.storage.session.get(key);
    return store[key] ?? null;
}

async function setPendingSave(tabId, frameId, pending) {
    await browser.storage.session.set({ [pendingKey(tabId, frameId)]: pending });
}

async function clearPendingSave(tabId, frameId) {
    await browser.storage.session.remove(pendingKey(tabId, frameId));
    await browser.alarms.clear(alarmName(tabId, frameId));
}

// Frame IDs that currently hold a pending credential in `tabId`. Needed
// because a page load (onTabComplete) and a tab close (onTabRemoved) are
// tab-level events: nothing tells us which frames exist, and the service
// worker may have been restarted since the submissions.
async function pendingFramesForTab(tabId) {
    let all = {};
    try {
        all = await browser.storage.session.get(null);
    } catch { return []; }
    const prefix = `pendingSave:${tabId}:`;
    return Object.keys(all || {})
        .filter(key => key.startsWith(prefix))
        .map(key => Number(key.slice(prefix.length)))
        .filter(frameId => Number.isInteger(frameId) && frameId >= 0);
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

async function onLoginSubmitted(tabId, frameId, message) {
    if (typeof tabId !== 'number' || tabId < 0) return;
    if (typeof frameId !== 'number' || frameId < 0) return;
    const domain = extractDomain(message.url);
    if (!domain || !message.username || !message.password) return;

    await clearPendingSave(tabId, frameId);
    await setPendingSave(tabId, frameId, {
        username: message.username,
        password: message.password,
        url: message.url,
        domain,
        entryId: null,
        mode: null,
        evaluated: false,
        // Where the bar is shown. Starts in the submitting frame and falls
        // back to the top frame if that frame disappears.
        barFrameId: frameId,
    });
    browser.alarms.create(alarmName(tabId, frameId), { delayInMinutes: PENDING_TTL_MS / 60000 });
    // AJAX/SPA logins never fire a page load — evaluate anyway shortly after.
    // A short setTimeout is fine here (unlike the TTL above): it fires a
    // couple of seconds after an event that just woke the service worker,
    // long before any inactivity eviction could occur.
    setTimeout(() => evaluatePendingSave(tabId, frameId), SPA_FALLBACK_MS);
}

// Show the bar for `pending`, in the frame recorded on it. A login widget in
// an iframe can submit and then remove itself, or navigate the whole tab
// (`target="_top"`); the submitting frame is then gone and `tabs.sendMessage`
// to it rejects. Dropping the credential there is exactly the loss the
// save-prompt exists to prevent, so fall back to the top frame once. The bar
// carries the pending key back on a click, so a bar in the top frame still
// saves the credential of the frame that submitted.
async function showBar(tabId, frameId, pending, payload) {
    const key = pendingKey(tabId, frameId);
    let target = Number.isInteger(pending.barFrameId) ? pending.barFrameId : frameId;
    for (;;) {
        try {
            await browser.tabs.sendMessage(tabId, { ...payload, pendingKey: key }, { frameId: target });
            break;
        } catch {
            if (target === 0) return false; // the tab itself is gone
            target = 0;
        }
    }
    if (pending.barFrameId !== target) {
        pending.barFrameId = target;
        await setPendingSave(tabId, frameId, pending);
    }
    return true;
}

async function evaluatePendingSave(tabId, frameId) {
    const pending = await getPendingSave(tabId, frameId);
    if (!pending || (pending.evaluated && !pending.awaitingUnlock)) return;
    pending.evaluated = true;
    await setPendingSave(tabId, frameId, pending);

    try {
        const configResp = await sendToAgent("GetConfig");
        if (!vaultUsable(configResp)) {
            // Locked (or logged out): don't drop the credential — the user can
            // still unlock and then wants Save/Update for the same submission.
            // Show a "locked" bar and leave the pending re-evaluable so an
            // unlock (VAULT_UNLOCKED / a later tab load) re-runs evaluation.
            if (!pending.awaitingUnlock) {
                // Restart the TTL once, at the first deferral: the original
                // window runs from the form submit, and whatever is left of it
                // is no basis for how long unlocking takes. Re-arming on every
                // later evaluation would instead keep the credential alive for
                // as long as the tab keeps navigating.
                browser.alarms.create(alarmName(tabId, frameId), { delayInMinutes: PENDING_TTL_MS / 60000 });
            }
            pending.awaitingUnlock = true;
            pending.evaluated = false;
            pending.mode = 'locked';
            await setPendingSave(tabId, frameId, pending);
            const shown = await showBar(tabId, frameId, pending, {
                type: 'SHOW_SAVE_BAR',
                mode: 'locked',
                domain: pending.domain,
                username: pending.username,
            });
            if (!shown) await clearPendingSave(tabId, frameId);
            return;
        }

        pending.awaitingUnlock = false;
        const matchResp = await sendToAgent({
            CheckLoginMatch: {
                domain: pending.domain,
                username: pending.username,
                password: pending.password,
            },
        });
        const mode = decideBarMode(matchResp);
        if (!mode) {
            await clearPendingSave(tabId, frameId);
            return;
        }

        pending.mode = mode;
        pending.entryId = matchResp.LoginMatch.entry_id;
        await setPendingSave(tabId, frameId, pending);
        // Display labels only — the password never goes back to the page.
        const shown = await showBar(tabId, frameId, pending, {
            type: 'SHOW_SAVE_BAR',
            mode,
            domain: pending.domain,
            username: pending.username,
            entryName: matchResp.LoginMatch.name,
        });
        if (!shown) await clearPendingSave(tabId, frameId);
    } catch {
        // Agent unreachable or tab gone; drop the credential.
        await clearPendingSave(tabId, frameId);
    }
}

// Returns the pending promise (harmlessly ignored by the tabs.onUpdated
// listener) so tests can await evaluation instead of racing it.
async function onTabComplete(tabId) {
    for (const frameId of await pendingFramesForTab(tabId)) {
        await evaluatePendingSave(tabId, frameId);
    }
}

async function onTabRemoved(tabId) {
    for (const frameId of await pendingFramesForTab(tabId)) {
        await clearPendingSave(tabId, frameId);
    }
}

// The pending a bar click is about. A bar normally carries the key of the
// pending it was shown for, which is the only reliable identity when the bar
// had to fall back to the top frame. The key is only a hint: it is accepted
// solely when it names a pending in the tab the click came from.
function pendingIdsFromKey(key, tabId) {
    const m = /^pendingSave:(\d+):(\d+)$/.exec(String(key || ''));
    if (!m) return null;
    if (Number(m[1]) !== tabId) return null;
    return { tabId: Number(m[1]), frameId: Number(m[2]) };
}

async function onBarAction(tabId, senderFrameId, action, key) {
    // Replies go to the frame the bar is in (the sender), not to the frame
    // that submitted: after a fallback those differ, and the bar waiting on
    // HIDE_SAVE_BAR is the one the user clicked.
    const replyFrame = senderFrameId;
    const ids = pendingIdsFromKey(key, tabId) || { tabId, frameId: senderFrameId };
    const pending = await getPendingSave(ids.tabId, ids.frameId);
    if (!pending) {
        // The bar is showing (the user just clicked it) but we have no
        // pending credential for this frame — it was lost or already cleared
        // out from under a bar the user is still looking at. Surface this
        // rather than leaving the bar stuck on "Saving…"/"Updating…" until
        // its own 30s auto-dismiss silently removes it with nothing saved.
        if (action !== 'dismiss') {
            console.warn(`cosmarden: no pending save for tab ${tabId} frame ${ids.frameId} on action "${action}"`);
            browser.tabs.sendMessage(tabId, { type: 'SAVE_BAR_ERROR' }, { frameId: replyFrame }).catch(() => {});
        }
        return { Ack: true };
    }

    if (action === 'unlock' && pending.mode === 'locked') {
        // Open the popup so the user can PIN-unlock; the pending credential
        // stays put and is re-evaluated on VAULT_UNLOCKED.
        try { await browser.action.openPopup(); } catch { /* popup can't open here */ }
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
        await clearPendingSave(ids.tabId, ids.frameId);
        if (action !== 'dismiss') {
            // Clicked action doesn't match what we evaluated (stale pending
            // state, e.g. overwritten by a second LOGIN_SUBMITTED before the
            // user acted) — the click did not save/update anything, so say so
            // rather than quietly clearing state with no feedback at all.
            console.warn(
                `cosmarden: action "${action}" did not match pending mode ` +
                `"${pending.mode}" for tab ${ids.tabId} frame ${ids.frameId}; dropping without saving`
            );
            browser.tabs.sendMessage(tabId, { type: 'SAVE_BAR_ERROR' }, { frameId: replyFrame }).catch(() => {});
        }
        return { Ack: true };
    }

    try {
        const response = await sendToAgent(agentAction);
        if (response === 'Ack' || (response && response.Ack !== undefined)) {
            await clearPendingSave(ids.tabId, ids.frameId);
            browser.tabs.sendMessage(tabId, { type: 'HIDE_SAVE_BAR' }, { frameId: replyFrame }).catch(() => {});
            updateBadge(tabId, pending.url);
            return { Ack: true };
        }
        console.warn(`cosmarden: ${action} rejected by agent:`, response);
        browser.tabs.sendMessage(tabId, { type: 'SAVE_BAR_ERROR' }, { frameId: replyFrame }).catch(() => {});
        return response;
    } catch (e) {
        console.warn(`cosmarden: ${action} failed:`, e);
        browser.tabs.sendMessage(tabId, { type: 'SAVE_BAR_ERROR' }, { frameId: replyFrame }).catch(() => {});
        return { Error: { message: String(e) } };
    }
}

function originOf(url) {
    try { return new URL(url).origin; } catch { return null; }
}

// Re-evaluate any deferred (locked-vault) pending saves now that the vault is
// unlocked — called from background.js on a VAULT_UNLOCKED message.
async function onVaultUnlocked() {
    let keys = [];
    try {
        const all = await browser.storage.session.get(null);
        keys = all ? Object.keys(all) : [];
    } catch { return; }
    for (const key of keys) {
        const m = /^pendingSave:(\d+):(\d+)$/.exec(key);
        if (m) await evaluatePendingSave(Number(m[1]), Number(m[2]));
    }
}

// Fires even if the service worker was killed and restarted since the alarm
// was scheduled — unlike setTimeout, browser.alarms survives that.
browser.alarms.onAlarm.addListener((alarm) => {
    const m = /^pendingSaveExpire:(\d+):(\d+)$/.exec(alarm.name);
    if (m) clearPendingSave(Number(m[1]), Number(m[2]));
});

globalThis.savePrompt = {
    onLoginSubmitted,
    onTabComplete,
    onBarAction,
    onTabRemoved,
    onVaultUnlocked,
    // exposed for unit tests
    decideBarMode,
    vaultUsable,
    getPendingSave,
};
