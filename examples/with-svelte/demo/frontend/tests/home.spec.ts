import {
  test,
  expect,
  API_URL,
  ACTION_TIMEOUT,
  trackConsoleErrors,
} from "./fixtures";

test.describe("Forge Demo", () => {
  test("homepage loads with all sections visible", async ({ page }) => {
    await page.goto("/");

    await expect(page.getByText("Forge Demo")).toBeVisible();
    await expect(page.getByText("ISS Location")).toBeVisible();
    await expect(page.getByText("Live Trades")).toBeVisible();
    await expect(page.getByText("Export Job")).toBeVisible();
    await expect(page.getByText("Verification")).toBeVisible();
    await expect(page.getByText("Webhook").first()).toBeVisible();
    await expect(page.getByRole("heading", { name: /users/i })).toBeVisible();
  });

  test("backend health check responds", async ({ request }) => {
    const response = await request.get(`${API_URL}/_api/health`);
    expect(response.ok()).toBeTruthy();

    const data = await response.json();
    expect(data.status).toBe("healthy");
  });

  test("creating user appears in list", async ({ page, gotoReady }) => {
    await gotoReady();

    const uniqueName = `TestUser-${Date.now()}`;
    const uniqueEmail = `test-${Date.now()}@example.com`;

    await page.getByPlaceholder("Name").fill(uniqueName);
    await page.getByPlaceholder("Email").fill(uniqueEmail);
    await page.getByRole("button", { name: "Create" }).click();

    await expect(page.getByText(uniqueName)).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
    await expect(page.getByText(uniqueEmail)).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("update mutation persists changes", async ({ rpc }) => {
    const uniqueName = `EditMe-${Date.now()}`;
    const uniqueEmail = `edit-${Date.now()}@example.com`;
    const updatedName = `Edited-${Date.now()}`;

    const user = (await rpc("create_user", {
      name: uniqueName,
      email: uniqueEmail,
    })) as { id: string; name: string };

    const updated = (await rpc("update_user", {
      id: user.id,
      name: updatedName,
      email: null,
      role: null,
    })) as { id: string; name: string };

    expect(updated.name).toBe(updatedName);
    expect(updated.id).toBe(user.id);

    await rpc("delete_user", { id: user.id });
  });

  test("deleting user removes from list", async ({ page, gotoReady }) => {
    await gotoReady();

    const uniqueName = `ToDelete-${Date.now()}`;
    const uniqueEmail = `delete-${Date.now()}@example.com`;

    await page.getByPlaceholder("Name").fill(uniqueName);
    await page.getByPlaceholder("Email").fill(uniqueEmail);
    await page.getByRole("button", { name: "Create" }).click();

    await expect(page.getByText(uniqueName)).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    const userRow = page.locator("tr", { hasText: uniqueName });
    await userRow.getByRole("button", { name: "Delete" }).click();
    await userRow.getByRole("button", { name: "Confirm" }).click();

    await page.reload();
    await expect(page.getByText(uniqueName)).not.toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("ISS location query returns valid shape via API", async ({
    request,
  }) => {
    const res = await request.post(`${API_URL}/_api/rpc/get_iss_location`, {
      headers: { "Content-Type": "application/json" },
      data: {},
    });
    expect(res.ok()).toBeTruthy();

    const json = await res.json();
    expect(json.success).toBe(true);
    if (json.data) {
      expect(typeof json.data.latitude).toBe("number");
      expect(typeof json.data.longitude).toBe("number");
    }
  });

  test("ISS location component renders without errors", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const issSection = page.locator("section", {
      has: page.getByText("ISS Location"),
    });

    // Component must show either real coordinates or the waiting placeholder
    const hasCoordinates = issSection.getByText(/\d+\.\d+\s*[NS]/i).first();
    const hasPlaceholder = issSection.getByText("Waiting for first cron run");

    await expect(hasCoordinates.or(hasPlaceholder)).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    // Must not show any error state
    await expect(issSection.getByText(/error|failed/i)).not.toBeVisible();
  });

  test("trades query returns valid shape via API", async ({ request }) => {
    const res = await request.post(`${API_URL}/_api/rpc/get_trades`, {
      headers: { "Content-Type": "application/json" },
      data: {},
    });
    expect(res.ok()).toBeTruthy();

    const json = await res.json();
    expect(json.success).toBe(true);
    expect(Array.isArray(json.data)).toBe(true);
    if (json.data.length > 0) {
      expect(json.data[0].symbol).toBeDefined();
      expect(typeof json.data[0].price).toBe("number");
    }
  });

  test("live trades component renders without errors", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const tradesSection = page.locator("section", {
      has: page.getByText("Live Trades"),
    });

    // Table headers must always render
    await expect(tradesSection.getByText("Symbol")).toBeVisible();
    await expect(tradesSection.getByText("Price")).toBeVisible();

    // Component must show either real trade data or the connecting placeholder
    const hasTradeData = tradesSection.getByText("EURUSDT").first();
    const hasPlaceholder = tradesSection.getByText("Connecting to Binance");

    await expect(hasTradeData.or(hasPlaceholder)).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    // Must not show any error state
    await expect(tradesSection.getByText(/error|failed/i)).not.toBeVisible();
  });

  test("export job completes with progress tracking", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const jobSection = page.locator("section", {
      has: page.getByText("Export Job"),
    });

    await jobSection.getByRole("button", { name: "Start Export" }).click();

    // Wait for completion: "Export complete" text appears when the job finishes
    await expect(jobSection.getByText(/Export complete/i)).toBeVisible({
      timeout: 15000,
    });

    // Final state should show 100% progress
    await expect(jobSection.getByText(/100%/)).toBeVisible();

    // Run Again should be available
    await expect(
      jobSection.getByRole("button", { name: "Run Again" }),
    ).toBeVisible();
  });

  test("verification workflow completes all steps", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const verifySection = page.locator("section", {
      has: page.getByText("Verification"),
    });

    await verifySection.getByRole("button", { name: "Start Workflow" }).click();

    // The workflow has a 2s durable sleep, so wait for completion
    // indicated by the "Run Again" button appearing
    await expect(
      verifySection.getByRole("button", { name: "Run Again" }),
    ).toBeVisible({ timeout: 15000 });

    // All five steps should be visible
    const expectedSteps = [
      "generate_token",
      "store_token",
      "send_email",
      "wait_period",
      "mark_verified",
    ];

    for (const step of expectedSteps) {
      await expect(verifySection.getByText(step)).toBeVisible();
    }

    // All steps should show completed status
    const completedSteps = verifySection.locator(".step.completed");
    await expect(completedSteps).toHaveCount(5);
  });

  test("webhook sends and shows processed event", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    const webhookSection = page.locator("section", {
      has: page.getByText("Webhook"),
    });

    // Grab the idempotency key before sending
    const keyInput = webhookSection.locator('input[type="text"]');
    const key = await keyInput.inputValue();

    await webhookSection.getByRole("button", { name: "Send" }).click();

    // Key input should turn green (used state) and show success message
    await expect(webhookSection.getByText(/Webhook processed/i)).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    // Send button should be disabled after use
    await expect(
      webhookSection.getByRole("button", { name: "Send" }),
    ).toBeDisabled();

    // The key should appear in Recent Events
    await expect(webhookSection.getByText(key)).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    // Generate new key and verify it's different
    await webhookSection.getByRole("button", { name: "New" }).click();
    const newKey = await keyInput.inputValue();
    expect(newKey).not.toBe(key);

    // Send button should be enabled again
    await expect(
      webhookSection.getByRole("button", { name: "Send" }),
    ).toBeEnabled();
  });

  test("webhook endpoint validates HMAC signature via API", async ({
    request,
  }) => {
    // Verify the webhook rejects invalid signatures
    const badSigResponse = await request.post(`${API_URL}/_api/webhooks/demo`, {
      headers: {
        "Content-Type": "application/json",
        "X-Webhook-Signature": "invalid",
        "X-Idempotency-Key": `test-bad-${Date.now()}`,
      },
      data: { action: "test" },
    });
    expect(badSigResponse.status()).toBe(401);
  });

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

  test("RPC endpoints respond correctly", async ({ request }) => {
    const usersResponse = await request.post(`${API_URL}/_api/rpc/get_users`, {
      headers: { "Content-Type": "application/json" },
      data: {},
    });
    expect(usersResponse.ok()).toBeTruthy();

    const usersData = await usersResponse.json();
    expect(usersData.success).toBe(true);
    expect(Array.isArray(usersData.data)).toBe(true);
  });

  test("mutation endpoints work correctly", async ({ request }) => {
    const uniqueEmail = `api-test-${Date.now()}@example.com`;

    const createResponse = await request.post(
      `${API_URL}/_api/rpc/create_user`,
      {
        headers: { "Content-Type": "application/json" },
        data: { args: { email: uniqueEmail, name: "API Test User" } },
      },
    );
    expect(createResponse.ok()).toBeTruthy();

    const createData = await createResponse.json();
    expect(createData.success).toBe(true);
    expect(createData.data.email).toBe(uniqueEmail);
    expect(createData.data.id).toBeDefined();

    const deleteResponse = await request.post(
      `${API_URL}/_api/rpc/delete_user`,
      {
        headers: { "Content-Type": "application/json" },
        data: { args: { id: createData.data.id } },
      },
    );
    expect(deleteResponse.ok()).toBeTruthy();
  });

  test("job dispatch and tracking via API", async ({ request }) => {
    // Dispatch job
    const dispatchRes = await request.post(`${API_URL}/_api/rpc/export_users`, {
      headers: { "Content-Type": "application/json" },
      data: { args: { format: "csv" } },
    });
    expect(dispatchRes.ok()).toBeTruthy();

    const dispatchData = await dispatchRes.json();
    expect(dispatchData.success).toBe(true);
    expect(dispatchData.data.job_id).toBeDefined();

    const jobId = dispatchData.data.job_id;
    expect(jobId).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/,
    );
  });

  test("workflow dispatch and tracking via API", async ({ request }) => {
    const dispatchRes = await request.post(
      `${API_URL}/_api/rpc/account_verification`,
      {
        headers: { "Content-Type": "application/json" },
        data: {
          args: {
            account_id: "api-test-user",
            email: "api-test@example.com",
          },
        },
      },
    );
    expect(dispatchRes.ok()).toBeTruthy();

    const dispatchData = await dispatchRes.json();
    expect(dispatchData.success).toBe(true);
    expect(dispatchData.data.workflow_id).toBeDefined();

    const workflowId = dispatchData.data.workflow_id;
    expect(workflowId).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/,
    );
  });

  test("no console errors on page load", async ({ page }) => {
    const errors = trackConsoleErrors(page);

    await page.goto("/");
    await expect(page.locator("body")).toBeVisible();

    expect(errors).toHaveLength(0);
  });

  test("invalid mutation returns proper error", async ({ request }) => {
    const response = await request.post(`${API_URL}/_api/rpc/delete_user`, {
      headers: { "Content-Type": "application/json" },
      data: { args: { id: "00000000-0000-0000-0000-000000000000" } },
    });

    expect(response.ok()).toBeTruthy();
    const data = await response.json();
    expect(data.success).toBe(true);
    expect(data.data).toBe(false);
  });
});
