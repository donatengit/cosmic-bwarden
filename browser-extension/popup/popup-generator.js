// Header "generate" menu and the password-history view. Both entries are agent
// calls (Action::GeneratePassword / Action::GetPasswordHistory): the agent owns
// the RNG, the "last used" settings and the device-local 7-day history, so the
// popup follows whatever the desktop pane or CLI last configured — see
// docs/password_generator_plan.md.
//
// Loaded after popup-menu.js (shared dropdown helpers) and before popup.js.
// Only function bodies touch popup.js globals (browser, showView, showStatus,
// hideStatus, currentView), so the load order is safe — the same rule
// popup-lock.js documents.

const HISTORY_MASK = '••••••••';

function agentErrorMessage(resp, fallback) {
    if (resp && resp.Error && typeof resp.Error.message === 'string' && resp.Error.message) {
        return resp.Error.message;
    }
    return fallback;
}

function thrownMessage(err, fallback) {
    if (err && typeof err.message === 'string' && err.message) return err.message;
    return fallback;
}

// The single clipboard path for generated secrets, so the 30 s auto-clear
// policy cannot be skipped by one of the two call sites.
async function writeSecretToClipboard(secret) {
    if (typeof secret !== 'string' || secret.length === 0) {
        throw new Error('nothing to copy');
    }
    if (typeof navigator === 'undefined' || !navigator.clipboard
        || typeof navigator.clipboard.writeText !== 'function') {
        throw new Error('clipboard unavailable');
    }
    await navigator.clipboard.writeText(secret);
    if (typeof scheduleClipboardClear === 'function') scheduleClipboardClear(secret);
}

// Relative age of a history entry. Timestamps arrive as Unix seconds. An
// unparsable or absent value reads as "unknown" instead of throwing a row
// away; a future timestamp means clock skew between agent and popup, and
// "just now" is the honest reading.
function formatAge(createdAt, nowSecs = Math.floor(Date.now() / 1000)) {
    if (typeof createdAt !== 'number' || !Number.isFinite(createdAt) || createdAt <= 0) {
        return 'unknown';
    }
    const diff = Math.floor(nowSecs - createdAt);
    if (diff < 60) return 'just now';
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    return `${Math.floor(diff / 86400)}d ago`;
}

// Open the two-item menu on the header button, or close it when the same
// button is clicked again. Reuses popup-list-actions.js's dropdown (appended to
// <body>, position: fixed) so it cannot be clipped by a scrolling ancestor.
function toggleGeneratorMenu(anchorBtn) {
    const wasOpenHere = !!(openDropdown && openDropdown.anchorBtn === anchorBtn);
    closeOpenDropdown();
    if (wasOpenHere) return;

    const menu = document.createElement('div');
    menu.className = 'copy-dropdown-menu gen-menu';
    menu.setAttribute('aria-label', 'Password generator');
    menu.append(
        makeDropdownItem('Generate password', generatePasswordToClipboard),
        makeDropdownItem('Show previous', showHistory)
    );
    showDropdownMenu(menu, anchorBtn);
}

async function generatePasswordToClipboard() {
    let password;
    try {
        // settings: null — reuse the last-used settings from any surface
        // (same request the context menu and inline field icon send).
        const resp = await browser.runtime.sendMessage({ GeneratePassword: { settings: null } });
        password = resp && resp.GeneratedPassword ? resp.GeneratedPassword.password : null;
        if (typeof password !== 'string' || password.length === 0) {
            showStatus(agentErrorMessage(resp, 'Unexpected agent response.'), 'error');
            return;
        }
    } catch (err) {
        showStatus(thrownMessage(err, 'Failed to generate a password.'), 'error');
        return;
    }

    try {
        await writeSecretToClipboard(password);
        showStatus('Password generated and copied to clipboard.');
    } catch {
        // The password was generated and recorded in the agent's history; only
        // the clipboard handoff failed. Reporting a plain failure would read as
        // "nothing happened", so say which half worked.
        showStatus('Password generated, but copying to the clipboard failed.', 'error');
    }
}

async function showHistory() {
    showView('history');
    hideStatus();
    clearHistoryView();
    try {
        const resp = await browser.runtime.sendMessage('GetPasswordHistory');
        // The agent answers with PasswordHistory { entries }; anything else is
        // a protocol mismatch or an Error, both of which must surface rather
        // than render an empty list that looks like "nothing generated yet".
        const entries = resp && resp.PasswordHistory ? resp.PasswordHistory.entries : null;
        if (!Array.isArray(entries)) {
            showStatus(agentErrorMessage(resp, 'Unexpected agent response.'), 'error');
            return;
        }
        // The user may have navigated away while the agent was answering;
        // rendering then would leave decrypted passwords in a hidden node.
        if (currentView !== 'history') return;
        renderHistory(entries);
    } catch (err) {
        showStatus(thrownMessage(err, 'Failed to load generated passwords.'), 'error');
    }
}

function renderHistory(entries, nowSecs = Math.floor(Date.now() / 1000)) {
    const content = document.getElementById('history-content');
    if (!content) return;
    content.replaceChildren();
    if (!Array.isArray(entries) || entries.length === 0) {
        const empty = document.createElement('div');
        empty.className = 'no-results';
        empty.textContent = 'No passwords generated in the last 7 days.';
        content.appendChild(empty);
        return;
    }
    const caption = document.createElement('div');
    caption.className = 'list-caption';
    caption.textContent = 'Last 7 days';
    content.appendChild(caption);
    // Newest-first is the agent's order (GetPasswordHistory sorts by
    // created_at descending). Render as received so a change there is not
    // silently contradicted by the popup.
    for (const entry of entries) content.appendChild(historyRow(entry, nowSecs));
}

function historyRow(entry, nowSecs) {
    const password = entry && typeof entry.password === 'string' ? entry.password : '';
    const createdAt = entry ? Number(entry.created_at) : NaN;

    const row = document.createElement('div');
    row.className = 'entry history-entry';

    const info = document.createElement('div');
    info.className = 'entry-info';
    const value = document.createElement('div');
    value.className = 'entry-name secret-text history-password';
    value.textContent = HISTORY_MASK;
    const age = document.createElement('div');
    age.className = 'history-age';
    age.textContent = formatAge(createdAt, nowSecs);
    info.append(value, age);

    const actions = document.createElement('div');
    actions.className = 'entry-actions';

    // Masked on render, like the detail view: opening the history is passive,
    // and a revealed plaintext should be an explicit click.
    let revealed = false;
    const revealBtn = document.createElement('button');
    revealBtn.type = 'button';
    revealBtn.className = 'btn-icon history-reveal-btn';
    setIcon(revealBtn, 'eye');
    revealBtn.title = 'Reveal password';
    revealBtn.setAttribute('aria-label', 'Reveal password');
    revealBtn.setAttribute('aria-pressed', 'false');
    revealBtn.onclick = () => {
        revealed = !revealed;
        value.textContent = revealed ? password : HISTORY_MASK;
        value.classList.toggle('revealed', revealed);
        setIcon(revealBtn, revealed ? 'eye-off' : 'eye');
        revealBtn.title = revealed ? 'Hide password' : 'Reveal password';
        revealBtn.setAttribute('aria-label', revealBtn.title);
        revealBtn.setAttribute('aria-pressed', String(revealed));
    };

    const copyBtn = document.createElement('button');
    copyBtn.type = 'button';
    copyBtn.className = 'btn-icon history-copy-btn';
    setIcon(copyBtn, 'copy');
    copyBtn.title = 'Copy password';
    copyBtn.setAttribute('aria-label', 'Copy password');
    copyBtn.onclick = async () => {
        try {
            await writeSecretToClipboard(password);
            showStatus('Password copied to clipboard.');
        } catch {
            showStatus('Failed to copy the password to the clipboard.', 'error');
        }
    };

    actions.append(revealBtn, copyBtn);
    row.append(info, actions);
    return row;
}

// Leaving the history view drops the rendered rows (and therefore every
// decrypted password held in this document).
function clearHistoryView() {
    const content = document.getElementById('history-content');
    if (content) content.replaceChildren();
}
