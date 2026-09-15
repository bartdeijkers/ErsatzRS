import { expect, type Page } from '@playwright/test';

/** Keep hydration failures distinct from the scenario's functional assertions. */
export async function waitForHydration(page: Page): Promise<void> {
  try {
    await expect
      .poll(() => page.evaluate(() => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true), { timeout: 15_000 })
      .toBe(true);
  } catch (error) {
    const diagnostics = await page.evaluate(() => ({
      readyState: document.readyState,
      visibility: document.visibilityState,
      phase: (window as Window & { __ersatzrs_hydration_phase?: string }).__ersatzrs_hydration_phase ?? 'not-started',
      islandLoaderReady: typeof (window as Window & { __hydrateIsland?: unknown }).__hydrateIsland === 'function',
      resources: performance.getEntriesByType('resource')
        .filter(entry => /\/pkg\/[^/]+\.(wasm|js)$/.test(new URL(entry.name).pathname))
        .map(entry => {
          const resource = entry as PerformanceResourceTiming;
          return {
            file: new URL(resource.name).pathname.split('/').pop(),
            duration: resource.duration,
            responseEnd: resource.responseEnd,
            transferSize: resource.transferSize,
          };
        }),
    })).catch(() => ({ pageUnavailable: true }));
    throw new Error(`Hydration did not complete: ${JSON.stringify(diagnostics)}`, { cause: error });
  }
}
