// Fill: write a Login entry into the active tab's form.
// Split out of popup.js once that file passed the project's target size; the
// functions call popup.js globals (browser, showStatus) from their bodies only.

// ── Fill ──────────────────────────────────────────────────────────────────────
function hostsMatchFill(tabHost, uriHost) {
    if (!tabHost || !uriHost) return false;
    const a = tabHost.toLowerCase();
    const b = uriHost.toLowerCase();
    if (a === b) return true;
    return a.endsWith('.' + b) || b.endsWith('.' + a);
}

function hostFromVaultUri(uri) {
    if (!uri) return null;
    try {
        return new URL(/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(uri) ? uri : `https://${uri}`).hostname;
    } catch { return null; }
}

function entryMatchesTab(login, tabUrl) {
    let tabHost;
    try { tabHost = new URL(tabUrl).hostname; } catch { return false; }
    const uris = (login && login.uris) || [];
    for (const u of uris) {
        const h = hostFromVaultUri(u && u.uri);
        if (h && hostsMatchFill(tabHost, h)) return true;
    }
    return false;
}

async function fillEntry(id) {
    try {
        const response = await browser.runtime.sendMessage({ "GetEntry": { "id": id, "password": null } });
        if (response.Entry) {
            const login = response.Entry.entry.data.Login;
            if (!login) { showStatus("Not a login entry."); return; }
            const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
            if (!tab) return;
            if (!entryMatchesTab(login, tab.url)) {
                showStatus("This login does not match the current site.");
                return;
            }
            let expectedHost = '';
            try { expectedHost = new URL(tab.url).hostname; } catch { /* leave empty */ }
            browser.tabs.sendMessage(tab.id, {
                type: "FILL_FORM",
                username: login.username || '',
                password: login.password || '',
                expectedHost,
            });
            window.close();
        }
    } catch { showStatus("Failed to fill form.", 'error'); }
}
