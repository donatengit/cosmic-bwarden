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
    let onAlarmListener = null;
    return {
        storage: {
            session: {
                async get(key) {
                    return Object.prototype.hasOwnProperty.call(sessionStore, key)
                        ? { [key]: sessionStore[key] }
                        : {};
                },
                async set(obj) { Object.assign(sessionStore, obj); },
                async remove(key) { delete sessionStore[key]; },
            },
        },
        alarms: {
            create(name, info) { alarms.set(name, info); },
            async clear(name) { return alarms.delete(name); },
            onAlarm: {
                addListener(fn) { onAlarmListener = fn; },
            },
            // test helper, not part of the real API
            _fire(name) { if (onAlarmListener) onAlarmListener({ name }); },
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
