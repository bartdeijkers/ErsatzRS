import { expect, type Locator, type Page } from '@playwright/test';
import { DatabaseSync } from 'node:sqlite';
import { test } from './isolated-web-host';

// This fixture proves hydrated UI intent and cancellation. Only scan POSTs
// are intercepted; SSR rows, list refreshes, locks and hydration use the host.
// Real application admission/scanner execution has its own row195 gate.
const kinds = ['Local', 'Plex', 'Jellyfin', 'Emby'] as const;
const labels = {
  'en-us': {
    library: 'Deep Scan Library', collections: 'Deep Scan Collections',
    ordinary: 'Scan Library', confirm: 'Deep Scan', cancel: 'Cancel',
    warning: 'even items that appear unchanged', required: 'JavaScript is required',
  },
  nl: {
    library: 'Bibliotheek diep scannen', collections: 'Collecties diep scannen',
    ordinary: 'Bibliotheek scannen', confirm: 'Diepe scan', cancel: 'Annuleren',
    warning: 'ook items die ongewijzigd lijken', required: 'JavaScript is vereist',
  },
} as const;

function seed(databasePath: string) {
  const database = new DatabaseSync(databasePath);
  try {
    database.exec('PRAGMA busy_timeout=10000; PRAGMA foreign_keys=ON; BEGIN IMMEDIATE;');
    kinds.forEach((kind, index) => {
      const id = 870_000 + index;
      database.prepare('INSERT INTO "MediaSource" ("Id") VALUES (?)').run(id);
      if (kind === 'Local') {
        database.prepare('INSERT INTO "LocalMediaSource" ("Id") VALUES (?)').run(id);
      } else {
        database.prepare(`INSERT INTO "${kind}MediaSource" ("Id", "ServerName") VALUES (?, ?)`)
          .run(id, `Deep fixture ${kind}`);
      }
      database.prepare('INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId") VALUES (?, ?, 1, ?)')
        .run(id, `Deep fixture ${kind} <target>`, id);
      if (kind === 'Local') {
        database.prepare('INSERT INTO "LocalLibrary" ("Id") VALUES (?)').run(id);
      } else {
        database.prepare(`INSERT INTO "${kind}Library" ("Id", "ShouldSyncItems") VALUES (?, 1)`).run(id);
      }
    });
    database.exec('COMMIT;');
  } finally {
    database.close();
  }
}

async function hydrated(page: Page) {
  await page.waitForFunction(() => {
    const state = window as Window & {
      __ersatzrs_hydrated?: boolean; __ersatzrs_hydration_phase?: string;
    };
    return state.__ersatzrs_hydrated === true && state.__ersatzrs_hydration_phase === 'ready';
  });
}

async function containedInViewport(locator: Locator) {
  await expect(locator).toBeVisible();
  const bounds = await locator.evaluate(element => {
    const rect = element.getBoundingClientRect();
    return {
      left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom,
      width: window.innerWidth, height: window.innerHeight,
    };
  });
  expect(bounds.left).toBeGreaterThanOrEqual(-1);
  expect(bounds.right).toBeLessThanOrEqual(bounds.width + 1);
  expect(bounds.top).toBeGreaterThanOrEqual(-1);
  expect(bounds.bottom).toBeLessThanOrEqual(bounds.height + 1);
}

for (const language of ['en-us', 'nl'] as const) {
  test(`deep library and collections scans require one confirmation (${language})`, async ({ browser, isolatedWebHost }, testInfo) => {
    test.setTimeout(120_000);
    seed(isolatedWebHost.databasePath);
    const context = await browser.newContext({ baseURL: isolatedWebHost.baseURL });
    try {
      await context.request.post('/admin-api/settings/ui/save', {
        form: { is_dark_mode: 'true', language }, maxRedirects: 0,
      });
      const page = await context.newPage();
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => {
        if (message.type() === 'error' && /hydration|failed_to_cast_marker_node|panicked/i.test(message.text())) {
          errors.push(message.text());
        }
      });
      const requests: { route: string; fields: URLSearchParams }[] = [];
      await page.route(/\/admin-api\/scan_(library|external_collections)[^/?]*(\?.*)?$/, async route => {
        expect(route.request().method()).toBe('POST');
        requests.push({ route: route.request().url(), fields: new URLSearchParams(route.request().postData() ?? '') });
        await route.fulfill({ contentType: 'application/json', body: 'true' });
      });
      await page.goto('/media/libraries');
      await hydrated(page);
      const text = labels[language];
      const dialog = page.getByRole('dialog', { includeHidden: true });
      await expect(dialog).toHaveCount(1);
      await expect(dialog).toBeHidden();
      await expect(page.locator('[data-library-scan-javascript-required]')).toBeHidden();

      const targets = [
        ...kinds.map((kind, index) => ({ kind, id: 870_000 + index, collections: false })),
        ...kinds.filter(kind => kind !== 'Local').map(kind => ({ kind, id: 870_000 + kinds.indexOf(kind), collections: true })),
      ];
      for (const target of targets) {
        const row = page.getByRole('row').filter({
          hasText: target.collections ? `Deep fixture ${target.kind}` : `Deep fixture ${target.kind} <target>`,
        }).filter({ has: page.getByRole('button', { name: target.collections ? text.collections : text.library, exact: true }) });
        const action = row.getByRole('button', { name: target.collections ? text.collections : text.library, exact: true });
        const before = requests.length;
        await expect(action).toBeEnabled();
        await action.click();
        await expect(dialog).toBeVisible();
        await expect(dialog.locator('[data-deep-scan-target]')).toContainText(
          target.collections ? target.kind : `Deep fixture ${target.kind} <target>`,
        );
        await expect(dialog).toContainText(text.warning);
        if (target.kind === 'Local' && !target.collections) {
          await containedInViewport(dialog.locator('[data-modal-panel]'));
          const screenshot = testInfo.outputPath(`library-deep-scan-${language}-desktop.png`);
          await page.screenshot({ path: screenshot });
          await testInfo.attach(`deep-scan-${language}-desktop`, { path: screenshot, contentType: 'image/png' });
        }
        expect(requests).toHaveLength(before);
        await dialog.getByRole('button', { name: text.cancel, exact: true }).click();
        await expect(dialog).toBeHidden();
        await expect(action).toBeFocused();
        expect(requests).toHaveLength(before);

        await action.click();
        await expect(dialog).toBeVisible();
        await dialog.press('Escape');
        await expect(dialog).toBeHidden();
        await expect(action).toBeFocused();
        expect(requests).toHaveLength(before);

        await action.click();
        const confirm = dialog.getByRole('button', { name: text.confirm, exact: true });
        await confirm.focus();
        await page.keyboard.press('Tab');
        await expect(dialog.getByRole('button', { name: text.cancel, exact: true })).toBeFocused();
        await page.keyboard.press('Shift+Tab');
        await expect(confirm).toBeFocused();
        await page.keyboard.press('Enter');
        await expect(dialog).toBeHidden();
        await expect.poll(() => requests.length).toBe(before + 1);
        await expect(action).toBeEnabled();
        expect(requests).toHaveLength(before + 1);
        const sent = requests[before];
        expect(sent.fields.get(target.collections ? 'media_source_id' : 'library_id')).toBe(String(target.id));
        expect(sent.fields.get('kind')).toBe(target.kind);
        expect(sent.fields.get('deep_scan')).toBe('true');
        expect(sent.route).toContain(target.collections ? 'scan_external_collections' : 'scan_library');
      }

      const localRow = page.getByRole('row').filter({ hasText: 'Deep fixture Local <target>' });
      const before = requests.length;
      await localRow.getByRole('button', { name: text.ordinary, exact: true }).click();
      await expect.poll(() => requests.length).toBe(before + 1);
      await expect(dialog).toBeHidden();
      expect(requests[before].fields.get('deep_scan')).toBe('false');

      // The table may scroll within its own wrapper. Its Local actions and the
      // confirmation must still fit the viewport without scrolling the page
      // horizontally; the shared modal must not clip its warning or buttons.
      await page.setViewportSize({ width: 320, height: 800 });
      const localActions = localRow.locator('button, a');
      await localActions.last().scrollIntoViewIfNeeded();
      for (const action of await localActions.all()) await containedInViewport(action);
      const narrowBefore = requests.length;
      const deepLocal = localRow.getByRole('button', { name: text.library, exact: true });
      await expect(deepLocal).toBeEnabled();
      await deepLocal.click();
      await expect(dialog).toBeVisible();
      await containedInViewport(dialog.locator('[data-modal-panel]'));
      await containedInViewport(dialog.locator('[data-deep-scan-target]'));
      await containedInViewport(dialog.getByText(text.warning, { exact: false }));
      await containedInViewport(dialog.getByRole('button', { name: text.cancel, exact: true }));
      await containedInViewport(dialog.getByRole('button', { name: text.confirm, exact: true }));
      const overflow = await dialog.evaluate(element => {
        const panel = element.querySelector<HTMLElement>('[data-modal-panel]');
        if (!panel) throw new Error('Missing confirmation panel');
        return {
          document: document.documentElement.scrollWidth - document.documentElement.clientWidth,
          modal: element.scrollWidth - element.clientWidth,
          panel: panel.scrollWidth - panel.clientWidth,
          content: Array.from(panel.querySelectorAll('p')).map(paragraph => paragraph.scrollWidth - paragraph.clientWidth),
        };
      });
      expect(overflow.document).toBeLessThanOrEqual(1);
      expect(overflow.modal).toBeLessThanOrEqual(1);
      expect(overflow.panel).toBeLessThanOrEqual(1);
      for (const width of overflow.content) expect(width).toBeLessThanOrEqual(1);
      await page.screenshot({ path: testInfo.outputPath(`library-deep-scan-${language}-320px.png`) });
      expect(requests).toHaveLength(narrowBefore);
      await dialog.getByRole('button', { name: text.cancel, exact: true }).click();
      await expect(dialog).toBeHidden();
      await expect(deepLocal).toBeFocused();
      expect(requests).toHaveLength(narrowBefore);
      expect(errors).toEqual([]);
    } finally {
      await context.close();
    }
  });

  test(`library scans explain JavaScript requirement without queueing (${language}, no JS)`, async ({ browser, isolatedWebHost }) => {
    seed(isolatedWebHost.databasePath);
    const context = await browser.newContext({ baseURL: isolatedWebHost.baseURL, javaScriptEnabled: false });
    try {
      await context.request.post('/admin-api/settings/ui/save', {
        form: { is_dark_mode: 'true', language }, maxRedirects: 0,
      });
      const page = await context.newPage();
      const scanRequests: string[] = [];
      page.on('request', request => {
        if (/\/admin-api\/scan_(library|external_collections)/.test(request.url())) scanRequests.push(request.url());
      });
      await page.goto('/media/libraries');
      await expect(page.locator('[data-library-scan-javascript-required]')).toContainText(labels[language].required);
      await expect(page.locator('[data-library-scan-javascript-required]')).toBeVisible();
      await expect(page.getByRole('dialog', { includeHidden: true })).toHaveCount(1);
      await expect(page.getByRole('dialog', { includeHidden: true })).toBeHidden();
      const scans = page.locator('table button');
      await expect(scans).toHaveCount(14);
      for (const scan of await scans.all()) await expect(scan).toBeDisabled();
      await expect(page.getByRole('link', { name: language === 'nl' ? 'Media zoeken' : 'Search media', exact: true })).toHaveCount(4);
      expect(scanRequests).toEqual([]);
    } finally {
      await context.close();
    }
  });
}
