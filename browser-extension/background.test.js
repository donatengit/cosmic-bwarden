// Unit tests for background.js's toolbar state: the action icon must reflect
// whether the vault is locked or unlocked — the same runtime-state idea as the
// COSMIC panel applet's lock-state icon swap (icons/white_locked.svg) — not
// just the black/white color theme. Today updateBadge() only ever sets the
// badge count and never consults GetConfig, so the toolbar looks identical
// whether the vault is open or locked.
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
    const calls = { setIcon: [], setBadgeText: [], setBadgeBackgroundColor: [] };
    return {
        calls,
        runtime: {
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
            onMessage: { addListener: () => {} },
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

function makeFakeAgent(locked) {
    const calls = [];
    return {
        calls,
        sendToAgent: async (action) => {
            calls.push(action);
            if (action === 'GetConfig') return { Config: { is_locked: locked, needs_login: false } };
            if (action && action.GetSidebarEntries) return { SidebarEntries: { entries: [] } };
            throw new Error('unexpected action: ' + JSON.stringify(action));
        },
    };
}

// Load the script through new Function in a fresh scope (same pattern as
// background-save.test.js) and return the functions under test.
function loadBackground(fakeBrowser) {
    const fn = new Function(
        'browser',
        `${source}\nreturn { updateBadge, setThemeIcon, extractDomain };`
    );
    return fn(fakeBrowser);
}

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
