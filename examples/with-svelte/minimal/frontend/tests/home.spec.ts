import { test, expect, API_URL, trackConsoleErrors } from "./fixtures";

test.describe("Application", () => {
  test("homepage loads successfully", async ({ page }) => {
    await page.goto("/");

    await expect(page.locator("body")).toBeVisible();
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  });

  test("navigation to about page works", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("link", { name: "About" }).click();
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "About",
    );
  });

  test("nav links are present in layout", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("link", { name: "Home" })).toBeVisible();
    await expect(page.getByRole("link", { name: "About" })).toBeVisible();
  });

  test("no unexpected console errors", async ({ page }) => {
    const errors = trackConsoleErrors(page);

    await page.goto("/");
    await expect(page.locator("body")).toBeVisible();

    expect(errors).toHaveLength(0);
  });
});

test.describe("Backend Connection", () => {
  test("health check responds", async ({ request }) => {
    const response = await request.get(`${API_URL}/_api/health`);
    expect(response.ok()).toBeTruthy();

    const data = await response.json();
    expect(data.status).toBe("healthy");
  });

  test("SSE connection establishes through ForgeProvider", async ({ page }) => {
    const sseRequests: string[] = [];

    page.on("request", (req) => {
      if (req.url().includes("/_api/events")) {
        sseRequests.push(req.url());
      }
    });

    await page.goto("/");
    await page.waitForTimeout(2000);

    expect(sseRequests.length).toBeGreaterThan(0);
  });
});
