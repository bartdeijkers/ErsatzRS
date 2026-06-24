import { expect, Page, TestInfo, test } from '@playwright/test';

function uniqueName(prefix: string): string {
  return `${prefix} ${Date.now()} ${Math.floor(Math.random() * 10_000)}`;
}

async function waitForHydration(page: Page): Promise<void> {
  await expect
    .poll(
      () => page.evaluate(() => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true),
      { timeout: 15_000 },
    )
    .toBe(true);
}

async function createCollection(page: Page, name: string): Promise<void> {
  await page.goto('/media/collections/add');
  await expect(page.getByRole('heading', { name: 'Collection' })).toBeVisible();
  await expect(page.locator('form[data-collection-editor-hydrated="true"]')).toBeVisible();
  await page.locator('input[name="name"]').fill(name);
  await page.getByRole('button', { name: /add collection/i }).first().click();
  await expect(page).toHaveURL(/\/media\/collections$/);
  await waitForHydration(page);
  const search = page.getByPlaceholder('Search for manual collections');
  await search.fill(name);
  await expect(search).toHaveValue(name);
  await expect(page.getByRole('link', { name }).first()).toBeVisible();
}

function screenshotPath(testInfo: TestInfo, name: string): string {
  return `test-results/${name}-${testInfo.project.name}.png`;
}

test.describe('collections parity', () => {
  test('hydrated add page creates a manual collection and returns to the list', async ({ page }) => {
    const name = uniqueName('Hydrated Collection');

    await createCollection(page, name);
  });

  test('no-JS add form posts through the HTML fallback', async ({ browser, baseURL, page }) => {
    const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await context.newPage();
    const name = uniqueName('Fallback Collection');

    await noJsPage.goto('/media/collections/add');
    await expect(noJsPage.getByRole('heading', { name: 'Collection' })).toBeVisible();
    await noJsPage.locator('input[name="name"]').fill(name);
    await noJsPage.getByRole('button', { name: /add collection/i }).first().click();
    await expect(noJsPage).toHaveURL(/\/media\/collections$/);
    await context.close();

    await page.goto('/media/collections');
    await waitForHydration(page);
    const search = page.getByPlaceholder('Search for manual collections');
    await search.fill(name);
    await expect(search).toHaveValue(name);
    await expect(page.getByRole('link', { name }).first()).toBeVisible();
  });

  test('manual collection list and empty detail keep the MudBlazor geometry contract', async ({ page }, testInfo) => {
    const name = uniqueName('Visual Collection');
    await createCollection(page, name);

    const actionBar = page
      .getByRole('link', { name: /add collection/i })
      .locator('xpath=ancestor::div[contains(@class, "action")][1]');
    await expect(actionBar).toBeVisible();
    const actionBox = await actionBar.boundingBox();
    expect(actionBox?.height).toBeGreaterThanOrEqual(60);
    expect(actionBox?.height).toBeLessThanOrEqual(80);

    const content = page
      .locator('section')
      .filter({ has: page.getByRole('heading', { name: 'Manual Collections' }) })
      .first();
    const contentBox = await content.boundingBox();
    expect(contentBox?.width).toBeGreaterThan(700);
    expect(contentBox?.width).toBeLessThanOrEqual(1540);

    await page.screenshot({ path: screenshotPath(testInfo, 'collections-list-desktop'), fullPage: true });
    await page.getByRole('link', { name }).first().click();
    await expect(page).toHaveURL(/\/media\/collections\/\d+$/);
    await expect(page.getByText('This collection has no items yet')).toBeVisible();

    const detailEdit = page.getByRole('link', { name: /edit collection/i }).first();
    await expect(detailEdit).toBeVisible();
    const detailBar = detailEdit.locator('xpath=ancestor::div[contains(@class, "action")][1]');
    const detailBox = await detailBar.boundingBox();
    expect(detailBox?.height).toBeGreaterThanOrEqual(60);
    expect(detailBox?.height).toBeLessThanOrEqual(80);

    await page.screenshot({ path: screenshotPath(testInfo, 'collections-detail-desktop'), fullPage: true });
  });

  test('smart collection list and editor keep the MudBlazor geometry contract', async ({ page }, testInfo) => {
    const name = uniqueName('Smart Visual Collection');

    await page.request.post('/api/smart-collections/save', {
      form: {
        id: '0',
        name,
        query: 'media_kind:movie',
      },
      maxRedirects: 0,
    });

    await page.goto('/media/smart-collections');
    await expect(page.getByRole('heading', { name: 'Smart Collections', exact: true })).toBeVisible();
    await expect(page.getByRole('cell', { name })).toBeVisible();

    const content = page
      .locator('section')
      .filter({ has: page.getByRole('heading', { name: 'Smart Collections', exact: true }) })
      .first();
    const contentBox = await content.boundingBox();
    expect(contentBox?.width).toBeGreaterThan(700);
    expect(contentBox?.width).toBeLessThanOrEqual(1540);

    await page.screenshot({ path: screenshotPath(testInfo, 'smart-collections-list-desktop'), fullPage: true });
    await expect(page.getByText(/total_pages|page\.update|saturating_sub/)).toHaveCount(0);
    const editHref = await page.getByRole('link', { name: /edit smart collection/i }).first().getAttribute('href');
    expect(editHref).toMatch(/^\/media\/smart-collections\/\d+\/edit$/);
    await page.goto(editHref ?? '/media/smart-collections');
    await expect(page.getByRole('heading', { name: 'Smart Collection', exact: true })).toBeVisible();
    await expect(page.locator('form[data-smart-collection-editor-hydrated="true"]')).toBeVisible();
    await expect(page.getByRole('link', { name: 'Back' })).toHaveCount(0);

    const saveBar = page
      .getByRole('button', { name: /save smart collection/i })
      .locator('xpath=ancestor::div[contains(@class, "action")][1]');
    const saveBarBox = await saveBar.boundingBox();
    expect(saveBarBox?.height).toBeGreaterThanOrEqual(60);
    expect(saveBarBox?.height).toBeLessThanOrEqual(80);

    await page.screenshot({ path: screenshotPath(testInfo, 'smart-collections-editor-desktop'), fullPage: true });
  });

  test('manual collections list keeps the action bar usable on mobile', async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/media/collections');

    const add = page.getByRole('link', { name: /add collection/i }).first();
    await expect(add).toBeVisible();
    const addBox = await add.boundingBox();
    expect(addBox?.width).toBeGreaterThan(40);
    expect(addBox?.height).toBeGreaterThan(32);

    await page.screenshot({ path: screenshotPath(testInfo, 'collections-list-mobile'), fullPage: true });
  });
});
