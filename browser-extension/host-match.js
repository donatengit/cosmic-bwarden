// Host matching shared by the popup (does the active tab match this entry?)
// and the content script (does this frame match this entry?). One
// implementation, because the content-script side decides whether a plaintext
// password may be written into a frame; two drifting copies is how a fill ends
// up in the wrong origin.

// Exact host, or one host nested under the other at a label boundary.
// `notexample.com` never matches `example.com`.
function hostMatches(a, b) {
    if (!a || !b) return false;
    const x = String(a).toLowerCase();
    const y = String(b).toLowerCase();
    if (x === y) return true;
    return x.endsWith('.' + y) || y.endsWith('.' + x);
}

// True when `host` matches any host in `hosts`. An empty `host` (file://,
// about:blank, a sandboxed frame) matches only when the entry itself has no
// host: a frame whose origin cannot be named must never receive a password
// for a real site.
function hostMatchesAny(host, hosts) {
    const list = (hosts || []).map((h) => String(h || '').toLowerCase()).filter(Boolean);
    const self = String(host || '').toLowerCase();
    if (!self) return list.length === 0;
    return list.some((h) => hostMatches(self, h));
}
