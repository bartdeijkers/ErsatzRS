import { expect, test, type Page } from '@playwright/test';
import { waitForHydration } from './hydration';

// Shared table row actions: fixed icon slots in an "Actions" column on wide
// screens, cards with 44 px targets on narrow ones, and no horizontal page
// scroll either way.

async function openList(page: Page, path: string) {
  expect((await page.goto(path))?.status()).toBe(200);
  await waitForHydration(page);
  await expect(page.locator('[data-row-actions]').first()).toBeVisible();
}

async function pageScrolls(page: Page): Promise<boolean> {
  return page.evaluate(
    () => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
  );
}

for (const path of ['/channels', '/ffmpeg']) {
  test(`${path} keeps row actions in view at 1440 px`, async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await openList(page, path);
    await expect(page.getByRole('columnheader', { name: 'Actions', exact: true })).toBeVisible();
    const deleteButton = page.locator('[data-row-actions]').first().locator('button').last();
    await expect(deleteButton).toBeInViewport();
    const size = await deleteButton.boundingBox();
    expect(size?.width).toBeCloseTo(36, 0);
    expect(await pageScrolls(page)).toBe(false);
    const tableScrolls = await page
      .locator('table')
      .first()
      .evaluate(table => {
        const scroll = table.parentElement!;
        return scroll.scrollWidth > scroll.clientWidth + 1;
      });
    expect(tableScrolls).toBe(false);
  });

  test(`${path} turns rows into cards at 390 px`, async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await openList(page, path);
    await expect(page.locator('thead').first()).toBeHidden();
    const row = page.locator('tbody tr').first();
    expect(await row.evaluate(element => getComputedStyle(element).display)).toBe('grid');
    for (const action of await row.locator('[data-row-actions] > *').all()) {
      const box = await action.boundingBox();
      if (box) expect(box.height).toBeGreaterThanOrEqual(44);
    }
    expect(await pageScrolls(page)).toBe(false);
  });
}
