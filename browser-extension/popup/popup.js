if (typeof globalThis.browser === 'undefined') { globalThis.browser = globalThis.chrome; }

// ── DOM refs ──────────────────────────────────────────────────────────────────
const searchInput = document.getElementById('search');
const resultsDiv  = document.getElementById('results');
const syncBtn     = document.getElementById('sync-btn');
const favBtn      = document.getElementById('fav-btn');
const addBtn      = document.getElementById('add-btn');
const backBtn     = document.getElementById('back-btn');
const lockBtn     = document.getElementById('lock-btn');
const statusDiv   = document.getElementById('status');
const viewList    = document.getElementById('view-list');
const viewDetail  = document.getElementById('view-detail');
const viewEdit    = document.getElementById('view-edit');
const viewLocked  = document.getElementById('view-locked');

// ── State ─────────────────────────────────────────────────────────────────────
let currentEntry = null;
let currentView  = 'list';
let currentTabDomain = null;
let favouritesOnly = false;
// True when the currently-rendered list came from a domain match on the
// current tab (as opposed to a search or the favourites fallback). A Login row
// click on a domain match fills the form instead of opening the site.
let listIsDomainMatch = false;

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
function showStatus(msg) {
    statusDiv.textContent = msg;
    statusDiv.classList.remove('hidden');
}
function hideStatus() { statusDiv.classList.add('hidden'); }

window.onerror = (msg, url, line) => showStatus(`JS Error: ${msg} at ${line}`);

// ── View management ───────────────────────────────────────────────────────────
function showView(view) {
    currentView = view;
    [viewList, viewDetail, viewEdit, viewLocked].forEach(v => v.classList.add('hidden'));
    [backBtn, searchInput, favBtn, addBtn, syncBtn, lockBtn].forEach(b => b.classList.add('hidden'));

    if (view === 'list') {
        viewList.classList.remove('hidden');
        [searchInput, favBtn, addBtn, syncBtn, lockBtn].forEach(b => b.classList.remove('hidden'));
        updateResults();
    } else if (view === 'detail') {
        viewDetail.classList.remove('hidden');
        [backBtn, lockBtn].forEach(b => b.classList.remove('hidden'));
    } else if (view === 'edit') {
        viewEdit.classList.remove('hidden');
        backBtn.classList.remove('hidden');
    } else if (view === 'locked') {
        viewLocked.classList.remove('hidden');
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
    // The ★ toggle restricts everything to favourites; with no search and no
    // domain match, favourites are shown anyway for quick access (same idea
    // as the applet's empty-query behaviour).
    const query = searchInput.value || null;
    try {
        const configResp = await browser.runtime.sendMessage("GetConfig");
        if (configResp.Config) {
            if (configResp.Config.is_locked) {
                await browser.runtime.sendMessage("RequestUnlock");
                await showLockedView();
                return;
            }
            if (configResp.Config.needs_login) {
                showStatus("Not logged in.");
                return;
            }
        }

        let entries;
        let caption = null;
        listIsDomainMatch = false;
        if (favouritesOnly) {
            entries = await getEntries({ query, only_pinned: true });
            caption = '★ Favourites';
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
                caption = '★ Favourites';
            }
        } else {
            entries = await getEntries({ only_pinned: true });
            caption = '★ Favourites';
        }
        renderEntries(entries, caption);
    } catch (e) { showStatus(e.message || "Failed to communicate with agent."); }
}

function renderEntries(entries, caption = null) {
    resultsDiv.innerHTML = '';
    if (entries.length === 0) {
        resultsDiv.innerHTML = '<div class="no-results">No entries found</div>';
        return;
    }
    if (caption) {
        const capEl = document.createElement('div');
        capEl.className = 'list-caption';
        capEl.textContent = caption;
        resultsDiv.appendChild(capEl);
    }

    for (const entry of entries) {
        const div = document.createElement('div');
        div.className = 'entry';
        // Login entries: clicking the row launches the site (its primary
        // purpose) — unless the list is the current tab's domain match, in
        // which case the click autofills the form. Other types have no URL to
        // open, so fall back to detail.
        div.onclick = () => {
            if (entry.entry_type === 'Login') {
                if (listIsDomainMatch) fillEntry(entry.id);
                else openEntrySite(entry.id);
            } else {
                showDetail(entry.id);
            }
        };

        const info = document.createElement('div');
        info.className = 'entry-info';
        const nameEl = document.createElement('div');
        nameEl.className = 'entry-name';
        nameEl.textContent = entry.name;
        const subEl = document.createElement('div');
        subEl.className = 'entry-user';
        subEl.textContent = entry.username || entry.entry_type;
        info.append(nameEl, subEl);

        const actions = document.createElement('div');
        actions.className = 'entry-actions';

        if (entry.entry_type === 'Login') {
            const fillBtn = document.createElement('button');
            fillBtn.textContent = 'Fill';
            fillBtn.title = 'Autofill';
            fillBtn.onclick = e => { e.stopPropagation(); fillEntry(entry.id); };
            actions.append(fillBtn, makeCopyDropdownBtn(entry));
        }

        const viewIconBtn = document.createElement('button');
        viewIconBtn.textContent = '👁';
        viewIconBtn.title = 'View details';
        viewIconBtn.onclick = e => { e.stopPropagation(); showDetail(entry.id); };

        actions.append(viewIconBtn);
        div.append(info, actions);
        resultsDiv.appendChild(div);
    }
}

// ── Fill ──────────────────────────────────────────────────────────────────────
async function fillEntry(id) {
    try {
        const response = await browser.runtime.sendMessage({ "GetEntry": { "id": id, "password": null } });
        if (response.Entry) {
            const login = response.Entry.entry.data.Login;
            if (!login) { showStatus("Not a login entry."); return; }
            const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
            if (tab) {
                browser.tabs.sendMessage(tab.id, {
                    type: "FILL_FORM",
                    username: login.username || '',
                    password: login.password || ''
                });
                window.close();
            }
        }
    } catch { showStatus("Failed to fill form."); }
}

// ── Utilities ─────────────────────────────────────────────────────────────────
function escapeHtml(s) {
    if (!s) return '';
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;")
            .replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#039;");
}

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
searchInput.addEventListener('input', updateResults);
searchInput.addEventListener('input', savePopupState);
favBtn.onclick = () => {
    favouritesOnly = !favouritesOnly;
    favBtn.textContent = favouritesOnly ? '★' : '☆';
    favBtn.classList.toggle('active', favouritesOnly);
    updateResults();
    savePopupState();
};
syncBtn.onclick = async () => { await browser.runtime.sendMessage("Sync"); updateResults(); };
lockBtn.onclick = async () => { await browser.runtime.sendMessage("Lock"); showView('list'); };
addBtn.onclick = () => showAddForm();
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
