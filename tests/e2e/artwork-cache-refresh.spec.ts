import { expect, type Locator, type Page } from '@playwright/test';
import { mkdtempSync, rmSync, statSync, utimesSync, writeFileSync } from 'node:fs';
import { createServer, request as httpRequest, type Server } from 'node:http';
import { dirname, join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { test } from './isolated-web-host';

// Valid synthetic 8x8 RGBA PNGs; no private media or image dependency.
const png = {
  red: Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAYAAADED76LAAAAEklEQVR4nGP4z8DwHx9mGBkKAMLXf4EvceABAAAAAElFTkSuQmCC', 'base64'),
  blue: Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAYAAADED76LAAAAEUlEQVR4nGNgYPj/Hz8eEQoAQ1d/gZtbdTsAAAAASUVORK5CYII=', 'base64'),
  green: Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAYAAADED76LAAAAD0lEQVR4nGNg+E8AjgwFAIMXf4H6lHc8AAAAAElFTkSuQmCC', 'base64'),
};
const title = 'Artwork cache fixture';
const mediaId = 890_195;
const detailPath = `/media/remote/streams/${mediaId}`;

function seed(databasePath: string, imagePath: string) {
  const database = new DatabaseSync(databasePath);
  try {
    database.exec('PRAGMA busy_timeout=10000; PRAGMA foreign_keys=ON; BEGIN IMMEDIATE;');
    database.prepare('INSERT INTO "MediaSource" ("Id") VALUES (?)').run(mediaId);
    database.prepare('INSERT INTO "LocalMediaSource" ("Id") VALUES (?)').run(mediaId);
    database.prepare('INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (?, ?, 7, ?)')
      .run(mediaId, title, mediaId);
    database.prepare('INSERT INTO "LocalLibrary" ("Id") VALUES (?)').run(mediaId);
    database.prepare('INSERT INTO "LibraryPath" ("Id","Path","LibraryId") VALUES (?, ?, ?)')
      .run(mediaId, dirname(imagePath), mediaId);
    database.prepare('INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (?, ?, 0)')
      .run(mediaId, mediaId);
    database.prepare('INSERT INTO "RemoteStream" ("Id","Url","Liveness","IsLive") VALUES (?, ?, 2, 1)')
      .run(mediaId, 'https://example.test/synthetic-live');
    database.prepare('INSERT INTO "RemoteStreamMetadata" ("Id","RemoteStreamId","MetadataKind","Title","SortTitle","DateAdded","DateUpdated") VALUES (?, ?, 0, ?, ?, ?, ?)')
      .run(mediaId, mediaId, title, title, '2026-10-07T00:00:00Z', '2026-10-07T00:00:00Z');
    database.prepare('INSERT INTO "MediaVersion" ("Id","RemoteStreamId","Name","Duration","DateAdded","DateUpdated","Width","Height","VideoScanKind") VALUES (?, ?, ?, ?, ?, ?, 8, 8, 0)')
      .run(mediaId, mediaId, 'Synthetic source', '00:01:00', '2026-10-07T00:00:00Z', '2026-10-07T00:00:00Z');
    database.prepare('INSERT INTO "Artwork" ("Id","RemoteStreamMetadataId","ArtworkKind","Path","DateAdded","DateUpdated") VALUES (?, ?, 1, ?, ?, ?)')
      .run(mediaId, mediaId, imagePath, '2026-10-07T00:00:00Z', '2026-10-07T00:00:00Z');
    database.exec('COMMIT;');
  } finally {
    database.close();
  }
}

async function pixel(image: Locator): Promise<number[]> {
  await expect(image).toBeVisible();
  await expect.poll(() => image.evaluate(node => {
    const img = node as HTMLImageElement;
    return img.complete && img.naturalWidth === 8 && img.naturalHeight === 8;
  })).toBe(true);
  return image.evaluate(node => {
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = 8;
    const context = canvas.getContext('2d');
    if (!context) throw new Error('Canvas pixel decoder unavailable');
    context.drawImage(node as HTMLImageElement, 0, 0);
    return Array.from(context.getImageData(4, 4, 1, 1).data);
  });
}

async function primePriorReleaseImage(page: Page, oldPath: string) {
  await page.goto('/healthz');
  await page.evaluate(async url => {
    const image = new Image();
    image.alt = 'Prior release thumbnail';
    const loaded = new Promise<void>((resolve, reject) => {
      image.onload = () => resolve();
      image.onerror = () => reject(new Error('Prior release thumbnail did not decode'));
    });
    image.src = url;
    document.body.append(image);
    await loaded;
  }, oldPath);
  expect(await pixel(page.getByRole('img', { name: 'Prior release thumbnail' })))
    .toEqual([255, 0, 0, 255]);
}

// Only the old image response is synthetic. Current images and detail SSR are
// forwarded unchanged from the real host. No Playwright route handlers exist:
// installing one would disable the browser HTTP cache being tested.
async function priorReleaseProxy(target: string, oldPath: string) {
  let oldRequests = 0;
  let forwardedCurrentImages = 0;
  const server = createServer((incoming, outgoing) => {
    if (incoming.url === oldPath) {
      oldRequests += 1;
      outgoing.writeHead(200, {
        'Content-Type': 'image/png', 'Cache-Control': 'public, max-age=3600',
        'Content-Length': png.red.length,
      });
      outgoing.end(png.red);
      return;
    }
    const destination = new URL(incoming.url ?? '/', target);
    const upstream = httpRequest(destination, {
      method: incoming.method, headers: { ...incoming.headers, host: destination.host },
    }, response => {
      if (incoming.url === `${oldPath}?v=2`) forwardedCurrentImages += 1;
      outgoing.writeHead(response.statusCode ?? 502, response.headers);
      response.pipe(outgoing);
    });
    upstream.setTimeout(15_000, () => upstream.destroy(new Error('Fixture upstream timeout')));
    upstream.on('error', () => {
      if (!outgoing.headersSent) outgoing.writeHead(502);
      outgoing.end();
    });
    incoming.on('aborted', () => upstream.destroy());
    incoming.pipe(upstream);
  });
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('No fixture HTTP port');
  return {
    server, baseURL: `http://127.0.0.1:${address.port}`,
    oldRequests: () => oldRequests, forwardedCurrentImages: () => forwardedCurrentImages,
  };
}

async function closeServer(server: Server) {
  server.closeIdleConnections();
  await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
}

test('local artwork upgrade bypasses a genuinely warmed prior-release URL', async ({ browser, isolatedWebHost }, testInfo) => {
  test.setTimeout(60_000);
  const root = mkdtempSync(join(dirname(isolatedWebHost.databasePath), 'artwork-upgrade-'));
  const imagePath = join(root, 'synthetic-thumbnail.png');
  writeFileSync(imagePath, png.blue);
  seed(isolatedWebHost.databasePath, imagePath);
  const oldPath = `/artwork/thumbnails/${encodeURIComponent(imagePath)}`;
  const proxy = await priorReleaseProxy(isolatedWebHost.baseURL, oldPath);
  const context = await browser.newContext({ baseURL: proxy.baseURL });
  try {
    const page = await context.newPage();
    await primePriorReleaseImage(page, oldPath);
    await primePriorReleaseImage(page, oldPath);
    expect(proxy.oldRequests()).toBe(1);
    const imageResponse = page.waitForResponse(response =>
      response.url() === `${proxy.baseURL}${oldPath}?v=2` && response.status() === 200);
    await page.goto(detailPath);
    await expect(page.getByRole('heading', { name: title, exact: true })).toBeVisible();
    const image = page.getByRole('img', { name: title, exact: true });
    await expect(image).toHaveAttribute('src', `${oldPath}?v=2`);
    expect(await pixel(image)).toEqual([0, 0, 255, 255]);
    expect((await imageResponse).headers()['cache-control']).toBe('no-cache');
    expect(proxy.forwardedCurrentImages()).toBe(1);
    expect(proxy.oldRequests()).toBe(1);
    await testInfo.attach('artwork-upgrade-proof', {
      body: JSON.stringify({ oldNetworkRequests: proxy.oldRequests(), currentForwardedImages: proxy.forwardedCurrentImages(), decodedPixel: [0, 0, 255, 255] }),
      contentType: 'application/json',
    });
    await page.screenshot({ path: testInfo.outputPath('artwork-upgrade.png') });
  } finally {
    try {
      await context.close();
    } finally {
      try {
        await closeServer(proxy.server);
      } finally {
        rmSync(root, { recursive: true, force: true });
      }
    }
  }
});

test('local artwork revalidates unchanged production URLs after same-mtime byte changes', async ({ browser, isolatedWebHost }, testInfo) => {
  test.setTimeout(60_000);
  const root = mkdtempSync(join(dirname(isolatedWebHost.databasePath), 'artwork-refresh-'));
  const imagePath = join(root, 'synthetic-thumbnail.png');
  writeFileSync(imagePath, png.blue);
  seed(isolatedWebHost.databasePath, imagePath);
  const source = `/artwork/thumbnails/${encodeURIComponent(imagePath)}?v=2`;
  const context = await browser.newContext({ baseURL: isolatedWebHost.baseURL });
  try {
    const page = await context.newPage();
    const firstResponse = page.waitForResponse(response =>
      response.url() === `${isolatedWebHost.baseURL}${source}` && response.status() === 200);
    await page.goto(detailPath);
    const image = page.getByRole('img', { name: title, exact: true });
    await expect(image).toHaveAttribute('src', source);
    expect(await pixel(image)).toEqual([0, 0, 255, 255]);
    expect((await firstResponse).headers()['cache-control']).toBe('no-cache');
    const original = statSync(imagePath);
    writeFileSync(imagePath, png.green);
    utimesSync(imagePath, original.atimeMs / 1000, original.mtimeMs / 1000);
    expect(Math.abs(statSync(imagePath).mtimeMs - original.mtimeMs)).toBeLessThan(1);
    // Ordinary navigation, not reload/cache-bypass or intercepted requests.
    // Scanner deep-refresh behavior has its own runtime gate; this case tests
    // the real artwork response after its source bytes have been replaced.
    await page.goto('/healthz');
    const refreshedResponse = page.waitForResponse(response =>
      response.url() === `${isolatedWebHost.baseURL}${source}` && response.status() === 200);
    await page.goto(detailPath);
    await expect(page.getByRole('heading', { name: title, exact: true })).toBeVisible();
    await expect(image).toHaveAttribute('src', source);
    expect(await pixel(image)).toEqual([0, 255, 0, 255]);
    const response = await refreshedResponse;
    expect(response.headers()['cache-control']).toBe('no-cache');
    expect(await response.body()).toEqual(png.green);
    await testInfo.attach('artwork-refresh-proof', {
      body: JSON.stringify({ unchangedSourceURL: true, restoredMtime: true, cacheControl: response.headers()['cache-control'], decodedPixel: [0, 255, 0, 255] }),
      contentType: 'application/json',
    });
    await page.screenshot({ path: testInfo.outputPath('artwork-refresh.png') });
  } finally {
    try {
      await context.close();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }
});
