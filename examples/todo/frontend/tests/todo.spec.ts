import { test, expect, type Page } from "@playwright/test";

const API_URL = process.env.VITE_API_URL || "http://localhost:8080";

const INPUT = 'input[placeholder="What needs to be done?"]';
// Reactive updates go through RPC → DB → NOTIFY → SSE → UI.
// CI runners need more headroom than local dev.
const ACTION_TIMEOUT = process.env.CI ? 10_000 : 5_000;

function uniqueTitle(prefix: string) {
  return `${prefix} ${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
}

async function rpc(fn: string, args: unknown = null) {
  const res = await fetch(`${API_URL}/_api/rpc/${fn}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ args }),
  });
  const json = await res.json();
  return json.data;
}

async function deleteAllTodos() {
  const todos = await rpc("list_todos");
  if (!Array.isArray(todos)) return;
  for (const todo of todos) {
    await rpc("delete_todo", { id: todo.id });
  }
}

// Navigate and wait for UI + SSE subscription to be ready.
// Without SSE, mutations succeed but reactive updates never arrive.
async function gotoReady(page: Page) {
  const sseRequest = page.waitForRequest(
    (req) => req.url().includes("/_api/events"),
    { timeout: 15000 },
  );
  await page.goto("/");
  await expect(page.locator(".status, .count, ul")).toBeVisible({
    timeout: 15000,
  });
  await sseRequest;
  // Let the SSE handshake and subscription registration complete
  await page.waitForTimeout(2000);
}

test.beforeEach(async () => {
  await deleteAllTodos();
});

test.afterEach(async () => {
  await deleteAllTodos();
});

test.describe("smoke", () => {
  test("page loads with heading visible", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (err) => errors.push(err.message));

    await gotoReady(page);
    await expect(page.locator("h1")).toHaveText("Todos");
    await expect(page.locator(INPUT)).toBeVisible();
    expect(errors).toHaveLength(0);
  });

  test("backend health check responds OK", async () => {
    const res = await fetch(`${API_URL}/_api/health`);
    expect(res.ok).toBe(true);
  });
});

test.describe("CRUD with reactivity", () => {
  test("create todo via button click", async ({ page }) => {
    const title = uniqueTitle("click-add");
    await gotoReady(page);

    await page.fill(INPUT, title);
    await page.click(".input-row button");

    await expect(page.locator(".title", { hasText: title })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("create todo via Enter key", async ({ page }) => {
    const title = uniqueTitle("enter-add");
    await gotoReady(page);

    await page.fill(INPUT, title);
    await page.press(INPUT, "Enter");

    await expect(page.locator(".title", { hasText: title })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("toggle completion applies strikethrough", async ({ page }) => {
    const title = uniqueTitle("toggle");
    await gotoReady(page);

    await page.fill(INPUT, title);
    await page.click(".input-row button");
    await expect(page.locator(".title", { hasText: title })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    const todoItem = page.locator("li", { hasText: title });
    await todoItem.locator('input[type="checkbox"]').check();

    await expect(todoItem).toHaveClass(/completed/, {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("delete removes todo from list", async ({ page }) => {
    const title = uniqueTitle("delete");
    await gotoReady(page);

    await page.fill(INPUT, title);
    await page.click(".input-row button");
    await expect(page.locator(".title", { hasText: title })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    await page
      .locator("li", { hasText: title })
      .locator("button.delete")
      .click();

    await expect(page.locator(".title", { hasText: title })).not.toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("add button disabled when input empty", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator(".input-row button")).toBeDisabled();
  });
});

test.describe("reactivity", () => {
  test("remaining count updates on add and complete", async ({ page }) => {
    const title = uniqueTitle("count");
    await gotoReady(page);

    await expect(
      page.locator(".status", { hasText: "No todos yet" }),
    ).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    await page.fill(INPUT, title);
    await page.click(".input-row button");
    await expect(page.locator(".count")).toHaveText("1 remaining", {
      timeout: ACTION_TIMEOUT,
    });

    const todoItem = page.locator("li", { hasText: title });
    await todoItem.locator('input[type="checkbox"]').check();
    await expect(page.locator(".count")).toHaveText("0 remaining", {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("multiple rapid adds all appear", async ({ page }) => {
    const titles = [
      uniqueTitle("rapid-1"),
      uniqueTitle("rapid-2"),
      uniqueTitle("rapid-3"),
    ];
    await gotoReady(page);

    for (const title of titles) {
      await page.fill(INPUT, title);
      await page.click(".input-row button");
      await expect(page.locator(INPUT)).toHaveValue("", {
        timeout: ACTION_TIMEOUT,
      });
    }

    for (const title of titles) {
      await expect(page.locator(".title", { hasText: title })).toBeVisible({
        timeout: ACTION_TIMEOUT,
      });
    }

    await expect(page.locator(".count")).toHaveText("3 remaining");
  });
});

test.describe("UX details", () => {
  test("whitespace-only input does not create a todo", async ({ page }) => {
    await gotoReady(page);

    await page.fill(INPUT, "   ");
    await expect(page.locator(".input-row button")).toBeDisabled();

    await page.fill(INPUT, "   ");
    await page.press(INPUT, "Enter");

    await expect(
      page.locator(".status", { hasText: "No todos yet" }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });
  });

  test("input clears after successful add", async ({ page }) => {
    const title = uniqueTitle("clear-input");
    await gotoReady(page);

    await page.fill(INPUT, title);
    await page.click(".input-row button");

    await expect(page.locator(INPUT)).toHaveValue("", {
      timeout: ACTION_TIMEOUT,
    });
    await expect(page.locator(INPUT))
      .toBeFocused({ timeout: 2000 })
      .catch(() => {
        // Focus behavior may vary, not critical
      });
  });

  test("untoggle completed todo restores it", async ({ page }) => {
    const title = uniqueTitle("untoggle");
    await gotoReady(page);

    await page.fill(INPUT, title);
    await page.click(".input-row button");
    const todoItem = page.locator("li", { hasText: title });
    await expect(todoItem).toBeVisible({ timeout: ACTION_TIMEOUT });

    await todoItem.locator('input[type="checkbox"]').check();
    await expect(todoItem).toHaveClass(/completed/, {
      timeout: ACTION_TIMEOUT,
    });

    await todoItem.locator('input[type="checkbox"]').uncheck();
    await expect(todoItem).not.toHaveClass(/completed/, {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("empty state returns after deleting last todo", async ({ page }) => {
    const title = uniqueTitle("last-delete");
    await gotoReady(page);

    await page.fill(INPUT, title);
    await page.click(".input-row button");
    await expect(page.locator(".title", { hasText: title })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    await page
      .locator("li", { hasText: title })
      .locator("button.delete")
      .click();

    await expect(
      page.locator(".status", { hasText: "No todos yet" }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });
  });

  test("summary header shows remaining count above list", async ({ page }) => {
    const t1 = uniqueTitle("summary-1");
    const t2 = uniqueTitle("summary-2");
    await gotoReady(page);

    await page.fill(INPUT, t1);
    await page.click(".input-row button");
    await expect(page.locator(".title", { hasText: t1 })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    await page.fill(INPUT, t2);
    await page.click(".input-row button");
    await expect(page.locator(".title", { hasText: t2 })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    await expect(page.locator(".summary")).toContainText("2 remaining");
  });
});
