/* Applies a URL-owned page size (shared PaginationLinks) as soon as one is
   picked, like the interactive tables do. Without JavaScript the visible Apply
   button stays the submitter. Arrow keys on a closed select fire a change per
   step, so keyboard changes wait for Enter. */
(() => {
  const selector = 'form[data-pagination-form] select[data-pagination-page-size]';
  document.documentElement.setAttribute('data-pagination-auto-apply', '');
  const lastKey = new WeakMap();
  const apply = select => {
    const form = select.form;
    if (!form) return;
    form.requestSubmit();
  };
  document.addEventListener('keydown', event => {
    const select = event.target;
    if (!(select instanceof HTMLSelectElement) || !select.matches(selector)) return;
    lastKey.set(select, Date.now());
    if (event.key === 'Enter') {
      event.preventDefault();
      apply(select);
    }
  });
  document.addEventListener('change', event => {
    const select = event.target;
    if (!(select instanceof HTMLSelectElement) || !select.matches(selector)) return;
    if (Date.now() - (lastKey.get(select) ?? 0) < 1000) return;
    apply(select);
  });
})();
