// Shared pure DOM heuristics for the fill (content.js) and submit-capture
// (content-submit.js) content scripts. Must be listed first in the manifest's
// content_scripts js array — all files share one isolated world.

const USERNAME_TEXT_INPUT_SELECTOR = 'input[type="text"], input[type="email"], input[type="tel"], input:not([type])';
const USERNAME_KEYWORDS = ["user", "email", "login", "customerid", "userid"];

// Mirrors (a lightweight version of) Bitwarden's viewable-element check:
// skips disabled/readonly fields and ones hidden via CSS — otherwise a
// honeypot field or a hidden legacy input can outrank the real one.
function isVisible(el) {
    if (!el || el.disabled || el.readOnly) return false;
    const style = getComputedStyle(el);
    // parseFloat("") is NaN, and unset opacity computes to "" in some engines
    // (real value is 1) — `< 0.1` treats that indeterminate case as visible.
    return style.display !== "none" && style.visibility !== "hidden" &&
        style.visibility !== "collapse" && !(parseFloat(style.opacity) < 0.1);
}

// The `autocomplete` IDL property (input.autocomplete) only reflects values
// the browser considers valid autofill tokens for that field, and silently
// returns "" otherwise (observed in Firefox for type="email" + "username").
// getAttribute() always returns the raw attribute, so use that instead.
function hasUsernameAutocomplete(input) {
    return (input.getAttribute("autocomplete") || "").toLowerCase().split(/\s+/).includes("username");
}

function matchesUsernameKeywords(input) {
    const haystack = [input.name, input.id, input.placeholder, input.getAttribute("autocomplete")]
        .filter(Boolean).join(" ").toLowerCase();
    return USERNAME_KEYWORDS.some(k => haystack.includes(k));
}

// Finds the best username-like field within `scope` (typically a <form> that
// already contains a password field). Falls back to the first visible
// candidate since the scope is already narrowed to one form/page.
function findUsernameInput(scope) {
    const candidates = Array.from(scope.querySelectorAll(USERNAME_TEXT_INPUT_SELECTOR)).filter(isVisible);

    return candidates.find(hasUsernameAutocomplete) ||
        candidates.find(matchesUsernameKeywords) ||
        candidates[0] || null;
}

// Finds a username-only field anywhere on the page, for multi-step logins
// (Google/Microsoft/SSO-style) whose first screen has no password field yet.
// Unlike findUsernameInput, this never falls back to "first visible field":
// the scope here is the whole document, so an unrelated search box or
// newsletter-signup field would otherwise get filled instead.
function findUsernameOnlyInput() {
    const candidates = Array.from(document.querySelectorAll(USERNAME_TEXT_INPUT_SELECTOR)).filter(isVisible);

    return candidates.find(hasUsernameAutocomplete) ||
        candidates.find(matchesUsernameKeywords) || null;
}

// Credentials present in `form` at submit time. The password is the value of
// the last non-empty password input: on registration and change-password
// forms that is the freshly chosen (or confirmed) password, not the old one.
function findSubmittedCredentials(form) {
    let password = '';
    for (const input of form.querySelectorAll('input[type="password"]')) {
        if (input.value) password = input.value;
    }

    const usernameInput = findUsernameInput(form);
    const username = usernameInput ? usernameInput.value.trim() : '';

    return { username, password };
}
