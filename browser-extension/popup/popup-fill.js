// Fill: write a Login entry into the active tab's form.
// Split out of popup.js once that file passed the project's target size; the
// functions call popup.js globals (browser, showStatus) from their bodies only.
// hostMatches comes from host-match.js, loaded before this file.

// ── Fill ──────────────────────────────────────────────────────────────────────
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
        if (h && hostMatches(tabHost, h)) return true;
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
            // The entry's own hosts, not the tab's: the content script runs in
            // every frame of the tab (manifest all_frames), and each frame
            // decides for itself whether the password may be written there.
            const entryHosts = ((login.uris) || [])
                .map(u => hostFromVaultUri(u && u.uri))
                .filter(Boolean);
            try {
                await browser.tabs.sendMessage(tab.id, {
                    type: "FILL_FORM",
                    username: login.username || '',
                    password: login.password || '',
                    entryHosts,
                });
            } catch {
                // No frame answered: the page was loaded before the extension
                // was installed or reloaded, so no content script is listening.
                // Report it instead of closing the popup as if the fill worked.
                showStatus("Failed to fill form.", 'error');
                return;
            }
            window.close();
        }
    } catch { showStatus("Failed to fill form.", 'error'); }
}
