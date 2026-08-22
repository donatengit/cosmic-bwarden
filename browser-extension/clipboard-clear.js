// Shared 30 s clipboard auto-clear for extension copy paths.
// Injected into the popup and the content-script world. Fake `hooks` in tests.
const CLIPBOARD_CLEAR_MS = 30000;
var clipboardClearGeneration = 0;

function scheduleClipboardClear(text, hooks) {
    if (!text) return;
    const wait = (hooks && hooks.setTimeout) || setTimeout;
    const readText = (hooks && hooks.readText) || (() => navigator.clipboard.readText());
    const writeText = (hooks && hooks.writeText) || ((t) => navigator.clipboard.writeText(t));
    const id = ++clipboardClearGeneration;
    wait(async () => {
        if (id !== clipboardClearGeneration) return;
        try {
            const cur = await readText();
            if (cur === text) await writeText('');
        } catch (_) { /* clipboard permission / closed popup */ }
    }, CLIPBOARD_CLEAR_MS);
}
