// Shared pure DOM heuristics for the fill (content.js) and submit-capture
// (content-submit.js) content scripts. Must be listed first in the manifest's
// content_scripts js array — all files share one isolated world.

const USERNAME_TEXT_INPUT_SELECTOR = 'input[type="text"], input[type="email"], input[type="tel"], input:not([type])';

// Identity roots. A token that starts with one scores full weight; one that
// merely contains it scores half. Tokens are split on camelCase and
// separators before the check, so `ldap-username`, `idLogin`, `user_name` and
// `emailAddress` all reach full weight, while a fused lowercase name like
// `idlogin` still matches at half weight. `benutzer` covers the German
// `benutzername` / `benutzer-id` login names.
const USERNAME_ROOTS = [
    "user", "username", "userid", "login", "email", "mail",
    "identifier", "account", "nickname", "handle", "phone", "mobile",
    "benutzer",
];

// Fields that are not login identifiers, whatever else they match. Short
// words are exact-token only: `pin` must not hit `shipping` and `cc` must not
// hit `success`. `address` is deliberately absent because `email-address` is
// a common login field name. `otc`, `onetime`, `twofa`, `2fa` and `approvals`
// mirror the one-time-code names Bitwarden keeps in its TOTP list —
// Microsoft's TOTP input is `otc-confirmation-input`, and the two-factor
// names can appear on a username-only step, where a fill would write the
// stored username into the code box.
const EXCLUDED_PREFIXES = [
    "search", "query", "filter", "find", "captcha", "recaptcha",
    "newsletter", "subscribe", "verification", "otp", "otc", "totp", "mfa",
    "csrf", "token", "onetime", "twofa", "twofactor", "2fa", "approvals",
];
const EXCLUDED_TOKENS = [
    "code", "pin", "cvv", "cc", "card", "company", "organization",
    "organisation", "website", "url", "message", "subject", "comment",
];

// Current/new password words. Prefix matching on normalized tokens tells
// `currentpassword`, `old_password` and `confirm-password` apart without
// matching `gold` for `old`.
const PASSWORD_CURRENT_HINTS = ["current", "old", "existing"];
const PASSWORD_NEW_HINTS = ["new", "confirm", "repeat", "again", "verify", "retype"];

// Mirrors (a lightweight version of) Bitwarden's viewable-element check:
// skips disabled/readonly fields and ones hidden via CSS — otherwise a
// honeypot field or a hidden legacy input can outrank the real one.
function isVisible(el) {
    if (!el || el.disabled || el.readOnly) return false;
    const style = getComputedStyle(el);
    // parseFloat("") is NaN, and unset opacity computes to "" in some engines
    // (real value 1) — `< 0.1` treats that indeterminate case as visible.
    return style.display !== "none" && style.visibility !== "hidden" &&
        style.visibility !== "collapse" && !(parseFloat(style.opacity) < 0.1);
}

// Split a DOM attribute into lowercase word tokens: camelCase boundaries and
// any non-alphanumeric run separate words.
function tokens(value) {
    if (!value) return [];
    return String(value)
        .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
        .toLowerCase()
        .split(/[^a-z0-9]+/)
        .filter(Boolean);
}

// Text of the <label> elements associated with the input — wrapping label or
// `label[for=...]`. Often the only human-readable name when a framework
// generates opaque ids; `input.labels` covers both association forms.
function labelText(input) {
    if (!input.labels || input.labels.length === 0) return "";
    return Array.from(input.labels).map(label => label.textContent).join(" ");
}

function fieldText(input) {
    return [input.name, input.id, input.placeholder,
        input.getAttribute("aria-label"), input.getAttribute("autocomplete"),
        input.getAttribute("aria-describedby")]
        .filter(Boolean).join(" ");
}

function matchScore(text, full, half) {
    let score = 0;
    for (const token of tokens(text)) {
        if (USERNAME_ROOTS.some(root => token.startsWith(root))) score += full;
        else if (USERNAME_ROOTS.some(root => token.includes(root))) score += half;
    }
    return score;
}

function isExcluded(text) {
    return tokens(text).some(token =>
        EXCLUDED_PREFIXES.some(prefix => token.startsWith(prefix)) ||
        EXCLUDED_TOKENS.includes(token));
}

// A form whose own id/name/class/action reads as a non-login form (a
// newsletter signup, for example). Only consulted on the document-wide
// multi-step path: there the scope is every field on the page, so a footer
// signup field would otherwise be filled with the stored username.
function formIsNonLogin(input) {
    const form = input.form;
    if (!form) return false;
    return isExcluded([form.id, form.name, form.className,
        form.getAttribute("action")].filter(Boolean).join(" "));
}

// Scoring, not boolean matching: autocomplete is the strongest signal, then
// name/id, then human-facing hints. `required` only breaks ties — many login
// forms never set it, and setting it is a validation choice, not an identity
// signal. An exclusion outweighs any combination of positives.
function scoreUsernameInput(input) {
    const identity = [input.name, input.id].filter(Boolean).join(" ");
    const hints = [
        input.placeholder,
        input.getAttribute("aria-label"),
        input.getAttribute("data-testid"),
        input.getAttribute("aria-describedby"),
        labelText(input),
    ].filter(Boolean).join(" ");
    const autocomplete = tokens(input.getAttribute("autocomplete"));
    const type = (input.type || "").toLowerCase();

    let score = matchScore(identity, 60, 25) + matchScore(hints, 30, 12);
    if (autocomplete.includes("username")) score += 100;
    if (autocomplete.includes("email")) score += 80;
    // `type="email"` is a login-identifier signal on its own: providers
    // routinely leave name/id empty and mark the field only this way.
    if (type === "email") score += 30;
    if (isExcluded(identity) || isExcluded(hints)) score -= 200;
    // Applied after the exclusions, and only when a positive signal already
    // matched: on its own `required` made any lone required text field (an
    // OTP box) eligible for the username.
    if (score > 0 && (input.required || input.getAttribute("aria-required") === "true")) score += 10;
    return score;
}

function bestUsernameCandidate(candidates) {
    let best = null;
    let bestScore = 0;
    for (const input of candidates) {
        const score = scoreUsernameInput(input);
        if (score > bestScore) {
            best = input;
            bestScore = score;
        }
    }
    return best;
}

// Finds the best username-like field within `scope` (typically a <form> that
// already contains a password field). When nothing scores, the first
// non-excluded visible candidate is used because the scope is already
// narrowed to one form; a search/captcha/OTP field never is, so it returns
// null and the caller fills nothing.
function findUsernameInput(scope) {
    const candidates = Array.from(scope.querySelectorAll(USERNAME_TEXT_INPUT_SELECTOR)).filter(isVisible);
    return bestUsernameCandidate(candidates) ||
        candidates.find(input => scoreUsernameInput(input) >= 0) || null;
}

// Finds a username-only field anywhere on the page, for multi-step logins
// (Google/Microsoft/SSO-style) whose first screen has no password field yet.
// Unlike findUsernameInput, this never falls back to "first visible field":
// the scope here is the whole document, so a search box, a newsletter signup
// field or an OTP box would otherwise get filled instead.
function findUsernameOnlyInput() {
    const candidates = Array.from(document.querySelectorAll(USERNAME_TEXT_INPUT_SELECTOR))
        .filter(input => isVisible(input) && !formIsNonLogin(input));
    return bestUsernameCandidate(candidates);
}

// Which password of a form this is: the field holding the account password,
// or a new/confirmation field that must not receive a stored password.
function passwordRole(input) {
    const autocomplete = (input.getAttribute("autocomplete") || "").toLowerCase();
    if (autocomplete.includes("current-password")) return "current";
    if (autocomplete.includes("new-password")) return "new";
    const words = tokens([input.name, input.id, input.placeholder, input.getAttribute("aria-label")]
        .filter(Boolean).join(" "));
    if (words.some(word => PASSWORD_CURRENT_HINTS.some(hint => word.startsWith(hint)))) return "current";
    if (words.some(word => PASSWORD_NEW_HINTS.some(hint => word.startsWith(hint)))) return "new";
    return "unknown";
}

// A password-typed field that is not a stored-login target: PINs, one-time
// codes and card fields are routinely `type="password"` for masking, and a
// vault password must never be written into them.
function isExcludedPasswordInput(input) {
    return isExcluded(fieldText(input));
}

// The password field a stored login should be written into: an explicit
// current-password field, else the first field that is not a new/confirm
// field, else the first usable field (sign-up-only forms). One target per
// scope, so a confirmation box is never overwritten with the stored password.
// `scope` may be a container or the input itself; null means filling nothing.
function findPasswordInput(scope) {
    const inputs = scope.matches && scope.matches('input[type="password"]')
        ? [scope]
        : Array.from(scope.querySelectorAll('input[type="password"]'));
    const usable = inputs.filter(input => isVisible(input) && !isExcludedPasswordInput(input));
    if (usable.length === 0) return null;
    return usable.find(input => passwordRole(input) === "current") ||
        usable.find(input => passwordRole(input) !== "new") ||
        usable[0];
}

// Credentials present in `form` at submit time. The password is the value of
// the last non-empty password input: on registration and change-password
// forms that is the freshly chosen (or confirmed) password, not the old one.
// Excluded fields (PIN, one-time code, card) are ignored, so a form that only
// has those reports no password and the background offers no save.
function findSubmittedCredentials(form) {
    let password = '';
    for (const input of form.querySelectorAll('input[type="password"]')) {
        if (isExcludedPasswordInput(input)) continue;
        if (input.value) password = input.value;
    }

    const usernameInput = findUsernameInput(form);
    const username = usernameInput ? usernameInput.value.trim() : '';

    return { username, password };
}
