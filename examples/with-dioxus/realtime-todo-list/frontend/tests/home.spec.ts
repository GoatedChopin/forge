import { test, expect, API_URL, ACTION_TIMEOUT, uniqueId } from "./fixtures";

const INPUT = 'input[placeholder="What needs to be done?"]';

async function deleteAllTodos(
  rpc: (fn: string, args?: unknown) => Promise<unknown>,
) {
  const todos = await rpc("list_todos");
  if (!Array.isArray(todos)) return;
  for (const todo of todos) {
    await rpc("delete_todo", { id: todo.id });
  }
}

test.beforeEach(async ({ rpc }) => {
  await deleteAllTodos(rpc);
});

test.afterEach(async ({ rpc }) => {
  await deleteAllTodos(rpc);
});

test.describe("smoke", () => {
  test("page loads with heading visible", async ({ page, gotoReady }) => {
    const errors: string[] = [];
    page.on("pageerror", (err) => errors.push(err.message));

    await gotoReady();
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
  test("create todo via button click", async ({ page, gotoReady }) => {
    const title = uniqueId("click-add");
    await gotoReady();

    await page.fill(INPUT, title);
    await page.click(".input-row button");

    await expect(page.locator(".title", { hasText: title })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("create todo via Enter key", async ({ page, gotoReady }) => {
    const title = uniqueId("enter-add");
    await gotoReady();

    await page.fill(INPUT, title);
    await page.press(INPUT, "Enter");

    await expect(page.locator(".title", { hasText: title })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("toggle completion applies strikethrough", async ({
    page,
    gotoReady,
  }) => {
    const title = uniqueId("toggle");
    await gotoReady();

    await page.fill(INPUT, title);
    await page.click(".input-row button");
    await expect(page.locator(".title", { hasText: title })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    const todoItem = page.locator("li", { hasText: title });
    await todoItem.locator("button.toggle").click();

    await expect(todoItem).toHaveClass(/completed/, {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("delete removes todo from list", async ({ page, gotoReady }) => {
    const title = uniqueId("delete");
    await gotoReady();

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

  test("add button disabled when input empty", async ({ page, gotoReady }) => {
    await gotoReady();
    await expect(page.locator(".input-row button")).toBeDisabled();
  });
});

test.describe("reactivity", () => {
  test("remaining count updates on add and complete", async ({
    page,
    gotoReady,
  }) => {
    const title = uniqueId("count");
    await gotoReady();

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
    await todoItem.locator("label").click();
    await expect(page.locator(".count")).toHaveText("0 remaining", {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("multiple rapid adds all appear", async ({ page, gotoReady }) => {
    const titles = [
      uniqueId("rapid-1"),
      uniqueId("rapid-2"),
      uniqueId("rapid-3"),
    ];
    await gotoReady();

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
  test("whitespace-only input does not create a todo", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    await page.fill(INPUT, "   ");
    await expect(page.locator(".input-row button")).toBeDisabled();

    await page.fill(INPUT, "   ");
    await page.press(INPUT, "Enter");

    await expect(
      page.locator(".status", { hasText: "No todos yet" }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });
  });

  test("input clears after successful add", async ({ page, gotoReady }) => {
    const title = uniqueId("clear-input");
    await gotoReady();

    await page.fill(INPUT, title);
    await page.click(".input-row button");

    await expect(page.locator(INPUT)).toHaveValue("", {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("untoggle completed todo restores it", async ({ page, gotoReady }) => {
    const title = uniqueId("untoggle");
    await gotoReady();

    await page.fill(INPUT, title);
    await page.click(".input-row button");
    const todoItem = page.locator("li", { hasText: title });
    await expect(todoItem).toBeVisible({ timeout: ACTION_TIMEOUT });

    await todoItem.locator("button.toggle").click();
    await expect(todoItem).toHaveClass(/completed/, {
      timeout: ACTION_TIMEOUT,
    });

    await todoItem.locator("button.toggle").click();
    await expect(todoItem).not.toHaveClass(/completed/, {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("empty state returns after deleting last todo", async ({
    page,
    gotoReady,
  }) => {
    const title = uniqueId("last-delete");
    await gotoReady();

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

  test("summary header shows remaining count above list", async ({
    page,
    gotoReady,
  }) => {
    const t1 = uniqueId("summary-1");
    const t2 = uniqueId("summary-2");
    await gotoReady();

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

  test("todos persist after page reload", async ({ page, gotoReady }) => {
    const title = uniqueId("persist");
    await gotoReady();

    await page.fill(INPUT, title);
    await page.click(".input-row button");
    await expect(page.locator(".title", { hasText: title })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    await page.reload();
    await expect(page.locator(".title", { hasText: title })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("completed state persists after page reload", async ({
    page,
    gotoReady,
  }) => {
    const title = uniqueId("persist-done");
    await gotoReady();

    await page.fill(INPUT, title);
    await page.click(".input-row button");
    const todoItem = page.locator("li", { hasText: title });
    await expect(todoItem).toBeVisible({ timeout: ACTION_TIMEOUT });

    await todoItem.locator("button.toggle").click();
    await expect(todoItem).toHaveClass(/completed/, {
      timeout: ACTION_TIMEOUT,
    });

    await page.reload();
    const reloaded = page.locator("li", { hasText: title });
    await expect(reloaded).toHaveClass(/completed/, {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("delete button becomes visible on hover", async ({
    page,
    gotoReady,
  }) => {
    const title = uniqueId("hover-del");
    await gotoReady();

    await page.fill(INPUT, title);
    await page.click(".input-row button");
    const todoItem = page.locator("li", { hasText: title });
    await expect(todoItem).toBeVisible({ timeout: ACTION_TIMEOUT });

    const deleteBtn = todoItem.locator("button.delete");
    await expect(deleteBtn).toHaveCSS("opacity", "0");

    await todoItem.hover();
    await expect(deleteBtn).toHaveCSS("opacity", "1");
  });

  test("bottom count reflects mix of completed and active", async ({
    page,
    gotoReady,
  }) => {
    const t1 = uniqueId("mix-1");
    const t2 = uniqueId("mix-2");
    const t3 = uniqueId("mix-3");
    await gotoReady();

    for (const t of [t1, t2, t3]) {
      await page.fill(INPUT, t);
      await page.click(".input-row button");
      await expect(page.locator(".title", { hasText: t })).toBeVisible({
        timeout: ACTION_TIMEOUT,
      });
    }

    await page.locator("li", { hasText: t1 }).locator("button.toggle").click();
    await expect(page.locator(".count")).toHaveText("2 remaining", {
      timeout: ACTION_TIMEOUT,
    });

    await page.locator("li", { hasText: t2 }).locator("button.toggle").click();
    await expect(page.locator(".count")).toHaveText("1 remaining", {
      timeout: ACTION_TIMEOUT,
    });
  });
});
