// List-row actions: open-site (used directly by popup.js's row click
// handler for Login entries) and the copy-username/password dropdown.
// Loaded *before* popup.js (see popup.html) — renderEntries() calls
// makeCopyDropdownBtn, and the row's onclick calls openEntrySite, during
// popup.js's init IIFE, which can resume via an already-queued microtask
// before the parser reaches a script tag that comes after popup.js (same
// reasoning as popup-lock.js). Only function *bodies* here reference
// popup.js's globals (browser, showStatus); nothing at this file's top
// level calls them, so load order is otherwise safe.

let openDropdown = null;

function closeOpenDropdown() {
    if (openDropdown) {
        openDropdown.remove();
        openDropdown = null;
    }
}

document.addEventListener('click', () => closeOpenDropdown());
document.addEventListener('keydown', e => { if (e.key === 'Escape') closeOpenDropdown(); });

// #results scrolls (overflow-y: auto) and tightly wraps its rows, so a menu
// nested inside a row would get clipped by that ancestor whenever it
// overflows the row's box — which happens with any list short enough not to
// need scrolling. Appending to <body> with position:fixed, positioned from
// the button's viewport rect, sidesteps that clipping entirely.
function positionDropdown(menu, anchorBtn) {
    const btnRect = anchorBtn.getBoundingClientRect();
    const menuHeight = menu.offsetHeight;
    const menuWidth = menu.offsetWidth;
    const openUpward = btnRect.bottom + menuHeight > window.innerHeight;
    menu.style.top = `${openUpward ? btnRect.top - menuHeight : btnRect.bottom}px`;
    menu.style.left = `${Math.min(btnRect.right - menuWidth, window.innerWidth - menuWidth - 4)}px`;
}

function makeDropdownItem(label, onSelect) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.textContent = label;
    btn.onclick = async e => {
        e.stopPropagation();
        closeOpenDropdown();
        await onSelect();
    };
    return btn;
}

// Mirrors is_uri_like in crates/cosmarden-ui/src/app/applet_search.rs
// (used there to decide whether the desktop applet's open-URI button is enabled):
// true if `name` looks like a URL or bare hostname a browser can open (has
// a dot, no spaces, no @).
function isUriLike(name) {
    const stripped = name.replace(/^https?:\/\//, '');
    const host = stripped.split('/')[0];
    return !host.includes(' ') && host.includes('.') && !host.includes('@');
}

// List summaries don't carry the entry's URIs, so fetch them on demand —
// same on-demand fetch pattern the detail view uses for the full entry.
// Entries with no saved URI but a hostname-like name (e.g.
// "account.facebook.com") open that instead, same fallback as the applet.
function vaultUriToTabUrl(uri) {
    if (!uri) return null;
    const url = /^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(uri) ? uri : `https://${uri}`;
    let parsed;
    try { parsed = new URL(url); } catch { return null; }
    if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return null;
    return url;
}

async function openEntrySite(id) {
    try {
        const response = await browser.runtime.sendMessage({ "GetEntryMeta": { "id": id } });
        // The agent answers GetEntryMeta with EntryMeta { entry, filled_secrets }
        // (or Entry on an older agent). Same crossing as the detail view: read
        // the redacted entry from either shape; only its non-secret fields (the
        // saved URI, the entry name) are needed here.
        const entry = (response.EntryMeta && response.EntryMeta.entry)
            || (response.Entry && response.Entry.entry);
        const login = entry && entry.data.Login;
        const uris = login && login.uris;
        let uri = uris && uris.length > 0 ? uris[0].uri : null;
        if (!uri && entry && isUriLike(entry.name)) uri = entry.name;
        if (!uri) { showStatus("No website saved for this entry."); return; }
        const url = vaultUriToTabUrl(uri);
        if (!url) { showStatus("Unsupported URL scheme."); return; }
        await browser.tabs.create({ url });
        window.close();
    } catch { showStatus("Failed to open website.", 'error'); }
}

function makeCopyDropdownBtn(entry) {
    const wrap = document.createElement('div');
    wrap.className = 'copy-dropdown';

    const toggleBtn = document.createElement('button');
    toggleBtn.className = 'btn-icon';
    setIcon(toggleBtn, 'copy');
    toggleBtn.title = 'Copy username or password';
    toggleBtn.setAttribute('aria-label', 'Copy username or password');
    toggleBtn.onclick = e => {
        e.stopPropagation();
        const wasOpenHere = openDropdown && openDropdown.anchorWrap === wrap;
        closeOpenDropdown();
        if (wasOpenHere) return;

        const menu = document.createElement('div');
        menu.className = 'copy-dropdown-menu';
        menu.append(
            makeDropdownItem('Copy Username', async () => {
                await navigator.clipboard.writeText(entry.username || '');
            }),
            makeDropdownItem('Copy Password', async () => {
                const resp = await browser.runtime.sendMessage({ "GetPassword": { "id": entry.id } });
                const pw = resp.Password ? resp.Password.password : '';
                await navigator.clipboard.writeText(pw);
                if (typeof scheduleClipboardClear === 'function') scheduleClipboardClear(pw);
            })
        );
        document.body.appendChild(menu);
        positionDropdown(menu, toggleBtn);
        menu.anchorWrap = wrap;
        openDropdown = menu;
    };

    wrap.appendChild(toggleBtn);
    return wrap;
}
