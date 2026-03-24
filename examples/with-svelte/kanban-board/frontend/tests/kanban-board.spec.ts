import { randomUUID } from "node:crypto";
import { test, expect, ACTION_TIMEOUT, uniqueId, makeUser } from "./fixtures";

test.describe("Kanban Board UI E2E", () => {
  test("registers, creates project, and renames project", async ({
    page,
    registerUser,
    createProject,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("project");
    const renamed = uniqueId("project-renamed");

    await registerUser(user);
    await createProject(projectName);

    await page
      .locator(".project-row", { hasText: projectName })
      .getByRole("button", { name: "Rename" })
      .click();

    const renameInput = page.locator(".rename-input");
    await expect(renameInput).toBeVisible();
    await renameInput.clear();
    await renameInput.fill(renamed);
    await renameInput.press("Enter");

    await expect(
      page.locator(".project-list a", { hasText: renamed }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });
  });

  test("opens board and performs task lifecycle through UI", async ({
    page,
    registerUser,
    createProject,
    openProject,
    createTask,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("board");
    const task = uniqueId("task");
    const editedTask = uniqueId("task-edited");

    await registerUser(user);
    await createProject(projectName);
    await openProject(projectName);
    await createTask(task);

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
    ).toBeVisible({ timeout: ACTION_TIMEOUT });

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
    ).toBeVisible({ timeout: ACTION_TIMEOUT });

    await page
      .locator(".column", { hasText: "In Progress" })
      .locator(".card", { hasText: editedTask })
      .locator("button.delete")
      .click();
    await expect(page.locator(".card", { hasText: editedTask })).toHaveCount(
      0,
      { timeout: ACTION_TIMEOUT },
    );
  });

  test("exports JSON and CSV through UI", async ({
    page,
    registerUser,
    createProject,
    openProject,
    createTask,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("export-project");

    await registerUser(user);
    await createProject(projectName);
    await openProject(projectName);

    await createTask(uniqueId("task-a"));
    await createTask(uniqueId("task-b"));

    // Job completion can be slow (worker poll + execution + SSE push).
    // Use a generous fixed timeout rather than a multiple of ACTION_TIMEOUT.
    const JOB_TIMEOUT = 60_000;

    await page.getByRole("button", { name: "Export JSON" }).click();
    await expect(page.locator(".job-status")).toContainText("tasks exported", {
      timeout: JOB_TIMEOUT,
    });

    await page.getByRole("button", { name: "Export CSV" }).click();
    await expect(page.locator(".job-status")).toContainText("Export:", {
      timeout: JOB_TIMEOUT,
    });
  });

  test("schedules deletion with countdown and allows unarchive", async ({
    page,
    registerUser,
    createProject,
    openProject,
    createTask,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("schedule-delete");

    await registerUser(user);
    await createProject(projectName);
    await openProject(projectName);
    await createTask(uniqueId("task-to-delete"));

    await page
      .getByRole("button", { name: "Schedule Export + Delete" })
      .click();

    await expect(page.locator(".confirm-banner")).toBeVisible();
    await page.getByRole("button", { name: "Confirm Delete" }).click();

    await expect(
      page.locator(".job-status", { hasText: "Deletion workflow:" }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });

    // Workflow sets archive_delete_at asynchronously. The PG NOTIFY may not
    // reach the SSE subscription in time, so reload to force a fresh fetch.
    await page.waitForTimeout(2_000);
    await page.reload();
    await expect(page.locator(".archive-notice")).toContainText(
      "scheduled for deletion in 7 days",
      { timeout: ACTION_TIMEOUT * 2 },
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

    // Same SSE propagation issue: reload to pick up the cleared archive state.
    await page.waitForTimeout(1_000);
    await page.reload();
    await expect(page.locator(".archive-notice")).toHaveCount(0, {
      timeout: ACTION_TIMEOUT,
    });
    await expect(page.locator(".badge.archived")).toHaveCount(0, {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("shows auth errors for duplicate register and invalid login", async ({
    page,
    registerUser,
    signOut,
  }) => {
    const user = makeUser();

    await registerUser(user);
    await signOut();

    await page.goto("/");
    await expect(
      page.getByRole("heading", { name: "Kanban Board" }),
    ).toBeVisible();
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
    registerUser,
    loginUser,
    signOut,
    createProject,
  }) => {
    const userA = makeUser();
    const userB = makeUser();
    const projectA = uniqueId("user-a-project");

    await registerUser(userA);
    await createProject(projectA);
    await signOut();

    await registerUser(userB);
    await expect(
      page.locator(".project-list a", { hasText: projectA }),
    ).toHaveCount(0);
    await signOut();

    await loginUser(userA);
    await expect(
      page.locator(".project-list a", { hasText: projectA }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });
  });

  test("blocks cross-user deep-link access to another user's project", async ({
    page,
    registerUser,
    signOut,
    createProject,
    getProjectId,
  }) => {
    const userA = makeUser();
    const userB = makeUser();
    const projectName = uniqueId("private-project");

    await registerUser(userA);
    await createProject(projectName);
    const projectId = await getProjectId(projectName);
    await signOut();

    await registerUser(userB);
    await page.goto(`/app/${projectId}`);
    await expect(page.locator(".error")).toContainText("Project not found", {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("rejects tampered user scope for list_projects", async ({
    page,
    registerUser,
    createProject,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("scope-project");

    await registerUser(user);
    await createProject(projectName);

    await page.evaluate((tamperedId) => {
      const raw = localStorage.getItem("forge_auth");
      if (!raw) return;
      const parsed = JSON.parse(raw) as { user?: { id?: string } };
      if (parsed.user) parsed.user.id = tamperedId;
      localStorage.setItem("forge_auth", JSON.stringify(parsed));
    }, randomUUID());

    await page.goto("/app");
    await expect(page.locator(".error")).toContainText(
      "does not match authenticated principal",
      { timeout: ACTION_TIMEOUT },
    );
  });

  test("move task backward through columns", async ({
    page,
    registerUser,
    createProject,
    openProject,
    createTask,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("backward");
    const task = uniqueId("move-back");

    await registerUser(user);
    await createProject(projectName);
    await openProject(projectName);
    await createTask(task);

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

  test("create task via form submit (Enter key)", async ({
    page,
    registerUser,
    createProject,
    openProject,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("enter-create");
    const task = uniqueId("enter-task");

    await registerUser(user);
    await createProject(projectName);
    await openProject(projectName);

    await page.getByPlaceholder("New task title").fill(task);
    await page.getByPlaceholder("New task title").press("Enter");

    await expect(page.locator(".card", { hasText: task })).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });
  });

  test("add task button disabled when title is empty", async ({
    page,
    registerUser,
    createProject,
    openProject,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("disabled-btn");

    await registerUser(user);
    await createProject(projectName);
    await openProject(projectName);

    await expect(page.getByRole("button", { name: "Add task" })).toBeDisabled();
  });

  test("project count displays on projects page", async ({
    page,
    registerUser,
    createProject,
  }) => {
    const user = makeUser();
    const p1 = uniqueId("count-p1");
    const p2 = uniqueId("count-p2");

    await registerUser(user);
    await createProject(p1);
    await createProject(p2);

    await expect(page.locator(".subtitle")).toContainText("2 project");
  });

  test("empty board shows four columns with zero counts", async ({
    page,
    registerUser,
    createProject,
    openProject,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("empty-board");

    await registerUser(user);
    await createProject(projectName);
    await openProject(projectName);

    for (const label of ["Backlog", "Todo", "In Progress", "Done"]) {
      await expect(page.locator(".column", { hasText: label })).toBeVisible();
      await expect(
        page
          .locator(".column", { hasText: label })
          .locator(".count", { hasText: "0" }),
      ).toBeVisible();
    }
  });

  test("task shows correct priority badge", async ({
    page,
    registerUser,
    createProject,
    openProject,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("priority-badge");
    const task = uniqueId("high-task");

    await registerUser(user);
    await createProject(projectName);
    await openProject(projectName);

    await page.getByPlaceholder("New task title").fill(task);
    await page.locator(".create-form select").selectOption("high");
    await page.getByRole("button", { name: "Add task" }).click();

    const card = page.locator(".card", { hasText: task });
    await expect(card).toBeVisible({ timeout: ACTION_TIMEOUT });
    await expect(card.locator(".priority")).toContainText("High");
  });

  test("create project via form submit (Enter key)", async ({
    page,
    registerUser,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("enter-project");

    await registerUser(user);

    await page.getByPlaceholder("New project name").fill(projectName);
    await page.getByPlaceholder("New project name").press("Enter");

    await expect(
      page.locator(".project-list a", { hasText: projectName }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });
  });

  test("create button disabled when project name empty", async ({
    page,
    registerUser,
  }) => {
    const user = makeUser();

    await registerUser(user);

    await expect(
      page.locator(".create-form button[type='submit']"),
    ).toBeDisabled();
  });

  test("empty projects page shows placeholder message", async ({
    page,
    registerUser,
  }) => {
    const user = makeUser();

    await registerUser(user);

    await expect(page.locator(".empty-panel")).toContainText("No projects yet");
  });

  test("rename via Escape cancels edit", async ({
    page,
    registerUser,
    createProject,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("esc-rename");

    await registerUser(user);
    await createProject(projectName);

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

  test("edit task via Escape cancels edit", async ({
    page,
    registerUser,
    createProject,
    openProject,
    createTask,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("esc-edit");
    const task = uniqueId("esc-task");

    await registerUser(user);
    await createProject(projectName);
    await openProject(projectName);
    await createTask(task);

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

  test("task column counts update correctly", async ({
    page,
    registerUser,
    createProject,
    openProject,
    createTask,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("col-count");
    const t1 = uniqueId("count-1");
    const t2 = uniqueId("count-2");

    await registerUser(user);
    await createProject(projectName);
    await openProject(projectName);

    await createTask(t1);
    await createTask(t2);

    await expect(
      page
        .locator(".column", { hasText: "Backlog" })
        .locator(".count", { hasText: "2" }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });

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

  test("sign in via Enter key submits form", async ({
    page,
    registerUser,
    signOut,
  }) => {
    const user = makeUser();

    await registerUser(user);
    await signOut();

    await page.goto("/");
    await expect(
      page.getByRole("heading", { name: "Kanban Board" }),
    ).toBeVisible();
    await page.getByLabel("Email").fill(user.email);
    await page.getByLabel("Password").fill(user.password);
    await page.getByLabel("Password").press("Enter");

    await expect(page).toHaveURL(/\/app$/);
  });

  test("delete task removes it from the board", async ({
    page,
    registerUser,
    createProject,
    openProject,
    createTask,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("del-task");
    const task = uniqueId("to-delete");

    await registerUser(user);
    await createProject(projectName);
    await openProject(projectName);
    await createTask(task);

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

  test("back to projects link navigates correctly", async ({
    page,
    registerUser,
    createProject,
    openProject,
  }) => {
    const user = makeUser();
    const projectName = uniqueId("back-nav");

    await registerUser(user);
    await createProject(projectName);
    await openProject(projectName);

    await page.locator("a", { hasText: "Projects" }).click();
    await expect(page).toHaveURL(/\/app$/);
    await expect(page.getByRole("heading", { name: "Projects" })).toBeVisible();
  });
});
