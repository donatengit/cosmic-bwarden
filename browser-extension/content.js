if (typeof globalThis.browser === 'undefined') { globalThis.browser = globalThis.chrome; }

browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (message.type === "FILL_FORM") {
        const expected = message.expectedHost;
        if (!expected) return;
        if (location.hostname
            && location.hostname.toLowerCase() !== String(expected).toLowerCase()) {
            return;
        }
        fillForm(message.username || '', message.password || '');
    }
});

function fillForm(username, password) {
    const passwordInputs = Array.from(document.querySelectorAll('input[type="password"]'))
        .filter(input => typeof isVisible === 'function' ? isVisible(input) : true);

    if (passwordInputs.length === 0) {
        // Multi-step logins (Google/Microsoft/SSO-style) show only a
        // username/email field first; the password field appears after
        // "Next" is clicked. findUsernameOnlyInput comes from
        // content-heuristics.js (loaded first).
        const usernameOnlyInput = findUsernameOnlyInput();
        if (usernameOnlyInput) setInputValue(usernameOnlyInput, username);
        return;
    }

    for (const passwordInput of passwordInputs) {
        const form = passwordInput.form || passwordInput.closest('form');
        // One target per scope: on change-password and sign-up forms the
        // target is the current (or only) field, so a confirmation box is
        // never overwritten with the stored login password. A form whose only
        // password fields are PIN/one-time-code/card targets nothing, and the
        // null return is what keeps the vault password out of them.
        // findPasswordInput comes from content-heuristics.js (loaded first).
        const target = findPasswordInput(form || passwordInput);
        if (!target || target !== passwordInput) continue;

        setInputValue(target, password);

        // findUsernameInput comes from content-heuristics.js (loaded first).
        const usernameInput = findUsernameInput(form || document);
        if (usernameInput) setInputValue(usernameInput, username);
    }
}

function setInputValue(el, value) {
    el.value = value;
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
}
