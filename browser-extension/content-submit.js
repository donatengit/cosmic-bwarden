// Detects login-form submissions and reports the captured credentials to the
// background script, which decides whether to offer saving them. Values are
// read synchronously at event time, before any navigation. Uses the helpers in
// content-dom.js/content-heuristics.js (loaded first).

let _lastSubmitReport = 0;

function reportSubmittedForm(scope) {
    const now = Date.now();
    // A click on a submit button also fires the form's submit event (and Enter
    // fires keydown + submit) — report each user action once.
    if (now - _lastSubmitReport < 500) return;

    const { username, password } = findSubmittedCredentials(scope);
    if (!username || !password) return;

    _lastSubmitReport = now;
    try {
        browser.runtime.sendMessage({
            type: 'LOGIN_SUBMITTED',
            username,
            password,
            url: window.location.href,
        });
    } catch { /* extension reloaded; context gone */ }
}

// Resolve the login scope for an element: its owning form, or the shadow root
// it lives in when the login widget is a web component with no <form>. A
// document with no form returns null — a page-wide fallback would turn any
// submit-like button into a save prompt for whatever login form happens to be
// elsewhere on the page.
function passwordScopeFor(el) {
    if (!el) return null;
    const form = formOf(el);
    if (form) {
        return passwordInputsIn(form).some(input => isVisible(input) && !isExcludedPasswordInput(input))
            ? form
            : null;
    }
    const root = el.getRootNode ? el.getRootNode() : null;
    if (!root || root.nodeType !== 11) return null; // 11 = DocumentFragment (shadow root)
    return passwordInputsIn(root).some(input => isVisible(input) && !isExcludedPasswordInput(input))
        ? root
        : null;
}

// The node the user actually acted on. A click or keydown from inside an open
// shadow root is composed, so it reaches this window listener — but retargeted
// to the component's host, which is not the button or input and carries none
// of its form association. composedPath() starts at the real inner node.
// (A `submit` event is not composed at all: a form inside a shadow root never
// reaches this listener, which is why the click and Enter paths below are the
// ones that capture a web-component login.)
function eventTarget(e) {
    const path = typeof e.composedPath === 'function' ? e.composedPath() : null;
    return (path && path.length > 0) ? path[0] : e.target;
}

// Capture phase on window so pages that stopPropagation() in their own
// handlers can't hide the submission. Covers form.requestSubmit() too.
window.addEventListener('submit', (e) => {
    const scope = passwordScopeFor(eventTarget(e));
    if (scope) reportSubmittedForm(scope);
}, true);

// JS-driven logins often preventDefault() the submit or use plain buttons:
// also capture clicks on submit-like buttons inside a password form.
window.addEventListener('click', (e) => {
    const target = eventTarget(e);
    if (!target || typeof target.closest !== 'function') return;
    const button = target.closest('button[type="submit"], input[type="submit"], button:not([type])');
    const scope = passwordScopeFor(button);
    if (scope) reportSubmittedForm(scope);
}, true);

// ...and Enter pressed inside a password field.
window.addEventListener('keydown', (e) => {
    if (e.key !== 'Enter') return;
    const t = eventTarget(e);
    if (t && t.tagName === 'INPUT' && isPasswordInput(t)) {
        const scope = passwordScopeFor(t);
        if (scope) reportSubmittedForm(scope);
    }
}, true);
