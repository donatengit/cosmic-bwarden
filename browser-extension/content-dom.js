// Shared DOM primitives for the content scripts: visibility, shadow-DOM
// traversal, form scoping and value writes. Loaded before
// content-heuristics.js in the manifest; every content script shares one
// isolated world, so these are plain globals.

// Mirrors (a lightweight version of) Bitwarden's viewable-element check.
// Checks the whole ancestor chain, crossing open shadow boundaries: a
// descendant of a `display: none` wrapper still computes `display: block` on
// itself, so testing the element alone hides nothing — and a hidden duplicate
// form would then outrank the real one. `readOnly` is deliberately not a
// rejection: readonly blocks typing, not script assignment, and sites use it
// as an anti-autofill trick that would otherwise make the field invisible to
// the heuristics.
function isVisible(el) {
    if (!el || el.disabled) return false;
    if (el.type === 'hidden') return false;
    for (let node = el; node; node = node.parentElement || node.getRootNode().host) {
        const style = getComputedStyle(node);
        // parseFloat("") is NaN, and unset opacity computes to "" in some
        // engines (real value 1) — `< 0.1` treats that case as visible.
        if (style.display === 'none' || style.visibility === 'hidden' ||
            style.visibility === 'collapse' || parseFloat(style.opacity) < 0.1) {
            return false;
        }
    }
    return true;
}

// Every element matching `selector` under `root`, descending into open shadow
// roots. `querySelectorAll` stops at a shadow boundary, so a login form
// rendered by a web component is invisible to a plain document query.
function collectElements(root, selector) {
    const out = [];
    if (!root) return out;
    const visit = (node) => {
        if (node.matches && node.matches(selector)) out.push(node);
        if (!node.querySelectorAll) return;
        for (const el of node.querySelectorAll(selector)) out.push(el);
        // Only elements can host a shadow root; the page's own shadow roots
        // are open, or nothing could reach them.
        for (const el of node.querySelectorAll('*')) {
            if (el.shadowRoot) visit(el.shadowRoot);
        }
    };
    visit(root);
    return out;
}

// The form that owns `el`, including one associated through the `form`
// attribute (valid HTML, used by SPAs that keep the email field in a header
// component). `closest('form')` stops at a shadow boundary, so it is only a
// fallback for the cases where `el.form` is null.
function formOf(el) {
    if (!el) return null;
    if (el.form) return el.form;
    return el.closest ? el.closest('form') : null;
}

// The tree an element lives in: its shadow root, or the document. Used to
// scope "which username belongs to this password" when there is no form.
function treeRootOf(el) {
    const root = el && el.getRootNode ? el.getRootNode() : document;
    return root && root.nodeType === 11 ? root : document;
}

// Write a value the way a keystroke would. The native prototype setter is the
// point: React installs a per-element `value` tracker, so a plain
// `el.value = x` updates the DOM but not React's snapshot — its onChange never
// fires and the next render or submit sends the old value. Calling the
// prototype setter bypasses the instance tracker, so the framework sees a real
// change.
//
// The events are `composed` because a real user input event is: a framework
// root outside the field's shadow root (React attaches its listener to the
// container it rendered into) never sees a non-composed event, so the value
// would land in the DOM and be ignored by the app.
function setInputValue(el, value) {
    const proto = el.tagName === 'TEXTAREA'
        ? HTMLTextAreaElement.prototype
        : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(proto, 'value').set;
    setter.call(el, value);
    el.dispatchEvent(new Event('input', { bubbles: true, composed: true }));
    el.dispatchEvent(new Event('change', { bubbles: true, composed: true }));
}
