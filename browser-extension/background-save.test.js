// Unit tests for the save-prompt state machine in background-save.js.
import { describe, it, expect, beforeEach } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const source = readFileSync(
    path.join(path.dirname(fileURLToPath(import.meta.url)), 'background-save.js'),
    'utf8'
);

// In-memory fake of the two browser APIs background-save.js depends on for
// cross-service-worker-restart durability. Shared across module instances in
// a test (see loadSavePrompt below) so it can stand in for the one real
// browser.storage.session/alarms backing store a restarted service worker
// would still see.
function makeFakeBrowser() {
    const sessionStore = {};
    const alarms = new Map();
    const created = new Map();
    let onAlarmListener = null;
    return {
        storage: {
            session: {
                async get(key) {
                    // A null/undefined key means "everything", the real API's
                    // behaviour — onVaultUnlocked uses it to find every
                    // deferred tab.
                    if (key === null || key === undefined) return { ...sessionStore };
                    return Object.prototype.hasOwnProperty.call(sessionStore, key)
                        ? { [key]: sessionStore[key] }
                        : {};
                },
                async set(obj) { Object.assign(sessionStore, obj); },
                async remove(key) { delete sessionStore[key]; },
            },
        },
        alarms: {
            create(name, info) {
                alarms.set(name, info);
                created.set(name, (created.get(name) ?? 0) + 1);
            },
            async clear(name) { return alarms.delete(name); },
            onAlarm: {
                addListener(fn) { onAlarmListener = fn; },
            },
            // test helpers, not part of the real API
            _fire(name) { if (onAlarmListener) onAlarmListener({ name }); },
            // How many times this alarm was (re-)armed — create() on an
            // existing name replaces it, so the count is the only way to see a
            // TTL restart.
            _created(name) { return created.get(name) ?? 0; },
        },
        tabs: {
            sentMessages: [],
            async sendMessage(tabId, message, options) {
                const frameId = options && options.frameId;
                this.sentMessages.push({ tabId, frameId, message });
                // Frames listed here are gone, as after a navigation: the real
                // API rejects with "Receiving end does not exist".
                if (this.failedFrames.has(frameId)) {
                    throw new Error('Could not establish connection. Receiving end does not exist.');
                }
                return { Ack: true };
            },
            failedFrames: new Set(),
        },
    };
}

// Vitest files are ESM (strict mode), where eval() keeps declarations local —
// load the script through new Function in a fresh scope each time, mirroring
// a fresh service-worker instance. `fakeBrowser` and `agentResponses` are
// shared across instances the way browser.storage.session and the real
// native-messaging agent are shared across service-worker restarts.
function loadSavePrompt(fakeBrowser, agent) {
    const fn = new Function(
        'browser', 'sendToAgent', 'extractDomain', 'updateBadge',
        `${source}\nreturn savePrompt;`
    );
    return fn(fakeBrowser, agent.sendToAgent, agent.extractDomain, agent.updateBadge);
}

function makeFakeAgent(responder) {
    const calls = [];
    return {
        calls,
        sendToAgent: async (action) => {
            calls.push(action);
            return responder(action);
        },
        extractDomain: (url) => new URL(url).hostname,
        updateBadge: () => {},
    };
}

describe('vaultUsable', () => {
    let savePrompt;
    beforeEach(() => {
        savePrompt = loadSavePrompt(makeFakeBrowser(), makeFakeAgent(() => ({ Ack: true })));
    });

    it('is true for an unlocked, logged-in vault', () => {
        expect(savePrompt.vaultUsable({ Config: { is_locked: false, needs_login: false } })).toBe(true);
    });

    it('is false when the vault is locked', () => {
        expect(savePrompt.vaultUsable({ Config: { is_locked: true, needs_login: false } })).toBe(false);
    });

    it('is false when not logged in', () => {
        expect(savePrompt.vaultUsable({ Config: { is_locked: false, needs_login: true } })).toBe(false);
    });

    it('is false for error or malformed responses', () => {
        expect(savePrompt.vaultUsable({ Error: { message: 'boom' } })).toBe(false);
        expect(savePrompt.vaultUsable(null)).toBe(false);
        expect(savePrompt.vaultUsable(undefined)).toBe(false);
    });
});

describe('decideBarMode', () => {
    let savePrompt;
    beforeEach(() => {
        savePrompt = loadSavePrompt(makeFakeBrowser(), makeFakeAgent(() => ({ Ack: true })));
    });

    it('offers save when no entry matches the domain+username', () => {
        const resp = { LoginMatch: { entry_id: null, name: null, password_matches: false } };
        expect(savePrompt.decideBarMode(resp)).toBe('save');
    });

    it('offers update when an entry matches but the password changed', () => {
        const resp = { LoginMatch: { entry_id: 'id-1', name: 'example.com', password_matches: false } };
        expect(savePrompt.decideBarMode(resp)).toBe('update');
    });

    it('stays silent when the credential is already stored as-is', () => {
        const resp = { LoginMatch: { entry_id: 'id-1', name: 'example.com', password_matches: true } };
        expect(savePrompt.decideBarMode(resp)).toBeNull();
    });

    it('stays silent on error or malformed responses', () => {
        expect(savePrompt.decideBarMode({ Error: { message: 'agent is locked' } })).toBeNull();
        expect(savePrompt.decideBarMode(null)).toBeNull();
        expect(savePrompt.decideBarMode(undefined)).toBeNull();
    });
});

describe('onBarAction', () => {
    it('adds the entry and clears pending state on save', async () => {
        const fakeBrowser = makeFakeBrowser();
        const agent = makeFakeAgent((action) => {
            if (action === 'GetConfig') return { Config: { is_locked: false, needs_login: false } };
            if (action.CheckLoginMatch) return { LoginMatch: { entry_id: null, name: null, password_matches: false } };
            if (action.AddEntry) return { Ack: true };
            throw new Error('unexpected action');
        });
        const savePrompt = loadSavePrompt(fakeBrowser, agent);

        await savePrompt.onLoginSubmitted(7, 0, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        // Bypass the SPA-fallback setTimeout — call the same evaluation path
        // it would have triggered, directly.
        await savePrompt.onTabComplete(7);

        expect(await savePrompt.getPendingSave(7, 0)).not.toBeNull();

        const result = await savePrompt.onBarAction(7, 0, 'save');
        expect(result).toEqual({ Ack: true });
        expect(agent.calls.at(-1)).toEqual({
            AddEntry: {
                name: 'example.com',
                entry_type: 'Login',
                username: 'alice',
                password: 'hunter2',
                notes: null,
                fields: [],
                totp: null,
                uris: [{ uri: 'https://example.com', match_type: null }],
            },
        });
        expect(await savePrompt.getPendingSave(7, 0)).toBeNull();
    });

    it('keeps pending save when the agent rejects so the user can retry', async () => {
        const fakeBrowser = makeFakeBrowser();
        const agent = makeFakeAgent((action) => {
            if (action === 'GetConfig') return { Config: { is_locked: false, needs_login: false } };
            if (action.CheckLoginMatch) return { LoginMatch: { entry_id: null, name: null, password_matches: false } };
            if (action.AddEntry) return { Error: { message: 'server down' } };
            throw new Error('unexpected action');
        });
        const savePrompt = loadSavePrompt(fakeBrowser, agent);

        await savePrompt.onLoginSubmitted(7, 0, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        await savePrompt.onTabComplete(7);

        const result = await savePrompt.onBarAction(7, 0, 'save');
        expect(result).toEqual({ Error: { message: 'server down' } });
        expect(await savePrompt.getPendingSave(7, 0)).not.toBeNull();
        expect(fakeBrowser.tabs.sentMessages.at(-1)).toEqual(
            { tabId: 7, frameId: 0, message: { type: 'SAVE_BAR_ERROR' } },
        );
    });

    it('surfaces an error (not silence) when there is no pending save for a save/update click', async () => {
        const fakeBrowser = makeFakeBrowser();
        const savePrompt = loadSavePrompt(fakeBrowser, makeFakeAgent(() => ({ Ack: true })));

        const result = await savePrompt.onBarAction(999, 0, 'save');
        expect(result).toEqual({ Ack: true });
        // The bar is stuck on "Saving…" waiting for a response — it must get
        // one, rather than silently hanging until its own auto-dismiss timer.
        expect(fakeBrowser.tabs.sentMessages).toEqual([
            { tabId: 999, frameId: 0, message: { type: 'SAVE_BAR_ERROR' } },
        ]);
    });

    it('stays quiet when dismiss is clicked with no pending save (already cleared, e.g. by TTL)', async () => {
        const fakeBrowser = makeFakeBrowser();
        const savePrompt = loadSavePrompt(fakeBrowser, makeFakeAgent(() => ({ Ack: true })));

        const result = await savePrompt.onBarAction(999, 0, 'dismiss');
        expect(result).toEqual({ Ack: true });
        expect(fakeBrowser.tabs.sentMessages).toEqual([]);
    });

    // Deterministic bug, independent of any service-worker timing: if the
    // pending state's mode no longer matches the clicked action (e.g. a
    // second LOGIN_SUBMITTED overwrote it before the user acted), the old
    // code cleared state and returned silently — the user saw the bar say
    // "Saving…" and then just get stuck/vanish, with nothing ever saved and
    // no error shown.
    it('surfaces an error when the clicked action does not match the evaluated mode', async () => {
        const fakeBrowser = makeFakeBrowser();
        const agent = makeFakeAgent((action) => {
            if (action === 'GetConfig') return { Config: { is_locked: false, needs_login: false } };
            if (action.CheckLoginMatch) return { LoginMatch: { entry_id: null, name: null, password_matches: false } };
            throw new Error('unexpected action');
        });
        const savePrompt = loadSavePrompt(fakeBrowser, agent);

        await savePrompt.onLoginSubmitted(3, 0, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        await savePrompt.onTabComplete(3); // mode ends up 'save'

        // Simulate clicking "Update" against a pending entry evaluated as 'save'.
        const result = await savePrompt.onBarAction(3, 0, 'update');
        expect(result).toEqual({ Ack: true });
        expect(await savePrompt.getPendingSave(3, 0)).toBeNull();
        expect(fakeBrowser.tabs.sentMessages.at(-1)).toEqual(
            { tabId: 3, frameId: 0, message: { type: 'SAVE_BAR_ERROR' } },
        );
    });

    // Regression test: a Chrome MV3 service worker is killed after ~30s of
    // inactivity, which is routinely less than the time a user takes to
    // read the save bar and click Save. Before this fix, pending-save state
    // lived in a plain module-level Map, so a restart silently wiped it and
    // the confirm click landed on onBarAction with nothing to act on — the
    // bar showed correctly but confirming did nothing. storage.session
    // (unlike a Map) is shared across module instances, so a second,
    // independently-loaded savePrompt instance here stands in for the
    // service worker having been killed and restarted before the click.
    it('survives a simulated service-worker restart between showing the bar and confirming', async () => {
        const fakeBrowser = makeFakeBrowser();
        const agent = makeFakeAgent((action) => {
            if (action === 'GetConfig') return { Config: { is_locked: false, needs_login: false } };
            if (action.CheckLoginMatch) return { LoginMatch: { entry_id: null, name: null, password_matches: false } };
            if (action.AddEntry) return { Ack: true };
            throw new Error('unexpected action');
        });

        const firstInstance = loadSavePrompt(fakeBrowser, agent);
        await firstInstance.onLoginSubmitted(42, 0, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        await firstInstance.onTabComplete(42);
        expect(await firstInstance.getPendingSave(42, 0)).not.toBeNull();

        // Simulate the service worker being killed and restarted: a brand
        // new module instance, sharing only the persistent fakeBrowser store.
        const restartedInstance = loadSavePrompt(fakeBrowser, agent);

        const result = await restartedInstance.onBarAction(42, 0, 'save');
        expect(result).toEqual({ Ack: true });
        expect(agent.calls.some((c) => c.AddEntry)).toBe(true);
    });

    it('clears pending state when the TTL alarm fires before the user acts', async () => {
        const fakeBrowser = makeFakeBrowser();
        const agent = makeFakeAgent((action) => {
            if (action === 'GetConfig') return { Config: { is_locked: false, needs_login: false } };
            if (action.CheckLoginMatch) return { LoginMatch: { entry_id: null, name: null, password_matches: false } };
            throw new Error('unexpected action');
        });
        const savePrompt = loadSavePrompt(fakeBrowser, agent);

        await savePrompt.onLoginSubmitted(5, 0, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        await savePrompt.onTabComplete(5);
        expect(await savePrompt.getPendingSave(5, 0)).not.toBeNull();

        fakeBrowser.alarms._fire('pendingSaveExpire:5:0');
        // clearPendingSave is async; let its microtasks settle.
        await new Promise((r) => setTimeout(r, 0));

        expect(await savePrompt.getPendingSave(5, 0)).toBeNull();
    });
});

describe('save prompt with a locked vault (Unlock & Save)', () => {
    // A user submitting a login while the vault is locked must not lose the
    // credential. v1 behavior was to silently drop the prompt
    // (evaluatePendingSave cleared the pending state when vaultUsable() was
    // false) — so after unlocking, nothing was ever offered. Desired flow:
    // keep the pending credential while locked, and once the vault is
    // unlocked offer Save/Update for the same submission. A fix must also
    // leave the pending state re-evaluable after deferral (the `evaluated`
    // flag blocks a second onTabComplete pass).
    it('keeps the pending credential while locked, then offers save after unlock', async () => {
        let locked = true;
        const fakeBrowser = makeFakeBrowser();
        const agent = makeFakeAgent((action) => {
            if (action === 'GetConfig') return { Config: { is_locked: locked, needs_login: false } };
            if (action.CheckLoginMatch) return { LoginMatch: { entry_id: null, name: null, password_matches: false } };
            throw new Error('unexpected action');
        });
        const savePrompt = loadSavePrompt(fakeBrowser, agent);

        await savePrompt.onLoginSubmitted(21, 0, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        await savePrompt.onTabComplete(21);

        // Locked vault: the submission survives evaluation instead of being
        // silently dropped, and a "locked" bar is shown so the user knows to
        // unlock.
        expect(await savePrompt.getPendingSave(21, 0)).not.toBeNull();
        expect(fakeBrowser.tabs.sentMessages.some(m =>
            m.message && m.message.type === 'SHOW_SAVE_BAR' && m.message.mode === 'locked'
        )).toBe(true);

        // User unlocks (e.g. via the applet/popup); re-evaluating the same
        // pending credential must now surface the save bar with the
        // save/update decision.
        locked = false;
        await savePrompt.onTabComplete(21);

        const barMessage = fakeBrowser.tabs.sentMessages.find(m =>
            m.message && m.message.type === 'SHOW_SAVE_BAR' && m.message.mode === 'save'
        );
        expect(barMessage).toBeTruthy();
        expect(agent.calls.some(c => c && c.CheckLoginMatch)).toBe(true);
    });

    // Same deferral, but a matching entry whose password changed must come
    // back as an Update offer after unlock.
    it('offers update (not save) after unlock when a matching entry exists', async () => {
        let locked = true;
        const fakeBrowser = makeFakeBrowser();
        const agent = makeFakeAgent((action) => {
            if (action === 'GetConfig') return { Config: { is_locked: locked, needs_login: false } };
            if (action.CheckLoginMatch) {
                return { LoginMatch: { entry_id: 'id-9', name: 'example.com', password_matches: false } };
            }
            throw new Error('unexpected action');
        });
        const savePrompt = loadSavePrompt(fakeBrowser, agent);

        await savePrompt.onLoginSubmitted(22, 0, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        await savePrompt.onTabComplete(22);

        expect(await savePrompt.getPendingSave(22, 0)).not.toBeNull();

        locked = false;
        await savePrompt.onTabComplete(22);

        const barMessage = fakeBrowser.tabs.sentMessages.find(m =>
            m.message && m.message.type === 'SHOW_SAVE_BAR' && m.message.mode === 'update'
        );
        expect(barMessage).toBeTruthy();
        expect(barMessage.message.entryName).toBe('example.com');
    });

    // The unlock path the popup actually drives: it sends VAULT_UNLOCKED, which
    // sweeps every deferred tab rather than waiting for a page load that may
    // never come. Uses storage.session.get(null), so it must ignore the other
    // keys sharing that namespace (the popup's own saved view state).
    it('re-offers every deferred tab on VAULT_UNLOCKED, ignoring foreign keys', async () => {
        let locked = true;
        const fakeBrowser = makeFakeBrowser();
        const agent = makeFakeAgent((action) => {
            if (action === 'GetConfig') return { Config: { is_locked: locked, needs_login: false } };
            if (action.CheckLoginMatch) return { LoginMatch: { entry_id: null, name: null, password_matches: false } };
            throw new Error('unexpected action');
        });
        const savePrompt = loadSavePrompt(fakeBrowser, agent);

        for (const tabId of [31, 32]) {
            await savePrompt.onLoginSubmitted(tabId, 0, {
                url: 'https://example.com/login', username: 'alice', password: 'hunter2',
            });
            await savePrompt.onTabComplete(tabId);
        }
        // popup-state.js shares browser.storage.session; it must not be mistaken
        // for a pending save.
        await fakeBrowser.storage.session.set({ popupState: { view: 'list' } });

        locked = false;
        await savePrompt.onVaultUnlocked();

        const offered = fakeBrowser.tabs.sentMessages.filter(m =>
            m.message.type === 'SHOW_SAVE_BAR' && m.message.mode === 'save'
        );
        expect(offered.map(m => m.tabId).sort()).toEqual([31, 32]);
    });

    // The TTL runs from the form submit. Deferring restarts it once, so the
    // clock the user races is "since I was told to unlock", not whatever was
    // left over — but a tab that keeps navigating must not renew it forever.
    it('restarts the TTL once when deferring, not on every re-evaluation', async () => {
        const fakeBrowser = makeFakeBrowser();
        const agent = makeFakeAgent((action) => {
            if (action === 'GetConfig') return { Config: { is_locked: true, needs_login: false } };
            throw new Error('unexpected action');
        });
        const savePrompt = loadSavePrompt(fakeBrowser, agent);

        await savePrompt.onLoginSubmitted(33, 0, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        const afterSubmit = fakeBrowser.alarms._created('pendingSaveExpire:33:0');

        await savePrompt.onTabComplete(33);
        expect(fakeBrowser.alarms._created('pendingSaveExpire:33:0')).toBe(afterSubmit + 1);

        await savePrompt.onTabComplete(33);
        await savePrompt.onTabComplete(33);
        expect(fakeBrowser.alarms._created('pendingSaveExpire:33:0')).toBe(afterSubmit + 1);
    });

    // Regression: the in-page bar auto-dismisses after 30s, and routing that
    // timeout through 'dismiss' cleared the pending credential — losing the
    // deferred save just as silently as the v1 drop it replaced, only 30s
    // later. content-bar.js now times the locked bar out without notifying the
    // background; an explicit Dismiss click must still clear it.
    it('an explicit dismiss on the locked bar drops the credential', async () => {
        const fakeBrowser = makeFakeBrowser();
        const agent = makeFakeAgent((action) => {
            if (action === 'GetConfig') return { Config: { is_locked: true, needs_login: false } };
            throw new Error('unexpected action');
        });
        const savePrompt = loadSavePrompt(fakeBrowser, agent);

        await savePrompt.onLoginSubmitted(34, 0, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        await savePrompt.onTabComplete(34);
        expect(await savePrompt.getPendingSave(34, 0)).not.toBeNull();

        await savePrompt.onBarAction(34, 0, 'dismiss');
        expect(await savePrompt.getPendingSave(34, 0)).toBeNull();
    });
});

describe('frames', () => {
    // The content scripts run in every frame of a tab, so two login widgets on
    // one page (the page's own form plus an embedded component) submit
    // independently. Keyed by tab alone, the second submission overwrote the
    // first and the visible bar then saved the wrong password.
    it('keeps two frames of the same tab separate', async () => {
        const fakeBrowser = makeFakeBrowser();
        const agent = makeFakeAgent((action) => {
            if (action === 'GetConfig') return { Config: { is_locked: false, needs_login: false } };
            if (action.CheckLoginMatch) return { LoginMatch: { entry_id: null, name: null, password_matches: false } };
            if (action.AddEntry) return { Ack: true };
            throw new Error('unexpected action');
        });
        const savePrompt = loadSavePrompt(fakeBrowser, agent);

        await savePrompt.onLoginSubmitted(9, 0, {
            url: 'https://example.com/login', username: 'alice', password: 'top-frame',
        });
        await savePrompt.onLoginSubmitted(9, 3, {
            url: 'https://widget.example.com/login', username: 'bob', password: 'iframe',
        });
        await savePrompt.onTabComplete(9);

        expect((await savePrompt.getPendingSave(9, 0)).username).toBe('alice');
        expect((await savePrompt.getPendingSave(9, 3)).username).toBe('bob');

        // Each bar is shown in the frame that submitted, not in every frame.
        const bars = fakeBrowser.tabs.sentMessages.filter(m => m.message.type === 'SHOW_SAVE_BAR');
        expect(bars.map(m => m.frameId).sort()).toEqual([0, 3]);

        // Saving the iframe's bar must save the iframe's credential and leave
        // the top frame's pending state alone.
        await savePrompt.onBarAction(9, 3, 'save');
        const addEntry = agent.calls.filter(c => c && c.AddEntry).at(-1);
        expect(addEntry.AddEntry.password).toBe('iframe');
        expect(await savePrompt.getPendingSave(9, 3)).toBeNull();
        expect(await savePrompt.getPendingSave(9, 0)).not.toBeNull();
    });

    it('clears every frame of a tab when the tab closes', async () => {
        const fakeBrowser = makeFakeBrowser();
        const agent = makeFakeAgent(() => ({ Config: { is_locked: true, needs_login: false } }));
        const savePrompt = loadSavePrompt(fakeBrowser, agent);

        for (const frameId of [0, 2]) {
            await savePrompt.onLoginSubmitted(11, frameId, {
                url: 'https://example.com/login', username: 'alice', password: 'hunter2',
            });
        }
        await savePrompt.onTabRemoved(11);

        expect(await savePrompt.getPendingSave(11, 0)).toBeNull();
        expect(await savePrompt.getPendingSave(11, 2)).toBeNull();
    });

    it('re-offers every deferred frame on VAULT_UNLOCKED', async () => {
        let locked = true;
        const fakeBrowser = makeFakeBrowser();
        const agent = makeFakeAgent((action) => {
            if (action === 'GetConfig') return { Config: { is_locked: locked, needs_login: false } };
            if (action.CheckLoginMatch) return { LoginMatch: { entry_id: null, name: null, password_matches: false } };
            throw new Error('unexpected action');
        });
        const savePrompt = loadSavePrompt(fakeBrowser, agent);

        for (const frameId of [0, 4]) {
            await savePrompt.onLoginSubmitted(12, frameId, {
                url: 'https://example.com/login', username: 'alice', password: 'hunter2',
            });
            await savePrompt.onTabComplete(12);
        }

        locked = false;
        await savePrompt.onVaultUnlocked();

        const offered = fakeBrowser.tabs.sentMessages.filter(m =>
            m.message.type === 'SHOW_SAVE_BAR' && m.message.mode === 'save'
        );
        expect(offered.map(m => `${m.tabId}:${m.frameId}`).sort()).toEqual(['12:0', '12:4']);
    });

    // A login widget in an iframe can submit and then disappear: it removes
    // itself on success, or its form targets the whole tab (`target="_top"`).
    // The frame id is gone, tabs.sendMessage to it rejects, and the credential
    // must not be dropped — the bar moves to the top frame instead.
    it('falls back to the top frame when the submitting frame is gone', async () => {
        const fakeBrowser = makeFakeBrowser();
        fakeBrowser.tabs.failedFrames.add(3);
        const agent = makeFakeAgent((action) => {
            if (action === 'GetConfig') return { Config: { is_locked: false, needs_login: false } };
            if (action.CheckLoginMatch) return { LoginMatch: { entry_id: null, name: null, password_matches: false } };
            if (action.AddEntry) return { Ack: true };
            throw new Error('unexpected action');
        });
        const savePrompt = loadSavePrompt(fakeBrowser, agent);

        await savePrompt.onLoginSubmitted(21, 3, {
            url: 'https://widget.example.com/login', username: 'bob', password: 'iframepass',
        });
        await savePrompt.onTabComplete(21);

        // Still pending, and the bar was shown in the top frame with the
        // submitting frame's key so the click can find it again.
        const pending = await savePrompt.getPendingSave(21, 3);
        expect(pending).not.toBeNull();
        expect(pending.barFrameId).toBe(0);
        const bars = fakeBrowser.tabs.sentMessages.filter(m => m.message.type === 'SHOW_SAVE_BAR');
        expect(bars.at(-1).frameId).toBe(0);
        expect(bars.at(-1).message.pendingKey).toBe('pendingSave:21:3');

        // Clicking that bar (frame 0) saves the iframe's credential...
        await savePrompt.onBarAction(21, 0, 'save', 'pendingSave:21:3');
        const addEntry = agent.calls.filter(c => c && c.AddEntry).at(-1);
        expect(addEntry.AddEntry.password).toBe('iframepass');
        expect(await savePrompt.getPendingSave(21, 3)).toBeNull();

        // ...and the bar that gets hidden is the one in the frame that clicked.
        const last = fakeBrowser.tabs.sentMessages.at(-1);
        expect(last).toMatchObject({ tabId: 21, frameId: 0, message: { type: 'HIDE_SAVE_BAR' } });
    });

    it('ignores a bar key that names another tab', async () => {
        const fakeBrowser = makeFakeBrowser();
        const agent = makeFakeAgent((action) => {
            if (action === 'GetConfig') return { Config: { is_locked: true, needs_login: false } };
            throw new Error('unexpected action');
        });
        const savePrompt = loadSavePrompt(fakeBrowser, agent);

        await savePrompt.onLoginSubmitted(21, 0, {
            url: 'https://example.com/login', username: 'alice', password: 'mine',
        });
        await savePrompt.onLoginSubmitted(99, 3, {
            url: 'https://other.example/login', username: 'mallory', password: 'theirs',
        });

        // A bar in tab 21 cannot dismiss tab 99's credential by naming its key.
        await savePrompt.onBarAction(21, 0, 'dismiss', 'pendingSave:99:3');
        expect(await savePrompt.getPendingSave(99, 3)).not.toBeNull();
        expect(await savePrompt.getPendingSave(21, 0)).toBeNull();
    });

    it('drops the credential when the tab itself is gone', async () => {
        const fakeBrowser = makeFakeBrowser();
        fakeBrowser.tabs.failedFrames.add(3);
        fakeBrowser.tabs.failedFrames.add(0);
        const agent = makeFakeAgent((action) => {
            if (action === 'GetConfig') return { Config: { is_locked: false, needs_login: false } };
            if (action.CheckLoginMatch) return { LoginMatch: { entry_id: null, name: null, password_matches: false } };
            throw new Error('unexpected action');
        });
        const savePrompt = loadSavePrompt(fakeBrowser, agent);

        await savePrompt.onLoginSubmitted(22, 3, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        await savePrompt.onTabComplete(22);

        expect(await savePrompt.getPendingSave(22, 3)).toBeNull();
    });
});
