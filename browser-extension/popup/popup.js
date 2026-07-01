if (typeof globalThis.browser === 'undefined') { globalThis.browser = globalThis.chrome; }

// ── DOM refs ──────────────────────────────────────────────────────────────────
const searchInput = document.getElementById('search');
const resultsDiv  = document.getElementById('results');
const syncBtn     = document.getElementById('sync-btn');
const addBtn      = document.getElementById('add-btn');
const backBtn     = document.getElementById('back-btn');
const lockBtn     = document.getElementById('lock-btn');
const statusDiv   = document.getElementById('status');
const viewList    = document.getElementById('view-list');
const viewDetail  = document.getElementById('view-detail');
const viewEdit    = document.getElementById('view-edit');

// ── State ─────────────────────────────────────────────────────────────────────
let currentEntry = null;
let currentView  = 'list';
let currentTabDomain = null;

// ── Domain helpers ────────────────────────────────────────────────────────────
function extractDomain(url) {
    try {
        const host = new URL(url).hostname.toLowerCase().replace(/^www\./, '');
        const parts = host.split('.');
        return parts.length > 1 ? parts.slice(-2).join('.') : host;
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
    [viewList, viewDetail, viewEdit].forEach(v => v.classList.add('hidden'));
    [backBtn, searchInput, addBtn, syncBtn, lockBtn].forEach(b => b.classList.add('hidden'));

    if (view === 'list') {
        viewList.classList.remove('hidden');
        [searchInput, addBtn, syncBtn, lockBtn].forEach(b => b.classList.remove('hidden'));
        updateResults();
    } else if (view === 'detail') {
        viewDetail.classList.remove('hidden');
        [backBtn, lockBtn].forEach(b => b.classList.remove('hidden'));
    } else if (view === 'edit') {
        viewEdit.classList.remove('hidden');
        backBtn.classList.remove('hidden');
    }
}

// ── Vault list ────────────────────────────────────────────────────────────────
async function updateResults() {
    hideStatus();
    // When no explicit search, use the current tab's domain so the agent
    // returns only name-matching entries (e.g. "iqos.ru" matches entry "iqos.ru").
    const query = searchInput.value || currentTabDomain || null;
    try {
        const configResp = await browser.runtime.sendMessage("GetConfig");
        if (configResp.Config) {
            if (configResp.Config.is_locked) {
                await browser.runtime.sendMessage("RequestUnlock");
                showStatus("Vault is locked — unlock via the COSMIC applet.");
                return;
            }
            if (configResp.Config.needs_login) {
                showStatus("Not logged in.");
                return;
            }
        }

        const response = await browser.runtime.sendMessage({
            "GetSidebarEntries": { "query": query, "entry_type": null, "only_pinned": false }
        });

        if (response.SidebarEntries) {
            renderEntries(response.SidebarEntries.entries);
        } else if (response.Error) {
            showStatus(response.Error.message);
        }
    } catch { showStatus("Failed to communicate with agent."); }
}

function renderEntries(entries) {
    resultsDiv.innerHTML = '';
    if (entries.length === 0) {
        resultsDiv.innerHTML = '<div class="no-results">No entries found</div>';
        return;
    }

    for (const entry of entries) {
        const div = document.createElement('div');
        div.className = 'entry';
        div.onclick = () => showDetail(entry.id);

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
            actions.append(fillBtn);
        }

        const editIconBtn = document.createElement('button');
        editIconBtn.textContent = '✏';
        editIconBtn.title = 'Edit';
        editIconBtn.onclick = e => { e.stopPropagation(); showEdit(entry.id); };

        actions.append(editIconBtn);
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
    showView('list');
})();
