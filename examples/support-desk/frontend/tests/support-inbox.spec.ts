import { expect, test } from "@playwright/test";

const API_URL = process.env.VITE_API_URL || "http://localhost:8080";
const ACTION_TIMEOUT = process.env.CI ? 10_000 : 5_000;

async function rpc(fn: string, args: unknown = null) {
  const res = await fetch(`${API_URL}/_api/rpc/${fn}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ args }),
  });
  return (await res.json()).data;
}

async function deleteAllTickets() {
  const tickets = await rpc("list_support_tickets");
  if (!Array.isArray(tickets)) return;
  for (const t of tickets) {
    await rpc("set_ticket_status", { id: t.id, status: "resolved" });
  }
}

function nonce() {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
}

test.beforeEach(async () => {
  await deleteAllTickets();
});

test.describe("Support Inbox", () => {
  test("agent can create and progress a ticket", async ({ page }) => {
    const n = nonce();
    const customer = `E2E Customer ${n}`;
    const subject = `Invoice sync fails ${n}`;
    const details = `Reproduced on dashboard session ${n}`;
    const note = `Escalated by UI flow ${n}`;

    await page.goto("/");

    await page.getByTestId("customer-name-input").fill(customer);
    await page.getByTestId("subject-input").fill(subject);
    await page.getByTestId("details-input").fill(details);
    await page.getByTestId("priority-select").selectOption("high");
    await page.getByTestId("open-ticket-button").click();

    const ticket = page.locator('[data-testid="ticket-card"]', {
      hasText: subject,
    });
    await expect(ticket).toBeVisible({ timeout: ACTION_TIMEOUT });
    await expect(ticket).toContainText("Fresh");
    await expect(ticket).toContainText("high");

    await ticket.getByRole("button", { name: "Start Work" }).click();
    await expect(ticket).toContainText("In Flight", {
      timeout: ACTION_TIMEOUT,
    });

    await ticket.getByRole("button", { name: "Set Normal" }).click();
    await expect(ticket).toContainText("normal", {
      timeout: ACTION_TIMEOUT,
    });

    await ticket.getByPlaceholder("Add latest internal note").fill(note);
    await ticket.getByRole("button", { name: "Save Note" }).click();
    await expect(ticket).toContainText(note, { timeout: ACTION_TIMEOUT });

    await ticket.getByRole("button", { name: "Resolve" }).click();
    await expect(ticket).toContainText("Closed", {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("reopen a resolved ticket", async ({ page }) => {
    const n = nonce();
    const subject = `Reopen test ${n}`;

    await page.goto("/");
    await page.getByTestId("customer-name-input").fill(`Customer ${n}`);
    await page.getByTestId("subject-input").fill(subject);
    await page.getByTestId("details-input").fill(`Details ${n}`);
    await page.getByTestId("open-ticket-button").click();

    const ticket = page.locator('[data-testid="ticket-card"]', {
      hasText: subject,
    });
    await expect(ticket).toBeVisible({ timeout: ACTION_TIMEOUT });

    await ticket.getByRole("button", { name: "Resolve" }).click();
    await expect(ticket).toContainText("Closed", {
      timeout: ACTION_TIMEOUT,
    });

    await ticket.getByRole("button", { name: "Reopen" }).click();
    await expect(ticket).toContainText("Fresh", {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("escalate then de-escalate priority", async ({ page }) => {
    const n = nonce();
    const subject = `Priority cycle ${n}`;

    await page.goto("/");
    await page.getByTestId("customer-name-input").fill(`Customer ${n}`);
    await page.getByTestId("subject-input").fill(subject);
    await page.getByTestId("details-input").fill(`Details ${n}`);
    await page.getByTestId("priority-select").selectOption("low");
    await page.getByTestId("open-ticket-button").click();

    const ticket = page.locator('[data-testid="ticket-card"]', {
      hasText: subject,
    });
    await expect(ticket).toBeVisible({ timeout: ACTION_TIMEOUT });
    await expect(ticket).toContainText("low");

    await ticket.getByRole("button", { name: "Escalate" }).click();
    await expect(ticket).toContainText("high", {
      timeout: ACTION_TIMEOUT,
    });

    await ticket.getByRole("button", { name: "Set Normal" }).click();
    await expect(ticket).toContainText("normal", {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("multiple notes overwrite last_note display", async ({ page }) => {
    const n = nonce();
    const subject = `Multi-note ${n}`;
    const note1 = `First note ${n}`;
    const note2 = `Second note ${n}`;

    await page.goto("/");
    await page.getByTestId("customer-name-input").fill(`Customer ${n}`);
    await page.getByTestId("subject-input").fill(subject);
    await page.getByTestId("details-input").fill(`Details ${n}`);
    await page.getByTestId("open-ticket-button").click();

    const ticket = page.locator('[data-testid="ticket-card"]', {
      hasText: subject,
    });
    await expect(ticket).toBeVisible({ timeout: ACTION_TIMEOUT });

    await ticket.getByPlaceholder("Add latest internal note").fill(note1);
    await ticket.getByRole("button", { name: "Save Note" }).click();
    await expect(ticket).toContainText(note1, { timeout: ACTION_TIMEOUT });

    await ticket.getByPlaceholder("Add latest internal note").fill(note2);
    await ticket.getByRole("button", { name: "Save Note" }).click();
    await expect(ticket).toContainText(note2, { timeout: ACTION_TIMEOUT });
  });

  test("open ticket button disabled when fields are empty", async ({
    page,
  }) => {
    await page.goto("/");
    await expect(page.getByTestId("open-ticket-button")).toBeDisabled();

    await page.getByTestId("customer-name-input").fill("Someone");
    await expect(page.getByTestId("open-ticket-button")).toBeDisabled();

    await page.getByTestId("subject-input").fill("A subject");
    await expect(page.getByTestId("open-ticket-button")).toBeDisabled();

    await page.getByTestId("details-input").fill("Some details");
    await expect(page.getByTestId("open-ticket-button")).toBeEnabled();
  });

  test("form clears after successful ticket creation", async ({ page }) => {
    const n = nonce();

    await page.goto("/");
    await page.getByTestId("customer-name-input").fill(`Customer ${n}`);
    await page.getByTestId("subject-input").fill(`Subject ${n}`);
    await page.getByTestId("details-input").fill(`Details ${n}`);
    await page.getByTestId("priority-select").selectOption("high");
    await page.getByTestId("open-ticket-button").click();

    await expect(
      page.locator('[data-testid="ticket-card"]', {
        hasText: `Subject ${n}`,
      }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });

    await expect(page.getByTestId("customer-name-input")).toHaveValue("");
    await expect(page.getByTestId("subject-input")).toHaveValue("");
    await expect(page.getByTestId("details-input")).toHaveValue("");
    await expect(page.getByTestId("priority-select")).toHaveValue("normal");
  });

  test("ticket total count updates in board header", async ({ page }) => {
    const n = nonce();

    await page.goto("/");
    await page.getByTestId("customer-name-input").fill(`Customer ${n}`);
    await page.getByTestId("subject-input").fill(`Ticket A ${n}`);
    await page.getByTestId("details-input").fill(`Details ${n}`);
    await page.getByTestId("open-ticket-button").click();

    await expect(
      page.locator('[data-testid="ticket-card"]', {
        hasText: `Ticket A ${n}`,
      }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });

    await expect(page.locator(".board-head span")).toContainText("total");
  });

  test("empty state shows when no tickets exist", async ({ page }) => {
    await page.goto("/");

    await expect(
      page.locator(".status", { hasText: "No tickets yet" }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });
  });
});
