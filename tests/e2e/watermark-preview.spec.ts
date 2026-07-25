import {
  APIRequestContext,
  expect,
  Page,
  TestInfo,
  test,
} from '@playwright/test';
import { execFileSync } from 'child_process';
import { readFileSync } from 'fs';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;
const INVALID_WATERMARK_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAlgAAAEsCAIAAACQX1rBAAAACXBIWXMAAAABAAAAAQBPJcTWAAAEtklEQVR4nO3cQQ0CUQxAQUgwAjbQAuLQwtpYLdxQwKY/vBkD7e2ll56v78cJAKou0wsAwCQhBCBNCAFIE0IA0oQQgDQhBCBNCAFIE0IA0oQQgDQhBCBNCAFIE0IA0oQQgDQhBCBNCAFIE0IA0oQQgDQhBCBNCAFIE0IA0oQQgDQhBCBNCAFIE0IA0oQQgDQhBCBNCAFIE0IA0oQQgDQhBCBNCAFIE0IA0oQQgDQhBCBNCAFIGwvhfn9NjQZgQbftOTLXRQhAmhACkCaEAKQJIQBpQghAmhACkCaEAKQJIQBpQghAmhACkCaEAKQtHcKpv3MA/NbK/6WXDiEAHE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDSlg7hfn9NrwDAn1s6hABwNCEEIE0IAUgTQgDShBCANCEEIE0IAUgTQgDShBCANCEEIE0IAUgbC+Fte06NBoAvFyEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApAmhACkCSEAaUIIQJoQApD2Ac8qDvD9ex54AAAAAElFTkSuQmCC',
  'base64',
);
const WATERMARK_PNG = readFileSync('artwork/Ersatzrs.png');

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for watermark seeding');
  }
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

async function forceEnglish(page: Page): Promise<void> {
  await page.request.post('/api/settings/ui/save', {
    form: { is_dark_mode: 'true', language: 'en-us' },
    maxRedirects: 0,
  });
}

async function uploadWatermark(
  request: APIRequestContext,
  buffer = WATERMARK_PNG,
): Promise<string> {
  const response = await request.post('/api/watermarks/image', {
    multipart: {
      file: {
        name: 'p7c-watermark.png',
        mimeType: 'image/png',
        buffer,
      },
    },
  });
  expect(response.ok()).toBeTruthy();
  const payload = await response.json() as { path: string };
  expect(payload.path).toMatch(/^[0-9a-f-]+\.png$/);
  return payload.path;
}

type WatermarkSeed = {
  ids: [number, number, number];
  names: [string, string, string];
  imagePath: string;
};

async function seedWatermarks(page: Page, testInfo: TestInfo): Promise<WatermarkSeed> {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, character) => total + character.charCodeAt(0),
    0,
  ) * 100;
  const ids: [number, number, number] = [
    740_001 + projectOffset,
    740_002 + projectOffset,
    740_003 + projectOffset,
  ];
  const names: [string, string, string] = [
    `P7C ${testInfo.project.name} 1 Alpha`,
    `P7C ${testInfo.project.name} 2 Bravo`,
    `P7C ${testInfo.project.name} 3 Charlie`,
  ];
  const imagePath = await uploadWatermark(page.request);

  sqlite(`
DELETE FROM "ChannelWatermark" WHERE "Name" LIKE 'P7C %';
INSERT INTO "ChannelWatermark" (
  "Id", "DurationSeconds", "FrequencyMinutes", "HorizontalMarginPercent",
  "Image", "ImageSource", "Location", "Mode", "Name", "Opacity",
  "OpacityExpression", "OriginalContentType", "PlaceWithinSourceContent",
  "Size", "VerticalMarginPercent", "WidthPercent", "ZIndex"
) VALUES
  (${ids[0]}, 15, 15, 5, '${imagePath}', 0, 0, 1, '${names[0]}', 100, NULL, 'image/png', 0, 1, 5, 15, 0),
  (${ids[1]}, 15, 15, 5, '${imagePath}', 0, 0, 1, '${names[1]}', 100, NULL, 'image/png', 0, 1, 5, 15, 0),
  (${ids[2]}, 15, 15, 5, '${imagePath}', 0, 0, 1, '${names[2]}', 100, NULL, 'image/png', 0, 1, 5, 15, 0);
`);
  return { ids, names, imagePath };
}

async function waitForHydration(page: Page): Promise<void> {
  await expect.poll(
    () => page.evaluate(
      () => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
    ),
    { timeout: 15_000, message: 'watermark editor did not finish hydration' },
  ).toBe(true);
}

async function expectContained(page: Page): Promise<void> {
  const containment = await page.locator('form[action="/api/watermarks/save"]').evaluate(
    (form) => {
      const preview = form.querySelector<HTMLElement>('[data-state]');
      const rect = preview?.getBoundingClientRect();
      return {
        documentFits: document.documentElement.scrollWidth <= window.innerWidth,
        formFits: form.scrollWidth <= form.clientWidth,
        previewLeft: rect?.left ?? -1,
        previewRight: rect?.right ?? Number.POSITIVE_INFINITY,
        viewportWidth: window.innerWidth,
      };
    },
  );
  expect(containment.documentFits).toBe(true);
  expect(containment.formFits).toBe(true);
  expect(containment.previewLeft).toBeGreaterThanOrEqual(0);
  expect(containment.previewRight).toBeLessThanOrEqual(containment.viewportWidth + 1);
}

test.describe('truthful watermark preview and comparison navigation', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed watermarks');

  test('proves actual geometry, guides, states, ordering, dirty protection, and containment', async ({
    page,
  }, testInfo) => {
    test.setTimeout(90_000);
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    page.on('console', (message) => {
      if (message.type() === 'error') {
        errors.push(message.text());
      }
    });

    await forceEnglish(page);
    const seed = await seedWatermarks(page, testInfo);
    const previewRequest = '**/*preview_watermark_asset_placement*';
    await page.route(previewRequest, async (route) => {
      await new Promise((resolve) => setTimeout(resolve, 700));
      await route.continue();
    });

    await page.goto(`/watermarks/${seed.ids[1]}`);
    await waitForHydration(page);
    const preview = page.locator('div[data-state]').filter({
      has: page.locator('img[data-preview-kind="asset"]'),
    });
    await expect(preview).toHaveAttribute('data-state', 'loading');
    await expect(preview).toHaveAttribute('data-state', 'ready', { timeout: 20_000 });
    await page.unroute(previewRequest);

    const image = preview.locator('img[data-preview-kind="asset"]');
    await expect(image).toHaveAttribute('data-asset-width', '1024');
    await expect(image).toHaveAttribute('data-asset-height', '1024');
    await expect(image).toHaveAttribute('data-rendered-width', '171');
    await expect(image).toHaveAttribute('data-rendered-height', '171');
    await expect(page.getByText('Native asset and rendered size: 1024×1024 px · 171×171 px'))
      .toBeVisible();

    const framePixels = await image.evaluate((element: HTMLImageElement) => {
      const canvas = document.createElement('canvas');
      canvas.width = element.naturalWidth;
      canvas.height = element.naturalHeight;
      const context = canvas.getContext('2d');
      if (!context) {
        throw new Error('2D canvas unavailable');
      }
      context.drawImage(element, 0, 0);
      return {
        sixteenNine: Array.from(context.getImageData(1, 1, 1, 1).data),
        fourThree: Array.from(context.getImageData(40, 6, 1, 1).data),
        watermark: Array.from(context.getImageData(218, 85, 1, 1).data),
      };
    });
    expect(framePixels.sixteenNine).not.toEqual(framePixels.fourThree);
    expect(framePixels.sixteenNine[1]).toBeGreaterThan(framePixels.sixteenNine[0]);
    expect(framePixels.fourThree[0]).toBeGreaterThan(framePixels.fourThree[2]);
    expect(framePixels.watermark[1]).toBeGreaterThan(framePixels.watermark[0]);
    await expect(page.getByText('16:9 frame', { exact: true })).toBeVisible();
    await expect(page.getByText('4:3 safe frame', { exact: true })).toBeVisible();

    const previous = page.getByRole('button', { name: 'Previous watermark' });
    const next = page.getByRole('button', { name: 'Next watermark' });
    await expect(previous).toHaveAttribute('title', `${seed.names[0]} — ${seed.ids[0]}`);
    await expect(next).toHaveAttribute('title', `${seed.names[2]} — ${seed.ids[2]}`);

    const nameInput = page.locator('input[name="name"]');
    await nameInput.fill(`${seed.names[1]} edited`);
    await next.click();
    await expect(page.getByText('Unsaved changes', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Cancel' }).click();
    await expect(page).toHaveURL(new RegExp(`/watermarks/${seed.ids[1]}$`));
    await expect(nameInput).toHaveValue(`${seed.names[1]} edited`);

    await next.click();
    await page.getByRole('button', { name: 'Discard changes' }).click();
    await expect(page).toHaveURL(new RegExp(`/watermarks/${seed.ids[2]}$`));
    await waitForHydration(page);
    await expect(page.getByRole('button', { name: 'Next watermark' })).toBeDisabled();

    await page.goto(`/watermarks/${seed.ids[0]}`);
    await waitForHydration(page);
    await expect(page.getByRole('button', { name: 'Previous watermark' })).toBeDisabled();

    const invalidImagePath = await uploadWatermark(page.request, INVALID_WATERMARK_PNG);
    sqlite(`
UPDATE "ChannelWatermark" SET "Image" = '${invalidImagePath}' WHERE "Id" = ${seed.ids[1]};
`);
    await page.goto(`/watermarks/${seed.ids[1]}`);
    await waitForHydration(page);
    await expect(preview).toHaveAttribute('data-state', 'error', { timeout: 20_000 });
    await expect(page.getByText(/Preview failed:/)).toBeVisible();
    sqlite(`
UPDATE "ChannelWatermark" SET "Image" = '${seed.imagePath}' WHERE "Id" = ${seed.ids[1]};
`);

    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(`/watermarks/${seed.ids[1]}`);
    await waitForHydration(page);
    await expect(preview).toHaveAttribute('data-state', 'ready', { timeout: 20_000 });
    await expectContained(page);

    expect(errors.filter((message) =>
      /failed_to_cast_marker_node|hydration|panicked/i.test(message)
    )).toEqual([]);
  });
});
