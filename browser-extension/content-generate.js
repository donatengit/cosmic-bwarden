// Inline "generate password" icon for registration / change-password forms,
// plus the clipboard relay for the background script's context-menu action.
// Reuses `setInputValue`/`findUsernameInput` (globals declared by content.js/
// content-heuristics.js, which load earlier in the same isolated world — see
// the manifest's content_scripts comment).

if (typeof globalThis.browser === 'undefined') { globalThis.browser = globalThis.chrome; }

// ── Clipboard relay (context-menu quick-generate) ─────────────────────────
// A Chrome MV3 service worker has no clipboard/DOM access, so the background
// script's context-menu handler asks this content script to do the write.
browser.runtime.onMessage.addListener((message) => {
    if (message && message.type === 'GENERATE_COPY_TO_CLIPBOARD') {
        navigator.clipboard.writeText(message.password).catch(() => {});
        return Promise.resolve({ Ack: true });
    }
});

// ── Inline generate icon ───────────────────────────────────────────────────

const ICON_SIZE = 20;

const GENERATE_ICON_CSS = `
.icon-btn {
    all: initial;
    position: fixed;
    z-index: 2147483647;
    width: ${ICON_SIZE}px;
    height: ${ICON_SIZE}px;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    background: #175DDC;
    color: #fff;
    font: 13px system-ui, -apple-system, sans-serif;
    line-height: ${ICON_SIZE}px;
    text-align: center;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
}
.icon-btn:hover { background: #1450b8; }
`;

// input[type=password] -> shadow-DOM host <div>. A plain Map (not WeakMap):
// scroll/resize need to iterate all decorated inputs to reposition them.
const decoratedIcons = new Map();

// Fallback for scopes with exactly one password field and no autocomplete
// hint: registration/signup forms routinely self-identify via id/name/class/
// action text even without explicit autocomplete markup.
function looksLikeRegistrationScope(scope) {
    const attrs = [
        scope.id,
        scope.name,
        scope.className,
        typeof scope.getAttribute === 'function' ? scope.getAttribute('action') : null,
    ].filter(Boolean).join(' ').toLowerCase();
    return /regist|sign[\s-]?up|create[\s-]?account/.test(attrs);
}

function isCurrentPasswordField(el) {
    return el.getAttribute('autocomplete') === 'current-password';
}

function isNewPasswordField(el) {
    return el.getAttribute('autocomplete') === 'new-password';
}

// Groups of password inputs that should get the generate icon: every input
// in a group is filled together on click. A field explicitly marked
// autocomplete="current-password" is never included, even in an otherwise
// qualifying scope (a change-password form's "current password" box must
// never be overwritten with a freshly generated value). A lone password
// field with no autocomplete hint only qualifies via an explicit
// registration-looking scope — a plain login form (one password field, no
// such signal) never gets the icon, since offering to overwrite a login
// password would be actively harmful.
function passwordGroupsIn(root) {
    const forms = Array.from(root.querySelectorAll('form'));
    const scopes = [...forms, root]; // root last: catches inputs outside any form
    const groups = [];
    const seen = new Set();

    for (const scope of scopes) {
        const passwords = Array.from(scope.querySelectorAll('input[type="password"]'))
            .filter((el) => !seen.has(el));
        if (passwords.length === 0) continue;
        passwords.forEach((el) => seen.add(el));

        const newPasswordFields = passwords.filter(isNewPasswordField);
        const nonCurrentFields = passwords.filter((el) => !isCurrentPasswordField(el));

        let group = null;
        if (newPasswordFields.length > 0) {
            group = newPasswordFields;
        } else if (nonCurrentFields.length >= 2) {
            group = nonCurrentFields;
        } else if (
            passwords.length === 1 &&
            !isCurrentPasswordField(passwords[0]) &&
            looksLikeRegistrationScope(scope)
        ) {
            group = passwords;
        }
        if (group) groups.push(group);
    }
    return groups;
}

function positionIcon(host, input) {
    const rect = input.getBoundingClientRect();
    const btn = host.shadowRoot.querySelector('.icon-btn');
    if (!btn) return;
    if (rect.width === 0 && rect.height === 0) {
        host.style.display = 'none';
        return;
    }
    host.style.display = '';
    btn.style.top = `${rect.top + Math.max(0, (rect.height - ICON_SIZE) / 2)}px`;
    btn.style.left = `${rect.right - ICON_SIZE - 4}px`;
}

async function onGenerateClick(group) {
    let response;
    try {
        response = await browser.runtime.sendMessage({ GeneratePassword: { settings: null } });
    } catch {
        return; // extension reloaded; context gone
    }
    if (!response || !response.GeneratedPassword) return;
    const password = response.GeneratedPassword.password;
    // setInputValue comes from content.js (loaded earlier in this isolated world).
    for (const el of group) setInputValue(el, password);
}

function makeIcon(input, group) {
    const host = document.createElement('div');
    // Identifies the host for tests/debugging; not used by any styling.
    host.setAttribute('data-cosmic-bwarden-generate-icon', input.name || input.id || '');
    const shadow = host.attachShadow({ mode: 'open' });
    const style = document.createElement('style');
    style.textContent = GENERATE_ICON_CSS;

    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'icon-btn';
    btn.title = 'Generate password (COSMIC BWarden)';
    btn.textContent = '⚄';
    btn.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        onGenerateClick(group);
    });

    shadow.append(style, btn);
    document.documentElement.appendChild(host);
    positionIcon(host, input);
    return host;
}

function decorate(group) {
    for (const input of group) {
        if (decoratedIcons.has(input)) continue;
        const host = makeIcon(input, group);
        decoratedIcons.set(input, host);
        new ResizeObserver(() => positionIcon(host, input)).observe(input);
    }
}

function scanAndDecorate(root) {
    for (const group of passwordGroupsIn(root)) decorate(group);
}

function repositionAll() {
    for (const [input, host] of decoratedIcons) {
        if (!document.contains(input)) {
            host.remove();
            decoratedIcons.delete(input);
            continue;
        }
        positionIcon(host, input);
    }
}

window.addEventListener('scroll', repositionAll, { capture: true, passive: true });
window.addEventListener('resize', repositionAll, { passive: true });

// SPA-safe: catch late-rendered forms without re-scanning on every mutation.
let rescanTimer = null;
new MutationObserver(() => {
    clearTimeout(rescanTimer);
    rescanTimer = setTimeout(() => scanAndDecorate(document), 250);
}).observe(document.documentElement, { childList: true, subtree: true });

scanAndDecorate(document);
