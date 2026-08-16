// In-page "Save/Update password?" notification bar. The bar never receives or
// holds the submitted password — the background script keeps it; messages to
// this script carry only display labels (domain, username, entry name).

const BAR_AUTO_DISMISS_MS = 30 * 1000;
const BAR_ERROR_LINGER_MS = 5 * 1000;

let _barHost = null;
let _barTimer = null;

browser.runtime.onMessage.addListener((message) => {
    if (!message || !message.type) return;
    if (message.type === 'SHOW_SAVE_BAR') showSaveBar(message);
    else if (message.type === 'HIDE_SAVE_BAR') removeSaveBar();
    else if (message.type === 'SAVE_BAR_ERROR') showSaveBarError();
});

const BAR_CSS = `
.bar {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    z-index: 2147483647;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 16px;
    padding: 10px 16px;
    font-family: system-ui, -apple-system, sans-serif;
    font-size: 14px;
    line-height: 1.3;
    background: #f5f5f5;
    color: #1a1a1a;
    border-bottom: 1px solid #c9c9c9;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
}
.text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
button {
    font: inherit;
    padding: 5px 14px;
    border-radius: 6px;
    border: 1px solid transparent;
    cursor: pointer;
}
button:disabled {
    opacity: 0.6;
    cursor: default;
}
.primary {
    background: #175DDC;
    color: #fff;
}
.secondary {
    background: transparent;
    color: inherit;
    border-color: #9a9a9a;
}
@media (prefers-color-scheme: dark) {
    .bar {
        background: #2b2b2b;
        color: #ececec;
        border-bottom-color: #4a4a4a;
    }
    .secondary {
        border-color: #6e6e6e;
    }
}
`;

function sendBarAction(action) {
    try {
        browser.runtime.sendMessage({ type: 'SAVE_BAR_ACTION', action });
    } catch { /* extension reloaded; context gone */ }
}

function removeSaveBar() {
    if (_barTimer) { clearTimeout(_barTimer); _barTimer = null; }
    if (_barHost) { _barHost.remove(); _barHost = null; }
}

function showSaveBar({ mode, domain, entryName }) {
    removeSaveBar(); // one bar at a time

    const isLocked = mode === 'locked';
    const isUpdate = mode === 'update';

    _barHost = document.createElement('div');
    _barHost.id = 'cosmic-bwarden-save-bar';
    // Open shadow root: the bar carries no secrets and open roots are
    // pierceable by test locators; closed would not stop page removal anyway.
    const shadow = _barHost.attachShadow({ mode: 'open' });

    const style = document.createElement('style');
    style.textContent = BAR_CSS;

    const bar = document.createElement('div');
    bar.className = 'bar';

    const text = document.createElement('span');
    text.className = 'text';
    // Page-derived strings: always textContent, never HTML.
    text.textContent = isLocked
        ? `Unlock COSMIC BWarden to save this password for ${domain}`
        : (isUpdate
            ? `Update password for "${entryName}" in COSMIC BWarden?`
            : `Save password for ${domain} in COSMIC BWarden?`);

    const primary = document.createElement('button');
    primary.className = 'primary';
    primary.textContent = isLocked ? 'Unlock' : (isUpdate ? 'Update' : 'Save');

    const dismiss = document.createElement('button');
    dismiss.className = 'secondary';
    dismiss.textContent = 'Dismiss';

    primary.addEventListener('click', () => {
        if (isLocked) {
            // Unlocking happens in the popup; the background keeps the pending
            // credential and re-offers Save/Update once the vault is unlocked.
            // The buttons stay live on purpose: opening the popup needs a user
            // gesture the background doesn't always have, and a bar that can be
            // neither retried nor dismissed is worse than one that did nothing.
            sendBarAction('unlock');
            return;
        }
        primary.disabled = true;
        dismiss.disabled = true;
        text.textContent = isUpdate ? 'Updating…' : 'Saving…';
        // Background answers with HIDE_SAVE_BAR or SAVE_BAR_ERROR.
        sendBarAction(isUpdate ? 'update' : 'save');
    });
    dismiss.addEventListener('click', () => {
        sendBarAction('dismiss');
        removeSaveBar();
    });

    bar.append(text, primary, dismiss);
    shadow.append(style, bar);
    // documentElement, not body: present even on exotic/broken pages.
    document.documentElement.appendChild(_barHost);

    _barTimer = setTimeout(() => {
        // The locked bar times out *silently*: 'dismiss' makes the background
        // drop the pending credential, and unlocking (open popup, type PIN)
        // routinely outlives this window — so a deferred save would be lost
        // exactly the way the deferral exists to prevent. The credential stays
        // pending until its own TTL, and VAULT_UNLOCKED re-offers it as a
        // Save/Update bar. An explicit Dismiss click still clears it.
        if (!isLocked) sendBarAction('dismiss');
        removeSaveBar();
    }, BAR_AUTO_DISMISS_MS);
}

function showSaveBarError() {
    if (!_barHost) return;
    const text = _barHost.shadowRoot.querySelector('.text');
    if (text) text.textContent = 'Saving failed — see the COSMIC BWarden agent logs.';
    if (_barTimer) clearTimeout(_barTimer);
    _barTimer = setTimeout(removeSaveBar, BAR_ERROR_LINGER_MS);
}
