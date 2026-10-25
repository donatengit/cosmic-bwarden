// Vault-list rendering: one row per entry, with the type-appropriate row
// action (Fill + copy menu for Logins, details for everything else).
//
// Loaded before popup.js (see popup.html), like the other feature modules:
// only function bodies call out, so the position only has to be ahead of the
// first render. It reads popup.js's `listIsDomainMatch` (set by
// updateResults) and calls showDetail (popup-detail.js), fillEntry
// (popup-fill.js), openEntrySite/makeCopyDropdownBtn (popup-list-actions.js)
// and setIcon (icons.js).

const resultsDiv = document.getElementById('results');
function renderEntries(entries, caption = null) {
    resultsDiv.innerHTML = '';
    if (entries.length === 0) {
        resultsDiv.innerHTML = '<div class="no-results">No entries found</div>';
        return;
    }
    if (caption) {
        const capEl = document.createElement('div');
        capEl.className = 'list-caption';
        capEl.textContent = caption;
        resultsDiv.appendChild(capEl);
    }

    for (const entry of entries) {
        const div = document.createElement('div');
        div.className = 'entry';
        // Login entries: clicking the row launches the site (its primary
        // purpose) — unless the list is the current tab's domain match, in
        // which case the click autofills the form. Other types have no URL to
        // open, so fall back to detail.
        div.onclick = () => {
            if (entry.entry_type === 'Login') {
                if (listIsDomainMatch) fillEntry(entry.id);
                else openEntrySite(entry.id);
            } else {
                showDetail(entry.id);
            }
        };

        const info = document.createElement('div');
        info.className = 'entry-info';
        const nameEl = document.createElement('div');
        nameEl.className = 'entry-name';
        nameEl.textContent = entry.name;
        const subEl = document.createElement('div');
        subEl.className = 'entry-user';
        subEl.textContent = entry.username || entry.entry_type;
        info.append(nameEl, subEl);

        const actions = document.createElement('div');
        actions.className = 'entry-actions';

        if (entry.entry_type === 'Login') {
            const fillBtn = document.createElement('button');
            fillBtn.textContent = 'Fill';
            fillBtn.className = 'btn btn-sm';
            fillBtn.title = 'Autofill';
            fillBtn.onclick = e => { e.stopPropagation(); fillEntry(entry.id); };
            actions.append(fillBtn, makeCopyDropdownBtn(entry));
        }

        const viewIconBtn = document.createElement('button');
        viewIconBtn.className = 'btn-icon';
        setIcon(viewIconBtn, 'eye');
        viewIconBtn.title = 'View details';
        viewIconBtn.setAttribute('aria-label', 'View details');
        viewIconBtn.onclick = e => { e.stopPropagation(); showDetail(entry.id); };

        actions.append(viewIconBtn);
        div.append(info, actions);
        resultsDiv.appendChild(div);
    }
}
