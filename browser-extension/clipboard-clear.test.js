import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const source = readFileSync(
    path.join(path.dirname(fileURLToPath(import.meta.url)), 'clipboard-clear.js'),
    'utf8'
);
const { scheduleClipboardClear, CLIPBOARD_CLEAR_MS } = new Function(
    `${source}\nreturn { scheduleClipboardClear, CLIPBOARD_CLEAR_MS };`
)();

describe('scheduleClipboardClear', () => {
    beforeEach(() => { vi.useFakeTimers(); });
    afterEach(() => { vi.useRealTimers(); });

    it('clears the clipboard after 30s if it still holds our secret', async () => {
        let clip = 'hunter2';
        const hooks = {
            setTimeout: (fn, ms) => setTimeout(fn, ms),
            readText: async () => clip,
            writeText: async (t) => { clip = t; },
        };
        scheduleClipboardClear('hunter2', hooks);
        await vi.advanceTimersByTimeAsync(CLIPBOARD_CLEAR_MS);
        expect(clip).toBe('');
    });

    it('does not overwrite a clipboard the user replaced', async () => {
        let clip = 'hunter2';
        const hooks = {
            setTimeout: (fn, ms) => setTimeout(fn, ms),
            readText: async () => clip,
            writeText: async (t) => { clip = t; },
        };
        scheduleClipboardClear('hunter2', hooks);
        clip = 'something-else';
        await vi.advanceTimersByTimeAsync(CLIPBOARD_CLEAR_MS);
        expect(clip).toBe('something-else');
    });
});
