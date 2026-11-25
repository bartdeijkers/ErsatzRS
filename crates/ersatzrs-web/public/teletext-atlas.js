// The server owns editions, rasterization and publication. Keep working edits in this tab.
(() => {
  function mount(root) {
    if (root.dataset.ready) return;
    root.dataset.ready = "true";
    const cfg = JSON.parse(root.dataset.config);
    const label = cfg.labels;
    const find = (selector) => root.querySelector(selector);
    const form = find("[data-edition]");
    const dialog = find("[data-editor]");
    const fields = dialog.querySelector("form").elements;
    const list = find("[data-index]");
    const mapping = find("[data-mapping]");
    const number = find("#atlas-number");
    const status = find("[data-status]");
    const image = find("[data-image]");
    const empty = find("[data-empty]");
    const track = find(".atlas-track i");
    const base = JSON.stringify(cfg.pages);
    const storage = "teletext-atlas:" + form.elements.channel_id.value;
    const revision = [
      form.elements.generation.value,
      form.elements.selection.value,
      form.elements.revision.value,
    ].join("/");
    const savedStatus = status.textContent;
    let pages = cfg.pages;
    const templates = cfg.custom?.library || [];
    const element = (p) => templates.find((t) => String(t.id) === p?.element);
    const initialGuide = form.elements.guide_on.checked;
    for (const t of templates) {
      fields.template.querySelector("[data-element-options]").append(new Option(t.name, "element:" + t.id));
    }
    const opened = new Set();
    const pendingNumbers = new Map();
    const colours = ["red", "green", "yellow", "blue"];
    let inventory = cfg.inventory;
    let navigation = cfg.navigation;
    let hits = cfg.hits;
    const hotspots = find("[data-hotspots]");
    let active = Number(number.value);
    let sub = Number(new URL(location.href).searchParams.get("subcode")) || 0;
    let reveal = false, digits = "", editor, batch, timer, dirty = false;
    let elapsed = 0, started = 0;
    let pictureSequence = 0;
    let pendingPicture = Promise.resolve();
    let saving = false;
    const pictures = new Map();
    if (!cfg.capable) {
      for (const option of fields.template.options) {
        option.disabled = !option.value.startsWith("element:");
      }
    }
    const channelName = (id) => cfg.channels.find((c) => c.id === id)?.name;
    const title = (p) =>
      p.title || element(p)?.name || channelName(p.source) || label.index;
    const guide = () => pages.find((p) => p.page === active);
    const allNumbers = () =>
      [
        ...new Set([
          ...pages.map((p) => p.page),
          ...inventory.map((p) => p.page),
        ]),
      ].sort((a, b) => a - b);
    function sheets(page) {
      const codes = inventory.filter((p) => p.page === page).map((p) =>
        p.subcode
      );
      const p = pages.find((p) => p.page === page);
      let count = p?.subpages?.length ||
        (p?.element ? element(p)?.subpages : undefined);
      if (p && !p.source && !p.element) {
        const required = Math.max(
          1,
          Math.ceil((pages.length - 1) / 8),
        );
        const explicit = (p.subpages || []).filter((s) => s.part <= required);
        const referenced = new Set(explicit.map((s) => s.part));
        count = explicit.length + required - referenced.size;
      }
      if (p?.element && count) {
        return count === 1 ? [0] : Array.from(
          { length: count },
          (_, i) => parseInt((i + 1).toString(), 16),
        );
      }
      if (count) {
        return Array.from(
          { length: count },
          (_, i) => Number.parseInt(i.toString(), 16),
        );
      }
      return codes.length ? codes : [0];
    }
    function node(tag, content) {
      const e = document.createElement(tag);
      if (content !== undefined) e.textContent = content;
      return e;
    }
    function button(action, caption, name, parent) {
      const b = node("button", caption);
      b.type = "button";
      b.dataset.do = action;
      b.title = name;
      b.setAttribute("aria-label", name);
      parent.append(b);
      return b;
    }
    function options(select, entries, selected) {
      select.replaceChildren(
        ...entries.map(([value, text]) =>
          new Option(text, value, false, String(value) === String(selected))
        ),
      );
    }
    function stop(reset = true) {
      if (timer) {
        elapsed += performance.now() - started;
        cancelAnimationFrame(timer);
      }
      timer = 0;
      if (reset) elapsed = 0;
      paintClock();
    }
    function paintClock() {
      const ms = (guide()?.interval || element(guide())?.cycle_seconds || 10) *
        1000;
      let time = elapsed + (timer ? performance.now() - started : 0);
      if (matchMedia("(prefers-reduced-motion: reduce)").matches) {
        time = Math.floor(time / 1000) * 1000;
      }
      track.style.transform = "scaleX(" + Math.min(time / ms, 1) + ")";
      find('[data-do="cycle"]').textContent = timer ? label.pause : label.play;
    }
    function play() {
      if (timer) {
        stop(false);
        return;
      }
      started = performance.now();
      const tick = () => {
        if (
          elapsed + performance.now() - started >=
            (guide()?.interval || element(guide())?.cycle_seconds || 10) * 1000
        ) {
          const codes = sheets(active);
          elapsed = 0;
          started = performance.now();
          select(active, codes[(codes.indexOf(sub) + 1) % codes.length], true);
        }
        timer = requestAnimationFrame(tick);
        paintClock();
      };
      tick();
    }
    function colourTarget(key) {
      const explicit = guide()?.links;
      if (explicit && Object.hasOwn(explicit, key)) {
        return explicit[key] && { page: explicit[key], subcode: 0 };
      }
      return navigation.find((n) => n.role === (key === "blue" ? "cyan" : key))
        ?.target;
    }
    function controls() {
      number.value = active || "";
      const codes = sheets(active);
      const at = codes.indexOf(sub);
      const numbers = allNumbers();
      const here = numbers.indexOf(active);
      for (
        const [action, disabled] of [
          ["prev", here <= 0],
          ["next", here < 0 || here >= numbers.length - 1],
          ["back", at <= 0],
          ["forward", at >= codes.length - 1],
          ["cycle", codes.length <= 1],
        ]
      ) find('[data-do="' + action + '"]').disabled = disabled;
      find("[data-counter]").textContent = active && numbers.length
        ? active + " · " + (at + 1) + "/" + codes.length
        : "";
      const key = find('.atlas-digits [data-do="reveal"]');
      key.textContent = reveal ? label.hide : label.show;
      key.setAttribute("aria-pressed", String(reveal));
      for (const b of root.querySelectorAll("[data-colour]")) {
        const target = colourTarget(b.dataset.colour);
        b.disabled = !target || !numbers.includes(target.page);
        b.textContent = target?.page ?? "—";
        b.title = b.getAttribute("aria-label") + " " + b.textContent;
      }
    }
    function drawHotspots() {
      hotspots.replaceChildren();
      const remoteReveal = find('.atlas-digits [data-do="reveal"]');
      remoteReveal.disabled = !hits?.concealed.length || image.hidden;
      if (!hits?.columns || image.hidden) return;
      function region(action, caption, row, col, width) {
        const key = button(action, "", caption, hotspots);
        key.style.left = (100 * col / hits.columns) + "%";
        key.style.top = (4 * row) + "%";
        key.style.width = (100 * width / hits.columns) + "%";
        return key;
      }
      const page = guide();
      if (page && !page.element && !page.source) {
        for (const [row, col, n] of hits.entries) {
          if (allNumbers().includes(n)) region("destination", label.from + " " + n, row, col, 3).dataset.destination = n;
        }
      }
      for (const [col, width, role] of hits.shortcuts) {
        const key = role === "cyan" ? "blue" : role;
        const target = colourTarget(key);
        if (target && allNumbers().includes(target.page)) {
          const caption = find('.atlas-keys [data-colour="' + key + '"]').title;
          region("shortcut", caption, 24, col, width).dataset.shortcut = key;
        }
      }
      for (const [row, col, width] of hits.concealed) {
        const key = region("reveal", reveal ? label.hide : label.show, row, col, width);
        key.setAttribute("aria-pressed", String(reveal));
        if (!reveal) key.textContent = "•".repeat(width);
      }
    }
    function rowTools(row, parent, part) {
      const tools = node("span");
      tools.dataset.toolsRow = "";
      if (part === undefined) {
        button("add-sheet", "＋", label.add + " " + label.sheet, tools);
      }
      button("edit", "✎", label.edit, tools);
      button("delete", "×", label.delete, tools);
      row.append(tools);
    }
    function drawIndex() {
      const items = [];
      for (const n of allNumbers()) {
        const p = pages.find((p) => p.page === n);
        const item = node("li");
        item.dataset.page = n;
        const row = node("div");
        row.dataset.row = "";
        if (p && (cfg.capable || p.element)) {
          const holder = node("span");
          holder.dataset.number = "";
          const input = node("input");
          Object.assign(input, {
            type: "number",
            min: 100,
            max: 899,
            value: pendingNumbers.get(n) ?? n,
            required: true,
          });
          input.setAttribute("aria-label", label.from || String(n));
          input.dataset.assign = n;
          if (pendingNumbers.has(n)) validateNumber(input);
          holder.append(input);
          const step = node("span");
          step.dataset.stepper = "";
          button("up", "▲", "＋", step);
          button("down", "▼", "−", step);
          holder.append(step);
          row.append(holder);
        } else row.append(node("b", n));
        const name = p
          ? title(p)
          : cfg.custom?.links.find((l) => Number(l.page) === n)?.name ||
            String(n);
        const open = button("open", name, name, row);
        open.dataset.open = "";
        if (n === active) open.setAttribute("aria-current", "page");
        const codes = sheets(n);
        if (p && (cfg.capable || p.element)) rowTools(row, p);
        if (codes.length > 1) {
          const b = button(
            "fold",
            String(codes.length),
            label.sheet,
            row,
          );
          b.setAttribute("aria-expanded", String(opened.has(n)));
        }
        item.append(row);
        if (opened.has(n) && codes.length > 1) {
          const children = node("ol");
          codes.forEach((code, i) => {
            const li = node("li");
            li.dataset.part = i;
            li.dataset.row = "";
            const s = p?.subpages?.[i];
            const b = button(
              "open-sheet",
              "",
              label.sheet + " " + (i + 1),
              li,
            );
            b.append(node("b", (i + 1) + "/" + codes.length), node("span", s?.title || name));
            b.dataset.open = "";
            if (n === active && code === sub) {
              b.setAttribute("aria-current", "page");
            }
            if (p && (cfg.capable || p.element)) rowTools(li, p, i);
            children.append(li);
          });
          item.append(children);
        }
        items.push(item);
      }
      list.replaceChildren(...items);
      if (!items.length) list.append(node("li", label.blank));
    }
    function drawSources() {
      const rows = [];
      for (const p of pages.filter((p) => p.source)) {
        function assignment(sheet, i) {
          const row = node(
            "label",
            p.page + (sheet ? " · " + (i + 1) : "") + " " +
              (sheet?.title || title(p)),
          );
          const input = node("select");
          options(input, [
            ...(sheet ? [["", label.inherit]] : []),
            ...cfg.channels.map((c) => [c.id, c.name]),
          ], sheet ? sheet.source || "" : p.source);
          input.disabled = !cfg.capable;
          input.addEventListener("change", () => {
            const target = sheet || p;
            if (input.value) target.source = input.value;
            else delete target.source;
            change();
          });
          row.append(input);
          rows.push(row);
        }
        assignment();
        (p.subpages || []).forEach(assignment);
      }
      mapping.replaceChildren(...rows);
    }
    function sync() {
      pages.sort((a, b) => a.page - b.page);
      form.elements.layout.value = JSON.stringify(pages);
      dirty = pendingNumbers.size > 0 || form.elements.layout.value !== base ||
        form.elements.guide_on.checked !== initialGuide;
      status.textContent = dirty ? label.unsaved : savedStatus;
      try {
        if (dirty) {
          sessionStorage.setItem(
            storage,
            JSON.stringify({
              revision,
              numbers: [...pendingNumbers],
              pages,
              on: form.elements.guide_on.checked,
            }),
          );
        } else sessionStorage.removeItem(storage);
      } catch { /* Editing also works without tab storage. */ }
    }
    function change() {
      stop();
      sync();
      drawIndex();
      drawSources();
      controls();
      picture();
    }
    async function picture() {
      if (saving) return;
      const sequence = ++pictureSequence;
      const receiver = image.parentElement;
      receiver.removeAttribute("aria-busy");
      hits = null;
      drawHotspots();
      find("[data-guide-off]").hidden = !guide() || Boolean(guide().element) || form.elements.guide_on.checked;
      if (!allNumbers().includes(active)) {
        image.hidden = true;
        empty.hidden = false;
        empty.textContent = label.blank;
        return;
      }
      if (!guide()?.element && !cfg.capable && !form.elements.guide_on.checked) {
        image.hidden = true;
        empty.hidden = false;
        empty.textContent = label.off;
        return;
      }
      const body = new URLSearchParams(new FormData(form));
      body.set("current", String(!dirty));
      body.set("page", active);
      body.set("subcode", sub);
      body.set("reveal", String(reveal));
      const key = body.toString();
      const display = (result) => {
        image.src = result.image;
        image.hidden = false;
        empty.hidden = true;
        const seen = new Set(result.pages.map((p) => p.page));
        inventory = [
          ...inventory.filter((p) => !seen.has(p.page)),
          ...result.pages,
        ];
        navigation = result.navigation;
        hits = result.hits;
        // Keep the active number input and its pending edit while a raster arrives.
        if (!document.activeElement?.matches("[data-assign]")) drawIndex();
        controls();
        drawHotspots();
      };
      if (pictures.has(key)) {
        display(pictures.get(key));
        return;
      }
      image.hidden = true;
      empty.hidden = false;
      empty.textContent = label.loading;
      receiver.setAttribute("aria-busy", "true");
      pendingPicture = pendingPicture.then(async () => {
        if (sequence !== pictureSequence) return;
        try {
          const response = await fetch("/admin-api/teletext/atlas/picture", {
            method: "POST",
            body,
          });
          if (!response.ok) throw Error();
          const result = await response.json();
          if (pictures.size >= 8) pictures.delete(pictures.keys().next().value);
          pictures.set(key, result);
          if (sequence === pictureSequence) display(result);
        } catch {
          if (sequence === pictureSequence) {
            empty.textContent = label.preview;
            navigation = [];
            controls();
          }
        } finally {
          if (sequence === pictureSequence) {
            receiver.removeAttribute("aria-busy");
          }
        }
      });
    }
    function select(n, code = 0, cycling = false) {
      if (!allNumbers().includes(n)) {
        number.setCustomValidity(label.invalid);
        number.reportValidity();
        return;
      }
      if (!cycling) stop();
      number.setCustomValidity("");
      active = n;
      sub = sheets(n).includes(code) ? code : sheets(n)[0];
      digits = "";
      reveal = false;
      navigation = [];
      drawIndex();
      controls();
      picture();
    }
    function free(
      n,
      dir,
      used = allNumbers(),
      first = 100,
      last = 899,
    ) {
      for (let v = n + dir; v >= first && v <= last; v += dir) {
        if (!used.includes(v)) return v;
      }
    }
    function renumber(p, n) {
      if (
        !Number.isInteger(n) || n < 100 ||
        n > 899 ||
        (n !== p.page && allNumbers().includes(n))
      ) return false;
      const old = p.page;
      pendingNumbers.delete(old);
      p.page = n;
      if (opened.delete(old)) opened.add(n);
      if (active === old) active = n;
      inventory = inventory.map((a) => a.page === old ? { ...a, page: n } : a);
      for (const src of pages) {
        for (const k of colours) if (src.links?.[k] === old) src.links[k] = n;
      }
      return true;
    }
    function openEditor(p, part, adding = false) {
      stop();
      const child = part !== undefined;
      find("#atlas-editor-title").textContent = p ? label.edit + " · " + p.page : label.add;
      find("[data-page-settings]").open = true;
      editor = { page: p?.page, part, adding };
      const proto = p ||
        {
          page: free(cfg.first - 1, 1, [
            ...allNumbers(),
            ...batch.map((p) => p.page),
          ]),
          source: cfg.channels[0]?.id,
        };
      const sheet = child && p?.subpages?.[part];
      fields.template.value = proto.element
        ? "element:" + proto.element
        : proto.source
        ? "guide"
        : "index";
      if (!p && !cfg.capable && templates.length) {
        fields.template.value = "element:" + templates[0].id;
      }
      fields.template.disabled = child;
      fields.title.value = (child ? sheet?.title : proto.title) || "";
      fields.page.value = proto.page || "";
      fields.page.min = 100;
      fields.page.max = 899;
      fields.page.disabled = child;
      fields.interval.value = proto.interval || element(proto)?.cycle_seconds ||
        10;
      fields.part.value = sheet?.part || (adding ? 1 : part + 1);
      fields.range.options[0].hidden = !child;
      fields.range.value = (child ? sheet?.range : proto.range) ||
        (child ? "" : "all");
      options(fields.source, [
        ...(child ? [["", label.inherit]] : []),
        ...cfg.channels.map((c) => [c.id, c.name]),
      ], child ? sheet?.source || "" : proto.source);
      for (const k of colours) {
        options(
          fields[k],
          [
            ["auto", label.auto],
            ["none", label.none],
            ...allNumbers().map((n) => [String(n), String(n)]),
          ],
          !Object.hasOwn(proto.links || {}, k)
            ? "auto"
            : proto.links[k] ?? "none",
        );
      }
      find("[data-part-label]").hidden = !child;
      find("[data-link-fields]").hidden = child;
      find("[data-interval-label]").hidden = child;
      find("[data-page-label]").hidden = child;
      find("[data-source-label]").hidden = fields.template.value === "index";
      find('[data-do="another"]').hidden = Boolean(p);
      templateFields(false);
      fields.page.setCustomValidity("");
      if (!dialog.open) dialog.showModal();
      (child ? fields.title : fields.template).focus();
    }
    function templateFields(changed) {
      const template = templates.find((t) =>
        "element:" + t.id === fields.template.value
      );
      const p = pages.find((p) => p.page === editor.page);
      const sheet = editor.part === undefined
        ? undefined
        : p?.subpages?.[editor.part];
      const custom = Boolean(template);
      if (changed && !editor.page && custom) {
        fields.page.value = free(
          Number(template.page) - 1,
          1,
          [...allNumbers(), ...batch.map((p) => p.page)],
          100,
          899,
        ) || "";
      }
      find("[data-source-label]").hidden = fields.template.value !== "guide";
      dialog.querySelector('a[href="/teletext-elements#guide-theme"]').hidden =
        custom;
      find("[data-range-label]").hidden = fields.template.value !== "guide";
      fields.part.max = custom ? template.subpages : 64;
      const container = find("[data-template-fields]");
      container.replaceChildren();
      for (const key of template?.fields || []) {
        const row = node("label", key), input = node("input");
        input.dataset.field = key;
        input.maxLength = 40;
        input.value = (sheet?.fields || p?.fields || {})[key] || "";
        row.append(input);
        container.append(row);
      }
    }
    function materialize(p) {
      return p.subpages ||= sheets(p.page).map((_, i) => ({ part: i + 1 }));
    }
    function apply(another = false) {
      if (!dialog.querySelector("form").reportValidity()) return;
      const p = pages.find((p) => p.page === editor.page);
      const name = fields.title.value.trim();
      if (editor.part !== undefined) {
        const parts = materialize(p),
          sheet = { part: Number(fields.part.value) };
        if (name) sheet.title = name;
        if (p.element) {
          sheet.fields = Object.fromEntries(
            [...dialog.querySelectorAll("[data-field]")].filter((i) => i.value)
              .map((i) => [i.dataset.field, i.value]),
          );
        }
        if (fields.range.value && p.source) sheet.range = fields.range.value;
        if (fields.source.value && p.source) sheet.source = fields.source.value;
        parts[editor.part] = sheet;
        opened.add(p.page);
      } else {
        const n = Number(fields.page.value);
        if (
          (p && !renumber(p, n)) ||
          (!p && [...allNumbers(), ...batch.map((p) => p.page)].includes(n))
        ) {
          fields.page.setCustomValidity(label.occupied);
          fields.page.reportValidity();
          return;
        }
        const target = p || { page: n };
        target.title = name || undefined;
        const template = fields.template.value;
        const nextElement = template.startsWith("element:")
          ? template.slice(8)
          : undefined;
        if (
          p &&
          (p.element !== nextElement ||
            Boolean(p.source) !== (template === "guide"))
        ) delete target.subpages;
        target.element = nextElement;
        target.source = template === "guide" ? fields.source.value : undefined;
        target.range = template === "guide" && fields.range.value !== "all"
          ? fields.range.value
          : undefined;
        target.fields = Object.fromEntries(
          [...dialog.querySelectorAll("[data-field]")].filter((i) => i.value)
            .map((i) => [i.dataset.field, i.value]),
        );
        target.interval = Number(fields.interval.value);
        target.links = {};
        for (const k of colours) {
          if (fields[k].value !== "auto") {
            target.links[k] = fields[k].value === "none"
              ? null
              : Number(fields[k].value);
          }
        }
        if (!p) batch.push(target);
      }
      if (another) {
        openEditor();
        return;
      }
      if (batch.length) {
        active = batch[0].page;
        sub = 0;
        pages.push(...batch);
      }
      const focusPage = p?.page || active;
      dialog.close();
      change();
      list.querySelector('[data-page="' + focusPage + '"] [data-do="open"]')
        ?.focus();
    }
    dialog.addEventListener("submit", (e) => {
      e.preventDefault();
      apply();
    });
    fields.template.addEventListener("change", () => templateFields(true));
    form.elements.guide_on.addEventListener("change", change);
    fields.page.addEventListener(
      "input",
      () => fields.page.setCustomValidity(""),
    );
    function validateNumber(input) {
      const n = input.valueAsNumber;
      const occupied = n !== Number(input.dataset.assign) && allNumbers().includes(n);
      input.setCustomValidity(occupied ? label.occupied : "");
    }
    list.addEventListener("change", (e) => {
      if (!e.target.matches("[data-assign]")) return;
      const input = e.target,
        p = pages.find((p) => p.page === Number(input.dataset.assign));
      if (!renumber(p, input.valueAsNumber)) {
        validateNumber(input);
        input.reportValidity();
        return;
      }
      change();
      list.querySelector('[data-assign="' + p.page + '"]')?.focus();
    });
    list.addEventListener("input", (e) => {
      const input = e.target;
      if (!input.matches("[data-assign]")) return;
      pendingNumbers.set(Number(input.dataset.assign), input.value);
      validateNumber(input);
      sync();
    });
    list.addEventListener("keydown", (e) => {
      if (
        !e.target.matches("[data-assign]") ||
        !["ArrowUp", "ArrowDown"].includes(e.key)
      ) return;
      e.preventDefault();
      e.target.parentElement.querySelector(
        e.key === "ArrowUp" ? '[data-do="up"]' : '[data-do="down"]',
      ).click();
    });
    function tab(name, nav = true) {
      for (const pane of root.querySelectorAll("[data-panel]")) {
        pane.hidden = pane.dataset.panel !== name;
      }
      for (const a of root.querySelectorAll("[data-tab]")) {
        const on = a.dataset.tab === name;
        a.className = on ? a.dataset.activeClass : a.dataset.idleClass;
        if (on) a.setAttribute("aria-current", "page");
        else a.removeAttribute("aria-current");
      }
      find("[data-tools]").hidden = name !== "sources";
      form.elements.tab.value = name;
      if (nav) {
        const u = new URL(location.href);
        u.searchParams.set("tab", name);
        history.pushState({}, "", u);
      }
    }
    root.addEventListener("click", (e) => {
      const a = e.target.closest("[data-tab]");
      if (a && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        tab(a.dataset.tab);
        return;
      }
      const themeLink = e.target.closest(
        'a[href="/teletext-elements#guide-theme"]',
      );
      if (themeLink && find("[data-theme-dialog] [data-guide-theme]")) {
        e.preventDefault();
        find("[data-theme-dialog]").showModal();
        return;
      }
      const b = e.target.closest("button");
      if (!b) return;
      if (b.dataset.digit !== undefined) {
        digits = (digits.length >= 3 ? "" : digits) + b.dataset.digit;
        number.value = digits;
        return;
      }
      if (b.dataset.colour) {
        const t = colourTarget(b.dataset.colour);
        if (t) select(t.page, t.subcode || 0);
        return;
      }
      const n = Number(b.closest("[data-page]")?.dataset.page),
        p = pages.find((p) => p.page === n);
      const child = b.closest("[data-part]");
      const part = child ? Number(child.dataset.part) : undefined;
      const all = allNumbers(),
        index = all.indexOf(active),
        codes = sheets(active),
        at = codes.indexOf(sub);
      switch (b.dataset.do) {
        case "add":
          batch = [];
          openEditor();
          break;
        case "another":
          apply(true);
          break;
        case "close":
          dialog.close();
          break;
        case "close-theme":
          find("[data-theme-dialog]").close();
          break;
        case "fold":
          if (opened.has(n)) opened.delete(n);
          else opened.add(n);
          drawIndex();
          break;
        case "edit":
          batch = [];
          openEditor(p, part);
          break;
        case "add-sheet":
          batch = [];
          openEditor(p, sheets(n).length, true);
          break;
        case "delete":
          if (
            !window.confirm(
              label.confirm + " · " + n + (child ? " / " + (part + 1) : ""),
            )
          ) return;
          if (child) {
            materialize(p).splice(part, 1);
            sub = 0;
          } else {
            pendingNumbers.delete(n);
            pages = pages.filter((p) => p.page !== n);
            inventory = inventory.filter((a) => a.page !== n);
            for (const other of pages) {
              for (const k of colours) {
                if (other.links?.[k] === n) other.links[k] = null;
              }
            }
            if (active === n) {
              active = allNumbers()[0];
              sub = 0;
            }
          }
          change();
          break;
        case "up":
        case "down": {
          const v = free(
            n,
            b.dataset.do === "up" ? 1 : -1,
            allNumbers(),
          );
          if (v && renumber(p, v)) {
            change();
            list.querySelector('[data-assign="' + v + '"]')?.focus();
          }
          break;
        }
        case "open":
          select(n, sheets(n)[0]);
          break;
        case "open-sheet":
          select(n, sheets(n)[part]);
          break;
        case "destination":
          select(Number(b.dataset.destination));
          break;
        case "shortcut": {
          const target = colourTarget(b.dataset.shortcut);
          if (target) select(target.page, target.subcode ?? 0);
          break;
        }
        case "go":
          select(number.valueAsNumber);
          break;
        case "home":
          select(pages.find((p) => !p.source && !p.element)?.page || all[0]);
          break;
        case "prev":
          select(all[index - 1]);
          break;
        case "next":
          select(all[index + 1]);
          break;
        case "back":
          select(active, codes[at - 1]);
          break;
        case "forward":
          select(active, codes[at + 1]);
          break;
        case "erase":
          digits = String(number.value).slice(0, -1);
          number.value = digits;
          break;
        case "cycle":
          play();
          break;
        case "reveal":
          stop();
          reveal = !reveal;
          controls();
          picture();
          break;
      }
    });
    number.addEventListener("input", (e) => {
      number.setCustomValidity("");
      if (e instanceof InputEvent && e.inputType) {
        digits = number.value;
        return;
      }
      const direction = Number(number.value) > active ? 1 : -1;
      const all = allNumbers(), next = all[all.indexOf(active) + direction];
      if (next) select(next);
      else number.value = active;
    });
    number.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        select(number.valueAsNumber);
      }
      if (e.key === "ArrowUp" || e.key === "ArrowDown") {
        e.preventDefault();
        find(e.key === "ArrowUp" ? '[data-do="next"]' : '[data-do="prev"]')
          .click();
      }
    });
    form.addEventListener("invalid", (e) => {
      if (e.target.matches("[data-assign]")) {
        tab("pages", false);
        e.target.focus();
      }
    }, true);
    window.addEventListener("pagehide", () => stop());
    form.addEventListener("submit", (e) => {
      for (const [n,value] of pendingNumbers) {
        const page = pages.find(p => p.page === n);
        const input = list.querySelector('[data-assign="' + n + '"]');
        if (!page || !input || !renumber(page, Number(value))) {
          e.preventDefault();
          tab("pages", false);
          input?.focus();
          input?.reportValidity();
          return;
        }
      }
      sync();
      if (!pages.length) {
        e.preventDefault();
        tab("pages");
        find('[data-do="add"]').focus();
        status.textContent = label.blank;
        return;
      }

      e.preventDefault();
      if (saving) return;
      saving = true;
      form.inert = true;
      stop();
      ++pictureSequence;
      form.querySelector('footer button[type="submit"]').disabled = true;
      // Preview and publication share a bounded native renderer. Finish an
      // active render before the normal POST takes its publication snapshot.
      pendingPicture.finally(() => HTMLFormElement.prototype.submit.call(form));
    });
    root.addEventListener("guide-theme-saved", () => {
      pictures.clear();
      picture();
      status.textContent = label.unsaved;
    });
    window.addEventListener(
      "popstate",
      () =>
        tab(
          new URL(location.href).searchParams.get("tab") === "sources"
            ? "sources"
            : "pages",
          false,
        ),
    );
    try {
      const saved = JSON.parse(sessionStorage.getItem(storage));
      if (saved?.revision === revision && Array.isArray(saved.pages)) {
        pages = saved.pages;
        for (const [n, value] of saved.numbers || []) pendingNumbers.set(n, value);
        if (saved.on !== undefined) form.elements.guide_on.checked = saved.on;
        sync();
      } else if (saved) sessionStorage.removeItem(storage);
    } catch { /* A stale tab draft can be discarded. */ }
    drawIndex();
    drawSources();
    controls();
    if (!sheets(active).includes(sub)) sub = sheets(active)[0];
    drawHotspots();
    find("[data-guide-off]").hidden = !guide() || Boolean(guide().element) || form.elements.guide_on.checked;
    if (image.hidden || dirty) picture();
  }
  document.querySelectorAll("[data-atlas]").forEach(mount);
})();
