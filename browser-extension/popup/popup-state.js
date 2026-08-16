// Transient popup-state persistence across close/reopen.
// A browser popup is destroyed the moment it loses focus (window switch,
// context-menu close); reopening creates a fresh document, so module-level
// variables are gone. storage.session is in-memory (cleared when the browser
// exits) — the same durability the background save-prompt uses — and survives
// the fresh document a reopened popup creates.
// Loaded before popup.js; only function *bodies* reference popup.js /
// popup-edit.js globals, so the load order is safe (same reasoning as
// popup-lock.js).
const POPUP_STATE_KEY = 'popupState';

function snapshotPopupState() {
    const state = {
        // The state belongs to the tab it was captured on. Without this, one
        // detail view left open would reopen on every unrelated site for the
        // rest of the browser session, hiding the current tab's matches — the
        // popup's whole job.
        domain: currentTabDomain,
        view: currentView,
        entryId: (currentView === 'detail' || currentView === 'edit') && currentEntry
            ? currentEntry.id : null,
        search: searchInput.value,
        favouritesOnly: !!favouritesOnly,
    };
    // Only the edit draft is persisted — never the entry's own secrets, which
    // are re-fetched via GetEntry on restore. A typed password lives here only
    // as long as the browser session does.
    if (currentView === 'edit') {
        const fields = {};
        getFieldsForType(editType.value).forEach(f => {
            const el = document.getElementById(`f-${f.key}`);
            if (el) fields[f.key] = el.value || '';
        });
        state.draft = {
            type: editType.value,
            name: editName.value,
            notes: editNotes.value,
            fields,
        };
    }
    return state;
}

function savePopupState() {
    // A locked vault drops the stored state instead of writing 'locked' over a
    // real view: the lock screen is where it must reopen anyway, and an edit
    // draft holds a password the user typed — that must not outlive the unlocked
    // session in storage.session (an agent-side idle lock reaches here too, via
    // updateResults -> showLockedView).
    if (currentView === 'locked') { clearPopupState(); return; }
    const state = snapshotPopupState();
    try {
        // Fire-and-forget: set() is async but every call overwrites the same
        // key, so the last write before the popup closes wins. Not awaiting also
        // keeps the write synchronous in the sessionStorage-backed test double,
        // avoiding a lost-write race with an immediate navigation.
        browser.storage.session.set({ [POPUP_STATE_KEY]: state }).catch(() => {});
    } catch { /* storage unavailable (some test doubles) — state just won't persist */ }
}

function clearPopupState() {
    try {
        browser.storage.session.remove(POPUP_STATE_KEY).catch(() => {});
    } catch { /* storage unavailable (some test doubles) */ }
}

async function loadPopupState() {
    try {
        const store = await browser.storage.session.get(POPUP_STATE_KEY);
        return store[POPUP_STATE_KEY] || null;
    } catch { return null; }
}

async function isVaultUsable() {
    try {
        const resp = await browser.runtime.sendMessage("GetConfig");
        return !!(resp && resp.Config && !resp.Config.is_locked && !resp.Config.needs_login);
    } catch { return false; }
}

// Rebuild the edit form from a persisted draft. Called after showEdit() /
// showAddForm() have populated the base form; the draft wins.
function applyEditDraft(draft) {
    if (!draft) return;
    if (draft.type !== undefined && draft.type !== editType.value) {
        editType.value = draft.type;
        renderDynamicFields(draft.type);
    }
    if (draft.name !== undefined) editName.value = draft.name;
    if (draft.notes !== undefined) editNotes.value = draft.notes;
    Object.entries(draft.fields || {}).forEach(([key, value]) => {
        const el = document.getElementById(`f-${key}`);
        if (el) el.value = value;
    });
}

async function restorePopupState() {
    let state = await loadPopupState();
    // Captured on a different domain: start clean so the popup shows this
    // tab's matches, not the last one's. (Both null — no tab access either
    // time — still restores; there is no domain list to shadow.)
    if (state && state.domain !== currentTabDomain) state = null;

    if (state) {
        searchInput.value = state.search || '';
        favouritesOnly = !!state.favouritesOnly;
        favBtn.textContent = favouritesOnly ? '★' : '☆';
        favBtn.classList.toggle('active', favouritesOnly);
    }

    // Detail/edit restore bypasses updateResults' lock gate, so gate them here;
    // otherwise the list default handles the locked/needs-login case.
    if (state && state.view === 'detail' && state.entryId && await isVaultUsable()) {
        await showDetail(state.entryId);
        return;
    }
    if (state && state.view === 'edit' && await isVaultUsable()) {
        if (state.entryId) await showEdit(state.entryId);
        else showAddForm();
        applyEditDraft(state.draft);
        return;
    }

    showView('list');
}
