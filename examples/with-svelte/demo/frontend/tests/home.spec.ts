import {
  test,
  expect,
  API_URL,
  ACTION_TIMEOUT,
  uniqueId,
  trackConsoleErrors,
} from "./fixtures";

test.describe("Forge Demo", () => {
  // ---------------------------------------------------------------------------
  // Page structure
  // ---------------------------------------------------------------------------

  test("homepage loads with all sections visible", async ({ page }) => {
    await page.goto("/");

    await expect(page.getByText("Forge Demo")).toBeVisible();
    await expect(page.getByText("ISS Location")).toBeVisible();
    await expect(page.getByText("Live Trades")).toBeVisible();
    await expect(page.getByText("Export Job")).toBeVisible();
    await expect(page.getByText("Verification")).toBeVisible();
    await expect(page.getByText("Webhook").first()).toBeVisible();
    await expect(page.getByText("refresh tokens")).toBeVisible();
    await expect(page.getByText("Cached Query")).toBeVisible();
    await expect(
      page.getByRole("heading", { name: /MCP Tools/ }),
    ).toBeVisible();
    await expect(page.getByRole("heading", { name: /users/i })).toBeVisible();
  });

  test("no console errors on page load", async ({ page }) => {
    const errors = trackConsoleErrors(page);
    await page.goto("/");
    await expect(page.locator("body")).toBeVisible();
    expect(errors).toHaveLength(0);
  });

  // ---------------------------------------------------------------------------
  // ISS Location (cron, reactive)
  // ---------------------------------------------------------------------------

  test("ISS location shows coordinates or waiting placeholder", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("ISS Location"),
    });

    const hasCoordinates = section.getByText(/\d+\.\d+\s*[NS]/i).first();
    const hasPlaceholder = section.getByText("Waiting for first cron run");

    await expect(hasCoordinates.or(hasPlaceholder)).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
    await expect(section.getByText(/error|failed/i)).not.toBeVisible();
  });

  test("ISS location receives live coordinates via SSE", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("ISS Location"),
    });

    // Cron runs every minute; wait long enough for at least one run
    await expect(section.getByText(/\d+\.\d+\s*[NS]/i).first()).toBeVisible({
      timeout: 90_000,
    });
    await expect(section.getByText(/\d+\.\d+\s*[EW]/i).first()).toBeVisible();
    await expect(section.getByText("Updated every minute via cron")).toBeVisible();
  });

  // ---------------------------------------------------------------------------
  // Live Trades (daemon, reactive)
  // ---------------------------------------------------------------------------

  test("live trades renders table with data or connecting state", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("Live Trades"),
    });

    await expect(section.getByText("Symbol")).toBeVisible();
    await expect(section.getByText("Price")).toBeVisible();

    const hasTradeData = section.getByText("EURUSDT").first();
    const hasPlaceholder = section.getByText("Connecting to Binance");

    await expect(hasTradeData.or(hasPlaceholder)).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
    await expect(section.getByText(/error|failed/i)).not.toBeVisible();
  });

  test.skip(!!process.env.CI, "Binance API often blocked in CI");
  test("live trades stream real data via SSE", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("Live Trades"),
    });

    // Wait for real trade rows to replace placeholders
    await expect(section.getByText("EURUSDT").first()).toBeVisible({
      timeout: 30_000,
    });
    await expect(section.getByText("Streaming from Binance")).toBeVisible();

    // At least one row should show BUY or SELL
    const hasTradeSide = section.locator("td.buy, td.sell").first();
    await expect(hasTradeSide).toBeVisible();
  });

  // ---------------------------------------------------------------------------
  // SSE
  // ---------------------------------------------------------------------------

  test("SSE connection establishes for real-time updates", async ({
    page,
    gotoReady,
  }) => {
    const sseRequests: string[] = [];
    page.on("request", (req) => {
      if (req.url().includes("/_api/events")) {
        sseRequests.push(req.url());
      }
    });

    await gotoReady();
    expect(sseRequests.length).toBeGreaterThan(0);
  });

  // ---------------------------------------------------------------------------
  // Users CRUD (all via UI)
  // ---------------------------------------------------------------------------

  test("creating a user appears in the list via SSE", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByRole("heading", { name: /users/i }),
    });
    const name = uniqueId("Create");
    const email = `${name.toLowerCase()}@test.com`;

    await section.getByPlaceholder("Name").fill(name);
    await section.getByPlaceholder("Email").fill(email);
    await section.getByRole("button", { name: "Create" }).click();

    await expect(page.getByText(name, { exact: true })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
    await expect(page.getByText(email)).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("editing a user inline persists changes", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByRole("heading", { name: /users/i }),
    });

    const name = uniqueId("EditMe");
    const email = `${name.toLowerCase()}@test.com`;
    const updatedName = uniqueId("Edited");

    await section.getByPlaceholder("Name").fill(name);
    await section.getByPlaceholder("Email").fill(email);
    await section.getByRole("button", { name: "Create" }).click();

    await expect(page.getByText(name, { exact: true })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    const row = page.locator("tr", { hasText: name });
    await row.getByRole("button", { name: "Edit" }).click();

    const editRow = page.locator("tr.editing");
    await expect(editRow).toBeVisible();

    const nameInput = editRow.locator('input[type="text"]');
    await nameInput.clear();
    await nameInput.fill(updatedName);
    await editRow.getByRole("button", { name: "Save" }).click();

    await expect(page.getByText(updatedName, { exact: true })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("cancel edit reverts the row without saving", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByRole("heading", { name: /users/i }),
    });

    const name = uniqueId("CancelEdit");
    const email = `${name.toLowerCase()}@test.com`;

    await section.getByPlaceholder("Name").fill(name);
    await section.getByPlaceholder("Email").fill(email);
    await section.getByRole("button", { name: "Create" }).click();

    await expect(page.getByText(name, { exact: true })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    const row = page.locator("tr", { hasText: name });
    await row.getByRole("button", { name: "Edit" }).click();
    await expect(page.locator("tr.editing")).toBeVisible();

    await page.locator("tr.editing").getByRole("button", { name: "Cancel" }).click();
    await expect(page.locator("tr.editing")).not.toBeVisible();
    await expect(page.getByText(name, { exact: true })).toBeVisible();
  });

  test("deleting a user removes it from the list", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByRole("heading", { name: /users/i }),
    });

    const name = uniqueId("ToDelete");
    const email = `${name.toLowerCase()}@test.com`;

    await section.getByPlaceholder("Name").fill(name);
    await section.getByPlaceholder("Email").fill(email);
    await section.getByRole("button", { name: "Create" }).click();

    await expect(page.getByText(name, { exact: true })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    const row = page.locator("tr", { hasText: name });
    await row.getByRole("button", { name: "Delete" }).click();
    await row.getByRole("button", { name: "Confirm" }).click();

    await expect(page.getByText(name, { exact: true })).not.toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("cancel delete popover keeps the user", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByRole("heading", { name: /users/i }),
    });

    const name = uniqueId("KeepMe");
    const email = `${name.toLowerCase()}@test.com`;

    await section.getByPlaceholder("Name").fill(name);
    await section.getByPlaceholder("Email").fill(email);
    await section.getByRole("button", { name: "Create" }).click();

    await expect(page.getByText(name, { exact: true })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    const row = page.locator("tr", { hasText: name });
    await row.getByRole("button", { name: "Delete" }).click();

    const popover = row.locator(".popover");
    await expect(popover).toBeVisible();
    await popover.getByRole("button", { name: "Cancel" }).click();

    await expect(popover).not.toBeVisible();
    await expect(page.getByText(name, { exact: true })).toBeVisible();
  });

  // ---------------------------------------------------------------------------
  // Export Job (UI only)
  // ---------------------------------------------------------------------------

  test("export job completes with progress tracking", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("Export Job"),
    });

    await section.getByRole("button", { name: "Start Export" }).click();

    await expect(section.getByText(/Export complete/i)).toBeVisible({
      timeout: 15_000,
    });
    await expect(section.getByText(/100%/)).toBeVisible();
    await expect(
      section.getByRole("button", { name: "Run Again" }),
    ).toBeVisible();
  });

  test("export Run Again re-triggers the job", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("Export Job"),
    });

    await section.getByRole("button", { name: "Start Export" }).click();
    await expect(
      section.getByRole("button", { name: "Run Again" }),
    ).toBeVisible({ timeout: 15_000 });

    await section.getByRole("button", { name: "Run Again" }).click();

    await expect(section.getByText(/100%/)).toBeVisible({ timeout: 15_000 });
  });

  // ---------------------------------------------------------------------------
  // Verification Workflow (UI only)
  // ---------------------------------------------------------------------------

  test("verification workflow completes all steps", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("Verification"),
    });

    await section.getByRole("button", { name: "Start Workflow" }).click();

    await expect(
      section.getByRole("button", { name: "Run Again" }),
    ).toBeVisible({ timeout: 15_000 });

    const expectedSteps = [
      "generate_token",
      "store_token",
      "send_email",
      "wait_period",
      "mark_verified",
    ];

    for (const step of expectedSteps) {
      await expect(section.getByText(step)).toBeVisible();
    }

    const completedSteps = section.locator(".step.completed");
    await expect(completedSteps).toHaveCount(5);
  });

  test("verification Run Again re-triggers the workflow", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("Verification"),
    });

    await section.getByRole("button", { name: "Start Workflow" }).click();
    await expect(
      section.getByRole("button", { name: "Run Again" }),
    ).toBeVisible({ timeout: 15_000 });

    await section.getByRole("button", { name: "Run Again" }).click();

    // Second run should also complete
    await expect(section.locator(".step.completed")).toHaveCount(5, {
      timeout: 15_000,
    });
  });

  // ---------------------------------------------------------------------------
  // Webhook (UI only, skip in CI due to timing)
  // ---------------------------------------------------------------------------

  test.skip(!!process.env.CI, "webhook timing unreliable in CI");
  test("webhook sends and shows processed event", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("Webhook"),
    });

    const keyInput = section.locator('input[type="text"]');
    const key = await keyInput.inputValue();

    await section.getByRole("button", { name: "Send" }).click();

    await expect(section.getByText(/Webhook processed/i)).toBeVisible({
      timeout: ACTION_TIMEOUT * 2,
    });
    await expect(
      section.getByRole("button", { name: "Send" }),
    ).toBeDisabled();

    // Key should appear in Recent Events via SSE
    await expect(section.getByText(key)).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    // Generate new key
    await section.getByRole("button", { name: "New" }).click();
    const newKey = await keyInput.inputValue();
    expect(newKey).not.toBe(key);
    await expect(
      section.getByRole("button", { name: "Send" }),
    ).toBeEnabled();
  });

  test("webhook event appears in recent events via SSE", async ({
    page,
    gotoReady,
    rpc: _rpc,
  }) => {
    // Fire a webhook via API so we don't depend on UI timing
    const key = `sse-test-${Date.now()}`;
    const secret = "demo-secret";
    const body = JSON.stringify({ action: "test" });
    const encoder = new TextEncoder();
    const keyData = await crypto.subtle.importKey(
      "raw",
      encoder.encode(secret),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    const sig = await crypto.subtle.sign("HMAC", keyData, encoder.encode(body));
    const hex = [...new Uint8Array(sig)]
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("");

    await fetch(`${API_URL}/_api/webhooks/demo`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Webhook-Signature": hex,
        "X-Idempotency-Key": key,
      },
      body,
    });

    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("Webhook"),
    });

    // The event list is SSE-driven; our key should appear
    await expect(section.getByText(key)).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("webhook HMAC validation rejects bad signatures", async ({
    request,
  }) => {
    // This is the one API-level test: can't send a bad HMAC from the UI
    const res = await request.post(`${API_URL}/_api/webhooks/demo`, {
      headers: {
        "Content-Type": "application/json",
        "X-Webhook-Signature": "invalid",
        "X-Idempotency-Key": `test-bad-${Date.now()}`,
      },
      data: { action: "test" },
    });
    expect(res.status()).toBe(401);
  });

  // ---------------------------------------------------------------------------
  // Auth (all via UI)
  // ---------------------------------------------------------------------------

  test("login with demo credentials shows token metadata", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("refresh tokens"),
    });

    await section.locator('button[type="submit"]').click();

    await expect(section.getByText("Logged in as")).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
    await expect(section.getByText("sub")).toBeVisible();
    await expect(section.getByText("exp")).toBeVisible();
    await expect(section.getByText("TOKEN METADATA")).toBeVisible();
  });

  test("register tab shows name field and registers new user", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("refresh tokens"),
    });

    // Switch to register tab
    await section.getByRole("button", { name: "Register" }).click();

    // Name field should now be visible
    const nameInput = section.getByPlaceholder("Name");
    await expect(nameInput).toBeVisible();

    const email = `reg-${Date.now()}@test.com`;
    await nameInput.fill("Test Register");
    await section.getByPlaceholder("Email").fill(email);
    await section.getByPlaceholder("Password (min 8 chars)").fill("testpassword123");
    await section.locator('button[type="submit"]').click();

    await expect(section.getByText("Logged in as")).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
    await expect(section.getByText(email)).toBeVisible();
  });

  test("refresh token button rotates tokens and shows count", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("refresh tokens"),
    });

    // Login first
    await section.locator('button[type="submit"]').click();
    await expect(section.getByText("Logged in as")).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    // Click refresh
    await section.getByRole("button", { name: "Refresh Token" }).click();
    await expect(section.getByText(/Token refreshed 1 time/)).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    // Second refresh
    await section.getByRole("button", { name: "Refresh Token" }).click();
    await expect(section.getByText(/Token refreshed 2 time/)).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("logout returns to login form", async ({ page, gotoReady }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("refresh tokens"),
    });

    // Login
    await section.locator('button[type="submit"]').click();
    await expect(section.getByText("Logged in as")).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    // Logout
    await section.getByRole("button", { name: "Logout" }).click();

    // Login form should be back
    await expect(section.locator('button[type="submit"]')).toBeVisible();
    await expect(section.getByText("Logged in as")).not.toBeVisible();
    await expect(section.getByPlaceholder("Email")).toBeVisible();
  });

  // ---------------------------------------------------------------------------
  // Cached Query (UI only)
  // ---------------------------------------------------------------------------

  test("fetch stats shows data in the UI", async ({ page, gotoReady }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("Cached Query"),
    });

    await section.getByRole("button", { name: "Fetch Stats" }).click();

    await expect(section.getByText("Users")).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
    await expect(section.getByText("Trades")).toBeVisible();
    await expect(section.getByText("Webhooks")).toBeVisible();
    await expect(section.getByText("Computed")).toBeVisible();
  });

  test("second fetch shows cache hit", async ({ page, gotoReady }) => {
    await gotoReady();

    const section = page.locator("section", {
      has: page.getByText("Cached Query"),
    });

    // First fetch: cache miss (simulated ~500ms)
    await section.getByRole("button", { name: "Fetch Stats" }).click();
    await expect(section.getByText("Users")).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
    await expect(section.getByText("fetch #1")).toBeVisible();

    // Second fetch: should hit cache (< 100ms)
    await section.getByRole("button", { name: "Fetch Stats" }).click();
    await expect(section.getByText("fetch #2")).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
    await expect(section.getByText("cache hit")).toBeVisible();
  });

  // ---------------------------------------------------------------------------
  // MCP Tools (static)
  // ---------------------------------------------------------------------------

  test("MCP tools section shows configuration", async ({ page }) => {
    await page.goto("/");

    const section = page.locator("section", {
      has: page.getByRole("heading", { name: /MCP Tools/ }),
    });

    await expect(
      section.getByText("claude mcp add forge-demo"),
    ).toBeVisible();
    await expect(section.getByText("demo.list_users")).toBeVisible();
    await expect(
      section.getByText("demo.get_user_by_email"),
    ).toBeVisible();
  });
});
