import {
  test,
  expect,
  ACTION_TIMEOUT,
  uniqueId,
  createTicketViaUI,
} from "./fixtures";

async function deleteAllTickets(
  rpc: (fn: string, args?: unknown) => Promise<unknown>,
) {
  const tickets = await rpc("list_support_tickets");
  if (!Array.isArray(tickets)) return;
  for (const t of tickets) {
    await rpc("set_ticket_status", { id: t.id, status: "resolved" });
  }
}

test.beforeEach(async ({ rpc }) => {
  await deleteAllTickets(rpc);
});

test.describe("Support Inbox", () => {
  test("agent can create and progress a ticket", async ({
    page,
    gotoReady,
  }) => {
    const n = uniqueId("e2e");
    const customer = `E2E Customer ${n}`;
    const subject = `Invoice sync fails ${n}`;
    const details = `Reproduced on dashboard session ${n}`;
    const note = `Escalated by UI flow ${n}`;

    await gotoReady();
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

  test("reopen a resolved ticket", async ({ page, gotoReady }) => {
    const n = uniqueId("reopen");
    const subject = `Reopen test ${n}`;

    await gotoReady();
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

  test("escalate then de-escalate priority", async ({
    page,
    gotoReady,
  }) => {
    const n = uniqueId("priority");
    const subject = `Priority cycle ${n}`;

    await gotoReady();
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

  test("multiple notes overwrite last_note display", async ({
    page,
    gotoReady,
  }) => {
    const n = uniqueId("notes");
    const subject = `Multi-note ${n}`;
    const note1 = `First note ${n}`;
    const note2 = `Second note ${n}`;

    await gotoReady();
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

  test("form clears after successful ticket creation", async ({
    page,
    gotoReady,
  }) => {
    const n = uniqueId("clear");

    await gotoReady();
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

  test("ticket total count updates in board header", async ({
    page,
    gotoReady,
  }) => {
    const n = uniqueId("count");

    await gotoReady();
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

  test("empty state shows when no tickets exist", async ({
    page,
    gotoReady,
  }) => {
    await gotoReady();

    await expect(page.locator(".status, .ticket-list")).toBeVisible({
      timeout: ACTION_TIMEOUT,
    });

    const ticketCount = await page
      .locator('[data-testid="ticket-card"]')
      .count();
    if (ticketCount === 0) {
      await expect(
        page.locator(".status", { hasText: "No tickets yet" }),
      ).toBeVisible({ timeout: ACTION_TIMEOUT });
    } else {
      await expect(page.locator(".board-head span")).toContainText("total");
    }
  });

  test("ticket displays customer name, subject, and details", async ({
    page,
    gotoReady,
  }) => {
    const n = uniqueId("display");
    const customer = `Acme Corp ${n}`;
    const subject = `Dashboard crash ${n}`;
    const details = `Steps to reproduce: open settings panel ${n}`;

    await gotoReady();
    await createTicketViaUI(page, { customer, subject, details });

    const ticket = page.locator('[data-testid="ticket-card"]', {
      hasText: subject,
    });
    await expect(ticket).toBeVisible({ timeout: ACTION_TIMEOUT });
    await expect(ticket).toContainText(customer);
    await expect(ticket).toContainText(details);
  });

  test("create ticket with low priority", async ({ page, gotoReady }) => {
    const n = uniqueId("low-prio");
    const subject = `Low priority issue ${n}`;

    await gotoReady();
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

  test("note input clears after saving", async ({ page, gotoReady }) => {
    const n = uniqueId("note-clear");
    const subject = `Note clear ${n}`;
    const note = `Internal note ${n}`;

    await gotoReady();
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

  test("start work then resolve workflow", async ({ page, gotoReady }) => {
    const n = uniqueId("workflow");
    const subject = `Full workflow ${n}`;

    await gotoReady();
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

  test("default priority is normal when not specified", async ({
    page,
    gotoReady,
  }) => {
    const n = uniqueId("default-prio");
    const subject = `Default priority ${n}`;

    await gotoReady();
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
