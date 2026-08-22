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
    const passwordInputs = document.querySelectorAll('input[type="password"]');

    if (passwordInputs.length === 0) {
        // Multi-step logins (Google/Microsoft/SSO-style) show only a
        // username/email field first; the password field appears after
        // "Next" is clicked. findUsernameOnlyInput comes from
        // content-heuristics.js (loaded first).
        const usernameOnlyInput = findUsernameOnlyInput();
        if (usernameOnlyInput) setInputValue(usernameOnlyInput, username);
        return;
    }

    passwordInputs.forEach(passwordInput => {
        if (typeof isVisible === 'function' && !isVisible(passwordInput)) return;
        const form = passwordInput.form || passwordInput.closest('form') || document;

        setInputValue(passwordInput, password);

        // findUsernameInput comes from content-heuristics.js (loaded first).
        const usernameInput = findUsernameInput(form);
        if (usernameInput) setInputValue(usernameInput, username);
    });
}

function setInputValue(el, value) {
    el.value = value;
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
}
