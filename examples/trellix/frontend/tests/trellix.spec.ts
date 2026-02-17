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
  const id = uniqueId("trellix-user");
  return {
    name: `User ${id}`,
    email: `${id}@example.com`,
    password: "password123",
  };
}

async function gotoAuth(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Trellix" })).toBeVisible();
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

test.describe("Trellix UI E2E", () => {
  test("registers, creates project, and renames project", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("project");
    const renamed = uniqueId("project-renamed");

    await registerUser(page, user);
    await createProject(page, projectName);

    page.once("dialog", (dialog) => dialog.accept(renamed));
    await page
      .locator(".project-row", { hasText: projectName })
      .getByRole("button", { name: "Rename" })
      .click();

    await expect(page.locator(".project-list a", { hasText: renamed })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("opens board and performs task lifecycle through UI", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("board");
    const task = uniqueId("task");
    const editedTask = uniqueId("task-edited");

    await registerUser(page, user);
    await createProject(page, projectName);
    await openProject(page, projectName);

    await createTask(page, task);

    page.once("dialog", (dialog) => dialog.accept(editedTask));
    await page.locator(".card", { hasText: task }).locator("button.edit").click();
    await expect(page.locator(".card", { hasText: editedTask })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    const card = page.locator(".card", { hasText: editedTask });
    await card.locator(".card-actions button").filter({ hasText: "→" }).first().click();
    await expect(page.locator(".column", { hasText: "Todo" }).locator(".card", { hasText: editedTask })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    await page
      .locator(".column", { hasText: "Todo" })
      .locator(".card", { hasText: editedTask })
      .locator(".card-actions button")
      .filter({ hasText: "→" })
      .first()
      .click();
    await expect(page.locator(".column", { hasText: "In Progress" }).locator(".card", { hasText: editedTask })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    await page
      .locator(".column", { hasText: "In Progress" })
      .locator(".card", { hasText: editedTask })
      .locator("button.delete")
      .click();
    await expect(page.locator(".card", { hasText: editedTask })).toHaveCount(0, {
      timeout: ACTION_TIMEOUT,
    });
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

  test("schedules deletion with countdown and allows unarchive", async ({ page }) => {
    const user = makeUser();
    const projectName = uniqueId("schedule-delete");

    await registerUser(page, user);
    await createProject(page, projectName);
    await openProject(page, projectName);
    await createTask(page, uniqueId("task-to-delete"));

    page.once("dialog", (dialog) => dialog.accept());
    await page.getByRole("button", { name: "Schedule Export + Delete" }).click();

    await expect(page.locator(".job-status", { hasText: "Deletion workflow:" })).toBeVisible({
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

    await page.getByRole("button", { name: "Unarchive (Cancel Delete)" }).click();
    await expect(page.locator(".archive-notice")).toHaveCount(0, {
      timeout: ACTION_TIMEOUT,
    });
    await expect(page.locator(".badge.archived")).toHaveCount(0, {
      timeout: ACTION_TIMEOUT * 2,
    });
  });

  test("shows auth errors for duplicate register and invalid login", async ({ page }) => {
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
    await expect(page.locator(".error")).toContainText("Invalid email or password");
  });

  test("isolates projects per user and supports sign out/sign in", async ({ page }) => {
    const userA = makeUser();
    const userB = makeUser();
    const projectA = uniqueId("user-a-project");

    await registerUser(page, userA);
    await createProject(page, projectA);
    await signOut(page);

    await registerUser(page, userB);
    await expect(page.locator(".project-list a", { hasText: projectA })).toHaveCount(0);
    await signOut(page);

    await loginUser(page, userA);
    await expect(page.locator(".project-list a", { hasText: projectA })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("blocks cross-user deep-link access to another user's project", async ({ page }) => {
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
      const raw = localStorage.getItem("trellix_user");
      if (!raw) return;
      const parsed = JSON.parse(raw) as { id?: string };
      parsed.id = tamperedId;
      localStorage.setItem("trellix_user", JSON.stringify(parsed));
    }, randomUUID());

    await page.goto("/app");
    await expect(page.locator(".error")).toContainText(
      "does not match authenticated principal",
      {
        timeout: ACTION_TIMEOUT,
      },
    );
  });
});
