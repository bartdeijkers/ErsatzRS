import { test, expect, Page } from '@playwright/test';
import { seedMetadataEditor } from './fixtures/addon-metadata';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

/** Hydration mismatches are logged to the console and then abort the module;
 *  they never surface as page errors. */
function watchConsoleErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  page.on('pageerror', error => errors.push(error.message));
  return errors;
}

test.describe('add-on list publication islands', () => {
  test.skip(!dbPath, 'An isolated E2E database is required');
  for (const task of ['', '/activity']) {
    test(`hydrate without errors on ${task || 'Items'}`, async ({ page }, info) => {
      const seed = seedMetadataEditor(info);
      const errors = watchConsoleErrors(page);
      await page.goto(`/media/addon-lists/${seed.listId}${task}`);
      await page.waitForFunction(() => (window as any).__ersatzrs_hydrated === true);
      // Islands hydrate after the page flag; give them time to report.
      await page.waitForTimeout(3000);
      expect(errors.filter(e => /hydrat|unreachable/i.test(e))).toEqual([]);
    });
  }
});
