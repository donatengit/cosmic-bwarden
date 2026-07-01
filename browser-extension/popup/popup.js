if (typeof globalThis.browser === 'undefined') { globalThis.browser = globalThis.chrome; }

function showError(msg) {
    const statusDiv = document.getElementById('status');
    if (statusDiv) {
        statusDiv.textContent = msg;
        statusDiv.classList.remove('hidden');
    }
    const resultsDiv = document.getElementById('results');
    if (resultsDiv && currentView === 'list') resultsDiv.innerHTML = '';
}

window.onerror = (msg, url, line) => {
    showError(`JS Error: ${msg} at ${line}`);
};

const searchInput = document.getElementById('search');
const resultsDiv = document.getElementById('results');
const syncBtn = document.getElementById('sync-btn');
const addBtn = document.getElementById('add-btn');
const backBtn = document.getElementById('back-btn');
const lockBtn = document.getElementById('lock-btn');
const statusDiv = document.getElementById('status');

const viewList = document.getElementById('view-list');
const viewDetail = document.getElementById('view-detail');
const viewEdit = document.getElementById('view-edit');

const detailContent = document.getElementById('detail-content');
const editForm = document.getElementById('edit-form');
const editType = document.getElementById('edit-type');
const editName = document.getElementById('edit-name');
const editNotes = document.getElementById('edit-notes');
const dynamicFields = document.getElementById('dynamic-fields');
const cancelBtn = document.getElementById('cancel-btn');
const editBtn = document.getElementById('edit-btn');
const deleteBtn = document.getElementById('delete-btn');

let currentEntry = null;
let currentView = 'list';

function showView(view) {
    currentView = view;
    viewList.classList.add('hidden');
    viewDetail.classList.add('hidden');
    viewEdit.classList.add('hidden');
    backBtn.classList.add('hidden');
    searchInput.classList.add('hidden');
    addBtn.classList.add('hidden');
    syncBtn.classList.add('hidden');

    if (view === 'list') {
        viewList.classList.remove('hidden');
        searchInput.classList.remove('hidden');
        addBtn.classList.remove('hidden');
        syncBtn.classList.remove('hidden');
        updateResults();
    } else if (view === 'detail') {
        viewDetail.classList.remove('hidden');
        backBtn.classList.remove('hidden');
    } else if (view === 'edit') {
        viewEdit.classList.remove('hidden');
        backBtn.classList.remove('hidden');
    }
}

async function updateResults() {
    const query = searchInput.value;
    try {
        const configResp = await browser.runtime.sendMessage("GetConfig");
        if (configResp.Config) {
            if (configResp.Config.is_locked) {
                showError("Vault is locked.");
                return;
            }
            if (configResp.Config.needs_login) {
                showError("Not logged in.");
                return;
            }
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
        showError("Failed to communicate with agent.");
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
        div.onclick = () => showDetail(entry.id);
        
        const info = document.createElement('div');
        info.className = 'entry-info';
        info.innerHTML = `
            <div class="entry-name">${escapeHtml(entry.name)}</div>
            <div class="entry-user">${escapeHtml(entry.username || entry.entry_type)}</div>
        `;
        
        const actions = document.createElement('div');
        actions.className = 'entry-actions';
        
        const fillBtn = document.createElement('button');
        fillBtn.textContent = 'Fill';
        fillBtn.onclick = (e) => {
            e.stopPropagation();
            fillEntry(entry.id);
        };
        
        actions.appendChild(fillBtn);
        div.appendChild(info);
        div.appendChild(actions);
        resultsDiv.appendChild(div);
    });
}

async function showDetail(id) {
    try {
        const response = await browser.runtime.sendMessage({
            "GetEntry": { "id": id, "password": null }
        });
        if (response.Entry) {
            currentEntry = response.Entry.entry;
            renderDetail(currentEntry);
            showView('detail');
        }
    } catch (e) {
        showError("Failed to load entry details.");
    }
}

function makeDetailItem(label, valueNode) {
    const item = document.createElement('div');
    item.className = 'detail-item';
    const lbl = document.createElement('div');
    lbl.className = 'detail-label';
    lbl.textContent = label;
    const val = document.createElement('div');
    val.className = 'detail-value';
    if (typeof valueNode === 'string') {
        val.textContent = valueNode;
    } else {
        val.appendChild(valueNode);
    }
    item.appendChild(lbl);
    item.appendChild(val);
    return item;
}

function makeCopyBtn(getText) {
    const btn = document.createElement('button');
    btn.textContent = 'Copy';
    btn.addEventListener('click', async () => {
        await navigator.clipboard.writeText(await getText());
    });
    return btn;
}

function renderDetail(entry) {
    detailContent.innerHTML = '';
    detailContent.appendChild(makeDetailItem('Name', String(entry.name || '')));
    detailContent.appendChild(makeDetailItem('Type', getEntryType(entry)));

    const data = entry.data;
    if (data.Login) {
        detailContent.appendChild(makeDetailItem('Username', data.Login.username || ''));
        const pwdFrag = document.createDocumentFragment();
        pwdFrag.appendChild(document.createTextNode('******** '));
        const password = data.Login.password || '';
        pwdFrag.appendChild(makeCopyBtn(() => password));
        const pwdItem = makeDetailItem('Password', '');
        pwdItem.querySelector('.detail-value').appendChild(pwdFrag);
        detailContent.appendChild(pwdItem);
        if (data.Login.totp) {
            const totpItem = makeDetailItem('TOTP', '');
            const entryId = entry.id;
            totpItem.querySelector('.detail-value').appendChild(makeCopyBtn(async () => {
                const response = await browser.runtime.sendMessage({ "GetTotp": { "id": entryId } });
                return response.Totp ? response.Totp.code : '';
            }));
            detailContent.appendChild(totpItem);
        }
    } else if (data.Card) {
        const cardNum = data.Card.number || '';
        const numFrag = document.createDocumentFragment();
        numFrag.appendChild(document.createTextNode('**** **** **** ' + cardNum.slice(-4) + ' '));
        numFrag.appendChild(makeCopyBtn(() => cardNum));
        const numItem = makeDetailItem('Number', '');
        numItem.querySelector('.detail-value').appendChild(numFrag);
        detailContent.appendChild(numItem);
        detailContent.appendChild(makeDetailItem('Cardholder', data.Card.cardholder_name || ''));
        detailContent.appendChild(makeDetailItem('Brand', data.Card.brand || ''));
        detailContent.appendChild(makeDetailItem('Expiry', `${data.Card.exp_month || ''}/${data.Card.exp_year || ''}`));
    } else if (data.Identity) {
        if (data.Identity.first_name) detailContent.appendChild(makeDetailItem('First Name', data.Identity.first_name));
        if (data.Identity.last_name) detailContent.appendChild(makeDetailItem('Last Name', data.Identity.last_name));
        if (data.Identity.email) detailContent.appendChild(makeDetailItem('Email', data.Identity.email));
        if (data.Identity.phone) detailContent.appendChild(makeDetailItem('Phone', data.Identity.phone));
        const addrParts = [
            data.Identity.address1, data.Identity.city,
            data.Identity.state, data.Identity.postal_code, data.Identity.country,
        ].filter(Boolean);
        if (addrParts.length) detailContent.appendChild(makeDetailItem('Address', addrParts.join(', ')));
    } else if (data.SshKey) {
        if (data.SshKey.public_key) {
            const pk = data.SshKey.public_key;
            const pkFrag = document.createDocumentFragment();
            pkFrag.appendChild(document.createTextNode(pk + ' '));
            pkFrag.appendChild(makeCopyBtn(() => pk));
            const pkItem = makeDetailItem('Public Key', '');
            const pkVal = pkItem.querySelector('.detail-value');
            pkVal.style.cssText = 'font-size:0.7em; word-break:break-all;';
            pkVal.appendChild(pkFrag);
            detailContent.appendChild(pkItem);
        }
        if (data.SshKey.fingerprint) {
            detailContent.appendChild(makeDetailItem('Fingerprint', data.SshKey.fingerprint));
        }
    }

    if (entry.notes) {
        detailContent.appendChild(makeDetailItem('Notes', entry.notes));
    }
}

function showAddForm() {
    currentEntry = null;
    editForm.reset();
    editType.disabled = false;
    renderDynamicFields('Login');
    showView('edit');
}

function showEditForm() {
    if (!currentEntry) return;
    const type = getEntryType(currentEntry);
    editType.value = type;
    editType.disabled = true;
    editName.value = currentEntry.name;
    editNotes.value = currentEntry.notes || '';
    renderDynamicFields(type, currentEntry.data);
    showView('edit');
}

editType.onchange = () => renderDynamicFields(editType.value);

function renderDynamicFields(type, data = {}) {
    dynamicFields.innerHTML = '';
    const fields = getFieldsForType(type);
    
    fields.forEach(field => {
        const group = document.createElement('div');
        group.className = 'form-group';
        const val = (data[type] && data[type][field.key]) || '';
        group.innerHTML = `
            <label for="f-${field.key}">${field.label}</label>
            ${field.type === 'textarea' 
                ? `<textarea id="f-${field.key}">${escapeHtml(val.toString())}</textarea>`
                : `<input type="${field.type || 'text'}" id="f-${field.key}" value="${escapeHtml(val.toString())}">`
            }
        `;
        dynamicFields.appendChild(group);
    });
}

function getFieldsForType(type) {
    switch (type) {
        case 'Login': return [
            { key: 'username', label: 'Username' },
            { key: 'password', label: 'Password', type: 'password' },
            { key: 'totp', label: 'TOTP Secret' }
        ];
        case 'Card': return [
            { key: 'cardholder_name', label: 'Cardholder Name' },
            { key: 'number', label: 'Number' },
            { key: 'brand', label: 'Brand' },
            { key: 'exp_month', label: 'Exp Month' },
            { key: 'exp_year', label: 'Exp Year' },
            { key: 'code', label: 'Security Code', type: 'password' }
        ];
        case 'Identity': return [
            { key: 'first_name', label: 'First Name' },
            { key: 'last_name', label: 'Last Name' },
            { key: 'email', label: 'Email' },
            { key: 'phone', label: 'Phone' },
            { key: 'address1', label: 'Address 1' },
            { key: 'city', label: 'City' },
            { key: 'state', label: 'State' },
            { key: 'postal_code', label: 'Postal Code' },
            { key: 'country', label: 'Country' }
        ];
        default: return [];
    }
}

editForm.onsubmit = async (e) => {
    e.preventDefault();
    const type = editType.value;
    const name = editName.value;
    const notes = editNotes.value;
    
    const payload = {
        name,
        notes: notes || null,
        fields: []
    };

    getFieldsForType(type).forEach(f => {
        const input = document.getElementById(`f-${f.key}`);
        payload[f.key] = input.value || null;
    });

    try {
        let action;
        if (currentEntry) {
            const updatedEntry = JSON.parse(JSON.stringify(currentEntry));
            updatedEntry.name = name;
            updatedEntry.notes = notes || null;
            const typeData = updatedEntry.data[type];
            getFieldsForType(type).forEach(f => {
                const input = document.getElementById(`f-${f.key}`);
                typeData[f.key] = input.value || null;
            });
            action = { "UpdateEntry": { "entry": updatedEntry } };
        } else {
            if (type === 'Login') {
                action = { "AddEntry": { ...payload, entry_type: 'Login' } };
            } else if (type === 'SecureNote') {
                action = { "AddSecureNote": payload };
            } else if (type === 'Card') {
                action = { "AddCard": payload };
            } else if (type === 'Identity') {
                action = { "AddIdentity": payload };
            }
        }

        const response = await browser.runtime.sendMessage(action);
        if (response === "Ack" || response.Ack) {
            showView('list');
        } else if (response.Error) {
            showError(response.Error.message);
        }
    } catch (e) {
        showError("Failed to save entry.");
    }
};

deleteBtn.onclick = async () => {
    if (!currentEntry || !confirm("Delete this entry?")) return;
    try {
        const response = await browser.runtime.sendMessage({ "DeleteEntry": { "id": currentEntry.id } });
        if (response === "Ack" || response.Ack) {
            showView('list');
        }
    } catch (e) {
        showError("Failed to delete entry.");
    }
};

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
        }
    } catch (e) {
        showError("Failed to fill form.");
    }
}

function escapeHtml(unsafe) {
    if (!unsafe) return '';
    return unsafe.toString()
         .replace(/&/g, "&amp;")
         .replace(/</g, "&lt;")
         .replace(/>/g, "&gt;")
         .replace(/"/g, "&quot;")
         .replace(/'/g, "&#039;");
}

function getEntryType(entry) {
    if (!entry || !entry.data) return 'Login';
    const data = entry.data;
    if (typeof data === 'string') return data;
    if (data.Login) return 'Login';
    if (data.Card) return 'Card';
    if (data.Identity) return 'Identity';
    if (data.SshKey) return 'SshKey';
    return 'Login';
}

searchInput.addEventListener('input', updateResults);
syncBtn.onclick = async () => {
    await browser.runtime.sendMessage("Sync");
    updateResults();
};
addBtn.onclick = showAddForm;
backBtn.onclick = () => showView('list');
cancelBtn.onclick = () => showView(currentEntry ? 'detail' : 'list');
editBtn.onclick = showEditForm;
lockBtn.onclick = async () => {
    await browser.runtime.sendMessage("Lock");
    showView('list');
};

// Initial load
showView('list');
