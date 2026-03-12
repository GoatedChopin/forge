import { test, expect, API_URL, trackConsoleErrors } from "./fixtures";

test("homepage loads successfully", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { level: 1 })).toContainText("demo");
});

test("backend health check responds", async ({ request }) => {
  const response = await request.get(`${API_URL}/_api/health`);
  expect(response.ok()).toBeTruthy();
});

test("no unexpected console errors", async ({ page }) => {
  const errors = trackConsoleErrors(page);
  await page.goto("/");
  await expect(page.locator("body")).toBeVisible();
  expect(errors).toHaveLength(0);
});
