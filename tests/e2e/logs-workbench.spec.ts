import { expect, test } from '@playwright/test';

test('logs workbench hydrates and exposes standardized search, sorting, and pagination', async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));

  await page.goto('/system/logs');
  const workbench = page.locator('[data-testid="logs-page"]');
  await expect(workbench).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
      ),
    )
    .toBe(true);

  const search = workbench.getByRole('searchbox', { name: 'Search' });
  await expect(search).toBeVisible();
  await search.fill('no-log-entry-can-match-this-value');
  await expect(workbench.getByText('No matching records found', { exact: true })).toBeVisible();
  await workbench.getByRole('button', { name: 'Clear', exact: true }).click();
  await expect(search).toHaveValue('');

  const timestampHeader = workbench.getByRole('columnheader', { name: /Timestamp/ });
  await expect(timestampHeader).toHaveAttribute('aria-sort', 'none');
  await timestampHeader.getByRole('button').click();
  await expect(timestampHeader).toHaveAttribute('aria-sort', 'ascending');
  await timestampHeader.getByRole('button').click();
  await expect(timestampHeader).toHaveAttribute('aria-sort', 'descending');

  await expect(workbench.locator('[data-pagination]')).toBeVisible();
  await expect(workbench.locator('[data-pagination-page-size]')).toBeVisible();
  expect(pageErrors).toEqual([]);
});
