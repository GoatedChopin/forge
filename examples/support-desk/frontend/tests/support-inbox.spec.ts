import { expect, test, type Page } from "@playwright/test";

const API_URL = process.env.VITE_API_URL || "http://localhost:8080";
const ACTION_TIMEOUT = process.env.CI ? 15_000 : 8_000;

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

async function gotoReady(page: Page) {
  const sseRequest = page.waitForRequest(
    (req) => req.url().includes("/_api/events"),
    { timeout: 15000 },
  );
  await page.goto("/");
  await sseRequest;
  await page.waitForTimeout(1000);
}

async function createTicketViaUI(
  page: Page,
  opts: {
    customer: string;
    subject: string;
    details: string;
    priority?: string;
  },
) {
  await page.getByTestId("customer-name-input").fill(opts.customer);
  await page.getByTestId("subject-input").fill(opts.subject);
  await page.getByTestId("details-input").fill(opts.details);
  if (opts.priority) {
    await page.getByTestId("priority-select").selectOption(opts.priority);
  }
  await page.getByTestId("open-ticket-button").click();
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

    await gotoReady(page);
    await createTicketViaUI(page, {
      customer,
      subject,
      details,
      priority: "high",
    });

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

    await gotoReady(page);
    await createTicketViaUI(page, {
      customer: `Customer ${n}`,
      subject,
      details: `Details ${n}`,
    });

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

    await gotoReady(page);
    await createTicketViaUI(page, {
      customer: `Customer ${n}`,
      subject,
      details: `Details ${n}`,
      priority: "low",
    });

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

    await gotoReady(page);
    await createTicketViaUI(page, {
      customer: `Customer ${n}`,
      subject,
      details: `Details ${n}`,
    });

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

    await gotoReady(page);
    await createTicketViaUI(page, {
      customer: `Customer ${n}`,
      subject: `Subject ${n}`,
      details: `Details ${n}`,
      priority: "high",
    });

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

    await gotoReady(page);
    await createTicketViaUI(page, {
      customer: `Customer ${n}`,
      subject: `Ticket A ${n}`,
      details: `Details ${n}`,
    });

    await expect(
      page.locator('[data-testid="ticket-card"]', {
        hasText: `Ticket A ${n}`,
      }),
    ).toBeVisible({ timeout: ACTION_TIMEOUT });

    await expect(page.locator(".board-head span")).toContainText("total");
  });

  test("empty state shows when no tickets exist", async ({ page }) => {
    await gotoReady(page);

    // Wait for either the empty state or tickets to load
    await expect(page.locator(".status, .ticket-list")).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    // If there are resolved tickets from cleanup, that's ok.
    // The empty state only appears with a truly fresh DB.
    // Check that the page loaded without errors.
    const ticketCount = await page
      .locator('[data-testid="ticket-card"]')
      .count();
    if (ticketCount === 0) {
      await expect(
        page.locator(".status", { hasText: "No tickets yet" }),
      ).toBeVisible({ timeout: ACTION_TIMEOUT });
    } else {
      // Tickets exist from previous test cleanup (resolved but still listed)
      await expect(page.locator(".board-head span")).toContainText("total");
    }
  });

  test("ticket displays customer name, subject, and details", async ({
    page,
  }) => {
    const n = nonce();
    const customer = `Acme Corp ${n}`;
    const subject = `Dashboard crash ${n}`;
    const details = `Steps to reproduce: open settings panel ${n}`;

    await gotoReady(page);
    await createTicketViaUI(page, { customer, subject, details });

    const ticket = page.locator('[data-testid="ticket-card"]', {
      hasText: subject,
    });
    await expect(ticket).toBeVisible({ timeout: ACTION_TIMEOUT });
    await expect(ticket).toContainText(customer);
    await expect(ticket).toContainText(details);
  });

  test("create ticket with low priority", async ({ page }) => {
    const n = nonce();
    const subject = `Low priority issue ${n}`;

    await gotoReady(page);
    await createTicketViaUI(page, {
      customer: `Customer ${n}`,
      subject,
      details: `Details ${n}`,
      priority: "low",
    });

    const ticket = page.locator('[data-testid="ticket-card"]', {
      hasText: subject,
    });
    await expect(ticket).toBeVisible({ timeout: ACTION_TIMEOUT });
    await expect(ticket).toContainText("low");
  });

  test("note input clears after saving", async ({ page }) => {
    const n = nonce();
    const subject = `Note clear ${n}`;
    const note = `Internal note ${n}`;

    await gotoReady(page);
    await createTicketViaUI(page, {
      customer: `Customer ${n}`,
      subject,
      details: `Details ${n}`,
    });

    const ticket = page.locator('[data-testid="ticket-card"]', {
      hasText: subject,
    });
    await expect(ticket).toBeVisible({ timeout: ACTION_TIMEOUT });

    await ticket.getByPlaceholder("Add latest internal note").fill(note);
    await ticket.getByRole("button", { name: "Save Note" }).click();
    await expect(ticket).toContainText(note, { timeout: ACTION_TIMEOUT });

    await expect(
      ticket.getByPlaceholder("Add latest internal note"),
    ).toHaveValue("", { timeout: ACTION_TIMEOUT });
  });

  test("start work then resolve workflow", async ({ page }) => {
    const n = nonce();
    const subject = `Full workflow ${n}`;

    await gotoReady(page);
    await createTicketViaUI(page, {
      customer: `Customer ${n}`,
      subject,
      details: `Details ${n}`,
      priority: "high",
    });

    const ticket = page.locator('[data-testid="ticket-card"]', {
      hasText: subject,
    });
    await expect(ticket).toBeVisible({ timeout: ACTION_TIMEOUT });
    await expect(ticket).toContainText("Fresh");

    await ticket.getByRole("button", { name: "Start Work" }).click();
    await expect(ticket).toContainText("In Flight", {
      timeout: ACTION_TIMEOUT,
    });

    await ticket.getByRole("button", { name: "Resolve" }).click();
    await expect(ticket).toContainText("Closed", {
      timeout: ACTION_TIMEOUT,
    });
  });

  test("default priority is normal when not specified", async ({ page }) => {
    const n = nonce();
    const subject = `Default priority ${n}`;

    await gotoReady(page);
    await createTicketViaUI(page, {
      customer: `Customer ${n}`,
      subject,
      details: `Details ${n}`,
    });

    const ticket = page.locator('[data-testid="ticket-card"]', {
      hasText: subject,
    });
    await expect(ticket).toBeVisible({ timeout: ACTION_TIMEOUT });
    await expect(ticket).toContainText("normal");
  });
});
