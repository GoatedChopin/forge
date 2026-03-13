import { test as base, expect } from "@playwright/test";

export { expect };

export const ACTION_TIMEOUT = process.env.CI ? 20_000 : 12_000;

export function uniqueId(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

export type UserCreds = {
  name: string;
  email: string;
  password: string;
};

export function makeUser(): UserCreds {
  const id = uniqueId("kb-user");
  return {
    name: `User ${id}`,
    email: `${id}@example.com`,
    password: "password123",
  };
}

type KanbanFixtures = {
  registerUser: (user: UserCreds) => Promise<void>;
  loginUser: (user: UserCreds) => Promise<void>;
  signOut: () => Promise<void>;
  createProject: (name: string) => Promise<void>;
  openProject: (name: string) => Promise<void>;
  getProjectId: (name: string) => Promise<string>;
  createTask: (title: string) => Promise<void>;
};

export const test = base.extend<KanbanFixtures>({
  registerUser: async ({ page }, use) => {
    await use(async (user: UserCreds) => {
      await page.goto("/");
      await expect(
        page.getByRole("heading", { name: "Kanban Board" }),
      ).toBeVisible();
      await page.getByRole("button", { name: "Register" }).click();
      await page.getByLabel("Name").fill(user.name);
      await page.getByLabel("Email").fill(user.email);
      await page.getByLabel("Password").fill(user.password);
      await page.getByRole("button", { name: "Create account" }).click();
      await expect(page).toHaveURL(/\/app$/);
      await expect(
        page.getByRole("heading", { name: "Projects" }),
      ).toBeVisible();
    });
  },

  loginUser: async ({ page }, use) => {
    await use(async (user: UserCreds) => {
      await page.goto("/");
      await expect(
        page.getByRole("heading", { name: "Kanban Board" }),
      ).toBeVisible();
      await page.getByLabel("Email").fill(user.email);
      await page.getByLabel("Password").fill(user.password);
      await page.getByRole("button", { name: "Sign in" }).click();
      await expect(page).toHaveURL(/\/app$/);
    });
  },

  signOut: async ({ page }, use) => {
    await use(async () => {
      await page.getByRole("button", { name: "Sign out" }).click();
      await expect(page).toHaveURL("/");
    });
  },

  createProject: async ({ page }, use) => {
    await use(async (name: string) => {
      await page.getByPlaceholder("New project name").fill(name);
      await page.getByRole("button", { name: "Create" }).click();
      await expect(
        page.locator(".project-list a", { hasText: name }),
      ).toBeVisible({ timeout: ACTION_TIMEOUT });
    });
  },

  openProject: async ({ page }, use) => {
    await use(async (name: string) => {
      await page.locator(".project-list a", { hasText: name }).click();
      await expect(page).toHaveURL(/\/app\/.+/);
      await expect(page.getByPlaceholder("New task title")).toBeVisible();
    });
  },

  getProjectId: async ({ page }, use) => {
    await use(async (name: string) => {
      const href = await page
        .locator(".project-list a", { hasText: name })
        .first()
        .getAttribute("href");
      if (!href) throw new Error(`Missing href for project ${name}`);
      const projectId = href.replace("/app/", "");
      if (!projectId)
        throw new Error(`Unexpected project link format: ${href}`);
      return projectId;
    });
  },

  createTask: async ({ page }, use) => {
    await use(async (title: string) => {
      await page.getByPlaceholder("New task title").fill(title);
      await page.getByRole("button", { name: "Add task" }).click();
      await expect(page.locator(".card", { hasText: title })).toBeVisible({
        timeout: ACTION_TIMEOUT,
      });
    });
  },
});
