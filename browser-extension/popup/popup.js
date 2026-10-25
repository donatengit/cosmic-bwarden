if (typeof globalThis.browser === 'undefined') { globalThis.browser = globalThis.chrome; }

// ── DOM refs ──────────────────────────────────────────────────────────────────
const searchInput = document.getElementById('search');
const syncBtn     = document.getElementById('sync-btn');
const favBtn      = document.getElementById('fav-btn');
const addBtn      = document.getElementById('add-btn');
const backBtn     = document.getElementById('back-btn');
const lockBtn     = document.getElementById('lock-btn');
const genBtn      = document.getElementById('gen-btn');
const statusDiv   = document.getElementById('status');
const viewList    = document.getElementById('view-list');
const viewDetail  = document.getElementById('view-detail');
const viewEdit    = document.getElementById('view-edit');
const viewLocked  = document.getElementById('view-locked');
const viewHistory = document.getElementById('view-history');

// ── State ─────────────────────────────────────────────────────────────────────
let currentEntry = null;
let currentView  = 'list';
let currentTabDomain = null;
let favouritesOnly = false;
// True when the currently-rendered list came from a domain match on the
// current tab (as opposed to a search or the favourites fallback). A Login row
// click on a domain match fills the form instead of opening the site.
let listIsDomainMatch = false;
// Cached GetConfig answer while the vault is known-usable (see updateResults).
let cachedConfig = null;

// ── Domain helpers ────────────────────────────────────────────────────────────
function extractDomain(url) {
    try {
        // Full host, only a leading "www." stripped. Never collapse labels
        // here: deriving a registrable domain is the agent's job, where the
        // Public Suffix List lives (see docs/public_suffix_list.md). The host
        // goes in GetSidebarEntries' `domain` field and the agent matches it
        // against each entry's stored URI hosts (exact / boundary-subdomain /
        // eTLD+1), so "account.facebook.com" surfaces a "facebook.com" entry
        // while "victim.co.uk" can never surface other ".co.uk" entries.
        return new URL(url).hostname.toLowerCase().replace(/^www\./, '');
    } catch { return null; }
}

// ── Status ────────────────────────────────────────────────────────────────────
// `kind` is 'info' (plain muted text) or 'error' (soft danger chip) — the
// same element carries both, so the caller names which it is.
function showStatus(msg, kind = 'info') {
    statusDiv.textContent = msg;
    statusDiv.classList.toggle('status-error', kind === 'error');
    statusDiv.classList.remove('hidden');
}
function hideStatus() { statusDiv.classList.add('hidden'); }

window.onerror = (msg, url, line) => showStatus(`JS Error: ${msg} at ${line}`, 'error');

// GetConfig is stable while the vault stays unlocked — cache the usable
// answer for the popup's lifetime (one roundtrip per search keystroke
// instead of two). A locked/needs-login answer is never cached, and the
// Lock handler clears the cache, so the lock gate can't go stale.
async function getConfig() {
    if (cachedConfig && !cachedConfig.Config?.is_locked && !cachedConfig.Config?.needs_login) {
        return cachedConfig;
    }
    cachedConfig = await browser.runtime.sendMessage("GetConfig");
    return cachedConfig;
}

// ── View management ───────────────────────────────────────────────────────────
function showView(view) {
    currentView = view;
    [viewList, viewDetail, viewEdit, viewLocked, viewHistory].forEach(v => v.classList.add('hidden'));
    [backBtn, searchInput, favBtn, addBtn, syncBtn, lockBtn, genBtn].forEach(b => b.classList.add('hidden'));
    // Leaving the history view drops its rows, so decrypted passwords never
    // outlive the view (popup-generator.js).
    if (view !== 'history') clearHistoryView();

    if (view === 'list') {
        viewList.classList.remove('hidden');
        [searchInput, favBtn, addBtn, genBtn, syncBtn, lockBtn].forEach(b => b.classList.remove('hidden'));
        updateResults();
    } else if (view === 'detail') {
        viewDetail.classList.remove('hidden');
        [backBtn, lockBtn].forEach(b => b.classList.remove('hidden'));
    } else if (view === 'edit') {
        viewEdit.classList.remove('hidden');
        backBtn.classList.remove('hidden');
    } else if (view === 'history') {
        viewHistory.classList.remove('hidden');
        [backBtn, genBtn].forEach(b => b.classList.remove('hidden'));
    } else if (view === 'locked') {
        // Generation and its history need no unlock (the agent generates
        // locally), so the menu stays reachable from the lock screen.
        viewLocked.classList.remove('hidden');
        genBtn.classList.remove('hidden');
    }
    savePopupState();
}

// ── Vault list ────────────────────────────────────────────────────────────────
async function getEntries(params) {
    const response = await browser.runtime.sendMessage({
        "GetSidebarEntries": {
            "query": null, "entry_type": null, "only_pinned": false, "domain": null,
            ...params
        }
    });
    if (response.SidebarEntries) return response.SidebarEntries.entries;
    throw new Error(response.Error ? response.Error.message : "Unexpected agent response.");
}

async function updateResults() {
    hideStatus();
    // A typed search is a substring query; otherwise the current tab's host
    // goes in `domain` and the agent domain-matches it against entry URIs.
    // The favourites toggle restricts everything to favourites; with no search
    // and no domain match, favourites are shown anyway for quick access (same
    // idea as the applet's empty-query behaviour).
    const query = searchInput.value || null;
    try {
        const configResp = await getConfig();
        if (configResp.Config) {
            // needs_login wins over is_locked: a logged-out vault is not
            // "locked" — there is no session to unlock.
            if (configResp.Config.needs_login) {
                showStatus("Not logged in.");
                return;
            }
            if (configResp.Config.is_locked) {
                await browser.runtime.sendMessage("RequestUnlock");
                await showLockedView();
                return;
            }
        }

        let entries;
        let caption = null;
        listIsDomainMatch = false;
        if (favouritesOnly) {
            entries = await getEntries({ query, only_pinned: true });
            caption = 'Favourites';
        } else if (query) {
            entries = await getEntries({ query });
        } else if (currentTabDomain) {
            entries = await getEntries({ domain: currentTabDomain });
            if (entries.length > 0) {
                // The agent matched the current tab's host; a row click should
                // autofill rather than open the site elsewhere.
                listIsDomainMatch = true;
            } else {
                entries = await getEntries({ only_pinned: true });
                caption = 'Favourites';
            }
        } else {
            entries = await getEntries({ only_pinned: true });
            caption = 'Favourites';
        }
        renderEntries(entries, caption);
    } catch (e) { showStatus(e.message || "Failed to communicate with agent.", 'error'); }
}

// ── Utilities ─────────────────────────────────────────────────────────────────
function getEntryType(entry) {
    if (!entry || !entry.data) return 'Login';
    const d = entry.data;
    if (typeof d === 'string') return d;
    if (d.Login) return 'Login';
    if (d.Card) return 'Card';
    if (d.Identity) return 'Identity';
    if (d.SshKey) return 'SshKey';
    return 'Login';
}

// ── Event wiring ──────────────────────────────────────────────────────────────
// Search is debounced: each keystroke otherwise fires two agent roundtrips
// (GetConfig + GetSidebarEntries) against a list that is rebuilt anyway.
let searchTimer = null;
searchInput.addEventListener('input', () => {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(updateResults, 150);
});
searchInput.addEventListener('input', savePopupState);
favBtn.onclick = () => {
    favouritesOnly = !favouritesOnly;
    // The star glyph's fill is CSS-driven (#fav-btn.active svg) — only the
    // state class and the a11y attribute change here.
    favBtn.classList.toggle('active', favouritesOnly);
    favBtn.setAttribute('aria-pressed', String(favouritesOnly));
    updateResults();
    savePopupState();
};
syncBtn.onclick = async () => { await browser.runtime.sendMessage("Sync"); updateResults(); };
lockBtn.onclick = async () => {
    // The cached config says "usable"; dropping it lets the next updateResults
    // re-check and route to the locked view instead of showing a stale list.
    cachedConfig = null;
    await browser.runtime.sendMessage("Lock");
    showView('list');
};
addBtn.onclick = () => showAddForm();
genBtn.onclick = e => {
    // Keep the document click listener in popup-list-actions.js (which closes
    // open menus) from closing the menu this same click just opened.
    e.stopPropagation();
    toggleGeneratorMenu(genBtn);
};
backBtn.onclick = () => showView(currentView === 'edit' && currentEntry ? 'detail' : 'list');

// ── Init ──────────────────────────────────────────────────────────────────────
(async () => {
    // Chrome MV3: service worker can't use matchMedia, so report theme from popup.
    try {
        const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
        browser.runtime.sendMessage({ SetTheme: { dark: isDark } });
    } catch { /* ignore */ }

    try {
        const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
        if (tab && tab.url) currentTabDomain = extractDomain(tab.url);
    } catch { /* no tab access */ }
    // Restore persisted state on the next task: showDetail/showEdit/showAddForm
    // live in popup-detail.js / popup-edit.js, which load after popup.js, so the
    // restore must not run until those scripts have been evaluated.
    setTimeout(() => restorePopupState(), 0);
})();
