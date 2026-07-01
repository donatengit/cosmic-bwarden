// Edit / Add form. Loaded after popup.js and popup-detail.js; references their globals.

const editForm = document.getElementById('edit-form');
const editType = document.getElementById('edit-type');
const editName = document.getElementById('edit-name');
const editNotes = document.getElementById('edit-notes');
const dynamicFields = document.getElementById('dynamic-fields');

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
    editNotes.value = currentEntry.notes ? String(currentEntry.notes) : '';
    renderDynamicFields(type, currentEntry.data);
    showView('edit');
}

editType.onchange = () => renderDynamicFields(editType.value);

function renderDynamicFields(type, data = {}) {
    dynamicFields.innerHTML = '';
    getFieldsForType(type).forEach(field => {
        const group = document.createElement('div');
        group.className = 'form-group';

        const label = document.createElement('label');
        label.setAttribute('for', `f-${field.key}`);
        label.textContent = field.label;
        group.appendChild(label);

        const raw = (data[type] && data[type][field.key]) || '';
        const val = typeof raw === 'object' ? '' : escapeHtml(String(raw));

        if (field.type === 'password') {
            const wrap = document.createElement('div');
            wrap.className = 'password-field-wrap';
            const inp = document.createElement('input');
            inp.type = 'password';
            inp.id = `f-${field.key}`;
            inp.value = val;
            const toggle = document.createElement('button');
            toggle.type = 'button';
            toggle.textContent = '👁';
            toggle.title = 'Reveal';
            toggle.onclick = () => {
                inp.type = inp.type === 'password' ? 'text' : 'password';
                toggle.textContent = inp.type === 'password' ? '👁' : '🙈';
            };
            wrap.append(inp, toggle);
            group.appendChild(wrap);
        } else {
            const inp = document.createElement('input');
            inp.type = field.type || 'text';
            inp.id = `f-${field.key}`;
            inp.value = val;
            group.appendChild(inp);
        }
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
    try {
        let action;
        if (currentEntry) {
            const updated = JSON.parse(JSON.stringify(currentEntry));
            updated.name = name;
            updated.notes = notes || null;
            const typeData = updated.data[type];
            getFieldsForType(type).forEach(f => {
                const el = document.getElementById(`f-${f.key}`);
                if (el) typeData[f.key] = el.value || null;
            });
            action = { "UpdateEntry": { "entry": updated } };
        } else {
            const payload = { name, notes: notes || null, fields: [] };
            getFieldsForType(type).forEach(f => {
                const el = document.getElementById(`f-${f.key}`);
                if (el) payload[f.key] = el.value || null;
            });
            if (type === 'Login') action = { "AddEntry": { ...payload, entry_type: 'Login' } };
            else if (type === 'SecureNote') action = { "AddSecureNote": payload };
            else if (type === 'Card') action = { "AddCard": payload };
            else if (type === 'Identity') action = { "AddIdentity": payload };
        }
        const response = await browser.runtime.sendMessage(action);
        if (response === "Ack" || response.Ack) showView('list');
        else if (response.Error) showStatus(response.Error.message);
    } catch { showStatus("Failed to save entry."); }
};
