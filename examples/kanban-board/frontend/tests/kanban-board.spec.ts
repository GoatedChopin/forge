import { randomUUID } from "node:crypto";
import { expect, test, type Page } from "@playwright/test";

const ACTION_TIMEOUT = process.env.CI ? 20_000 : 12_000;

type UserCreds = {
  name: string;
  email: string;
  password: string;
};

function uniqueId(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function makeUser(): UserCreds {
  const id = uniqueId("kb-user");
  return {
    name: `User ${id}`,
    email: `${id}@example.com`,
    password: "password123",
  };
}

async function gotoAuth(page: Page) {
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "Kanban Board" }),
  ).toBeVisible();
}

async function registerUser(page: Page, user: UserCreds) {
  await gotoAuth(page);
  await page.getByRole("button", { name: "Register" }).click();
  await page.getByLabel("Name").fill(user.name);
  await page.getByLabel("Email").fill(user.email);
  await page.getByLabel("Password").fill(user.password);
  await page.getByRole("button", { name: "Create account" }).click();
  await expect(page).toHaveURL(/\/app$/);
  await expect(page.getByRole("heading", { name: "Projects" })).toBeVisible();
}

async function loginUser(page: Page, user: UserCreds) {
  await gotoAuth(page);
  await page.getByLabel("Email").fill(user.email);
  await page.getByLabel("Password").fill(user.password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL(/\/app$/);
}

async function signOut(page: Page) {
  await page.getByRole("button", { name: "Sign out" }).click();
  await expect(page).toHaveURL("/");
}

async function createProject(page: Page, name: string) {
  await page.getByPlaceholder("New project name").fill(name);
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.locator(".project-list a", { hasText: name })).toBeVisible({
    timeout: ACTION_TIMEOUT,
  });
}

async function openProject(page: Page, name: string) {
  await page.locator(".project-list a", { hasText: name }).click();
  await expect(page).toHaveURL(/\/app\/.+/);
  await expect(page.getByPlaceholder("New task title")).toBeVisible();
}

async function getProjectId(page: Page, name: string): Promise<string> {
  const href = await page
    .locator(".project-list a", { hasText: name })
    .first()
    .getAttribute("href");
  if (!href) {
    throw new Error(`Missing href for project ${name}`);
  }

  const projectId = href.replace("/app/", "");
  if (!projectId) {
    throw new Error(`Unexpected project link format: ${href}`);
  }

  return projectId;
}

async function createTask(page: Page, title: string) {
  await page.getByPlaceholder("New task title").fill(title);
  await page.getByRole("button", { name: "Add task" }).click();
  await expect(page.locator(".card", { hasText: title })).toBeVisible({
    timeout: ACTION_TIMEOUT,
  });
}

test.describe("Kanban Board UI E2E", () => {
  test("registers, creates project, and renames project", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("project");
    const renamed = uniqueId("project-renamed");

    await registerUser(page, user);
    await createProject(page, projectName);

    // Click rename to show inline input
    await page
      .locator(".project-row", { hasText: projectName })
      .getByRole("button", { name: "Rename" })
      .click();

    // Fill inline rename input and submit with Enter
    const renameInput = page.locator(".rename-input");
    await expect(renameInput).toBeVisible();
    await renameInput.clear();
    await renameInput.fill(renamed);
    await renameInput.press("Enter");

    await expect(
      page.locator(".project-list a", { hasText: renamed }),
    ).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("opens board and performs task lifecycle through UI", async ({
    page,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("board");
    const task = uniqueId("task");
    const editedTask = uniqueId("task-edited");

    await registerUser(page, user);
    await createProject(page, projectName);
    await openProject(page, projectName);

    await createTask(page, task);

    // Click edit to show inline input
    await page
      .locator(".card", { hasText: task })
      .locator("button.edit")
      .click();

    const editInput = page.locator(".edit-input");
    await expect(editInput).toBeVisible();
    await editInput.clear();
    await editInput.fill(editedTask);
    await editInput.press("Enter");

    await expect(page.locator(".card", { hasText: editedTask })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    const card = page.locator(".card", { hasText: editedTask });
    await card
      .locator(".card-actions button")
      .filter({ hasText: "→" })
      .first()
      .click();
    await expect(
      page
        .locator(".column", { hasText: "Todo" })
        .locator(".card", { hasText: editedTask }),
    ).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    await page
      .locator(".column", { hasText: "Todo" })
      .locator(".card", { hasText: editedTask })
      .locator(".card-actions button")
      .filter({ hasText: "→" })
      .first()
      .click();
    await expect(
      page
        .locator(".column", { hasText: "In Progress" })
        .locator(".card", { hasText: editedTask }),
    ).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    await page
      .locator(".column", { hasText: "In Progress" })
      .locator(".card", { hasText: editedTask })
      .locator("button.delete")
      .click();
    await expect(page.locator(".card", { hasText: editedTask })).toHaveCount(
      0,
      {
        timeout: ACTION_TIMEOUT,
      },
    );
  });

  test("exports JSON and CSV through UI", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("export-project");

    await registerUser(page, user);
    await createProject(page, projectName);
    await openProject(page, projectName);

    await createTask(page, uniqueId("task-a"));
    await createTask(page, uniqueId("task-b"));

    await page.getByRole("button", { name: "Export JSON" }).click();
    await expect(page.locator(".job-status")).toContainText("tasks exported", {
      timeout: ACTION_TIMEOUT,
    });

    await page.getByRole("button", { name: "Export CSV" }).click();
    await expect(page.locator(".job-status")).toContainText("Export:", {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("schedules deletion with countdown and allows unarchive", async ({
    page,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("schedule-delete");

    await registerUser(page, user);
    await createProject(page, projectName);
    await openProject(page, projectName);
    await createTask(page, uniqueId("task-to-delete"));

    // Click schedule and confirm via inline banner
    await page
      .getByRole("button", { name: "Schedule Export + Delete" })
      .click();

    await expect(page.locator(".confirm-banner")).toBeVisible();
    await page.getByRole("button", { name: "Confirm Delete" }).click();

    await expect(
      page.locator(".job-status", { hasText: "Deletion workflow:" }),
    ).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
    await expect(page.locator(".archive-notice")).toContainText(
      "scheduled for deletion in 7 days",
      { timeout: ACTION_TIMEOUT },
    );
    await expect(page.locator(".archive-notice")).toContainText("Time left:", {
      timeout: ACTION_TIMEOUT,
    });

    await expect(page.locator(".badge.archived")).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    await page
      .getByRole("button", { name: "Unarchive (Cancel Delete)" })
      .click();
    await expect(page.locator(".archive-notice")).toHaveCount(0, {
      timeout: ACTION_TIMEOUT,
    });
    await expect(page.locator(".badge.archived")).toHaveCount(0, {
      timeout: ACTION_TIMEOUT * 2,
    });
  });

  test("shows auth errors for duplicate register and invalid login", async ({
    page,
  }) => {
    const user = makeUser();

    await registerUser(page, user);
    await signOut(page);

    await gotoAuth(page);
    await page.getByRole("button", { name: "Register" }).click();
    await page.getByLabel("Name").fill(`${user.name} Duplicate`);
    await page.getByLabel("Email").fill(user.email);
    await page.getByLabel("Password").fill(user.password);
    await page.getByRole("button", { name: "Create account" }).click();
    await expect(page.locator(".error")).toContainText("already registered");

    await page.getByRole("button", { name: "Sign in" }).click();
    await page.getByLabel("Email").fill(user.email);
    await page.getByLabel("Password").fill("wrong-password");
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page.locator(".error")).toContainText(
      "Invalid email or password",
    );
  });

  test("isolates projects per user and supports sign out/sign in", async ({
    page,
  }) => {
    const userA = makeUser();
    const userB = makeUser();
    const projectA = uniqueId("user-a-project");

    await registerUser(page, userA);
    await createProject(page, projectA);
    await signOut(page);

    await registerUser(page, userB);
    await expect(
      page.locator(".project-list a", { hasText: projectA }),
    ).toHaveCount(0);
    await signOut(page);

    await loginUser(page, userA);
    await expect(
      page.locator(".project-list a", { hasText: projectA }),
    ).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("blocks cross-user deep-link access to another user's project", async ({
    page,
  }) => {
    const userA = makeUser();
    const userB = makeUser();
    const projectName = uniqueId("private-project");

    await registerUser(page, userA);
    await createProject(page, projectName);
    const projectId = await getProjectId(page, projectName);
    await signOut(page);

    await registerUser(page, userB);
    await page.goto(`/app/${projectId}`);
    await expect(page.locator(".error")).toContainText("Project not found", {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("rejects tampered user scope for list_projects", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("scope-project");

    await registerUser(page, user);
    await createProject(page, projectName);

    await page.evaluate((tamperedId) => {
      const raw = localStorage.getItem("kanban_user");
      if (!raw) return;
      const parsed = JSON.parse(raw) as { id?: string };
      parsed.id = tamperedId;
      localStorage.setItem("kanban_user", JSON.stringify(parsed));
    }, randomUUID());

    await page.goto("/app");
    await expect(page.locator(".error")).toContainText(
      "does not match authenticated principal",
      {
        timeout: ACTION_TIMEOUT,
      },
    );
  });

  test("move task backward through columns", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("backward");
    const task = uniqueId("move-back");

    await registerUser(page, user);
    await createProject(page, projectName);
    await openProject(page, projectName);
    await createTask(page, task);

    // Move forward: Backlog -> Todo
    await page
      .locator(".card", { hasText: task })
      .locator(".card-actions button")
      .filter({ hasText: "→" })
      .first()
      .click();
    await expect(
      page
        .locator(".column", { hasText: "Todo" })
        .locator(".card", { hasText: task }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });

    // Move backward: Todo -> Backlog
    await page
      .locator(".column", { hasText: "Todo" })
      .locator(".card", { hasText: task })
      .locator(".card-actions button")
      .filter({ hasText: "←" })
      .first()
      .click();
    await expect(
      page
        .locator(".column", { hasText: "Backlog" })
        .locator(".card", { hasText: task }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });
  });

  test("create task via form submit (Enter key)", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("enter-create");
    const task = uniqueId("enter-task");

    await registerUser(page, user);
    await createProject(page, projectName);
    await openProject(page, projectName);

    await page.getByPlaceholder("New task title").fill(task);
    await page.getByPlaceholder("New task title").press("Enter");

    await expect(page.locator(".card", { hasText: task })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("add task button disabled when title is empty", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("disabled-btn");

    await registerUser(page, user);
    await createProject(page, projectName);
    await openProject(page, projectName);

    await expect(page.getByRole("button", { name: "Add task" })).toBeDisabled();
  });

  test("project count displays on projects page", async ({ page }) => {
    const user = makeUser();
    const p1 = uniqueId("count-p1");
    const p2 = uniqueId("count-p2");

    await registerUser(page, user);
    await createProject(page, p1);
    await createProject(page, p2);

    await expect(page.locator(".subtitle")).toContainText("2 project");
  });

  test("empty board shows four columns with zero counts", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("empty-board");

    await registerUser(page, user);
    await createProject(page, projectName);
    await openProject(page, projectName);

    for (const label of ["Backlog", "Todo", "In Progress", "Done"]) {
      await expect(page.locator(".column", { hasText: label })).toBeVisible();
      await expect(
        page
          .locator(".column", { hasText: label })
          .locator(".count", { hasText: "0" }),
      ).toBeVisible();
    }
  });

  test("task shows correct priority badge", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("priority-badge");
    const task = uniqueId("high-task");

    await registerUser(page, user);
    await createProject(page, projectName);
    await openProject(page, projectName);

    await page.getByPlaceholder("New task title").fill(task);
    await page.locator(".create-form select").selectOption("high");
    await page.getByRole("button", { name: "Add task" }).click();

    const card = page.locator(".card", { hasText: task });
    await expect(card).toBeVisible({ timeout: ACTION_TIMEOUT });
    await expect(card.locator(".priority")).toContainText("High");
  });

  test("create project via form submit (Enter key)", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("enter-project");

    await registerUser(page, user);

    await page.getByPlaceholder("New project name").fill(projectName);
    await page.getByPlaceholder("New project name").press("Enter");

    await expect(
      page.locator(".project-list a", { hasText: projectName }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });
  });

  test("create button disabled when project name empty", async ({ page }) => {
    const user = makeUser();

    await registerUser(page, user);

    await expect(
      page.locator(".create-form button[type='submit']"),
    ).toBeDisabled();
  });

  test("empty projects page shows placeholder message", async ({ page }) => {
    const user = makeUser();

    await registerUser(page, user);

    await expect(page.locator(".empty-panel")).toContainText("No projects yet");
  });

  test("rename via Escape cancels edit", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("esc-rename");

    await registerUser(page, user);
    await createProject(page, projectName);

    await page
      .locator(".project-row", { hasText: projectName })
      .getByRole("button", { name: "Rename" })
      .click();

    const renameInput = page.locator(".rename-input");
    await expect(renameInput).toBeVisible();
    await renameInput.clear();
    await renameInput.fill("should-not-save");
    await renameInput.press("Escape");

    await expect(
      page.locator(".project-list a", { hasText: projectName }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });
    await expect(
      page.locator(".project-list a", { hasText: "should-not-save" }),
    ).toHaveCount(0);
  });

  test("edit task via Escape cancels edit", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("esc-edit");
    const task = uniqueId("esc-task");

    await registerUser(page, user);
    await createProject(page, projectName);
    await openProject(page, projectName);
    await createTask(page, task);

    await page
      .locator(".card", { hasText: task })
      .locator("button.edit")
      .click();

    const editInput = page.locator(".edit-input");
    await expect(editInput).toBeVisible();
    await editInput.clear();
    await editInput.fill("should-not-save");
    await editInput.press("Escape");

    await expect(page.locator(".card", { hasText: task })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
    await expect(
      page.locator(".card", { hasText: "should-not-save" }),
    ).toHaveCount(0);
  });

  test("task column counts update correctly", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("col-count");
    const t1 = uniqueId("count-1");
    const t2 = uniqueId("count-2");

    await registerUser(page, user);
    await createProject(page, projectName);
    await openProject(page, projectName);

    await createTask(page, t1);
    await createTask(page, t2);

    await expect(
      page
        .locator(".column", { hasText: "Backlog" })
        .locator(".count", { hasText: "2" }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });

    // Move one task forward
    await page
      .locator(".card", { hasText: t1 })
      .locator(".card-actions button")
      .filter({ hasText: "→" })
      .first()
      .click();

    await expect(
      page
        .locator(".column", { hasText: "Backlog" })
        .locator(".count", { hasText: "1" }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });
    await expect(
      page
        .locator(".column", { hasText: "Todo" })
        .locator(".count", { hasText: "1" }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });
  });

  test("sign in via Enter key submits form", async ({ page }) => {
    const user = makeUser();

    await registerUser(page, user);
    await signOut(page);

    await gotoAuth(page);
    await page.getByLabel("Email").fill(user.email);
    await page.getByLabel("Password").fill(user.password);
    await page.getByLabel("Password").press("Enter");

    await expect(page).toHaveURL(/\/app$/);
  });

  test("delete task removes it from the board", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("del-task");
    const task = uniqueId("to-delete");

    await registerUser(page, user);
    await createProject(page, projectName);
    await openProject(page, projectName);
    await createTask(page, task);

    await page
      .locator(".card", { hasText: task })
      .locator("button.delete")
      .click();

    await expect(page.locator(".card", { hasText: task })).toHaveCount(0, {
      timeout: ACTION_TIMEOUT,
    });
    await expect(
      page
        .locator(".column", { hasText: "Backlog" })
        .locator(".count", { hasText: "0" }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });
  });

  test("back to projects link navigates correctly", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("back-nav");

    await registerUser(page, user);
    await createProject(page, projectName);
    await openProject(page, projectName);

    await page.locator("a", { hasText: "Projects" }).click();
    await expect(page).toHaveURL(/\/app$/);
    await expect(page.getByRole("heading", { name: "Projects" })).toBeVisible();
  });
});
