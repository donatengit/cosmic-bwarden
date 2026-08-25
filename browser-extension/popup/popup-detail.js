// Detail view rendering. Loaded after popup.js and popup-edit.js; references
// their globals (getEntryType, setIcon, showStatus, showView, currentEntry,
// scheduleClipboardClear).

const detailContent = document.getElementById('detail-content');
const editBtn = document.getElementById('edit-btn');
const deleteBtn = document.getElementById('delete-btn');
const cancelBtn = document.getElementById('cancel-btn');

async function showDetail(id) {
    try {
        const response = await browser.runtime.sendMessage({ "GetEntryMeta": { "id": id } });
        // The agent answers with EntryMeta { entry, filled_secrets }. Cross the
        // `filled_secrets` through to the renderer so masked rows appear only
        // for actually-filled secret slots (password, totp, card code, hidden
        // custom fields, notes) — presence never implies pulling a plaintext.
        let entry;
        let filledSecrets = [];
        if (response.EntryMeta) {
            entry = response.EntryMeta.entry;
            filledSecrets = response.EntryMeta.filled_secrets || [];
            // A SecureNote's content lives in the notes field, which meta reads
            // redact — fetch the full entry (its content) just like the edit
            // flow does, before rendering. (EntryData::SecureNote arrives as
            // the string "SecureNote".)
            const isSecureNote =
                entry.data === 'SecureNote'
                || !!(entry.data && entry.data.SecureNote);
            if (isSecureNote) {
                const full = await browser.runtime.sendMessage({ "GetEntry": { "id": id, "password": null } });
                if (full.Entry) entry = full.Entry.entry;
            }
        } else if (response.Entry) {
            // Older agent (no EntryMeta yet): render non-secret fields only.
            entry = response.Entry.entry;
        }
        if (entry) {
            currentEntry = entry;
            renderDetail(entry, filledSecrets);
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

// Extracts the plaintext for a stable field key from a full (non-redacted)
// entry. Mirrors the agent/UI's `field_plaintext` mapping so reveal/copy
// recover exactly what the entry stores, including hidden custom fields.
function secretValue(fullEntry, key) {
    const data = fullEntry.data;
    // Values arrive as plain JSON strings over native messaging.
    const str = s => (s == null ? '' : String(s));
    if (data.Login) {
        if (key === 'Password') return str(data.Login.password);
        if (key === 'TOTP') return null; // TOTP reveal fetches the live code
    }
    if (data.SshKey && key === 'Private Key') return str(data.SshKey.private_key);
    if (data.Card) {
        if (key === 'Card Number') return str(data.Card.number);
        if (key === 'Security Code') return str(data.Card.code);
    }
    if (data.BankAccount) {
        const b = data.BankAccount;
        if (key === 'Account Number') return str(b.account_number);
        if (key === 'Routing Number') return str(b.routing_number);
        if (key === 'PIN') return str(b.pin);
        if (key === 'IBAN') return b.iban || '';
        if (key === 'SWIFT Code') return b.swift_code || '';
        if (key === 'Branch Number') return b.branch_number || '';
    }
    if (data.Identity) {
        const id = data.Identity;
        if (key === 'SSN') return id.ssn || '';
        if (key === 'License Number') return id.license_number || '';
        if (key === 'Passport Number') return id.passport_number || '';
    }
    if (data.DriversLicense && key === 'License Number') return str(data.DriversLicense.license_number);
    if (data.Passport) {
        if (key === 'Passport Number') return str(data.Passport.passport_number);
        if (key === 'National Identification Number') return data.Passport.national_identification_number || '';
        if (key === 'Date of Birth') return data.Passport.date_of_birth || '';
    }
    if (key === 'Notes') {
        return fullEntry.notes ? str(fullEntry.notes) : '';
    }
    // Custom fields (including hidden ones) are fetched by name.
    const f = (fullEntry.fields || []).find(x => x.name === key);
    return f && f.value ? str(f.value) : '';
}

// Returns a fragment with masked text, reveal toggle, and copy button.
// Secrets are fetched on demand via GetPassword/GetTotp/GetEntry, never held
// eagerly. Static secrets are memoized per popup session; TOTP codes rotate
// every 30 s, so they are always refetched via GetTotp (reveal or copy) to
// avoid serving a stale code.
function makeSecretRow(entryId, key) {
    const frag = document.createDocumentFragment();
    const span = document.createElement('span');
    span.className = 'secret-text';
    span.textContent = '••••••••';
    const isTotp = key === 'TOTP';
    let revealed = null;

    const getSecret = async () => {
        if (isTotp) {
            const resp = await browser.runtime.sendMessage({ "GetTotp": { "id": entryId } });
            return (resp.Totp && resp.Totp.code) || '';
        }
        if (revealed !== null) return revealed;
        if (key === 'Password') {
            const resp = await browser.runtime.sendMessage({ "GetPassword": { "id": entryId } });
            revealed = (resp.Password && resp.Password.password) || '';
        } else {
            // Everything else (card code, private key, hidden custom fields,
            // notes, document numbers) needs the full entry — the agent
            // decrypts it, so the plaintext never transits until reveal.
            const resp = await browser.runtime.sendMessage({ "GetEntry": { "id": entryId, "password": null } });
            if (resp.Entry) revealed = secretValue(resp.Entry.entry, key);
        }
        return revealed || '';
    };

    const revealBtn = document.createElement('button');
    revealBtn.className = 'btn btn-ghost btn-sm reveal-btn';
    setIcon(revealBtn, 'eye');
    revealBtn.title = isTotp ? 'Show code' : 'Reveal';
    revealBtn.setAttribute('aria-label', revealBtn.title);
    revealBtn.addEventListener('click', async () => {
        const isHidden = span.textContent === '••••••••';
        const secret = isHidden ? await getSecret() : '';
        span.textContent = isHidden ? secret : '••••••••';
        setIcon(revealBtn, isHidden ? 'eye-off' : 'eye');
        // Masked secrets keep letter-spacing so the dot count reads; a
        // revealed secret drops it (see popup.css .secret-text).
        span.classList.toggle('revealed', !isHidden);
    });

    frag.append(span, revealBtn, makeCopyBtn(getSecret));
    return frag;
}

// Ordered list of the detail rows to render: only *filled* fields. Non-secret
// fields carry their value; secret fields carry only `secret: true` (their
// presence comes from `filledSecrets`, their value is masked until reveal).
function detailFields(entry, filledSecrets) {
    const filled = new Set(filledSecrets || []);
    const out = [];
    // Secret slot: show a masked row only if the agent says it's filled.
    const pushSecret = (key) => { if (filled.has(key)) out.push({ key, secret: true }); };
    // Non-secret, non-empty field.
    const pushText = (label, value) => { if (value) out.push({ key: label, label, value: String(value) }); };

    const data = entry.data;
    if (data.Login) {
        pushText('Username', data.Login.username);
        pushSecret('Password');
        pushSecret('TOTP');
        (data.Login.uris || []).forEach((u, i) => {
            if (u && u.uri) out.push({ key: `URL${i}`, label: i === 0 ? 'URL' : `URL ${i + 1}`, value: u.uri });
        });
    } else if (data.Card) {
        pushSecret('Card Number');
        pushText('Cardholder', data.Card.cardholder_name);
        pushText('Brand', data.Card.brand);
        pushText('Expiry', [data.Card.exp_month, data.Card.exp_year].filter(Boolean).join('/'));
        pushSecret('Security Code');
    } else if (data.Identity) {
        const id = data.Identity;
        pushText('Title', id.title);
        pushText('First Name', id.first_name);
        pushText('Middle Name', id.middle_name);
        pushText('Last Name', id.last_name);
        pushText('Username', id.username);
        pushText('Email', id.email);
        pushText('Phone', id.phone);
        pushText('Address', [id.address1, id.address2, id.address3, id.city, id.state, id.postal_code, id.country].filter(Boolean).join(', '));
        pushSecret('SSN');
        pushSecret('License Number');
        pushSecret('Passport Number');
    } else if (data.SshKey) {
        pushSecret('Private Key');
        pushText('Public Key', data.SshKey.public_key);
        pushText('Fingerprint', data.SshKey.fingerprint);
    } else if (data.BankAccount) {
        const b = data.BankAccount;
        pushText('Bank Name', b.bank_name);
        pushText('Name on Account', b.name_on_account);
        pushText('Account Type', b.account_type);
        pushSecret('Account Number');
        pushSecret('Routing Number');
        pushSecret('Branch Number');
        pushSecret('PIN');
        pushSecret('SWIFT Code');
        pushSecret('IBAN');
        pushText('Bank Contact Phone', b.bank_contact_phone);
    } else if (data.DriversLicense) {
        const d = data.DriversLicense;
        pushText('First Name', d.first_name);
        pushText('Middle Name', d.middle_name);
        pushText('Last Name', d.last_name);
        pushText('Date of Birth', d.date_of_birth);
        pushSecret('License Number');
        pushText('Issuing Country', d.issuing_country);
        pushText('Issuing State', d.issuing_state);
        pushText('Issue Date', d.issue_date);
        pushText('Expiration Date', d.expiration_date);
        pushText('Issuing Authority', d.issuing_authority);
        pushText('License Class', d.license_class);
    } else if (data.Passport) {
        const p = data.Passport;
        pushText('Surname', p.surname);
        pushText('Given Name', p.given_name);
        pushSecret('Date of Birth');
        pushText('Sex', p.sex);
        pushText('Birth Place', p.birth_place);
        pushText('Nationality', p.nationality);
        pushText('Issuing Country', p.issuing_country);
        pushSecret('Passport Number');
        pushText('Passport Type', p.passport_type);
        pushSecret('National Identification Number');
        pushText('Issuing Authority', p.issuing_authority);
        pushText('Issue Date', p.issue_date);
        pushText('Expiration Date', p.expiration_date);
    }

    if (data === 'SecureNote' || (data && data.SecureNote)) {
        // A SecureNote's content IS its notes: show the plaintext (the full
        // entry was fetched in showDetail), not a masked row.
        if (entry.notes) out.push({ key: 'Notes', label: 'Notes', value: String(entry.notes) });
    } else if (filled.has('Notes')) {
        // Non-SecureNote entries: notes are a secret-class slot that the meta
        // read redacts, so a filled one renders as a masked row (reveal via
        // GetEntry) rather than leaking plaintext into the passive detail view.
        out.push({ key: 'Notes', secret: true });
    }

    // Custom (user-defined) fields always render: their labels are the plain
    // names the user gave. Hidden ones are secret-class → masked row; visible
    // ones keep their plaintext value from the meta entry. `ty` is a number
    // (FieldType is #repr(u16): Text=0, Hidden=1 — see core api/models).
    (entry.fields || []).forEach(f => {
        if (!f || !f.name) return;
        if (Number(f.ty) === 1) {
            // Hidden — a masked row, shown only if the agent reports it filled.
            if (filled.has(f.name)) out.push({ key: f.name, label: f.name, secret: true });
        } else if (f.value != null) {
            // Text (0) or null/unknown ty — visible field with a value.
            out.push({ key: f.name, label: f.name, value: String(f.value) });
        }
    });

    return out;
}

function renderDetail(entry, filledSecrets) {
    detailContent.innerHTML = '';
    detailContent.appendChild(makeDetailItem('Name', String(entry.name || '')));
    detailContent.appendChild(makeDetailItem('Type', getEntryType(entry)));

    const fields = detailFields(entry, filledSecrets);
    for (const field of fields) {
        if (field.secret) {
            const frag = document.createDocumentFragment();
            frag.appendChild(makeSecretRow(entry.id, field.key));
            const item = makeDetailItem(field.label || field.key, frag);
            detailContent.appendChild(item);
            continue;
        }
        const frag = document.createDocumentFragment();
        frag.appendChild(document.createTextNode(field.value));
        // Copy makes sense for most values (usernames, uris, public keys).
        if (field.key !== 'Type') frag.appendChild(makeCopyBtn(() => field.value));
        const item = makeDetailItem(field.label || field.key, frag);
        if (field.key === 'Public Key') item.querySelector('.detail-value').classList.add('detail-value--mono');
        detailContent.appendChild(item);
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
