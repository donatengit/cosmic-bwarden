if (typeof globalThis.browser === 'undefined') { globalThis.browser = globalThis.chrome; }

browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (message.type === "FILL_FORM") {
        fillForm(message.username || '', message.password || '');
    }
});

function fillForm(username, password) {
    const passwordInputs = document.querySelectorAll('input[type="password"]');

    passwordInputs.forEach(passwordInput => {
        const form = passwordInput.form || passwordInput.closest('form') || document;

        setInputValue(passwordInput, password);

        const textInputs = form.querySelectorAll('input[type="text"], input[type="email"], input:not([type])');
        let usernameInput = null;

        for (const input of textInputs) {
            const name = (input.name || "").toLowerCase();
            const id = (input.id || "").toLowerCase();
            const placeholder = (input.placeholder || "").toLowerCase();
            if (name.includes("user") || name.includes("email") || name.includes("login") ||
                id.includes("user") || id.includes("email") || id.includes("login") ||
                placeholder.includes("user") || placeholder.includes("email") || placeholder.includes("login")) {
                usernameInput = input;
                break;
            }
        }

        if (!usernameInput && textInputs.length > 0) usernameInput = textInputs[0];
        if (usernameInput) setInputValue(usernameInput, username);
    });
}

function setInputValue(el, value) {
    el.value = value;
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
}
