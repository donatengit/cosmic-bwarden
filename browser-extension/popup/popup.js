const searchInput = document.getElementById('search');
const resultsDiv = document.getElementById('results');
const syncBtn = document.getElementById('sync-btn');
const lockBtn = document.getElementById('lock-btn');
const statusDiv = document.getElementById('status');

async function updateResults() {
    const query = searchInput.value;
    try {
        // Check vault status first
        const configResp = await browser.runtime.sendMessage("GetConfig");
        if (configResp.Config) {
            if (configResp.Config.is_locked) {
                showError("Vault is locked. Please unlock it in the Cosmic BWarden app.");
                return;
            }
            if (configResp.Config.needs_login) {
                showError("Not logged in. Please log in using the Cosmic BWarden app.");
                return;
            }
        } else if (configResp.Error) {
            showError(configResp.Error.message);
            return;
        }

        const response = await browser.runtime.sendMessage({
            "GetSidebarEntries": {
                "query": query || null,
                "entry_type": null,
                "only_pinned": false
            }
        });

        if (response.SidebarEntries) {
            renderEntries(response.SidebarEntries.entries);
            statusDiv.classList.add('hidden');
        } else if (response.Error) {
            showError(response.Error.message);
        }
    } catch (e) {
        showError("Failed to communicate with agent. Is it running?");
    }
}

function renderEntries(entries) {
    resultsDiv.innerHTML = '';
    if (entries.length === 0) {
        resultsDiv.innerHTML = '<div class="no-results">No entries found</div>';
        return;
    }

    entries.forEach(entry => {
        const div = document.createElement('div');
        div.className = 'entry';
        
        const info = document.createElement('div');
        info.className = 'entry-info';
        info.innerHTML = `
            <div class="entry-name">${escapeHtml(entry.name)}</div>
            <div class="entry-user">${escapeHtml(entry.username || '')}</div>
        `;
        
        const actions = document.createElement('div');
        actions.className = 'entry-actions';
        
        const copyBtn = document.createElement('button');
        copyBtn.textContent = 'Copy';
        copyBtn.title = 'Copy Password';
        copyBtn.onclick = (e) => copyPassword(entry.id, e.target);
        
        const totpBtn = document.createElement('button');
        totpBtn.textContent = 'TOTP';
        totpBtn.title = 'Copy TOTP Code';
        totpBtn.onclick = (e) => copyTotp(entry.id, e.target);

        const fillBtn = document.createElement('button');
        fillBtn.textContent = 'Fill';
        fillBtn.title = 'Fill Form';
        fillBtn.onclick = () => fillEntry(entry.id);
        
        actions.appendChild(copyBtn);
        actions.appendChild(totpBtn);
        actions.appendChild(fillBtn);
        
        div.appendChild(info);
        div.appendChild(actions);
        resultsDiv.appendChild(div);
    });
}

async function copyPassword(id, btn) {
    try {
        const response = await browser.runtime.sendMessage({
            "GetPassword": { "id": id, "password": null }
        });
        if (response.Password) {
            await navigator.clipboard.writeText(response.Password.password);
            showFeedback(btn, 'Copied!');
        } else if (response.Error) {
            showError(response.Error.message);
        }
    } catch (e) {
        showError("Failed to copy password.");
    }
}

async function copyTotp(id, btn) {
    try {
        const response = await browser.runtime.sendMessage({
            "GetTotp": { "id": id }
        });
        if (response.Totp) {
            await navigator.clipboard.writeText(response.Totp.code);
            showFeedback(btn, 'Copied!');
        } else if (response.Error) {
            showError(response.Error.message);
        }
    } catch (e) {
        showError("Failed to copy TOTP.");
    }
}

function showFeedback(btn, text) {
    const originalText = btn.textContent;
    btn.textContent = text;
    setTimeout(() => btn.textContent = originalText, 1000);
}

async function fillEntry(id) {
    try {
        const response = await browser.runtime.sendMessage({
            "GetEntry": { "id": id, "password": null }
        });
        if (response.Entry) {
            const tabs = await browser.tabs.query({ active: true, currentWindow: true });
            if (tabs[0]) {
                browser.tabs.sendMessage(tabs[0].id, {
                    type: "FILL_FORM",
                    entry: response.Entry.entry
                });
            }
        } else if (response.Error) {
            showError(response.Error.message);
        }
    } catch (e) {
        showError("Failed to fill form.");
    }
}

function showError(msg) {
    statusDiv.textContent = msg;
    statusDiv.classList.remove('hidden');
    resultsDiv.innerHTML = '';
}

function escapeHtml(unsafe) {
    return unsafe
         .replace(/&/g, "&amp;")
         .replace(/</g, "&lt;")
         .replace(/>/g, "&gt;")
         .replace(/"/g, "&quot;")
         .replace(/'/g, "&#039;");
}

searchInput.addEventListener('input', updateResults);

syncBtn.onclick = async () => {
    await browser.runtime.sendMessage("Sync");
    updateResults();
};

lockBtn.onclick = async () => {
    await browser.runtime.sendMessage("Lock");
    updateResults();
};

// Initial load
updateResults();
