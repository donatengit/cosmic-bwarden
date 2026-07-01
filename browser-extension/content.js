if (typeof globalThis.browser === 'undefined') { globalThis.browser = globalThis.chrome; }

browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (message.type === "FILL_FORM") {
        fillForm(message.entry);
    }
});

function fillForm(entry) {
    // Basic heuristic for finding login forms
    const passwordInputs = document.querySelectorAll('input[type="password"]');
    
    passwordInputs.forEach(passwordInput => {
        const form = passwordInput.form || passwordInput.closest('form') || document;
        
        // Fill password
        // The agent returns EntryData enum, which is serialized as an object with the variant name as key
        const loginData = entry.data.Login;
        if (!loginData) return;

        passwordInput.value = loginData.password || "";
        passwordInput.dispatchEvent(new Event('input', { bubbles: true }));
        passwordInput.dispatchEvent(new Event('change', { bubbles: true }));

        // Try to find username input in the same form/context
        const textInputs = form.querySelectorAll('input[type="text"], input[type="email"], input:not([type])');
        let usernameInput = null;
        
        // Look for common username field names/ids
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

        // If not found by name, take the first text input before the password field
        if (!usernameInput && textInputs.length > 0) {
            usernameInput = textInputs[0];
        }

        if (usernameInput) {
            usernameInput.value = loginData.username || "";
            usernameInput.dispatchEvent(new Event('input', { bubbles: true }));
            usernameInput.dispatchEvent(new Event('change', { bubbles: true }));
        }
    });
}
