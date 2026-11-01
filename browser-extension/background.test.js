// Unit tests for background.js's toolbar state: the action icon reflects
// whether the vault is locked or unlocked — the same runtime-state idea as the
// COSMIC panel applet's lock-state icon swap (icons/white_locked.svg) — not
// just the black/white color theme. The badge shows the number of entries the
// agent matched for the tab's host, on a neutral grey background.
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const source = readFileSync(
    path.join(path.dirname(fileURLToPath(import.meta.url)), 'background.js'),
    'utf8'
);

// Fake of the WebExtension APIs background.js wires up at load time. Unlike
// the unit tests for background-save.js, sendToAgent cannot simply be injected
// as a parameter: background.js declares its own (the native-messaging queue),
// which would shadow the parameter and hang on a port that never answers. So
// connectNative() returns a port whose postMessage() resolves the agent call
// and delivers the response through the real onMessage listener — exercising
// background.js's own queue machinery against the fake agent.
function makeFakeBrowser(agent) {
    const calls = { setIcon: [], setBadgeText: [], setBadgeBackgroundColor: [], onMessage: null };
    return {
        calls,
        runtime: {
            getURL: (p) => 'moz-extension://test-uuid/' + p,
            connectNative: () => {
                const port = {
                    _onMessage: null,
                    onMessage: { addListener: (fn) => { port._onMessage = fn; } },
                    onDisconnect: { addListener: () => {} },
                    postMessage: async (action) => {
                        const response = await agent.sendToAgent(action);
                        if (port._onMessage) port._onMessage(response);
                    },
                    disconnect: () => {},
                };
                return port;
            },
            onMessage: { addListener: (fn) => { calls.onMessage = fn; } },
        },
        action: {
            setIcon: (opts) => calls.setIcon.push(opts),
            setBadgeText: (opts) => calls.setBadgeText.push(opts),
            setBadgeBackgroundColor: (opts) => calls.setBadgeBackgroundColor.push(opts),
        },
        tabs: {
            query: async () => [{ id: 1, url: 'https://example.com/' }],
            get: async (tabId) => ({ id: tabId, url: 'https://example.com/' }),
            onActivated: { addListener: () => {} },
            onUpdated: { addListener: () => {} },
            onRemoved: { addListener: () => {} },
        },
        contextMenus: {
            create: () => {},
            onClicked: { addListener: () => {} },
        },
        // background-save.js's alarms/storage are referenced only inside
        // listener bodies that these tests never fire; stubs keep the wiring
        // from crashing if a listener is registered at load time.
        alarms: { onAlarm: { addListener: () => {} } },
        storage: {
            session: { get: async () => ({}), set: async () => {}, remove: async () => {} },
        },
    };
}

function makeFakeAgent(locked, entryCount = 0) {
    const calls = [];
    return {
        calls,
        sendToAgent: async (action) => {
            calls.push(action);
            if (action === 'GetConfig') return { Config: { is_locked: locked, needs_login: false } };
            if (action && action.GetSidebarEntries) {
                const entries = Array.from({ length: entryCount }, (_, i) => ({ id: `entry-${i}` }));
                return { SidebarEntries: { entries } };
            }
            throw new Error('unexpected action: ' + JSON.stringify(action));
        },
    };
}

// Load the script through new Function in a fresh scope (same pattern as
// background-save.test.js) and return the functions under test.
function loadBackground(fakeBrowser, savePrompt = { onVaultUnlocked: () => {} }) {
    const fn = new Function(
        'browser',
        'savePrompt',
        `${source}\nreturn { updateBadge, setThemeIcon, extractDomain, isPopupSender };`
    );
    return fn(fakeBrowser, savePrompt);
}

describe('content-script message boundary', () => {
    it('rejects an agent action sent from a web-page content script', async () => {
        // No content script needs an agent call any more: the inline generate
        // icon (the last GeneratePassword proxy) was removed. Fill and save are
        // handled locally in the listener above this guard.
        const agent = makeFakeAgent(false);
        const fakeBrowser = makeFakeBrowser(agent);
        loadBackground(fakeBrowser);
        const response = await fakeBrowser.calls.onMessage(
            { GeneratePassword: { settings: null } },
            { tab: { id: 1 }, frameId: 0, url: 'https://example.com/' }
        );
        expect(response).toEqual({ Error: { message: 'action not allowed from content script' } });
        expect(agent.calls.some(c => c && c.GeneratePassword)).toBe(false);
    });

    it('allows the popup page itself even when rendered in a tab', () => {
        // The full E2E specs open popup.html in a tab; the background must not
        // mistake that for a web-page content script (whose sender.url is an
        // http(s) origin that cannot forge the extension's own popup/ URL).
        const agent = makeFakeAgent(false);
        const { isPopupSender } = loadBackground(makeFakeBrowser(agent));
        expect(isPopupSender({ url: 'moz-extension://test-uuid/popup/popup.html', tab: {} })).toBe(true);
        expect(isPopupSender({ url: 'moz-extension://other/popup/popup.html', tab: {} })).toBe(false);
        expect(isPopupSender({ url: 'https://example.com/', tab: {} })).toBe(false);
        expect(isPopupSender(null)).toBe(false);
    });
});

describe('toolbar match badge', () => {
    it('shows the domain match count on a neutral grey background', async () => {
        const agent = makeFakeAgent(false, 3);
        const fakeBrowser = makeFakeBrowser(agent);
        const { updateBadge } = loadBackground(fakeBrowser);

        await updateBadge(1, 'https://example.com/login');

        expect(fakeBrowser.calls.setBadgeText.at(-1)).toEqual({ text: '3', tabId: 1 });
        expect(fakeBrowser.calls.setBadgeBackgroundColor.at(-1)).toEqual({
            color: '#5c5f66',
            tabId: 1,
        });
        const query = agent.calls.find(c => c && c.GetSidebarEntries);
        expect(query.GetSidebarEntries.domain).toBe('example.com');
    });

    it('clears the badge when no entry matches the domain', async () => {
        const agent = makeFakeAgent(false, 0);
        const fakeBrowser = makeFakeBrowser(agent);
        const { updateBadge } = loadBackground(fakeBrowser);

        await updateBadge(1, 'https://example.com/login');

        expect(fakeBrowser.calls.setBadgeText.at(-1)).toEqual({ text: '', tabId: 1 });
        expect(fakeBrowser.calls.setBadgeBackgroundColor).toEqual([]);
    });

    it('refreshes the count after the vault unlocks', async () => {
        // The count cannot be read while locked, so the unlock message must
        // recompute it instead of waiting for the next tab activation.
        const agent = makeFakeAgent(false, 2);
        const fakeBrowser = makeFakeBrowser(agent);
        loadBackground(fakeBrowser);

        await fakeBrowser.calls.onMessage(
            { type: 'VAULT_UNLOCKED' },
            { url: 'moz-extension://test-uuid/popup/popup.html', tab: {} }
        );

        // The refresh is deliberately fire-and-forget, so wait for the agent
        // round trips it schedules instead of assuming they finished.
        for (let i = 0; i < 50; i++) {
            const last = fakeBrowser.calls.setBadgeText.at(-1);
            if (last && last.text === '2') break;
            await new Promise((resolve) => setTimeout(resolve, 0));
        }
        expect(fakeBrowser.calls.setBadgeText.at(-1)).toEqual({ text: '2', tabId: 1 });
        expect(fakeBrowser.calls.setBadgeBackgroundColor.at(-1)).toEqual({
            color: '#5c5f66',
            tabId: 1,
        });
    });
});

describe('toolbar lock-state icon', () => {
    it('switches the action icon to the locked variant when the vault is locked', async () => {
        const agent = makeFakeAgent(true);
        const fakeBrowser = makeFakeBrowser(agent);
        const { updateBadge } = loadBackground(fakeBrowser);

        await updateBadge(1, 'https://example.com/');

        // The last icon write must be the locked variant. Naming follows the
        // applet's `_locked` suffix (icons/white_locked.svg); theme stays
        // black/white as today — black for the light theme.
        const iconCall = fakeBrowser.calls.setIcon.at(-1);
        expect(iconCall).toBeTruthy();
        expect(iconCall.path).toEqual({
            16: 'icons/black_locked16.png',
            32: 'icons/black_locked32.png',
            64: 'icons/black_locked64.png',
            128: 'icons/black_locked128.png',
        });
    });

    it('never uses the locked variant when the vault is unlocked', async () => {
        const agent = makeFakeAgent(false);
        const fakeBrowser = makeFakeBrowser(agent);
        const { updateBadge } = loadBackground(fakeBrowser);

        await updateBadge(1, 'https://example.com/');

        // The lock state must actually be consulted (GetConfig), not assumed.
        expect(agent.calls.some(c => c === 'GetConfig')).toBe(true);

        const iconCall = fakeBrowser.calls.setIcon.at(-1);
        if (iconCall) {
            expect(JSON.stringify(iconCall.path)).not.toContain('_locked');
        }
    });
});
