// Offline controls for the design prototype. Only the fictional channel is stored.
(() => {
  const byId = id => document.getElementById(id);
  // The live admin selects its theme server-side. The offline review defaults
  // to the observed dark shell and accepts ?theme=light for the other palette.
  if (new URLSearchParams(location.search).get('theme') === 'light') {
    document.documentElement.dataset.theme = 'light';
  }
  function setupDrawer() {
    const root = document.documentElement;
    const drawer = byId('admin-navigation-drawer');
    const toggle = byId('nav-toggle');
    const desktop = window.matchMedia('(min-width: 768px)');
    function setOpen(open) {
      if (!open && drawer.contains(document.activeElement)) toggle.focus();
      drawer.inert = !open;
      root.dataset.navDrawerOpen = String(open);
      toggle.setAttribute('aria-expanded', String(open));
    }
    root.dataset.jsEnabled = '';
    setOpen(desktop.matches);
    toggle.addEventListener('click', () => setOpen(drawer.inert));
    desktop.addEventListener('change', () => setOpen(desktop.matches));
    document.addEventListener('keydown', event => {
      if (event.key === 'Escape' && !desktop.matches && !drawer.inert && !document.querySelector('dialog[open]')) {
        setOpen(false);
        toggle.focus();
      }
    });
  }
  const tabs = [...document.querySelectorAll('.route-tabs a')];
  const sections = [...document.querySelectorAll('.atlas-panes > section')];
  const catalog = {
    200: 'Guide index', 201: 'Today', 202: 'Today', 203: 'Today', 310: 'Station notes',
    320: 'Harbour notices', 330: 'Coastal weather',
  };
  const defaultSource = {201: '01', 202: '02', 203: '03'};
  const channels = {
    '01': {name: 'Northline', early: [['18:00', 'Evening bulletin'], ['18:30', 'Coast to coast'], ['20:15', 'Night signals']], late: [['21:00', 'Harbour lights'], ['22:00', 'The last ferry']]},
    '02': {name: 'Coastline', early: [['17:30', 'Harbour report'], ['18:45', 'Island kitchen'], ['20:30', 'Coastal paths']], late: [['21:30', 'Night crossing'], ['22:45', 'At the quay']]},
    '03': {name: 'Horizon', early: [['17:45', 'City journal'], ['19:00', 'Open studio'], ['20:45', 'After hours']], late: [['21:45', 'Last train'], ['23:00', 'Midnight music']]},
  };
  // This offline catalog models library elements and templates supplied by the enabled add-on.
  const templates = {
    index: {label: 'Guide index', owner: 'TV guide add-on', sheet: '200', fields: []},
    guide: {label: 'Programme guide', owner: 'TV guide add-on', sheet: '201', fields: ['source', 'period']},
    station: {label: 'Station card', owner: 'User library', sheet: '310', fields: ['message']},
    notice: {label: 'Text bulletin', owner: 'User library', sheet: '320', fields: ['message']},
  };
  const periods = {full: 'Whole evening', early: 'Early evening', late: 'Late evening'};
  const colourDefaults = {red: 'Index', green: 'Today', yellow: 'Late', blue: 'Notes'};
  const colourLinks = document.querySelectorAll('.fastext [data-colour]');
  const defaultCycleSeconds = 10;
  const indexEntriesPerSheet = 8;
  const isGuidePage = item => templates[item.template].owner === 'TV guide add-on';
  const indexSheetCount = pageCount => Math.max(1, Math.ceil((pageCount - 1) / indexEntriesPerSheet));
  const sampleMessages = {310: 'Broadcasting from the coast.', 320: 'The evening ferry leaves at 21:30.', 330: 'Light winds from the west.'};
  const palette = {Blue: '#0000ff', Red: '#ff0000', Green: '#00ff00', Cyan: '#00ffff', Yellow: '#ffff00', Magenta: '#ff00ff', White: '#ffffff'};
  const defaultTheme = {banner: 'GUIDE', band: 'Blue', accent: 'Green'};
  const themeKey = 'ersatzrs.design.page-atlas.library-theme.v1';
  function readTheme(value) {
    if (!value || !Object.hasOwn(palette, value.band) || !Object.hasOwn(palette, value.accent)
        || typeof value.banner !== 'string' || !value.banner.trim() || value.banner.length > 14) return defaultTheme;
    return {banner: value.banner, band: value.band, accent: value.accent};
  }
  let libraryTheme = defaultTheme;
  try { libraryTheme = readTheme(JSON.parse(localStorage.getItem(themeKey))); } catch { /* Local session defaults. */ }
  function legacyTemplate(id) {
    if (defaultSource[id]) return 'guide';
    if (id === '200') return 'index';
    return id === '310' ? 'station' : 'notice';
  }
  // Built-in slots keep their identity when the page's programme range changes.
  // Extra slots follow the built-ins and keep their own settings after renumbering.
  function subpageKeys(item, pageCount) {
    let builtIn = item.template !== 'guide' ? ['base']
      : item.period === 'full' ? ['early', 'late'] : [item.period];
    if (item.template === 'index') {
      const count = indexSheetCount(pageCount ?? Object.keys(working).length);
      // Each required continuation has a stable key for its own title and settings.
      builtIn = Array.from({length: count}, (_, i) => i === 0 ? 'base' : 'index' + (i + 1));
    }
    const shown = builtIn.filter(key => (item.template === 'index' && builtIn.length > 1) || !item.subpages?.[key]?.removed)
      .concat(Object.keys(item.subpages ?? {}).filter(key => key.startsWith('extra')));
    return shown.length ? shown : [builtIn[0]];
  }

  function subpageValues(item, key) {
    const saved = item.subpages?.[key] ?? {};
    const period = saved.period ?? (key === 'late' || item.period === 'late' ? 'late' : 'early');
    const title = saved.title ?? (key === 'late' ? periods.late : item.title);
    const label = saved.title ?? (item.template === 'guide' ? periods[period] : title);
    return {title, label, period, source: saved.source ?? item.source, message: saved.message ?? item.message};
  }

  function readSubpages(item) {
    const records = {};
    for (const [key, values] of Object.entries(item.subpages)) {
      const builtIn = item.template === 'guide' ? ['early', 'late'] : ['base'];
      const continuation = item.template === 'index' && /^index[1-9][0-9]*$/.test(key) && key !== 'index1';
      if (!continuation && !builtIn.includes(key) && !/^extra[1-9][0-9]*$/.test(key)) throw new Error('Invalid subpage');
      const record = {};
      for (const [field, value] of Object.entries(values)) {
        if (field === 'removed' && value === true && builtIn.includes(key)) {
          record.removed = true;
          continue;
        }
        const allowed = field === 'title' || templates[item.template].fields.includes(field);
        if (!allowed || typeof value !== 'string' || !value.trim()
            || (field === 'source' && !Object.hasOwn(channels, value))
            || (field === 'period' && !['early', 'late'].includes(value))
            || (field === 'title' && value.length > 34) || (field === 'message' && value.length > 40)) {
          throw new Error('Invalid subpage setting');
        }
        record[field] = value;
      }
      records[key] = record;
    }
    return records;
  }

  function configuredPage(id, item) {
    const key = item.template ?? legacyTemplate(id);
    if (!Object.hasOwn(templates, key)) throw new Error('Unknown template');
    const cycleSeconds = item.cycleSeconds ?? defaultCycleSeconds;
    if (!Number.isInteger(cycleSeconds) || cycleSeconds < 1 || cycleSeconds > 600) throw new Error('Invalid subpage interval');
    const record = {number: String(item.number), title: item.title, template: key, cycleSeconds};
    for (const field of templates[key].fields) {
      if (field === 'source') {
        record.source = item.source ?? defaultSource[id] ?? '01';
        if (!Object.hasOwn(channels, record.source)) throw new Error('Unknown source');
      } else if (field === 'period') {
        record.period = item.period ?? 'full';
        if (!Object.hasOwn(periods, record.period)) throw new Error('Invalid guide period');
      } else {
        record.message = item.message ?? sampleMessages[id] ?? '';
        if (!record.message.trim() || record.message.length > 40) throw new Error('Invalid bulletin');
      }
    }
    if (item.colourLinks) {
      const links = {};
      for (const [colour, target] of Object.entries(item.colourLinks)) {
        if (!Object.hasOwn(colourDefaults, colour) || typeof target !== 'string') throw new Error('Invalid colour link');
        links[colour] = target;
      }
      if (Object.keys(links).length) record.colourLinks = links;
    }
    if (item.subpages) record.subpages = readSubpages(item);
    return record;
  }

  function carrySubpages(next, original) {
    if (!original?.subpages) return next;
    if (original.template === next.template) {
      next.subpages = {...original.subpages};
      if (original.period !== next.period) {
        for (const key of subpageKeys({...next, subpages: {}})) {
          if (next.subpages[key]?.removed) delete next.subpages[key];
        }
      }
    } else {
      // Keep authored sheets in order, carrying only fields the new template accepts.
      const slots = subpageKeys(next);
      next.subpages = {};
      const authored = subpageKeys(original).filter(key => !key.startsWith('index') || original.subpages[key]);
      for (const [i, oldKey] of authored.entries()) {
        const key = slots[i] ?? 'extra' + (i + 1 - slots.length);
        const fields = original.subpages[oldKey] ?? {};
        const record = Object.fromEntries(Object.entries(fields).filter(([field]) =>
          field === 'title' || templates[next.template].fields.includes(field)));
        if (Object.keys(record).length || !slots.includes(key)) next.subpages[key] = record;
      }
    }
    if (!Object.keys(next.subpages).length) delete next.subpages;
    return next;
  }
  const index = document.querySelector('.page-index');
  const sheets = [...document.querySelectorAll('.sheet')];
  const pageInput = byId('page-number');
  const guideToggle = byId('guide-enabled');
  const saveStatus = byId('save-status');
  const saveHint = saveStatus.textContent;
  const storageKey = 'ersatzrs.design.page-atlas.northline.pages.v2';
  const initial = {
    guideEnabled: true, selectedGuideEnabled: true,
    pages: {200: 'Guide index', 201: 'Today', 202: 'Today', 310: 'Station notes'}, revision: 8,
    selectedRevision: 7, generation: 7, selectedPages: {200: 'Guide index', 201: 'Today', 202: 'Today', 310: 'Station notes'},
  };
  // Older local drafts store just the title; the catalog key was their page number.
  function readPages(records) {
    const pages = {};
    const numbers = new Set();
    for (const [id, record] of Object.entries(records)) {
      const item = typeof record === 'string' ? { number: id, title: record } : record;
      if ((!Object.hasOwn(catalog, id) && !/^n[1-9][0-9]*$/.test(id)) || typeof item?.title !== 'string' || !item.title.trim()
          || !/^[1-8][0-9]{2}$/.test(item.number) || numbers.has(String(item.number))) {
        throw new Error('Invalid local page set');
      }
      pages[id] = configuredPage(id, item);
      numbers.add(pages[id].number);
    }
    for (const item of Object.values(pages)) {
      if (Object.values(item.colourLinks ?? {}).some(id => id !== '' && !Object.hasOwn(pages, id))) {
        throw new Error('Unknown colour link destination');
      }
    }
    return pages;
  }
  let model = { ...initial, pages: readPages(initial.pages), selectedPages: readPages(initial.selectedPages) };
  let storedChannel = null;
  try {
    storedChannel = localStorage.getItem(storageKey);
    const saved = JSON.parse(storedChannel);
    if (saved && [saved.revision, saved.generation, saved.selectedRevision].every(Number.isSafeInteger)
        && saved.pages && saved.selectedPages) {
      model = {
        guideEnabled: saved.guideEnabled !== false, selectedGuideEnabled: saved.selectedGuideEnabled !== false,
        revision: saved.revision, pages: readPages(saved.pages), theme: readTheme(saved.theme),
        generation: saved.generation, selectedRevision: saved.selectedRevision,
        selectedPages: readPages(saved.selectedPages), selectedTheme: readTheme(saved.selectedTheme),
      };
    }
  } catch { /* Private browsing may restrict storage for local files. */ }

  let lastPage = '#p200';
  let published = false;
  let page = '200';
  let subpage = 1;
  let freshEntry = true;
  const working = { ...model.pages };
  let guideEnabled = model.guideEnabled;
  guideToggle.disabled = false;
  const pendingNumbers = new Map();
  const dirtyDraft = () => guideEnabled !== model.guideEnabled || pendingNumbers.size > 0 || JSON.stringify(model.pages) !== JSON.stringify(working)
    || (Object.values(working).some(item => templates[item.template].owner === 'TV guide add-on')
      && JSON.stringify(readTheme(model.theme)) !== JSON.stringify(libraryTheme));
  const rows = new Map();
  const cycleButton = byId('cycle-preview');
  const progressLine = byId('cycle-progress');
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
  let cycleFrame = null;
  let remainingMs = 0;
  let cycleDeadline = 0;
  const dialog = byId('page-dialog');
  let operation = '';
  let editing = '';
  const pad = value => String(value).padStart(2, '0');
  function visiblePages() {
    if (!published) return working;
    if (model.selectedGuideEnabled) return model.selectedPages;
    return Object.fromEntries(Object.entries(model.selectedPages).filter(([, item]) => !isGuidePage(item)));
  }
  const pageHash = (id, part = 1) =>
    '#' + (published ? 'g' : 'p') + (published ? model.selectedPages : working)[id].number + (part > 1 ? '-' + part : '');
  const orderedIds = pages => Object.keys(pages).sort((a, b) => Number(pages[a].number) - Number(pages[b].number));
  const lookup = (pages, number) => Object.keys(pages).find(id => pages[id].number === number);

  function persist(next) {
    try {
      if (localStorage.getItem(storageKey) !== storedChannel) {
        saveStatus.textContent = 'Het kanaal is in een ander tabblad gewijzigd. Herlaad de pagina om de nieuwste editie te openen.';
        return false;
      }
      if (next !== model) {
        const snapshot = JSON.stringify(next);
        localStorage.setItem(storageKey, snapshot);
        storedChannel = snapshot;
        model = next;
      }
      return true;
    } catch {
      saveStatus.textContent = 'Publiceren mislukt. Sta lokale browseropslag toe en probeer opnieuw.';
      return false;
    }
  }

  function go(hash) {
    if (location.hash !== hash) history.pushState(null, '', hash);
    render();
  }

  function paintTheme(element, theme) {
    element.style.setProperty('--guide-band', palette[theme.band]);
    element.style.setProperty('--guide-ink', theme.band === 'Blue' ? 'white' : 'black');
    element.style.setProperty('--guide-accent', palette[theme.accent]);
  }

  function renderColourLinks(pages, available) {
    const item = pages[page];
    const guide = item?.template === 'guide' ? page : available.find(id => pages[id].template === 'guide');
    const defaults = {
      red: available.find(id => pages[id].template === 'index'),
      green: guide, yellow: guide,
      blue: available.find(id => ['station', 'notice'].includes(pages[id].template)),
    };
    for (const link of colourLinks) {
      const colour = link.dataset.colour;
      const override = item?.colourLinks?.[colour];
      const id = override ?? defaults[colour];
      const target = pages[id];
      const late = override === undefined && colour === 'yellow';
      const part = target && late
        ? subpageKeys(target, available.length).findIndex(key => subpageValues(target, key).period === 'late') + 1
        : 1;
      const name = colour[0].toUpperCase() + colour.slice(1);
      if (target && part) {
        const automatic = override === undefined;
        link.textContent = automatic ? colourDefaults[colour] : target.number;
        link.href = pageHash(id, part);
        link.title = (automatic ? name + ' · ' + colourDefaults[colour] : name) + ' · ' + target.number + ' · ' + target.title
          + (part > 1 ? ' · subpage ' + part : '');
        link.removeAttribute('aria-disabled');
      } else {
        link.textContent = '—';
        link.removeAttribute('href');
        link.setAttribute('aria-disabled', 'true');
        link.title = name + ' · No linked page';
      }
      link.setAttribute('aria-label', link.title);
    }
  }

  function updateReader(hash, automatic = false) {
    const match = /^#([pg])([0-9]{3})(?:-([1-9][0-9]*))?$/.exec(hash);
    published = match?.[1] === 'g';
    const pages = visiblePages();
    const available = orderedIds(pages);
    const previousPage = page;
    const previousSubpage = subpage;
    page = lookup(pages, match?.[2]) ?? available[0];
    const number = pages[page]?.number ?? '';
    const item = pages[page];
    const template = templates[item?.template];
    const keys = item ? subpageKeys(item, available.length) : [];
    const multiple = keys.length > 1;
    const requested = Number(match?.[3] ?? 1);
    subpage = match?.[2] === number && requested <= keys.length ? requested : 1;
    if (!automatic && subpage > 1 && (page !== previousPage || subpage !== previousSubpage)) {
      rows.get(page).querySelector('.subpage-group').open = true;
    }
    const content = item && subpageValues(item, keys[subpage - 1]);
    const nextPage = page ? pageHash(page, subpage) : (published ? '#g200' : '#p200');
    if (nextPage !== lastPage) setReveal(false);
    lastPage = nextPage;
    byId('add-pages').hidden = published;
    byId('working-pages').hidden = !published;
    byId('empty-reader').hidden = Boolean(page);
    byId('empty-reader').textContent = published ? 'No pages in the published channel edition.' : 'No pages in this channel draft.';
    byId('guide-off').hidden = published || guideEnabled || !item || !isGuidePage(item);
    document.querySelector('.teletext-page').hidden = !page;
    document.querySelector('.screen figcaption').hidden = !page;
    byId('remote').hidden = !page;
    byId('reveal').disabled = item?.template !== 'index';
    byId('reveal').title = item?.template === 'index' ? 'Reveal concealed text' : 'This page has no concealed text';
    const part = subpage + '/' + keys.length;
    const late = item?.template === 'guide' && content.period === 'late';
    const screen = document.querySelector('.teletext-page');
    const themed = template?.owner === 'TV guide add-on';
    const theme = themed ? (published ? readTheme(model.selectedTheme) : libraryTheme) : defaultTheme;
    screen.classList.toggle('guide-themed', themed);
    paintTheme(screen, theme);
    screen.querySelector('.tx-mast span').textContent = themed ? theme.banner : 'TEXT';
    for (const sheet of sheets) {
      sheet.hidden = !template || sheet.id !== 'p' + template.sheet + (late ? '-2' : '');
      if (sheet.hidden) continue;
      const heading = sheet.querySelector('h2');
      heading.textContent = content.title;
      if (item.template === 'guide') {
        const channel = channels[content.source];
        heading.textContent = content.title + ' · ' + channel.name;
        const listings = late ? channel.late : channel.early;
        sheet.querySelectorAll('.tx-time').forEach((line, i) => {
          const [time, programme] = listings[i];
          const clock = line.querySelector('b');
          clock.textContent = published && model.selectedRevision === 7 && content.source === '01' && time === '20:15' ? '20:00' : time;
          line.replaceChildren(clock, ' ' + programme);
        });
        for (const el of sheet.querySelectorAll('h3,hr,.tx-entry')) el.hidden = !multiple;
        if (multiple) {
          const target = subpage < keys.length ? subpage + 1 : subpage - 1;
          const entry = sheet.querySelector('.tx-entry');
          const direction = sheet.querySelector('h3');
          if (direction) direction.textContent = target > subpage ? 'Next subpage' : 'Previous subpage';
          entry.querySelector('span').textContent = subpageValues(item, keys[target - 1]).label;
          const link = entry.querySelector('a');
          link.href = pageHash(page, target);
          link.textContent = target + '/' + keys.length;
        }
      } else if (content.message) {
        sheet.querySelector('p').textContent = content.message;
      } else if (item.template === 'index') {
        const list = byId('reader-index-entries');
        list.replaceChildren();
        const start = (subpage - 1) * indexEntriesPerSheet;
        for (const id of available.filter(id => id !== page).slice(start, start + indexEntriesPerSheet)) {
          const target = pages[id];
          const line = document.createElement('div');
          line.className = 'tx-entry';
          const label = document.createElement('span');
          label.textContent = target.title + (target.source ? ' · ' + channels[target.source].name : '');
          label.title = label.textContent;
          const link = document.createElement('a');
          link.href = pageHash(id);
          link.textContent = target.number;
          line.append(label, link);
          list.append(line);
        }
      }
      sheet.setAttribute('aria-label', 'Page ' + item.number + ', ' + heading.textContent);
    }
    for (const [id, row] of rows) {
      row.hidden = !(id in pages);
      if (row.hidden) continue;
      const item = pages[id];
      row.querySelector('a').href = pageHash(id);
      row.querySelector('.row-title').textContent = item.title;
      const sourceName = row.querySelector('.row-source');
      sourceName.textContent = item.source ? channels[item.source].name : '';
      const input = row.querySelector('input');
      input.readOnly = published;
      const error = byId('number-error-' + id);
      error.hidden = published;
      if (!published && error.textContent) input.setAttribute('aria-invalid', 'true');
      else input.removeAttribute('aria-invalid');
      input.setAttribute('aria-label', 'Page number for ' + item.title + (item.source ? ' · ' + channels[item.source].name : ''));
      input.value = published ? item.number : (pendingNumbers.get(id) ?? item.number);
      for (const button of row.querySelectorAll('.row-tools button')) {
        button.title = button.dataset.action === 'Add subpage' ? 'Add subpage to ' + item.number : button.dataset.action + ' page ' + item.number;
        button.setAttribute('aria-label', button.title);
      }
      row.querySelector('.row-tools').hidden = published;
      renderSubpageRows(row, id, item, available, pages);
      markIndexSelection(row, id);
    }
    let previous = byId('index-heading');
    const focused = document.activeElement;
    for (const id of available) {
      const row = rows.get(id);
      if (previous.nextElementSibling !== row) index.insertBefore(row, previous.nextElementSibling);
      previous = row;
    }
    if (index.contains(focused) && document.activeElement !== focused) focused.focus({ preventScroll: true });
    if (!automatic && page && (page !== previousPage || subpage !== previousSubpage)) {
      const current = rows.get(page).querySelector('[aria-current]');
      if (current) {
        const selected = current.getBoundingClientRect();
        const scroller = index.getBoundingClientRect();
        const top = scroller.top + byId('index-heading').offsetHeight;
        if (selected.top < top) index.scrollTop += selected.top - top;
        else if (selected.bottom > scroller.bottom) index.scrollTop += selected.bottom - scroller.bottom;
      }
    }
    renderColourLinks(pages, available);
    const home = document.querySelector('.jump a');
    const start = Object.hasOwn(pages, '200') ? '200' : available[0];
    if (start) home.setAttribute('href', pageHash(start));
    home.setAttribute('aria-label', 'Home page ' + (pages[start]?.number ?? ''));
    home.title = home.getAttribute('aria-label');
    byId('page-id').textContent = number;
    byId('subcode').textContent = 'Page ' + number + ' · ' + part;
    cycleButton.hidden = !multiple;
    progressLine.hidden = !multiple;
    if (multiple) byId('cycle-delay').textContent = item.cycleSeconds + ' s';
    if (!published && dirtyDraft()) byId('edition-label').textContent = 'Working draft · unsaved';
    else if (!byId('guide-off').hidden) byId('edition-label').textContent = 'Page preview';
    else if (published || model.revision === model.selectedRevision) byId('edition-label').textContent = 'Published G' + pad(model.generation);
    else byId('edition-label').textContent = 'Draft R' + pad(model.revision);
    for (const time of document.querySelectorAll('#p200 [data-night]')) {
      time.textContent = published && model.selectedRevision === 7 ? '20:00' : '20:15';
    }
    pageInput.value = number;
    freshEntry = true;
    clearError();
    byId('prev-page').disabled = page === available[0];
    byId('next-page').disabled = page === available.at(-1);
    byId('prev-sub').disabled = subpage <= 1;
    byId('next-sub').disabled = subpage >= keys.length;
  }

  function render() {
    stopPreviewCycle();
    const hash = location.hash;
    const tab = hash === '#sources' ? hash : '#home';
    for (const section of sections) section.hidden = '#' + section.id !== tab;
    for (const link of tabs) {
      const active = link.hash === tab;
      link.className = active ? 'tab-active' : 'tab';
      if (active) link.setAttribute('aria-current', 'page');
      else link.removeAttribute('aria-current');
    }
    ensureRows({...model.selectedPages, ...working});
    if (tab === '#home') updateReader(hash === '#home' ? lastPage : hash);
    if (tab === '#sources') renderSourceOverview();
    guideToggle.checked = guideEnabled;
    byId('guide-off-help').hidden = guideEnabled;
    byId('working-pages').setAttribute('href', '#p' + (working['200']?.number ?? working[orderedIds(working)[0]]?.number ?? '200'));
  }

  function showCycleAction(playing) {
    cycleButton.classList.toggle('playing', playing);
    cycleButton.querySelector('use').setAttribute('href', playing ? '#glyph-pause' : '#glyph-play');
  }

  function stopPreviewCycle() {
    if (cycleFrame === null && remainingMs === 0) return;
    cancelAnimationFrame(cycleFrame);
    cycleFrame = null;
    remainingMs = 0;
    progressLine.value = 0;
    showCycleAction(false);
  }

  cycleButton.addEventListener('click', () => {
    if (cycleFrame !== null) {
      remainingMs = Math.max(1, cycleDeadline - performance.now());
      cancelAnimationFrame(cycleFrame);
      cycleFrame = null;
      showCycleAction(false);
      return;
    }
    const pages = visiblePages();
    const item = pages[page];
    const total = subpageKeys(item, Object.keys(pages).length).length;
    const duration = item.cycleSeconds * 1000;
    cycleButton.focus();
    showCycleAction(true);
    progressLine.max = duration;
    cycleDeadline = performance.now() + (remainingMs || duration);
    function tick(now) {
      if (now >= cycleDeadline) {
        const hash = pageHash(page, subpage % total + 1);
        history.replaceState(null, '', hash);
        updateReader(hash, true);
        cycleDeadline = now + duration;
      }
      remainingMs = cycleDeadline - now;
      const elapsed = duration - remainingMs;
      const position = reducedMotion.matches ? Math.floor(elapsed / 1000) * 1000 : elapsed;
      if (progressLine.value !== position) progressLine.value = position;
      cycleFrame = requestAnimationFrame(tick);
    }
    cycleFrame = requestAnimationFrame(tick);
  });
  function pauseOnInteraction(event) {
    if (!cycleButton.contains(event.target)) stopPreviewCycle();
  }
  document.addEventListener('focusin', pauseOnInteraction);
  document.addEventListener('pointerdown', pauseOnInteraction);
  document.addEventListener('visibilitychange', () => { if (document.hidden) stopPreviewCycle(); });
  window.addEventListener('pagehide', stopPreviewCycle);

  function setReveal(show) {
    byId('mystery-guest').hidden = !show;
    byId('conceal-mask').hidden = show;
    byId('preview-reveal').setAttribute('aria-expanded', String(show));
    byId('preview-reveal').setAttribute('aria-label', show ? 'Hide mystery guest ' + byId('mystery-guest').textContent : 'Reveal mystery guest');
    byId('reveal').setAttribute('aria-expanded', String(show));
    byId('reveal').textContent = show ? 'Hide text' : 'Show text';
  }
  function toggleReveal() {
    setReveal(byId('mystery-guest').hidden);
  }
  byId('reveal').addEventListener('click', toggleReveal);
  byId('preview-reveal').addEventListener('click', toggleReveal);
  byId('preview-reveal').disabled = false;

  function clearError() {
    byId('page-error').textContent = '';
    pageInput.removeAttribute('aria-invalid');
  }

  function enterNumber(event) {
    event.preventDefault();
    const value = pageInput.value;
    const id = lookup(visiblePages(), value);
    if (id) {
      go(pageHash(id));
    } else {
      byId('page-error').textContent = 'Page unavailable. Choose a number from the page index.';
      pageInput.setAttribute('aria-invalid', 'true');
      pageInput.focus();
      pageInput.select();
      freshEntry = true;
    }
  }

  byId('remote').addEventListener('submit', enterNumber);
  pageInput.addEventListener('focus', () => pageInput.select());
  let dialBeforeInput;
  pageInput.addEventListener('beforeinput', () => { dialBeforeInput = Number(pageInput.value); });
  pageInput.addEventListener('input', event => {
    const from = dialBeforeInput ?? Number(visiblePages()[page]?.number);
    dialBeforeInput = undefined;
    const delta = pageInput.valueAsNumber - from;
    if (isNativeNumberStep(event) && Number.isFinite(delta) && delta !== 0) {
      adjacent(Math.sign(delta), from);
      return;
    }
    freshEntry = false;
    clearError();
  });
  pageInput.addEventListener('keydown', event => {
    if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
      event.preventDefault();
      adjacent(event.key === 'ArrowUp' ? 1 : -1, Number(pageInput.value));
    }
  });
  document.querySelector('.digits').addEventListener('click', event => {
    const button = event.target.closest('button');
    if (!button || button.id === 'reveal') return;
    if (button.id === 'erase') pageInput.value = pageInput.value.slice(0, -1);
    else pageInput.value = (freshEntry ? '' : pageInput.value).concat(button.textContent).slice(0, 3);
    freshEntry = false;
    clearError();
  });
  function adjacent(direction, from) {
    const pages = visiblePages();
    if (!page) return;
    const start = Number.isFinite(from) ? from : Number(pages[page].number);
    const ids = orderedIds(pages);
    const target = direction > 0
      ? ids.find(id => Number(pages[id].number) > start) ?? ids.at(-1)
      : ids.findLast(id => Number(pages[id].number) < start) ?? ids[0];
    if (target !== page) {
      go(pageHash(target));
    } else {
      pageInput.value = pages[page].number;
      freshEntry = true;
      clearError();
    }
  }
  byId('prev-page').onclick = () => adjacent(-1);
  byId('next-page').onclick = () => adjacent(1);
  byId('prev-sub').onclick = () => go(pageHash(page, subpage - 1));
  byId('next-sub').onclick = () => go(pageHash(page, subpage + 1));

  function changed() {
    updateSaveHint();
    render();
  }

  function updateSaveHint() {
    saveStatus.textContent = dirtyDraft() ? 'Niet-opgeslagen wijzigingen' : saveHint;
  }

  function openOperation(kind, id = '') {
    operation = kind;
    editing = id;
    byId('dialog-title').textContent = {
      add: 'Add pages to channel', edit: 'Edit page ' + working[id]?.number, remove: 'Remove page ' + working[id]?.number + '?',
    }[kind];
    byId('page-configs').replaceChildren();
    if (kind !== 'remove') appendConfig(kind === 'edit' ? working[id] : null);
    byId('another-page').hidden = kind !== 'add';
    byId('operation-error').textContent = '';
    byId('removal-help').hidden = kind !== 'remove';
    byId('apply-operation').textContent = {add: 'Add to page index', edit: 'Apply change', remove: 'Remove from channel'}[kind];
    byId('apply-operation').className = kind === 'remove' ? 'btn-danger' : 'btn-primary';
    dialog.showModal();
    if (kind === 'edit') dialog.querySelector('[name=template]').focus();
    if (kind === 'remove') byId('cancel-operation').focus();
  }

  function freeNumber(preferred, reserved = []) {
    let value = Number(preferred);
    for (let count = 0; count < 800; count++) {
      if (!lookup(working, String(value)) && !reserved.includes(String(value))) return String(value);
      value = value === 899 ? 100 : value + 1;
    }
    return '';
  }

  function clearNumberError(id, input) {
    byId('number-error-' + id).textContent = '';
    input.removeAttribute('aria-invalid');
  }

  function showNumberError(id, input) {
    const error = /^[1-8][0-9]{2}$/.test(input.value)
      ? 'Page ' + input.value + ' is already in use.'
      : 'Enter a page number from 100 to 899.';
    byId('number-error-' + id).textContent = error;
    input.setAttribute('aria-invalid', 'true');
  }

  // Chromium emits a plain input event for the spinner, Firefox a replacement.
  function isNativeNumberStep(event) {
    return event.isTrusted && (!event.inputType || event.inputType === 'insertReplacementText');
  }

  function applyNumberStep(id, input, previous, direction) {
    input.focus({ preventScroll: true });
    const occupied = new Set(Object.values(working).map(item => item.number));
    occupied.delete(working[id].number);
    const start = /^[1-8][0-9]{2}$/.test(previous) ? previous : working[id].number;
    let candidate = Number(start) + direction;
    while (candidate >= 100 && candidate <= 899) {
      if (!occupied.has(String(candidate))) {
        input.value = String(candidate);
        applyNumber(id, input);
        return;
      }
      candidate += direction;
    }
    input.value = previous;
  }

  function applyNumber(id, input) {
    if (published) return;
    const value = input.value;
    const owner = lookup(working, value);
    clearNumberError(id, input);
    if (!/^[1-8][0-9]{2}$/.test(value) || (owner && owner !== id)) {
      pendingNumbers.set(id, value);
      if (value.length === 3) showNumberError(id, input);
      updateSaveHint();
      return;
    }
    pendingNumbers.delete(id);
    working[id] = { ...working[id], number: value };
    // Keep the open sheet, including its subpage, while changing its address or sort position.
    if (page) history.replaceState(null, '', pageHash(page, subpage));
    changed();
  }

  function removePage(id) {
    pendingNumbers.delete(id);
    clearNumberError(id, rows.get(id).querySelector('input'));
    delete working[id];
    for (const [owner, item] of Object.entries(working)) {
      if (!Object.values(item.colourLinks ?? {}).includes(id)) continue;
      const links = {...item.colourLinks};
      for (const colour of Object.keys(links)) if (links[colour] === id) links[colour] = '';
      working[owner] = {...item, colourLinks: links};
    }
  }

  function field(parent, caption, control) {
    const label = document.createElement('label');
    label.append(caption, control);
    parent.append(label);
    return control;
  }

  function pageNumberField() {
    const input = document.createElement('input');
    input.type = 'number';
    input.min = '100';
    input.max = '899';
    input.step = '1';
    input.inputMode = 'numeric';
    input.autocomplete = 'off';
    return input;
  }

  function colourLinkFields(original) {
    const group = document.createElement('fieldset');
    group.className = 'colour-links';
    const legend = document.createElement('legend');
    legend.textContent = 'Colour buttons';
    group.append(legend);
    const options = orderedIds(working).map(id => {
      const item = working[id];
      const source = item.source ? ' · ' + channels[item.source].name : '';
      return [item.number + ' · ' + item.title + source, id];
    });
    for (const [colour, fallback] of Object.entries(colourDefaults)) {
      const caption = document.createElement('span');
      caption.dataset.colour = colour;
      caption.textContent = colour[0].toUpperCase() + colour.slice(1);
      const select = field(group, caption, document.createElement('select'));
      select.name = colour;
      select.add(new Option('Auto · ' + fallback, 'auto'));
      select.add(new Option('No link', ''));
      for (const [label, id] of options) select.add(new Option(label, id));
      select.value = original?.colourLinks?.[colour] ?? 'auto';
    }
    return group;
  }

  function updateIntervalFields() {
    const boxes = byId('page-configs').children;
    const original = operation === 'edit' ? working[editing] : null;
    const count = Object.keys(working).length + (original ? 0 : boxes.length);
    for (const box of boxes) {
      const candidate = {template: box.querySelector('[name=template]').value, period: box.querySelector('[name=period]')?.value};
      const multiple = candidate.template && subpageKeys(carrySubpages(candidate, original), count).length > 1;
      const cycle = box.querySelector('[name=cycleSeconds]');
      cycle.disabled = !multiple;
      cycle.closest('label').hidden = !multiple;
    }
  }

  function appendConfig(original) {
    const list = byId('page-configs');
    const box = document.createElement('fieldset');
    if (original) {
      box.setAttribute('aria-labelledby', 'dialog-title');
    } else {
      const legend = document.createElement('legend');
      legend.textContent = 'New page';
      box.append(legend);
    }
    const template = field(box, 'Teletext template', document.createElement('select'));
    template.name = 'template';
    template.required = true;
    template.add(new Option('Choose a template…', ''));
    for (const owner of ['TV guide add-on', 'User library']) {
      const group = document.createElement('optgroup');
      group.label = owner;
      for (const [key, spec] of Object.entries(templates)) {
        if (spec.owner === owner) group.append(new Option(spec.label, key));
      }
      template.append(group);
    }
    const pageSettings = document.createElement('details');
    const summary = document.createElement('summary');
    summary.textContent = 'Page settings';
    pageSettings.append(summary);
    pageSettings.open = true;
    box.append(pageSettings);
    const number = pageNumberField();
    number.name = 'number';
    number.required = true;
    number.value = original?.number ?? freeNumber('201', [...list.querySelectorAll('[name=number]')].map(input => input.value));
    field(box, 'Page number', number);
    const title = document.createElement('input');
    title.name = 'title';
    title.maxLength = 34;
    title.required = true;
    title.value = original?.title ?? '';
    field(box, 'Page title', title);
    const settings = document.createElement('div');
    pageSettings.append(settings);
    const cycle = document.createElement('input');
    cycle.type = 'number';
    cycle.name = 'cycleSeconds';
    cycle.min = '1';
    cycle.max = '600';
    cycle.step = '1';
    cycle.required = true;
    cycle.value = original?.cycleSeconds ?? defaultCycleSeconds;
    field(pageSettings, 'Subpage interval (1–600 seconds)', cycle);
    pageSettings.append(colourLinkFields(original));
    const sharedTheme = document.createElement('button');
    sharedTheme.type = 'button';
    sharedTheme.className = 'library-link';
    sharedTheme.dataset.openLibrary = '';
    sharedTheme.setAttribute('aria-haspopup', 'dialog');
    sharedTheme.textContent = 'Edit shared TV guide theme';
    pageSettings.append(sharedTheme);
    function showFields() {
      settings.replaceChildren();
      const spec = templates[template.value];
      pageSettings.hidden = !spec;
      sharedTheme.hidden = !spec || !isGuidePage({template: template.value});
      if (!spec) return;
      for (const name of spec.fields) {
        const saved = original?.template === template.value ? original[name] : '';
        const control = document.createElement(name === 'message' ? 'input' : 'select');
        control.name = name;
        control.required = true;
        if (name === 'source') {
          control.add(new Option('Choose a channel…', ''));
          fillChannelOptions(control);
          control.value = saved || '';
          field(settings, 'Guide source channel', control);
        } else if (name === 'period') {
          for (const [key, text] of Object.entries(periods)) control.add(new Option(text, key));
          control.value = saved || 'full';
          field(settings, 'Programme range', control);
          control.addEventListener('change', updateIntervalFields);
        } else {
          control.maxLength = 40;
          control.value = saved || '';
          field(settings, 'Page text (up to 40 characters)', control);
        }
      }
    }
    template.value = original?.template ?? '';
    template.onchange = () => {
      if (!title.value.trim() || Object.values(templates).some(spec => spec.label === title.value)) title.value = templates[template.value]?.label ?? '';
      showFields();
      updateIntervalFields();
    };
    showFields();
    if (!original && list.children.length) {
      const remove = document.createElement('button');
      remove.type = 'button';
      remove.className = 'library-link';
      remove.textContent = 'Remove this entry';
      remove.onclick = () => { box.remove(); updateIntervalFields(); };
      box.append(remove);
    }
    list.append(box);
    updateIntervalFields();
  }

  function fillChannelOptions(select) {
    for (const [id, channel] of Object.entries(channels)) select.add(new Option(id + ' · ' + channel.name, id));
  }

  const subDialog = byId('subpage-dialog');
  const subForm = byId('subpage-form');
  let childPage;
  let childKey;

  function openSubpage(id, key) {
    const item = working[id];
    const keys = subpageKeys(item);
    childPage = id;
    const addingChild = !key;
    if (addingChild) {
      let n = 1;
      while (item.subpages?.['extra' + n]) n++;
      key = 'extra' + n;
    }
    childKey = key;
    const total = addingChild ? keys.length + 1 : keys.length;
    const address = (addingChild ? total : keys.indexOf(key) + 1) + '/' + total;
    const defaults = subpageValues({...item, subpages: {}}, key);
    const saved = item.subpages?.[key] ?? {};
    const controls = subForm.elements;
    byId('subpage-heading').textContent = (addingChild ? 'Add subpage ' : 'Edit subpage ') + item.number + ' · ' + address;
    byId('apply-subpage').textContent = addingChild ? 'Add subpage' : 'Apply change';
    controls.title.value = saved.title ?? '';
    controls.title.placeholder = defaults.title;
    controls.source.replaceChildren(new Option('Use page source · ' + (channels[item.source]?.name ?? ''), ''));
    fillChannelOptions(controls.source);
    controls.source.value = saved.source ?? '';
    controls.period.value = saved.period ?? defaults.period;
    controls.message.value = saved.message ?? '';
    controls.message.placeholder = item.message ?? '';
    for (const name of ['source', 'period', 'message']) {
      const applies = templates[item.template].fields.includes(name);
      controls[name].disabled = !applies;
      controls[name].closest('label').hidden = !applies;
    }
    subDialog.showModal();
    controls.title.focus();
  }

  function saveChild(id, key, record) {
    const item = working[id];
    const subpages = {...item.subpages};
    if (Object.keys(record).length || key.startsWith('extra')) subpages[key] = record;
    else delete subpages[key];
    working[id] = {...item};
    if (Object.keys(subpages).length) working[id].subpages = subpages;
    else delete working[id].subpages;
  }

  byId('cancel-subpage').onclick = () => subDialog.close();
  subForm.addEventListener('submit', event => {
    event.preventDefault();
    const item = working[childPage];
    const defaults = subpageValues({...item, subpages: {}}, childKey);
    const record = {};
    for (const [field, raw] of new FormData(subForm)) {
      const value = raw.trim();
      if (value && (field !== 'period' || value !== defaults.period)) record[field] = value;
    }
    saveChild(childPage, childKey, record);
    const part = subpageKeys(working[childPage]).indexOf(childKey) + 1;
    subDialog.close();
    updateSaveHint();
    rows.get(childPage).querySelector('.subpage-group').open = true;
    go(pageHash(childPage, part));
    rows.get(childPage).querySelector('.subpages a[aria-current]')?.focus({preventScroll: true});
  });

  function markIndexSelection(row, id) {
    const group = row.querySelector('.subpage-group');
    row.querySelector('[aria-current]')?.removeAttribute('aria-current');
    if (id !== page) return;
    const current = group.open && !group.hidden
      ? row.querySelector('.subpages').children[subpage - 1]?.querySelector('a')
      : row.querySelector(':scope > a');
    current?.setAttribute('aria-current', 'page');
  }

  function renderSubpageRows(row, id, item, available, pages) {
    const group = row.querySelector('.subpage-group');
    const list = group.querySelector('ol');
    const keys = subpageKeys(item, available.length);
    group.hidden = keys.length === 1 && !Object.keys(item.subpages ?? {}).length;
    if (group.hidden) return;
    const summary = group.querySelector('summary');
    summary.textContent = keys.length;
    summary.title = keys.length + (keys.length === 1 ? ' subpage' : ' subpages') + ' of page ' + item.number;
    summary.setAttribute('aria-label', summary.title);
    list.setAttribute('aria-label', 'Subpages of ' + item.number);
    const targets = item.template === 'index' ? available.filter(key => key !== id) : [];
    const oldRows = new Map([...list.children].map(li => [li.dataset.key, li]));
    for (const [index, key] of keys.entries()) {
      let li = oldRows.get(key);
      if (!li) {
        li = document.createElement('li');
        li.dataset.key = key;
        const link = document.createElement('a');
        link.append(document.createElement('b'), document.createElement('span'));
        const edit = byId('page-row-tools').content.querySelector('[data-action="Edit"]').cloneNode(true);
        edit.setAttribute('aria-haspopup', 'dialog');
        edit.onclick = () => openSubpage(id, key);
        const remove = byId('page-row-tools').content.querySelector('[data-action="Remove"]').cloneNode(true);
        remove.onclick = () => openSubpageRemoval(id, key);
        li.append(link, edit, remove);
      }
      const part = index + 1;
      const address = part + '/' + keys.length;
      const link = li.querySelector('a');
      const entries = targets.slice(index * indexEntriesPerSheet, (index + 1) * indexEntriesPerSheet);
      const range = entries.length ? pages[entries[0]].number + '–' + pages[entries.at(-1)].number : '';
      const label = item.subpages?.[key]?.title || range || subpageValues(item, key).label;
      link.href = pageHash(id, part);
      link.firstElementChild.textContent = address;
      link.lastElementChild.textContent = label;
      link.setAttribute('aria-label', item.number + ' · ' + address + ' ' + label);
      for (const button of li.querySelectorAll('button')) {
        const requiredIndexSheet = item.template === 'index' && indexSheetCount(available.length) > 1 && !key.startsWith('extra');
        button.hidden = published || (button.dataset.action === 'Remove' && (keys.length === 1 || requiredIndexSheet));
        button.title = button.dataset.action + ' subpage ' + item.number + ' · ' + address;
        button.setAttribute('aria-label', button.title);
      }
      if (list.children[index] !== li) list.insertBefore(li, list.children[index] ?? null);
      oldRows.delete(key);
    }
    for (const stale of oldRows.values()) stale.remove();
  }

  function openSubpageRemoval(id, key) {
    openOperation('remove', id);
    operation = 'remove-subpage';
    childKey = key;
    const keys = subpageKeys(working[id]);
    byId('dialog-title').textContent = 'Remove subpage ' + working[id].number + ' · ' + (keys.indexOf(key) + 1) + '/' + keys.length + '?';
    byId('removal-help').hidden = true;
    byId('apply-operation').textContent = 'Remove subpage';
  }

  function setupTemplateLibrary() {
    const library = byId('elements-library');
    const form = byId('library-form');
    const controls = form.elements;
    const sample = byId('theme-sample');
    for (const control of [controls.band, controls.accent]) {
      for (const colour of Object.keys(palette)) control.add(new Option(colour, colour));
    }
    for (const spec of Object.values(templates)) {
      const entry = document.createElement('li');
      entry.textContent = spec.label;
      byId(spec.owner === 'TV guide add-on' ? 'addon-inventory' : 'user-inventory').append(entry);
    }
    const values = () => ({banner: controls.banner.value.trim(), band: controls.band.value, accent: controls.accent.value});
    function preview() {
      paintTheme(sample, values());
      sample.querySelector('.theme-banner span').textContent = controls.banner.value;
      controls.banner.setCustomValidity('');
    }
    function open() {
      if (library.open) return;
      byId('template-inventory').open = false;
      for (const [key, value] of Object.entries(libraryTheme)) controls[key].value = value;
      byId('library-error').textContent = '';
      preview();
      library.showModal();
    }
    document.addEventListener('click', event => {
      if (event.target.closest('[data-open-library]')) open();
    });
    form.addEventListener('input', preview);
    form.addEventListener('submit', event => {
      event.preventDefault();
      const theme = values();
      if (!theme.banner) {
        controls.banner.setCustomValidity('Enter the text to show in the guide banner.');
        controls.banner.reportValidity();
        return;
      }
      try { localStorage.setItem(themeKey, JSON.stringify(theme)); }
      catch {
        byId('library-error').textContent = 'Browser storage is unavailable.';
        return;
      }
      libraryTheme = theme;
      library.close();
      changed();
    });
    for (const id of ['library-close', 'theme-cancel']) byId(id).onclick = () => library.close();
    library.addEventListener('close', () => {
      if (location.hash === '#elements') history.replaceState(null, '', '#home');
    });
    window.addEventListener('hashchange', () => {
      if (location.hash === '#elements') open();
    });
    window.addEventListener('storage', event => {
      if (event.key !== themeKey) return;
      libraryTheme = readTheme(event.newValue && JSON.parse(event.newValue));
      changed();
    });
    if (location.hash === '#elements') open();
  }

  function renderSourceOverview() {
    const mapping = byId('guide-mapping');
    const focused = mapping.contains(document.activeElement) ? document.activeElement.id : '';
    const labels = [];
    function mappingRow(caption, selectId, source, inherited, onChange) {
      const row = document.createElement('label');
      row.className = 'source-mapping';
      const text = document.createElement('span');
      text.textContent = caption;
      const select = document.createElement('select');
      select.id = selectId;
      row.htmlFor = selectId;
      if (inherited) select.add(new Option('Use page source · ' + channels[inherited].name, ''));
      fillChannelOptions(select);
      select.value = source;
      select.onchange = () => { onChange(select.value); changed(); };
      row.append(text, select);
      labels.push(row);
    }
    for (const id of orderedIds(working)) {
      const item = working[id];
      if (!item.source) continue;
      mappingRow(item.number + ' · ' + item.title, 'source-for-' + id, item.source, '', source => {
        working[id] = {...working[id], source};
      });
      const keys = subpageKeys(item);
      for (const [i, key] of keys.entries()) {
        const saved = item.subpages?.[key];
        if (!saved?.source) continue;
        const caption = item.number + ' · ' + (i + 1) + '/' + keys.length + ' ' + subpageValues(item, key).label;
        mappingRow(caption, 'source-for-' + id + '-' + key, saved.source, item.source, source => {
          const record = {...saved};
          if (source) record.source = source;
          else delete record.source;
          saveChild(id, key, record);
        });
      }
    }
    mapping.replaceChildren(...labels);
    byId('no-guides').hidden = labels.length > 0;
    if (focused) (byId(focused) ?? byId(focused.split('-').slice(0, 3).join('-')))?.focus({preventScroll: true});
  }

  function ensureRows(pages) {
    for (const [id, item] of Object.entries(pages)) {
      if (rows.has(id)) continue;
      const row = document.createElement('div');
      row.className = 'page-row';
      const link = document.createElement('a');
      link.href = '#p' + item.number;
      const number = pageNumberField();
      number.className = 'index-number';
      number.setAttribute('aria-describedby', 'number-error-' + id);
      number.addEventListener('focus', () => number.select());
      number.addEventListener('input', event => {
        const previous = pendingNumbers.get(id) ?? working[id].number;
        const delta = number.valueAsNumber - Number(previous);
        if (isNativeNumberStep(event) && Math.abs(delta) === 1) {
          applyNumberStep(id, number, previous, delta);
        } else {
          applyNumber(id, number);
        }
      });
      number.addEventListener('blur', () => {
        if (!number.readOnly && pendingNumbers.has(id)) showNumberError(id, number);
      });
      number.addEventListener('keydown', event => {
        if (number.readOnly) return;
        if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
          event.preventDefault();
          applyNumberStep(id, number, number.value, event.key === 'ArrowUp' ? 1 : -1);
        } else if (event.key === 'Escape') {
          pendingNumbers.delete(id);
          clearNumberError(id, number);
          changed();
          number.select();
        } else if (event.key === 'Enter') {
          event.preventDefault();
          applyNumber(id, number);
          if (pendingNumbers.has(id)) showNumberError(id, number);
        }
      });
      const error = document.createElement('small');
      error.id = 'number-error-' + id;
      error.className = 'number-error';
      error.setAttribute('role', 'status');
      const label = document.createElement('span');
      label.className = 'row-title';
      label.textContent = item.title;
      const source = document.createElement('small');
      source.className = 'row-source';
      const caption = document.createElement('span');
      caption.append(label, source);
      link.append(caption);
      const tools = document.createElement('span');
      tools.className = 'row-tools';
      tools.append(byId('page-row-tools').content.cloneNode(true));
      for (const button of tools.querySelectorAll('button')) {
        button.addEventListener('click', () => {
          if (button.dataset.action === 'Add subpage') openSubpage(id);
          else openOperation(button.dataset.action.toLowerCase(), id);
        });
      }
      const children = document.createElement('ol');
      children.className = 'subpages';
      const group = document.createElement('details');
      group.className = 'subpage-group';
      group.append(document.createElement('summary'), children);
      group.addEventListener('toggle', () => markIndexSelection(row, id));
      row.append(number, link, tools, error, group);
      index.append(row);
      rows.set(id, row);
    }
  }

  byId('another-page').onclick = () => appendConfig(null);
  byId('add-pages').onclick = () => openOperation('add');
  byId('cancel-operation').onclick = () => dialog.close();
  byId('page-operation').addEventListener('submit', event => {
    event.preventDefault();
    if (operation === 'remove') {
      removePage(editing);
    } else if (operation === 'remove-subpage') {
      const item = working[editing];
      const selected = subpageKeys(item)[subpage - 1];
      const subpages = {...item.subpages};
      if (childKey.startsWith('extra')) delete subpages[childKey];
      else subpages[childKey] = {removed: true};
      working[editing] = {...item, subpages};
      if (page === editing) {
        const remaining = subpageKeys(working[editing]);
        const target = remaining.includes(selected) ? remaining.indexOf(selected) + 1 : Math.min(subpage, remaining.length);
        history.replaceState(null, '', pageHash(editing, target));
      }
    } else {
      const reserved = new Set(Object.entries(working).filter(([id]) => id !== editing).map(([, item]) => item.number));
      const additions = {};
      for (const box of byId('page-configs').children) {
        const data = Object.fromEntries([...box.querySelectorAll('[name]:enabled')].map(input => [input.name, input.value.trim()]));
        if (!data.title || (templates[data.template].fields.includes('message') && !data.message)) {
          byId('operation-error').textContent = 'Fill in the title and the required template fields.';
          return;
        }
        if (!/^[1-8][0-9]{2}$/.test(data.number) || reserved.has(data.number)) {
          byId('operation-error').textContent = reserved.has(data.number)
            ? 'Page ' + data.number + ' is already in use.'
            : 'Enter a page number from 100 to 899.';
          box.querySelector('[name=number]').focus();
          return;
        }
        reserved.add(data.number);
        let id = editing;
        if (!id) {
          let n = 1;
          do { id = 'n' + n++; } while (working[id] || model.selectedPages[id] || additions[id]);
        }
        data.cycleSeconds = data.cycleSeconds === undefined ? working[id]?.cycleSeconds : Number(data.cycleSeconds);
        data.colourLinks = Object.fromEntries(Object.keys(colourDefaults)
          .filter(colour => data[colour] !== 'auto').map(colour => [colour, data[colour]]));
        additions[id] = carrySubpages(configuredPage(id, data), working[id]);
      }
      const selectedKey = working[page] && additions[page]?.template === working[page].template
        ? subpageKeys(working[page])[subpage - 1] : null;
      Object.assign(working, additions);
      if (operation === 'edit') {
        pendingNumbers.delete(editing);
        clearNumberError(editing, rows.get(editing).querySelector('input'));
        if (page === editing) {
          const keys = subpageKeys(working[page]);
          const part = keys.includes(selectedKey) ? keys.indexOf(selectedKey) + 1 : Math.min(subpage, keys.length);
          history.replaceState(null, '', pageHash(page, part));
        }
      }
    }
    dialog.close();
    changed();
    if (operation === 'remove') byId('add-pages').focus();
    if (operation === 'remove-subpage') rows.get(editing).querySelector('a').focus();
  });
  guideToggle.addEventListener('change', () => {
    guideEnabled = guideToggle.checked;
    changed();
  });
  byId('channel-settings').addEventListener('submit', event => {
    event.preventDefault();
    if (pendingNumbers.size) {
      const [id] = pendingNumbers.keys();
      go('#p' + working[id].number);
      const input = rows.get(id).querySelector('input');
      showNumberError(id, input);
      saveStatus.textContent = 'Controleer het gemarkeerde paginanummer om te publiceren.';
      input.focus();
      return;
    }
    if (!Object.keys(working).length) {
      go('#p200');
      saveStatus.textContent = 'Voeg een pagina toe om te publiceren.';
      byId('add-pages').focus();
      return;
    }
    let pages;
    try {
      pages = readPages(working);
    } catch {
      go('#home');
      saveStatus.textContent = 'Controleer voor elke pagina het sjabloon en de verplichte velden.';
      return;
    }
    const revision = dirtyDraft() ? model.revision + 1 : model.revision;
    const current = revision === model.selectedRevision;
    const next = current ? model : {
      pages, guideEnabled, theme: libraryTheme, revision,
      selectedPages: pages, selectedGuideEnabled: guideEnabled, selectedTheme: libraryTheme, selectedRevision: revision,
      generation: model.generation + 1,
    };
    if (!persist(next)) return;
    saveStatus.textContent = current
      ? 'Deze editie is al gepubliceerd.'
      : 'Opgeslagen en gepubliceerd · editie ' + pad(next.generation) + '.';
    render();
  });
  document.addEventListener('click', event => {
    const link = event.target.closest('a[href^="#"]');
    if (!link || link.classList.contains('skip') || event.button || event.ctrlKey ||
        event.metaKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    go(link.hash);
  });
  window.addEventListener('hashchange', render);
  window.addEventListener('popstate', render);
  byId('remote').hidden = false;
  byId('channel-settings').querySelector('button').disabled = false;
  setupDrawer();
  setupTemplateLibrary();
  index.replaceChildren(byId('index-heading'));
  document.body.classList.add('enhanced');
  render();
})();
