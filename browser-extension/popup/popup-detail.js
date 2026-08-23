// Detail view rendering. Loaded after popup.js; references its globals.

const detailContent = document.getElementById('detail-content');
const editBtn = document.getElementById('edit-btn');
const deleteBtn = document.getElementById('delete-btn');
const cancelBtn = document.getElementById('cancel-btn');

async function showDetail(id) {
    try {
        const response = await browser.runtime.sendMessage({ "GetEntryMeta": { "id": id } });
        if (response.Entry) {
            currentEntry = response.Entry.entry;
            // A SecureNote's content lives in the notes field, which meta
            // reads redact — fetch the full entry (its content) just like
            // the edit flow does, before rendering. (EntryData::SecureNote is
            // a unit variant, so it arrives as the string "SecureNote".)
            const isSecureNote =
                currentEntry.data === 'SecureNote'
                || !!(currentEntry.data && currentEntry.data.SecureNote);
            if (isSecureNote) {
                const full = await browser.runtime.sendMessage({ "GetEntry": { "id": id, "password": null } });
                if (full.Entry) currentEntry = full.Entry.entry;
            }
            renderDetail(currentEntry);
            showView('detail');
        }
    } catch { showStatus("Failed to load entry.", 'error'); }
}

// Fetch with secrets (user explicitly entering edit mode)
async function showEdit(id) {
    try {
        const response = await browser.runtime.sendMessage({ "GetEntry": { "id": id, "password": null } });
        if (response.Entry) {
            currentEntry = response.Entry.entry;
            showEditForm();
        }
    } catch { showStatus("Failed to load entry.", 'error'); }
}

function makeDetailItem(label, valueNode) {
    const item = document.createElement('div');
    item.className = 'detail-item';
    const lbl = document.createElement('div');
    lbl.className = 'detail-label';
    lbl.textContent = label;
    const val = document.createElement('div');
    val.className = 'detail-value';
    if (typeof valueNode === 'string') val.textContent = valueNode;
    else val.appendChild(valueNode);
    item.append(lbl, val);
    return item;
}

function makeCopyBtn(getTextFn) {
    const btn = document.createElement('button');
    btn.className = 'btn btn-ghost btn-sm copy-btn';
    btn.textContent = 'Copy';
    btn.title = 'Copy';
    btn.addEventListener('click', async () => {
        const text = await getTextFn();
        await navigator.clipboard.writeText(text);
        if (typeof scheduleClipboardClear === 'function') scheduleClipboardClear(text);
    });
    return btn;
}

// Returns a fragment with masked text, reveal toggle, and copy button.
// Secrets are fetched on demand via GetPassword, never held eagerly.
function makeSecretRow(entryId) {
    const frag = document.createDocumentFragment();
    const span = document.createElement('span');
    span.className = 'secret-text';
    span.textContent = '••••••••';
    let revealed = null;

    const getSecret = async () => {
        if (revealed === null) {
            const resp = await browser.runtime.sendMessage({ "GetPassword": { "id": entryId } });
            revealed = resp.Password ? resp.Password.password : '';
        }
        return revealed;
    };

    const revealBtn = document.createElement('button');
    revealBtn.className = 'btn btn-ghost btn-sm reveal-btn';
    setIcon(revealBtn, 'eye');
    revealBtn.title = 'Reveal';
    revealBtn.setAttribute('aria-label', 'Reveal password');
    revealBtn.addEventListener('click', async () => {
        const secret = await getSecret();
        const isHidden = span.textContent === '••••••••';
        span.textContent = isHidden ? secret : '••••••••';
        setIcon(revealBtn, isHidden ? 'eye-off' : 'eye');
        // Masked secrets keep letter-spacing so the dot count reads; a
        // revealed secret drops it (see popup.css .secret-text).
        span.classList.toggle('revealed', !isHidden);
    });

    frag.append(span, revealBtn, makeCopyBtn(getSecret));
    return frag;
}

function renderDetail(entry) {
    detailContent.innerHTML = '';
    detailContent.appendChild(makeDetailItem('Name', String(entry.name || '')));
    detailContent.appendChild(makeDetailItem('Type', getEntryType(entry)));

    const data = entry.data;
    if (data.Login) {
        const username = data.Login.username || '';
        const userFrag = document.createDocumentFragment();
        userFrag.appendChild(document.createTextNode(username));
        if (username) userFrag.appendChild(makeCopyBtn(() => username));
        detailContent.appendChild(makeDetailItem('Username', userFrag));
        detailContent.appendChild(makeDetailItem('Password', makeSecretRow(entry.id)));
        if (data.Login.totp !== undefined && data.Login.totp !== null) {
            const frag = document.createDocumentFragment();
            frag.appendChild(makeCopyBtn(async () => {
                const r = await browser.runtime.sendMessage({ "GetTotp": { "id": entry.id } });
                return r.Totp ? r.Totp.code : '';
            }));
            detailContent.appendChild(makeDetailItem('TOTP', frag));
        }
        if (data.Login.uris && data.Login.uris.length > 0) {
            detailContent.appendChild(makeDetailItem('URL', data.Login.uris[0].uri || ''));
        }
    } else if (data.Card) {
        detailContent.appendChild(makeDetailItem('Number', makeSecretRow(entry.id)));
        detailContent.appendChild(makeDetailItem('Cardholder', data.Card.cardholder_name || ''));
        detailContent.appendChild(makeDetailItem('Brand', data.Card.brand || ''));
        detailContent.appendChild(makeDetailItem('Expiry',
            `${data.Card.exp_month || ''}/${data.Card.exp_year || ''}`));
    } else if (data.Identity) {
        const id = data.Identity;
        if (id.first_name) detailContent.appendChild(makeDetailItem('First Name', id.first_name));
        if (id.last_name) detailContent.appendChild(makeDetailItem('Last Name', id.last_name));
        if (id.email) detailContent.appendChild(makeDetailItem('Email', id.email));
        if (id.phone) detailContent.appendChild(makeDetailItem('Phone', id.phone));
        const addr = [id.address1, id.city, id.state, id.postal_code, id.country].filter(Boolean);
        if (addr.length) detailContent.appendChild(makeDetailItem('Address', addr.join(', ')));
    } else if (data.SshKey) {
        if (data.SshKey.public_key) {
            const pk = data.SshKey.public_key;
            const frag = document.createDocumentFragment();
            frag.append(document.createTextNode(pk + ' '), makeCopyBtn(() => pk));
            const item = makeDetailItem('Public Key', frag);
            item.querySelector('.detail-value').classList.add('detail-value--mono');
            detailContent.appendChild(item);
        }
        if (data.SshKey.fingerprint)
            detailContent.appendChild(makeDetailItem('Fingerprint', data.SshKey.fingerprint));
    } else if (data === 'SecureNote' || (data && data.SecureNote)) {
        if (entry.notes) detailContent.appendChild(makeDetailItem('Notes', entry.notes));
    }
}

// currentEntry here comes from showDetail()'s meta-only GetEntryMeta (no
// password). Re-fetch the full entry via showEdit before rendering the
// form — showEditForm() alone would prefill the password field empty, and
// on save that empty value overwrites the stored password with null.
editBtn.onclick = () => currentEntry && showEdit(currentEntry.id);

// Two-step delete: the first click arms the button (3 s), the second deletes.
// Replaces the native confirm() dialog — the popup never leaves its own
// visual language.
let deleteArmed = false;
let deleteArmTimer = null;
const DELETE_ARM_LABEL = 'Confirm delete';

function disarmDelete() {
    deleteArmed = false;
    if (deleteArmTimer) { clearTimeout(deleteArmTimer); deleteArmTimer = null; }
    deleteBtn.textContent = 'Delete';
}

deleteBtn.onclick = async () => {
    if (!currentEntry) return;
    if (!deleteArmed) {
        deleteArmed = true;
        deleteBtn.textContent = DELETE_ARM_LABEL;
        deleteArmTimer = setTimeout(disarmDelete, 3000);
        return;
    }
    disarmDelete();
    try {
        const r = await browser.runtime.sendMessage({ "DeleteEntry": { "id": currentEntry.id } });
        if (r === "Ack" || r.Ack) showView('list');
    } catch { showStatus("Failed to delete entry.", 'error'); }
};
cancelBtn.onclick = () => showView(currentEntry ? 'detail' : 'list');
