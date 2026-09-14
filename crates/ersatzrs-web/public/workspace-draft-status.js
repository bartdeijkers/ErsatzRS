/* Display-only comparison of the native task form. Never stores or submits a draft. */
(() => {
  const status = document.querySelector('[data-workspace-save-status]');
  if (!status) return;
  const form = document.getElementById('addon-list-workspace');
  if (!form) return;
  const retained = status.dataset.retained === 'true';
  const changed = (element, baselines) => {
    const name = element.name;
    if (!name || !(name === 'delivery_mode' || name.startsWith('option_') || baselines.has(name))) return false;
    if (element instanceof HTMLInputElement) {
      if (element.type === 'hidden' || element.type === 'submit') return false;
      if (element.type === 'checkbox' || element.type === 'radio') return element.checked !== element.defaultChecked;
      return element.value !== element.defaultValue;
    }
    if (element instanceof HTMLTextAreaElement) return element.value !== element.defaultValue;
    if (element instanceof HTMLSelectElement) {
      const original = Array.from(element.options).findIndex(option => option.defaultSelected);
      return element.selectedIndex !== Math.max(0, original);
    }
    return false;
  };
  const update = () => {
    const filter = document.getElementById('workspace-category');
    const label = document.querySelector('.selected-filter-label');
    if (filter && label) label.textContent = filter.selectedOptions[0]?.textContent || '';
    // Index the existing native baselines once, including on large All pages.
    const elements = Array.from(form.elements);
    const baselines = new Set(elements.filter(element => element.name?.startsWith('baseline_'))
      .map(element => element.name.slice('baseline_'.length)));
    const dirty = retained || elements.some(element => changed(element, baselines));
    status.textContent = dirty ? status.dataset.unsaved : status.dataset.saved;
    status.dataset.dirty = String(dirty);
  };
  document.addEventListener('input', update);
  document.addEventListener('change', update);
  update();
})();
