import { Browser, expect, Locator, Page, TestInfo, test } from '@playwright/test';

function uniqueName(prefix: string): string {
  return `${prefix} ${Date.now()} ${Math.floor(Math.random() * 10_000)}`;
}

function screenshotPath(testInfo: TestInfo, name: string): string {
  return `test-results/${name}-${testInfo.project.name}.png`;
}

async function forceEnglish(page: Page): Promise<void> {
  await page.request.post('/api/settings/ui/save', {
    form: {
      is_dark_mode: 'true',
      language: 'en-us',
    },
    maxRedirects: 0,
  });
}

async function waitForHydration(page: Page): Promise<void> {
  await expect
    .poll(
      () =>
        page.evaluate(
          () =>
            (window as Window & { __ersatzrs_hydrated?: boolean })
              .__ersatzrs_hydrated === true,
        ),
      { timeout: 15_000 },
    )
    .toBe(true);
}

async function expectNoRuntimeErrors(page: Page, runtimeErrors: string[]): Promise<void> {
  await expect(page.locator('[role="alert"]')).toHaveCount(0);
  expect(runtimeErrors).toEqual([]);
}

async function waitForOption(page: Page, label: string): Promise<void> {
  await expect
    .poll(async () => page.locator('option').filter({ hasText: label }).count())
    .toBeGreaterThan(0);
}

async function fillStable(locator: Locator, value: string): Promise<void> {
  await expect
    .poll(async () => {
      await locator.fill(value);
      return locator.inputValue();
    })
    .toBe(value);
}

async function clickUntilVisible(
  page: Page,
  trigger: Locator,
  target: Locator,
  timeoutMs = 15_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  let lastError: unknown;

  while (Date.now() < deadline) {
    await expect(trigger).toBeEnabled();
    await trigger.click();
    try {
      await expect(target).toBeVisible({ timeout: 1_000 });
      return;
    } catch (error) {
      lastError = error;
      await page.waitForTimeout(250);
    }
  }

  if (lastError instanceof Error) {
    throw lastError;
  }
  throw new Error('Timed out waiting for target after click');
}

async function expectNoJsForm(
  browser: Browser,
  baseURL: string | undefined,
  path: string,
  selector: string,
): Promise<void> {
  const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
  const noJsPage = await context.newPage();
  await noJsPage.goto(path);
  await expect(noJsPage.locator(selector)).toBeVisible();
  await context.close();
}

async function createSchedule(page: Page, name: string): Promise<string> {
  await page.goto('/schedules/add');
  await waitForHydration(page);
  await expect(page.getByRole('heading', { name: 'Schedule' })).toBeVisible();
  await page.locator('input[name="name"]').fill(name);
  await page.getByLabel('Cooldown Preset').selectOption('balanced');
  await expect(page.getByLabel('Videos Between Similar Items')).toHaveValue('4');
  await expect(page.getByLabel('Cooldown Duration (Minutes)')).toHaveValue('30');
  await page.getByRole('button', { name: /add schedule/i }).click();
  await expect(page).toHaveURL(/\/schedules\/\d+\/items$/);
  return page.url().match(/\/schedules\/(\d+)\/items$/)?.[1] ?? '0';
}

async function createBlock(page: Page, groupName: string, blockName: string): Promise<string> {
  await page.goto('/blocks');
  await waitForHydration(page);
  await expect(page.getByRole('heading', { name: 'Block Groups' })).toBeVisible();
  await fillStable(page.getByPlaceholder('Group Name'), groupName);
  const addGroup = page.getByRole('button', { name: /add block group/i });
  await expect(addGroup).toBeEnabled();
  await addGroup.click();
  await waitForOption(page, groupName);

  await page.locator('select').first().selectOption({ label: groupName });
  await fillStable(page.getByPlaceholder('Block Name'), blockName);
  const addBlock = page.getByRole('button', { name: /add block$/i });
  await expect(addBlock).toBeEnabled();
  await addBlock.click();
  await expect(page.getByRole('link', { name: blockName })).toBeVisible();

  const href = await page.getByRole('link', { name: blockName }).first().getAttribute('href');
  expect(href).toMatch(/^\/blocks\/\d+$/);
  return href?.match(/\/blocks\/(\d+)$/)?.[1] ?? '0';
}

async function createTemplate(page: Page, groupName: string, templateName: string): Promise<string> {
  await page.goto('/templates');
  await waitForHydration(page);
  await expect(page.getByRole('heading', { name: 'Template Groups' })).toBeVisible();
  await fillStable(page.locator('#template-group-name'), groupName);
  const addGroup = page.getByRole('button', { name: /add template group/i });
  await expect(addGroup).toBeEnabled();
  await addGroup.click();
  await waitForOption(page, groupName);

  await page.locator('#template-group').selectOption({ label: groupName });
  await fillStable(page.locator('#template-name'), templateName);
  const addTemplate = page.getByRole('button', { name: /add template$/i });
  await expect(addTemplate).toBeEnabled();
  await addTemplate.click();
  await expect(page.getByRole('link', { name: templateName })).toBeVisible();

  const href = await page.getByRole('link', { name: templateName }).first().getAttribute('href');
  expect(href).toMatch(/^\/templates\/\d+$/);
  return href?.match(/\/templates\/(\d+)$/)?.[1] ?? '0';
}

async function createDeco(page: Page, groupName: string, decoName: string): Promise<string> {
  await page.goto('/decos');
  await waitForHydration(page);
  await expect(page.getByRole('heading', { name: 'Deco Groups' })).toBeVisible();
  await fillStable(page.locator('#deco-group-name'), groupName);
  const addGroup = page.getByRole('button', { name: /add deco group/i });
  await expect(addGroup).toBeEnabled();
  await addGroup.click();
  await waitForOption(page, groupName);

  await page.locator('#deco-group').selectOption({ label: groupName });
  await fillStable(page.locator('#deco-name'), decoName);
  const addDeco = page.getByRole('button', { name: /add deco$/i });
  await expect(addDeco).toBeEnabled();
  await addDeco.click();
  await expect(page.getByRole('link', { name: decoName })).toBeVisible();

  const href = await page.getByRole('link', { name: decoName }).first().getAttribute('href');
  expect(href).toMatch(/^\/decos\/\d+$/);
  return href?.match(/\/decos\/(\d+)$/)?.[1] ?? '0';
}

async function createDecoTemplate(
  page: Page,
  groupName: string,
  templateName: string,
): Promise<string> {
  await page.goto('/deco-templates');
  await waitForHydration(page);
  await expect(page.getByRole('heading', { name: 'Deco Template Groups' })).toBeVisible();
  await fillStable(page.locator('#deco-template-group-name'), groupName);
  const addGroup = page.getByRole('button', { name: /add deco template group/i });
  await expect(addGroup).toBeEnabled();
  await addGroup.click();
  await waitForOption(page, groupName);

  await page.locator('#deco-template-group').selectOption({ label: groupName });
  await fillStable(page.locator('#deco-template-name'), templateName);
  const addTemplate = page.getByRole('button', { name: /add deco template$/i });
  await expect(addTemplate).toBeEnabled();
  await addTemplate.click();
  await expect(page.getByRole('link', { name: templateName })).toBeVisible();

  const href = await page.getByRole('link', { name: templateName }).first().getAttribute('href');
  expect(href).toMatch(/^\/deco-templates\/\d+$/);
  return href?.match(/\/deco-templates\/(\d+)$/)?.[1] ?? '0';
}

test.describe('scheduling route-family parity', () => {
  test('schedules, blocks, templates, decos, and deco templates keep hydrated and fallback workflows intact', async ({
    browser,
    baseURL,
    page,
  }, testInfo) => {
    test.skip(
      testInfo.project.name !== 'chromium',
      'This spec mutates shared scheduling state; Chromium proof avoids cross-project data races.',
    );

    const runtimeErrors: string[] = [];
    page.on('console', (message) => {
      if (message.type() === 'error') {
        runtimeErrors.push(message.text());
      }
    });
    page.on('pageerror', (error) => runtimeErrors.push(error.message));
    await forceEnglish(page);

    const scheduleName = uniqueName('E2E Schedule');
    const scheduleId = await createSchedule(page, scheduleName);
    expect(Number(scheduleId)).toBeGreaterThan(0);
    await page.goto(`/schedules/${scheduleId}`);
    await waitForHydration(page);
    await expect(page.getByLabel('Cooldown Preset')).toHaveValue('balanced');
    await expect(page.getByLabel('Videos Between Similar Items')).toHaveValue('4');
    await expect(page.getByLabel('Cooldown Duration (Minutes)')).toHaveValue('30');
    await page.getByRole('button', { name: 'Preview Effective Tags' }).click();
    await expect(page.getByText('No matching media emitted cooldown tags.')).toBeVisible();
    await page.getByLabel('Cooldown Preset').scrollIntoViewIfNeeded();
    await page.screenshot({
      path: screenshotPath(testInfo, 'schedule-cooldown-desktop'),
    });
    const mobileContext = await browser.newContext({
      baseURL,
      viewport: { width: 390, height: 844 },
    });
    const mobilePage = await mobileContext.newPage();
    const mobileRuntimeErrors: string[] = [];
    mobilePage.on('console', (message) => {
      if (message.type() === 'error' || message.type() === 'warning') {
        mobileRuntimeErrors.push(message.text());
      }
    });
    mobilePage.on('pageerror', (error) => mobileRuntimeErrors.push(error.message));
    await forceEnglish(mobilePage);
    await mobilePage.goto(`/schedules/${scheduleId}`);
    await waitForHydration(mobilePage);
    await expect(mobilePage.getByLabel('Cooldown Preset')).toHaveValue('balanced');
    await mobilePage.getByLabel('Cooldown Preset').scrollIntoViewIfNeeded();
    await mobilePage.screenshot({
      path: screenshotPath(testInfo, 'schedule-cooldown-mobile'),
    });
    await expectNoRuntimeErrors(mobilePage, mobileRuntimeErrors);
    await mobileContext.close();
    await page.goto(`/schedules/${scheduleId}/items`);
    await expect(page.getByRole('heading', { name: 'Schedule Items' })).toBeVisible();
    await clickUntilVisible(
      page,
      page.getByRole('button', { name: /add schedule item/i }),
      page.getByText('Schedule Item Filler'),
    );
    await expect(page.locator('form#schedule-items-fallback-form')).toHaveCount(0);
    await expectNoJsForm(
      browser,
      baseURL,
      `/schedules/${scheduleId}/items`,
      'form#schedule-items-fallback-form',
    );

    await page.goto('/schedules');
    await expect(page.getByRole('heading', { name: 'Schedules' })).toBeVisible();
    await page.getByPlaceholder('Search for schedules').fill(scheduleName);
    await expect(page.getByRole('cell', { name: scheduleName }).first()).toBeVisible();
    await page.screenshot({ path: screenshotPath(testInfo, 'scheduling-schedules'), fullPage: true });

    const blockGroupName = uniqueName('E2E Block Group');
    const blockName = uniqueName('E2E Block');
    const blockId = await createBlock(page, blockGroupName, blockName);
    expect(Number(blockId)).toBeGreaterThan(0);
    await page.goto(`/blocks/${blockId}`);
    await expect(page.getByRole('heading', { name: 'Block', exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: /save block/i })).toBeVisible();
    await expect(page.getByRole('button', { name: /preview/i })).toBeVisible();
    await expectNoJsForm(browser, baseURL, `/blocks/${blockId}`, 'form#block-items-fallback-form');
    await page.screenshot({ path: screenshotPath(testInfo, 'scheduling-block-editor'), fullPage: true });

    const templateGroupName = uniqueName('E2E Template Group');
    const templateName = uniqueName('E2E Template');
    const templateId = await createTemplate(page, templateGroupName, templateName);
    expect(Number(templateId)).toBeGreaterThan(0);
    await page.goto(`/templates/${templateId}`);
    await expect(page.getByRole('heading', { name: 'General' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Add Content', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Remove Content', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Content', exact: true })).toBeVisible();
    await expect(page.locator('form#template-editor-form').first()).toBeVisible();
    await page.screenshot({ path: screenshotPath(testInfo, 'scheduling-template-editor'), fullPage: true });

    const decoGroupName = uniqueName('E2E Deco Group');
    const decoName = uniqueName('E2E Deco');
    const decoId = await createDeco(page, decoGroupName, decoName);
    expect(Number(decoId)).toBeGreaterThan(0);
    await page.goto(`/decos/${decoId}`);
    await expect(page.getByRole('heading', { name: 'Deco', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Break Content', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Default Filler', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Dead Air Fallback', exact: true })).toBeVisible();
    await expect(page.locator('form#deco-editor-form').first()).toBeVisible();
    await page.screenshot({ path: screenshotPath(testInfo, 'scheduling-deco-editor'), fullPage: true });

    const decoTemplateGroupName = uniqueName('E2E Deco Template Group');
    const decoTemplateName = uniqueName('E2E Deco Template');
    const decoTemplateId = await createDecoTemplate(page, decoTemplateGroupName, decoTemplateName);
    expect(Number(decoTemplateId)).toBeGreaterThan(0);
    await page.goto(`/deco-templates/${decoTemplateId}`);
    await expect(page.getByRole('heading', { name: 'Deco Template', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Decos', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Content', exact: true })).toBeVisible();
    await expect(page.locator('form#deco-template-editor-form').last()).toBeVisible();
    await page.screenshot({
      path: screenshotPath(testInfo, 'scheduling-deco-template-editor'),
      fullPage: true,
    });

    await expectNoRuntimeErrors(page, runtimeErrors);
  });
});
