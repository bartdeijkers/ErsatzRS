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
      .locator('section[class*="content"]:not([data-admin-resource])')
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
      .locator('section[class*="content"]:not([data-admin-resource])')
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
    await expect(page.getByRole('link', { name: 'Back to search' })).toBeVisible();

    const saveBar = page
      .getByRole('button', { name: /save smart collection/i })
      .locator('xpath=ancestor::div[contains(@class, "action")][1]');
    const saveBarBox = await saveBar.boundingBox();
    expect(saveBarBox?.height).toBeGreaterThanOrEqual(60);
    expect(saveBarBox?.height).toBeLessThanOrEqual(80);

    await page.screenshot({ path: screenshotPath(testInfo, 'smart-collections-editor-desktop'), fullPage: true });
  });

  test('global search hands its query to the shallow-first collection composer', async ({ page }, testInfo) => {
    const query = '(genre:drama OR genre:comedy) AND (actor:Alice OR actor:Bob)';

    await page.goto(`/search?query=${encodeURIComponent(query)}`);
    await waitForHydration(page);
    const create = page.getByRole('link', { name: 'Create smart collection' });
    await expect(create).toBeVisible();
    await create.click();

    await expect(page).toHaveURL(/\/media\/smart-collections\/add\?query=/);
    await expect(page.locator('form[data-smart-collection-editor-hydrated="true"]')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Match all' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Match any' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Exclude any' })).toBeVisible();
    await page.getByText('Canonical query').click();
    await expect(page.locator('textarea[name="query"]')).toHaveValue(query);
    await expect(page.locator('form[data-smart-collection-editor-hydrated="true"]')).toHaveAttribute(
      'data-composer-all',
      '1',
    );
    await expect(page.locator('form[data-smart-collection-editor-hydrated="true"]')).toHaveAttribute(
      'data-composer-any',
      '2',
    );
    await page.getByText('Saved searches').first().click();
    await page.screenshot({
      path: screenshotPath(testInfo, 'search-composer-advanced-desktop'),
      fullPage: true,
    });

    await page.getByRole('button', { name: 'Edit nested group' }).focus();
    await page.keyboard.press('Enter');
    await expect(page.getByRole('heading', { name: 'Match any' })).toBeVisible();
    await expect(page.getByRole('button', { name: /back to rules/i })).toBeVisible();

    await page.setViewportSize({ width: 320, height: 800 });
    await expect(page.getByRole('button', { name: 'Toggle navigation' })).toHaveAttribute(
      'aria-expanded',
      'false',
    );
    await expect
      .poll(() =>
        page.locator('[data-nav-drawer]').evaluate((drawer) => drawer.getBoundingClientRect().right),
      )
      .toBeLessThanOrEqual(0);
    await expect
      .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth))
      .toBe(true);
    await page.screenshot({
      path: screenshotPath(testInfo, 'search-composer-nested-mobile'),
      fullPage: true,
    });
  });

  test('no-JS composer preserves the carried query and can save it', async ({ browser, baseURL, page }) => {
    const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await context.newPage();
    const name = uniqueName('Fallback Smart Collection');
    const query = 'genre:"Science Fiction" AND year:[1980 TO 1999]';

    await noJsPage.goto(`/media/smart-collections/add?query=${encodeURIComponent(query)}`);
    await expect(noJsPage.locator('textarea[name="query"]')).toHaveValue(query);
    await noJsPage.locator('input[name="name"]').fill(name);
    await noJsPage.getByRole('button', { name: /save smart collection/i }).click();
    await expect(noJsPage).toHaveURL(/\/media\/smart-collections$/);
    await context.close();

    await page.goto('/media/smart-collections');
    await expect(page.getByRole('cell', { name })).toBeVisible();
  });

  test('structured edits cannot save a stale canonical query', async ({ page }) => {
    const name = uniqueName('Composer race guard');

    await page.goto('/media/smart-collections/add?query=title%3AInitial');
    await expect(page.locator('form[data-smart-collection-editor-hydrated="true"]')).toBeVisible();
    await page.locator('input[name="name"]').fill(name);
    await page.getByRole('button', { name: 'Edit rule' }).click();

    const dialog = page.getByRole('dialog', { name: 'Edit rule' });
    await dialog.getByLabel('Value').first().fill('Star Wars');
    await dialog.getByRole('button', { name: 'Apply' }).click();

    // Click immediately. Playwright can only complete it after the current
    // model has composed, proving the stale pre-edit query is never submitted.
    await page.getByRole('button', { name: /save smart collection/i }).click();
    await expect(page).toHaveURL(/\/media\/smart-collections$/);

    const row = page.getByRole('row').filter({ hasText: name });
    await row.getByRole('link', { name: /edit smart collection/i }).click();
    await page.getByText('Canonical query').click();
    await expect(page.locator('textarea[name="query"]')).toHaveValue('title:Star\\ Wars');
  });

  test('incomplete carried queries stay invalid after initial projection', async ({ page }) => {
    await page.goto('/media/smart-collections/add?query=title%3A');
    await expect(page.locator('form[data-smart-collection-editor-hydrated="true"]')).toBeVisible();
    await expect(page.getByText('Query needs attention')).toBeVisible();
    await expect(page.getByText('Every search rule needs a value.')).toBeVisible();
    await expect(page.getByRole('button', { name: /save smart collection/i })).toBeDisabled();
  });

  test('advanced queries can be saved, recalled, modified, and updated', async ({ page }) => {
    const savedName = uniqueName('Reusable advanced search');
    const originalQuery = '(genre:drama OR genre:comedy) AND year:[1980 TO 1999]';
    const updatedQuery = '(genre:drama OR genre:comedy) AND year:[1990 TO 2020]';

    await page.goto(`/media/smart-collections/add?query=${encodeURIComponent(originalQuery)}`);
    await expect(page.locator('form[data-smart-collection-editor-hydrated="true"]')).toBeVisible();
    await page.getByText('Saved searches').first().click();
    await page.locator('input[name="saved_search_name"]').fill(savedName);
    await page.getByRole('button', { name: 'Save search' }).click();
    await expect(page.getByText('Saved search saved').first()).toBeVisible();

    await page.getByText('Canonical query').click();
    await page.locator('textarea[name="query"]').fill(updatedQuery);
    await page.getByRole('button', { name: 'Update saved search' }).click();
    await expect(page.getByText('Saved search saved').first()).toBeVisible();

    await page.getByRole('button', { name: 'New saved search' }).click();
    await page.locator('select[name="saved_search_id"]').selectOption({ label: savedName });
    await page.getByRole('button', { name: 'Load saved search' }).click();
    await expect(page.locator('textarea[name="query"]')).toHaveValue(updatedQuery);
    await expect(page.getByText('Saved search loaded').first()).toBeVisible();
  });

  test('saved advanced queries retain recall and update fallbacks without JavaScript', async ({ browser, baseURL }) => {
    const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await context.newPage();
    const savedName = uniqueName('Fallback saved search');
    const originalQuery = 'genre:drama AND year:[1980 TO 1999]';
    const updatedQuery = 'genre:drama AND year:[1990 TO 2020]';

    await noJsPage.goto(`/media/smart-collections/add?query=${encodeURIComponent(originalQuery)}`);
    await noJsPage.getByText('Saved searches').first().click();
    await noJsPage.locator('input[name="saved_search_name"]').fill(savedName);
    await noJsPage.getByRole('button', { name: 'Save search' }).click();
    await expect(noJsPage).toHaveURL(/saved=search/);

    await noJsPage.getByText('Saved searches').first().click();
    await noJsPage.locator('select[name="saved_search_id"]').selectOption({ label: savedName });
    await noJsPage.getByRole('button', { name: 'Load saved search' }).click();
    await expect(noJsPage).toHaveURL(/loaded=search/);
    await expect(noJsPage.locator('textarea[name="query"]')).toHaveValue(originalQuery);

    await noJsPage.getByText('Saved searches').first().click();
    await noJsPage.getByText('Canonical query').click();
    await noJsPage.locator('textarea[name="query"]').fill(updatedQuery);
    await noJsPage.getByRole('button', { name: 'Update saved search' }).click();
    await expect(noJsPage).toHaveURL(/saved=search/);

    await noJsPage.getByText('Saved searches').first().click();
    await noJsPage.getByRole('button', { name: 'Load saved search' }).click();
    await expect(noJsPage.locator('textarea[name="query"]')).toHaveValue(updatedQuery);
    await context.close();
  });

  test('no-JS save failures preserve the exact search return URL', async ({ browser, baseURL, page }) => {
    const name = uniqueName('Duplicate smart');
    const returnTo = '/search?query=genre%3Adrama%20AND%20year%3A2020';

    await page.request.post('/api/smart-collections/save', {
      form: { id: '0', name, query: 'genre:drama', return_to: returnTo },
      maxRedirects: 0,
    });

    const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await context.newPage();
    await noJsPage.goto(
      `/media/smart-collections/add?query=genre%3Acomedy&return_to=${encodeURIComponent(returnTo)}`,
    );
    await noJsPage.locator('input[name="name"]').fill(name);
    await noJsPage.getByRole('button', { name: /save smart collection/i }).click();

    await expect(noJsPage).toHaveURL(/\/media\/smart-collections\/add\?error=save/);
    await expect(noJsPage.locator('input[name="name"]')).toHaveValue(name);
    await expect(noJsPage.getByRole('link', { name: 'Back to search' })).toHaveAttribute('href', returnTo);
    await context.close();
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
