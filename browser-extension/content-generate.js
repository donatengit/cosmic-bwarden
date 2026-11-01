// Clipboard relay for the background script's context-menu "Generate
// password" action. A Chrome MV3 service worker has no clipboard or DOM
// access, so the background asks the target frame's content script to do the
// write.
//
// This script deliberately does not scan the page. Password fields are
// located only when the popup sends FILL_FORM (content.js), so no input
// element is inspected or written before an explicit Fill.

if (typeof globalThis.browser === 'undefined') { globalThis.browser = globalThis.chrome; }

browser.runtime.onMessage.addListener((message) => {
    if (message && message.type === 'GENERATE_COPY_TO_CLIPBOARD') {
        navigator.clipboard.writeText(message.password).then(() => {
            if (typeof scheduleClipboardClear === 'function') {
                scheduleClipboardClear(message.password);
            }
        }).catch(() => {});
        return Promise.resolve({ Ack: true });
    }
});
