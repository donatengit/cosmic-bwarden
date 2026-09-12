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
:host {
    all: initial;
}
.bar {
    position: fixed;
    top: 12px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 2147483647;
    display: flex;
    align-items: center;
    gap: 12px;
    max-width: min(560px, calc(100vw - 24px));
    padding: 10px 10px 10px 16px;
    font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
    font-size: 13px;
    line-height: 1.45;
    color: var(--text);
    background: color-mix(in oklab, var(--bg) 88%, transparent);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
}
.text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
button {
    font: inherit;
    font-size: 13px;
    min-height: 28px;
    padding: 4px 12px;
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
    cursor: pointer;
}
.primary {
    background: var(--accent);
    color: var(--on-accent);
}
.primary:hover {
    background: var(--accent-hover);
}
.secondary {
    background: transparent;
    color: var(--text-secondary);
    border-color: var(--border);
}
.secondary:hover {
    background: var(--surface);
}
button:disabled {
    opacity: 0.55;
    cursor: default;
}
button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
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
    _barHost.id = 'cosmarden-save-bar';
    // themeCss() (theme.js) supplies the palette as custom properties on the
    // host; the shadow <style> consumes them via var(--...) — the same token
    // names popup.css uses. display:block so a page rule like
    // `div { display:none }` cannot hide the bar's shadow tree.
    _barHost.style.cssText = 'display:block;' + themeCss();
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
        ? `Unlock Cosmarden to save this password for ${domain}`
        : (isUpdate
            ? `Update password for "${entryName}" in Cosmarden?`
            : `Save password for ${domain} in Cosmarden?`);

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
    if (text) text.textContent = 'Saving failed — see the Cosmarden agent logs.';
    if (_barTimer) clearTimeout(_barTimer);
    _barTimer = setTimeout(removeSaveBar, BAR_ERROR_LINGER_MS);
}
