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
            async sendMessage(tabId, message) {
                this.sentMessages.push({ tabId, message });
                return { Ack: true };
            },
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

        await savePrompt.onLoginSubmitted(7, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        // Bypass the SPA-fallback setTimeout — call the same evaluation path
        // it would have triggered, directly.
        await savePrompt.onTabComplete(7);

        expect(await savePrompt.getPendingSave(7)).not.toBeNull();

        const result = await savePrompt.onBarAction(7, 'save');
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
        expect(await savePrompt.getPendingSave(7)).toBeNull();
    });

    it('surfaces an error (not silence) when there is no pending save for a save/update click', async () => {
        const fakeBrowser = makeFakeBrowser();
        const savePrompt = loadSavePrompt(fakeBrowser, makeFakeAgent(() => ({ Ack: true })));

        const result = await savePrompt.onBarAction(999, 'save');
        expect(result).toEqual({ Ack: true });
        // The bar is stuck on "Saving…" waiting for a response — it must get
        // one, rather than silently hanging until its own auto-dismiss timer.
        expect(fakeBrowser.tabs.sentMessages).toEqual([
            { tabId: 999, message: { type: 'SAVE_BAR_ERROR' } },
        ]);
    });

    it('stays quiet when dismiss is clicked with no pending save (already cleared, e.g. by TTL)', async () => {
        const fakeBrowser = makeFakeBrowser();
        const savePrompt = loadSavePrompt(fakeBrowser, makeFakeAgent(() => ({ Ack: true })));

        const result = await savePrompt.onBarAction(999, 'dismiss');
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

        await savePrompt.onLoginSubmitted(3, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        await savePrompt.onTabComplete(3); // mode ends up 'save'

        // Simulate clicking "Update" against a pending entry evaluated as 'save'.
        const result = await savePrompt.onBarAction(3, 'update');
        expect(result).toEqual({ Ack: true });
        expect(await savePrompt.getPendingSave(3)).toBeNull();
        expect(fakeBrowser.tabs.sentMessages.at(-1)).toEqual(
            { tabId: 3, message: { type: 'SAVE_BAR_ERROR' } },
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
        await firstInstance.onLoginSubmitted(42, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        await firstInstance.onTabComplete(42);
        expect(await firstInstance.getPendingSave(42)).not.toBeNull();

        // Simulate the service worker being killed and restarted: a brand
        // new module instance, sharing only the persistent fakeBrowser store.
        const restartedInstance = loadSavePrompt(fakeBrowser, agent);

        const result = await restartedInstance.onBarAction(42, 'save');
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

        await savePrompt.onLoginSubmitted(5, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        await savePrompt.onTabComplete(5);
        expect(await savePrompt.getPendingSave(5)).not.toBeNull();

        fakeBrowser.alarms._fire('pendingSaveExpire:5');
        // clearPendingSave is async; let its microtasks settle.
        await new Promise((r) => setTimeout(r, 0));

        expect(await savePrompt.getPendingSave(5)).toBeNull();
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

        await savePrompt.onLoginSubmitted(21, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        await savePrompt.onTabComplete(21);

        // Locked vault: the submission survives evaluation instead of being
        // silently dropped, and a "locked" bar is shown so the user knows to
        // unlock.
        expect(await savePrompt.getPendingSave(21)).not.toBeNull();
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

        await savePrompt.onLoginSubmitted(22, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        await savePrompt.onTabComplete(22);

        expect(await savePrompt.getPendingSave(22)).not.toBeNull();

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
            await savePrompt.onLoginSubmitted(tabId, {
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

        await savePrompt.onLoginSubmitted(33, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        const afterSubmit = fakeBrowser.alarms._created('pendingSaveExpire:33');

        await savePrompt.onTabComplete(33);
        expect(fakeBrowser.alarms._created('pendingSaveExpire:33')).toBe(afterSubmit + 1);

        await savePrompt.onTabComplete(33);
        await savePrompt.onTabComplete(33);
        expect(fakeBrowser.alarms._created('pendingSaveExpire:33')).toBe(afterSubmit + 1);
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

        await savePrompt.onLoginSubmitted(34, {
            url: 'https://example.com/login', username: 'alice', password: 'hunter2',
        });
        await savePrompt.onTabComplete(34);
        expect(await savePrompt.getPendingSave(34)).not.toBeNull();

        await savePrompt.onBarAction(34, 'dismiss');
        expect(await savePrompt.getPendingSave(34)).toBeNull();
    });
});
