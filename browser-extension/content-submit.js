// Detects login-form submissions and reports the captured credentials to the
// background script, which decides whether to offer saving them. Values are
// read synchronously at event time, before any navigation. Uses
// findSubmittedCredentials from content-heuristics.js (loaded first).

let _lastSubmitReport = 0;

function reportSubmittedForm(form) {
    const now = Date.now();
    // A click on a submit button also fires the form's submit event (and Enter
    // fires keydown + submit) — report each user action once.
    if (now - _lastSubmitReport < 500) return;

    const { username, password } = findSubmittedCredentials(form);
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

// Resolve the login form for an element: its owning form, if that form
// actually contains a password field.
function passwordFormFor(el) {
    if (!el || typeof el.closest !== 'function') return null;
    const form = el.form || el.closest('form');
    if (!form || typeof form.querySelector !== 'function') return null;
    return form.querySelector('input[type="password"]') ? form : null;
}

// Capture phase on window so pages that stopPropagation() in their own
// handlers can't hide the submission. Covers form.requestSubmit() too.
window.addEventListener('submit', (e) => {
    const form = passwordFormFor(e.target);
    if (form) reportSubmittedForm(form);
}, true);

// JS-driven logins often preventDefault() the submit or use plain buttons:
// also capture clicks on submit-like buttons inside a password form.
window.addEventListener('click', (e) => {
    if (!e.target || typeof e.target.closest !== 'function') return;
    const button = e.target.closest('button[type="submit"], input[type="submit"], button:not([type])');
    const form = passwordFormFor(button);
    if (form) reportSubmittedForm(form);
}, true);

// ...and Enter pressed inside a password field.
window.addEventListener('keydown', (e) => {
    if (e.key !== 'Enter') return;
    const t = e.target;
    if (t && t.tagName === 'INPUT' && t.type === 'password') {
        const form = passwordFormFor(t);
        if (form) reportSubmittedForm(form);
    }
}, true);
