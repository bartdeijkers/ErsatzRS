import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import path from 'node:path';

const ROUTER_PATH = path.join('crates', 'ersatzrs-web', 'src', 'admin', 'routes.rs');
const HYDRATION_PATH = path.join('docs', 'progress', 'hydration.toml');

function productionRouterRoutes(source: string): string[] {
  const routeTags = source.match(/<Route\b[\s\S]*?\/>/g) ?? [];
  return routeTags
    .map((tag) => {
      const segments = [...tag.matchAll(/(StaticSegment|ParamSegment)\("([^"]*)"\)/g)].map(
        ([, segmentType, value]) => (segmentType === 'ParamSegment' ? `:${value}` : value),
      );
      return `/${segments.filter(Boolean).join('/')}`;
    })
    .filter((route) => route !== '/dev/components')
    .sort();
}

function documentedRoutes(source: string): string[] {
  return [...source.matchAll(/routes\s*=\s*\[([^\]]*)\]/g)]
    .flatMap(([, values]) => [...values.matchAll(/'([^']+)'/g)].map(([, route]) => route))
    .sort();
}

test('every production admin router page has hydration and browser evidence', () => {
  const router = readFileSync(path.resolve(ROUTER_PATH), 'utf8');
  const hydration = readFileSync(path.resolve(HYDRATION_PATH), 'utf8');

  const productionRoutes = productionRouterRoutes(router);
  const evidenceRoutes = documentedRoutes(hydration);

  expect(new Set(productionRoutes).size, 'the router must not register a production route twice').toBe(
    productionRoutes.length,
  );
  expect(new Set(evidenceRoutes).size, 'hydration evidence must own each route exactly once').toBe(
    evidenceRoutes.length,
  );
  expect(evidenceRoutes).toEqual(productionRoutes);
});
