import {
  test,
  expect,
  ACTION_TIMEOUT,
  uniqueId,
  trackConsoleErrors,
} from "./fixtures";

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

test("todo flow stays reactive through create, toggle, and delete", async ({
  page,
  gotoReady,
}) => {
  const title = uniqueId("release");
  const errors = trackConsoleErrors(page);

  await gotoReady();
  await expect(page.locator("h1")).toHaveText("Todos");
  await expect(
    page.locator(".status", { hasText: "No todos yet" }),
  ).toBeVisible({
    timeout: ACTION_TIMEOUT,
  });

  await page.fill(INPUT, title);
  await page.click(".input-row button");

  const todoItem = page.locator("li", { hasText: title });
  await expect(todoItem).toBeVisible({ timeout: ACTION_TIMEOUT });
  await expect(page.locator(".count")).toHaveText("1 remaining", {
    timeout: ACTION_TIMEOUT,
  });
  await expect(page.locator(INPUT)).toHaveValue("", {
    timeout: ACTION_TIMEOUT,
  });

  await todoItem.locator("label").click();
  await expect(todoItem).toHaveClass(/completed/, { timeout: ACTION_TIMEOUT });
  await expect(page.locator(".count")).toHaveText("0 remaining", {
    timeout: ACTION_TIMEOUT,
  });

  await todoItem.locator("button.delete").click();
  await expect(todoItem).not.toBeVisible({ timeout: ACTION_TIMEOUT });
  await expect(
    page.locator(".status", { hasText: "No todos yet" }),
  ).toBeVisible({
    timeout: ACTION_TIMEOUT,
  });
  expect(errors).toHaveLength(0);
});
