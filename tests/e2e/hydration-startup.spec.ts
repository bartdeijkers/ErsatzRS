import { createServer, type ServerResponse } from 'node:http';
import type { AddressInfo } from 'node:net';
import { waitForHydration } from './hydration';
import { expect, test } from '@playwright/test';

// requestIdleCallback has no execution guarantee without a timeout. Model a
// browser that never grants idle time without consuming CPU or slowing I/O.
// Essential admin controls must still initialize within the existing bound.
test('admin controls initialize even when the browser grants no idle callbacks', async ({ page }) => {
  await page.addInitScript(() => {
    window.requestIdleCallback = () => 1;
    window.cancelIdleCallback = () => {};
  });
  await page.request.post('/admin-api/settings/ui/save', {
    form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
  });
  await page.goto('/settings/addons');
  await waitForHydration(page);

  // Prove that actual event handlers run; merely downloading WASM is insufficient.
  const settings = page.getByTestId('addon-detail-wait-settings');
  await settings.getByTestId('addon-detail-wait-minimum').fill('12');
  await settings.getByTestId('addon-detail-wait-maximum').fill('11');
  await settings.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('[data-toast-severity="error"]').filter({
    hasText: 'minimum does not exceed the maximum',
  })).toBeVisible();
});

test('document load does not report readiness while WASM initialization is pending', async ({ page }) => {
  let release!: () => void;
  const pending = new Promise<void>(resolve => { release = resolve; });
  await page.route('**/pkg/*.wasm', async route => {
    await pending;
    await route.continue();
  });
  try {
    await page.goto('/settings/addons', { waitUntil: 'load' });
    await expect.poll(() => page.evaluate(() =>
      (window as Window & { __ersatzrs_hydration_phase?: string }).__ersatzrs_hydration_phase,
    )).toBe('initializing-wasm');
    expect(await page.evaluate(() =>
      (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
    )).toBe(false);
  } finally {
    release();
  }
  await waitForHydration(page);
  await expect.poll(() => page.evaluate(() =>
    (window as Window & { __ersatzrs_hydration_phase?: string }).__ersatzrs_hydration_phase,
  )).toBe('ready');
});

// Following a link while WASM is still downloading cancels that download.
// Firefox rejects it before the old page unloads: with AbortError once the
// response has started, and with "TypeError: NetworkError" while the response
// headers are still outstanding. Leaving a page is not a startup failure, so
// neither may become an uncaught page error. Holding a request inside
// page.route changes how Firefox reports the cancellation, so a local server
// either withholds the response or sends the real headers and the first bytes
// of the real WASM, then stalls the rest the way a slow connection would.
for (const stall of ['before the response headers', 'after the first bytes'] as const) {
  test(`following a link while WASM downloads reports no page error (${stall})`, async ({ page, baseURL }) => {
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(`${error.name}: ${error.message}`));
    const phase = () => page.evaluate(() =>
      (window as Window & { __ersatzrs_hydration_phase?: string }).__ersatzrs_hydration_phase,
    );
    const stalled = new Set<ServerResponse>();
    let reached!: () => void;
    const stallReached = new Promise<void>(resolve => { reached = resolve; });
    const slow = createServer(async (request, response) => {
      stalled.add(response);
      if (stall === 'after the first bytes') {
        const upstream = await fetch(new URL(request.url ?? '/', baseURL));
        const body = new Uint8Array(await upstream.arrayBuffer());
        response.writeHead(upstream.status, {
          'content-type': upstream.headers.get('content-type') ?? 'application/wasm',
          'content-length': String(body.length),
        });
        response.write(body.subarray(0, Math.min(1024, body.length)));
      }
      reached();
    });
    await new Promise<void>(resolve => slow.listen(0, '127.0.0.1', resolve));
    const { port } = slow.address() as AddressInfo;
    let diverted = false;
    await page.route('**/pkg/*.wasm', route => {
      if (diverted) return route.continue();
      diverted = true;
      const url = new URL(route.request().url());
      return route.continue({ url: `http://127.0.0.1:${port}${url.pathname}` });
    });
    try {
      await page.goto('/settings/addons', { waitUntil: 'load' });
      await expect.poll(phase).toBe('initializing-wasm');
      await stallReached;
      await page.evaluate(() => {
        const link = document.createElement('a');
        link.href = '/playouts';
        document.body.append(link);
        link.click();
      });
      await page.waitForURL('**/playouts');
      await waitForHydration(page);
      await expect.poll(phase).toBe('ready');
      expect(errors).toEqual([]);
    } finally {
      for (const response of stalled) response.destroy();
      await new Promise<void>(resolve => slow.close(() => resolve()));
    }
  });
}
