import { test, expect, API_URL, trackConsoleErrors } from "./fixtures";

test.describe("Application", () => {
  test("homepage loads successfully", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator("body")).toBeVisible();
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  });

  test("no console errors on page load", async ({ page }) => {
    const errors = trackConsoleErrors(page);
    await page.goto("/");
    // Wait for async initialization to complete
    await expect(page.locator("body")).toBeVisible();
    expect(errors).toHaveLength(0);
  });

  test("backend health check", async ({ request }) => {
    const response = await request.get(`${API_URL}/_api/health`);
    expect(response.ok()).toBeTruthy();
  });
});
