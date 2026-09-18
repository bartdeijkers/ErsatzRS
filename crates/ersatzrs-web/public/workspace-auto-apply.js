/* Applies the Items filter category and page size as soon as one is picked.
   The native Search and Apply buttons stay the submitters, so the whole draft
   is posted exactly as without JavaScript. Arrow keys on a closed select fire
   a change per step, so keyboard changes wait for Enter or the visible button. */
(() => {
  const form = document.getElementById('addon-list-workspace');
  if (!form) return;
  const submitters = { 'workspace-category': 'filter', 'workspace-page_size': 'page-size' };
  const refocusKey = 'ersatzrs-workspace-refocus';
  const lastKey = new WeakMap();
  const apply = select => {
    const button = form.ownerDocument.querySelector(
      `button[form="${form.id}"][name="intent"][value="${submitters[select.id]}"]`);
    if (!button || button.disabled) return;
    try { sessionStorage.setItem(refocusKey, select.id); } catch { /* focus is a convenience */ }
    form.requestSubmit(button);
  };
  document.addEventListener('keydown', event => {
    const select = event.target;
    if (!(select instanceof HTMLSelectElement) || !(select.id in submitters)) return;
    lastKey.set(select, Date.now());
    if (event.key === 'Enter') {
      event.preventDefault();
      apply(select);
    }
  });
  document.addEventListener('change', event => {
    const select = event.target;
    if (!(select instanceof HTMLSelectElement) || !(select.id in submitters)) return;
    if (Date.now() - (lastKey.get(select) ?? 0) < 1000) return;
    apply(select);
  });
  // Return focus to the control that applied the change; an error summary
  // keeps its own focus.
  let refocus = null;
  try {
    refocus = sessionStorage.getItem(refocusKey);
    sessionStorage.removeItem(refocusKey);
  } catch { /* storage can be unavailable */ }
  const target = refocus && document.getElementById(refocus);
  if (target && !document.getElementById('workspace-errors')) target.focus();
})();
