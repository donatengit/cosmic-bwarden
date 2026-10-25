// @vitest-environment jsdom
// Unit tests for popup-generator.js: the header menu, the generate-and-copy
// action, and the 7-day history view. Both agent calls are asserted by the
// exact message the popup sends (not by a hand-fed response shape), and every
// failure path the agent or the clipboard can take is exercised.
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const here = path.dirname(fileURLToPath(import.meta.url));
const menuSrc = readFileSync(path.join(here, 'popup-menu.js'), 'utf8');
const listActionsSrc = readFileSync(path.join(here, 'popup-list-actions.js'), 'utf8');
const generatorSrc = readFileSync(path.join(here, 'popup-generator.js'), 'utf8');

// popup-menu.js owns `openDropdown` and the menu helpers; concatenating the
// sources keeps that lexical binding shared exactly as the <script> tags share
// it in the real popup. popup.js globals are stubbed.
function loadHarness() {
    const source = `
        let currentView = 'list';
        const statuses = [];
        const clipboardClearCalls = [];
        function showView(view) { currentView = view; }
        function showStatus(msg, kind) { statuses.push({ msg, kind: kind || 'info' }); }
        function hideStatus() { statuses.push({ msg: null, kind: 'hidden' }); }
        function setIcon(el) { return el; }
        function scheduleClipboardClear(text) { clipboardClearCalls.push(text); }
        ${menuSrc}
        ${listActionsSrc}
        ${generatorSrc}
        return {
            toggleGeneratorMenu,
            generatePasswordToClipboard,
            showHistory,
            renderHistory,
            historyRow,
            formatAge,
            clearHistoryView,
            statuses,
            clipboardClearCalls,
            showView,
            getView: () => currentView,
            getOpenMenu: () => openDropdown,
        };
    `;
    return new Function(source)();
}

const flush = () => new Promise(resolve => setTimeout(resolve, 0));

function installClipboard(impl) {
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: impl });
}

function clipboardRecorder() {
    const writes = [];
    installClipboard({
        writeText: async (text) => { writes.push(text); },
        readText: async () => writes[writes.length - 1] ?? '',
    });
    return writes;
}

let harness;

beforeEach(() => {
    document.body.innerHTML = `
        <div id="results"></div>
        <div id="history-content"></div>
        <button id="gen-btn" aria-expanded="false"></button>
    `;
    globalThis.browser = { runtime: { sendMessage: async () => ({}) } };
    harness = loadHarness();
    delete navigator.clipboard;
});

afterEach(() => {
    delete navigator.clipboard;
});

const lastStatus = () => harness.statuses[harness.statuses.length - 1];

describe('formatAge', () => {
    it('reports unknown for a missing or unusable timestamp', () => {
        expect(harness.formatAge(undefined, 1000)).toBe('unknown');
        expect(harness.formatAge(null, 1000)).toBe('unknown');
        expect(harness.formatAge(0, 1000)).toBe('unknown');
        expect(harness.formatAge(-5, 1000)).toBe('unknown');
        expect(harness.formatAge('1700000000', 1000)).toBe('unknown');
        expect(harness.formatAge(NaN, 1000)).toBe('unknown');
        expect(harness.formatAge(Infinity, 1000)).toBe('unknown');
    });

    it('reads a future timestamp (clock skew) as just now', () => {
        expect(harness.formatAge(1_700_000_120, 1_700_000_000)).toBe('just now');
    });

    it('scales through seconds, minutes, hours and days', () => {
        const now = 1_700_000_000;
        expect(harness.formatAge(now, now)).toBe('just now');
        expect(harness.formatAge(now - 59, now)).toBe('just now');
        expect(harness.formatAge(now - 60, now)).toBe('1m ago');
        expect(harness.formatAge(now - 3599, now)).toBe('59m ago');
        expect(harness.formatAge(now - 3600, now)).toBe('1h ago');
        expect(harness.formatAge(now - 86399, now)).toBe('23h ago');
        expect(harness.formatAge(now - 86400, now)).toBe('1d ago');
        expect(harness.formatAge(now - 6 * 86400, now)).toBe('6d ago');
    });
});

describe('generator menu', () => {
    it('offers exactly Generate password and Show previous', () => {
        harness.toggleGeneratorMenu(document.getElementById('gen-btn'));
        const labels = [...document.querySelectorAll('.gen-menu button')].map(b => b.textContent);
        expect(labels).toEqual(['Generate password', 'Show previous']);
        expect(document.getElementById('gen-btn').getAttribute('aria-expanded')).toBe('true');
    });

    it('opens with the ARIA menu semantics and focuses the first item', () => {
        harness.toggleGeneratorMenu(document.getElementById('gen-btn'));

        const menu = document.querySelector('.gen-menu');
        expect(menu.getAttribute('role')).toBe('menu');
        expect(menu.getAttribute('aria-label')).toBe('Password generator');
        expect(document.activeElement).toBe(menu.querySelector('[role="menuitem"]'));
    });

    it('closes on a second click on the anchor and clears aria-expanded', () => {
        const btn = document.getElementById('gen-btn');
        harness.toggleGeneratorMenu(btn);
        harness.toggleGeneratorMenu(btn);
        expect(document.querySelector('.gen-menu')).toBeNull();
        expect(harness.getOpenMenu()).toBeNull();
        expect(btn.getAttribute('aria-expanded')).toBe('false');
    });

    it('closes on an outside click and on Escape', () => {
        const btn = document.getElementById('gen-btn');
        harness.toggleGeneratorMenu(btn);
        document.body.click();
        expect(document.querySelector('.gen-menu')).toBeNull();
        expect(btn.getAttribute('aria-expanded')).toBe('false');

        harness.toggleGeneratorMenu(btn);
        document.dispatchEvent(new window.KeyboardEvent('keydown', { key: 'Escape' }));
        expect(document.querySelector('.gen-menu')).toBeNull();
    });

    it('Generate password sends the agent the reuse-last-settings request', async () => {
        const sent = [];
        globalThis.browser = { runtime: { sendMessage: async (m) => { sent.push(m); return { GeneratedPassword: { password: 'gen-pw' } }; } } };
        const writes = clipboardRecorder();

        harness.toggleGeneratorMenu(document.getElementById('gen-btn'));
        document.querySelector('.gen-menu button').click();
        await flush();

        expect(sent).toEqual([{ GeneratePassword: { settings: null } }]);
        expect(writes).toEqual(['gen-pw']);
    });

    it('Show previous requests the history and opens the history view', async () => {
        const sent = [];
        globalThis.browser = { runtime: { sendMessage: async (m) => { sent.push(m); return { PasswordHistory: { entries: [] } }; } } };

        harness.toggleGeneratorMenu(document.getElementById('gen-btn'));
        document.querySelectorAll('.gen-menu button')[1].click();
        await flush();

        expect(sent).toEqual(['GetPasswordHistory']);
        expect(harness.getView()).toBe('history');
    });
});

describe('generate password', () => {
    it('copies the generated password once and schedules the clipboard clear', async () => {
        globalThis.browser = { runtime: { sendMessage: async () => ({ GeneratedPassword: { password: 'A-b3!x' } }) } };
        const writes = clipboardRecorder();

        await harness.generatePasswordToClipboard();

        expect(writes).toEqual(['A-b3!x']);
        expect(harness.clipboardClearCalls).toEqual(['A-b3!x']);
        expect(lastStatus()).toEqual({ msg: 'Password generated and copied to clipboard.', kind: 'info' });
    });

    it('surfaces the agent error and does not touch the clipboard', async () => {
        globalThis.browser = { runtime: { sendMessage: async () => ({ Error: { message: 'length must be between 8 and 32' } }) } };
        const writes = clipboardRecorder();

        await harness.generatePasswordToClipboard();

        expect(writes).toEqual([]);
        expect(lastStatus()).toEqual({ msg: 'length must be between 8 and 32', kind: 'error' });
    });

    it('rejects responses without a GeneratedPassword variant', async () => {
        const writes = clipboardRecorder();
        for (const resp of [undefined, null, {}, { Ack: true }, { GeneratedPassword: null }]) {
            globalThis.browser = { runtime: { sendMessage: async () => resp } };
            harness.statuses.length = 0;
            await harness.generatePasswordToClipboard();
            expect(lastStatus()).toEqual({ msg: 'Unexpected agent response.', kind: 'error' });
        }
        expect(writes).toEqual([]);
    });

    it('rejects an empty or non-string password instead of copying it', async () => {
        const writes = clipboardRecorder();
        for (const password of ['', null, 42, { v: 'x' }]) {
            globalThis.browser = { runtime: { sendMessage: async () => ({ GeneratedPassword: { password } }) } };
            harness.statuses.length = 0;
            await harness.generatePasswordToClipboard();
            expect(lastStatus()).toEqual({ msg: 'Unexpected agent response.', kind: 'error' });
        }
        expect(writes).toEqual([]);
    });

    it('reports an unreachable agent (rejected sendMessage) as an error', async () => {
        globalThis.browser = { runtime: { sendMessage: async () => { throw new Error('Agent disconnected'); } } };
        const writes = clipboardRecorder();

        await harness.generatePasswordToClipboard();

        expect(lastStatus()).toEqual({ msg: 'Agent disconnected', kind: 'error' });
        expect(writes).toEqual([]);
    });

    it('reports a rejected sendMessage without a message rather than throwing', async () => {
        globalThis.browser = { runtime: { sendMessage: async () => { throw 'boom'; } } };
        clipboardRecorder();

        await harness.generatePasswordToClipboard();

        expect(lastStatus()).toEqual({ msg: 'Failed to generate a password.', kind: 'error' });
    });

    it('says the password was generated when only the clipboard write fails', async () => {
        globalThis.browser = { runtime: { sendMessage: async () => ({ GeneratedPassword: { password: 'gen-pw' } }) } };
        installClipboard({ writeText: async () => { throw new Error('denied'); } });

        await harness.generatePasswordToClipboard();

        expect(lastStatus()).toEqual({
            msg: 'Password generated, but copying to the clipboard failed.',
            kind: 'error',
        });
        expect(harness.clipboardClearCalls).toEqual([]);
    });

    it('reports the same clipboard failure when the API is missing', async () => {
        globalThis.browser = { runtime: { sendMessage: async () => ({ GeneratedPassword: { password: 'gen-pw' } }) } };
        // navigator.clipboard is deleted in beforeEach: no write API at all.
        await harness.generatePasswordToClipboard();

        expect(lastStatus()).toEqual({
            msg: 'Password generated, but copying to the clipboard failed.',
            kind: 'error',
        });
    });

    it('never prints the password in the status line', async () => {
        globalThis.browser = { runtime: { sendMessage: async () => ({ GeneratedPassword: { password: 'top-secret-gen' } }) } };
        clipboardRecorder();

        await harness.generatePasswordToClipboard();

        expect(JSON.stringify(harness.statuses)).not.toContain('top-secret-gen');
        expect(document.body.textContent).not.toContain('top-secret-gen');
    });
});

describe('history view', () => {
    const entries = [
        { password: 'newest-pw', created_at: 1700000200 },
        { password: 'older-pw', created_at: 1700000100 },
    ];

    it('renders the agent order, masked, with an age per row', async () => {
        globalThis.browser = { runtime: { sendMessage: async () => ({ PasswordHistory: { entries } }) } };

        await harness.showHistory();

        const rows = [...document.querySelectorAll('#history-content .history-entry')];
        expect(rows).toHaveLength(2);
        expect(rows.map(r => r.querySelector('.history-password').textContent)).toEqual(['••••••••', '••••••••']);
        expect(rows[0].querySelector('.history-age').textContent).toMatch(/(just now|m ago|h ago|d ago|unknown)/);
        expect(document.getElementById('history-content').textContent).not.toContain('newest-pw');
        expect(document.getElementById('history-content').textContent).not.toContain('older-pw');
        expect(lastStatus()).toEqual({ msg: null, kind: 'hidden' });
    });

    it('shows the empty state instead of a blank view', async () => {
        globalThis.browser = { runtime: { sendMessage: async () => ({ PasswordHistory: { entries: [] } }) } };

        await harness.showHistory();

        expect(document.getElementById('history-content').textContent)
            .toContain('No passwords generated in the last 7 days.');
        expect(document.querySelectorAll('.history-entry')).toHaveLength(0);
    });

    it('surfaces the agent error and leaves the view empty', async () => {
        globalThis.browser = { runtime: { sendMessage: async () => ({ Error: { message: 'failed to load password history' } }) } };

        await harness.showHistory();

        expect(lastStatus()).toEqual({ msg: 'failed to load password history', kind: 'error' });
        expect(document.querySelectorAll('.history-entry')).toHaveLength(0);
    });

    it('rejects malformed history responses', async () => {
        for (const resp of [undefined, null, {}, { PasswordHistory: null }, { PasswordHistory: { entries: null } }, { PasswordHistory: { entries: 'nope' } }]) {
            globalThis.browser = { runtime: { sendMessage: async () => resp } };
            harness.statuses.length = 0;
            await harness.showHistory();
            expect(lastStatus()).toEqual({ msg: 'Unexpected agent response.', kind: 'error' });
            expect(document.querySelectorAll('.history-entry')).toHaveLength(0);
        }
    });

    it('reports an unreachable agent', async () => {
        globalThis.browser = { runtime: { sendMessage: async () => { throw new Error('Agent disconnected'); } } };

        await harness.showHistory();

        expect(lastStatus()).toEqual({ msg: 'Agent disconnected', kind: 'error' });
    });

    it('drops rows from a previous open when the next fetch fails', async () => {
        globalThis.browser = { runtime: { sendMessage: async () => ({ PasswordHistory: { entries } }) } };
        await harness.showHistory();
        expect(document.querySelectorAll('.history-entry')).toHaveLength(2);

        globalThis.browser = { runtime: { sendMessage: async () => ({ Error: { message: 'nope' } }) } };
        await harness.showHistory();

        expect(document.querySelectorAll('.history-entry')).toHaveLength(0);
        expect(document.getElementById('history-content').textContent).not.toContain('newest-pw');
    });

    it('does not render when the user left the view before the answer arrived', async () => {
        let release;
        const answer = new Promise(resolve => { release = resolve; });
        globalThis.browser = { runtime: { sendMessage: () => answer } };

        const pending = harness.showHistory();
        harness.showView('list');
        harness.clearHistoryView();
        harness.statuses.length = 0;
        release({ PasswordHistory: { entries } });
        await pending;

        expect(document.querySelectorAll('.history-entry')).toHaveLength(0);
        expect(document.getElementById('history-content').textContent).not.toContain('newest-pw');
    });

    it('clearHistoryView drops every password from the document', async () => {
        globalThis.browser = { runtime: { sendMessage: async () => ({ PasswordHistory: { entries } }) } };
        await harness.showHistory();
        document.querySelector('.history-password').textContent = 'newest-pw';

        harness.clearHistoryView();

        expect(document.getElementById('history-content').textContent).not.toContain('newest-pw');
        expect(document.querySelectorAll('.history-entry')).toHaveLength(0);
    });

    it('renders a masked row for an entry missing its password or timestamp', () => {
        harness.renderHistory([{ created_at: 1700000200 }, { password: 'x' }], 1700000300);

        const rows = [...document.querySelectorAll('.history-entry')];
        expect(rows).toHaveLength(2);
        expect(rows[0].querySelector('.history-password').textContent).toBe('••••••••');
        expect(rows[1].querySelector('.history-age').textContent).toBe('unknown');
    });
});

describe('history row actions', () => {
    function rowFor(entry) {
        return harness.historyRow(entry, 1700000300);
    }

    it('reveals on click and re-masks on the next click', () => {
        const row = rowFor({ password: 'row-pw', created_at: 1700000200 });
        document.body.appendChild(row);
        const value = row.querySelector('.history-password');
        const reveal = row.querySelector('.history-reveal-btn');

        reveal.click();
        expect(value.textContent).toBe('row-pw');
        expect(value.classList.contains('revealed')).toBe(true);
        expect(reveal.getAttribute('aria-pressed')).toBe('true');

        reveal.click();
        expect(value.textContent).toBe('••••••••');
        expect(value.classList.contains('revealed')).toBe(false);
        expect(reveal.getAttribute('aria-pressed')).toBe('false');
    });

    it('copies the row password with the shared auto-clear', async () => {
        const writes = clipboardRecorder();
        const row = rowFor({ password: 'row-pw', created_at: 1700000200 });
        document.body.appendChild(row);

        row.querySelector('.history-copy-btn').click();
        await flush();

        expect(writes).toEqual(['row-pw']);
        expect(harness.clipboardClearCalls).toEqual(['row-pw']);
        expect(lastStatus()).toEqual({ msg: 'Password copied to clipboard.', kind: 'info' });
    });

    it('reports a clipboard failure instead of silently doing nothing', async () => {
        installClipboard({ writeText: async () => { throw new Error('denied'); } });
        const row = rowFor({ password: 'row-pw', created_at: 1700000200 });
        document.body.appendChild(row);

        row.querySelector('.history-copy-btn').click();
        await flush();

        expect(lastStatus()).toEqual({
            msg: 'Failed to copy the password to the clipboard.',
            kind: 'error',
        });
    });

    it('reports a copy of an empty entry as a failure', async () => {
        clipboardRecorder();
        const row = rowFor({ created_at: 1700000200 });
        document.body.appendChild(row);

        row.querySelector('.history-copy-btn').click();
        await flush();

        expect(lastStatus()).toEqual({
            msg: 'Failed to copy the password to the clipboard.',
            kind: 'error',
        });
    });

    it('reveals malformed entries as an empty string rather than throwing', () => {
        const row = rowFor({ created_at: 1700000200 });
        document.body.appendChild(row);

        row.querySelector('.history-reveal-btn').click();

        expect(row.querySelector('.history-password').textContent).toBe('');
    });
});
