if (typeof globalThis.browser === 'undefined') { globalThis.browser = globalThis.chrome; }

browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (message && message.type === "FILL_FORM") {
        // The vault entry's hosts, not the tab's: with all_frames the message
        // reaches every frame in the tab, and each one decides for itself.
        if (!hostMatchesAny(location.hostname, message.entryHosts)) return;
        fillForm(message.username || '', message.password || '');
    }
});

function fillForm(username, password) {
    // findPasswordInput/passwordInputsIn/findUsernameInput come from
    // content-heuristics.js; isVisible/formOf/treeRootOf/setInputValue from
    // content-dom.js (loaded before it).
    const passwordInputs = passwordInputsIn(document).filter(isVisible);

    if (passwordInputs.length === 0) {
        // Multi-step logins (Google/Microsoft/SSO-style) show only a
        // username/email field first; the password field appears after
        // "Next" is clicked.
        const usernameOnlyInput = findUsernameOnlyInput();
        if (usernameOnlyInput) setInputValue(usernameOnlyInput, username);
        return;
    }

    for (const passwordInput of passwordInputs) {
        const form = formOf(passwordInput);
        // One target per scope: on change-password and sign-up forms the
        // target is the current (or only) field, so a confirmation box is
        // never overwritten with the stored login password. A form whose only
        // password fields are PIN/one-time-code/card targets nothing, and the
        // null return is what keeps the vault password out of them. Without a
        // form, the field is its own scope.
        const target = findPasswordInput(form || passwordInput);
        if (!target || target !== passwordInput) continue;

        setInputValue(target, password);

        // A field inside a web component has no form and its shadow boundary
        // stops closest('form'), so the username is searched in the same tree
        // rather than the whole document.
        const usernameInput = findUsernameInput(form || treeRootOf(passwordInput));
        if (usernameInput) setInputValue(usernameInput, username);
    }
}
