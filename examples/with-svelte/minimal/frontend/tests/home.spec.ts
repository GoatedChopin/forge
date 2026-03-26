import { test, expect, trackConsoleErrors } from "./fixtures";

test("application boots without console errors", async ({ page }) => {
  const errors = trackConsoleErrors(page);

  await page.goto("/");

  await expect(page.locator("body")).toBeVisible();
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  await expect(page.getByRole("link", { name: "About" })).toBeVisible();
  expect(errors).toHaveLength(0);
});
