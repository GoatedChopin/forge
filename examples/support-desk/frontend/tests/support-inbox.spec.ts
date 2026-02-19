import { expect, test } from "@playwright/test";

test.describe("Support Inbox", () => {
  test("agent can create and progress a ticket", async ({ page }) => {
    const nonce = Date.now();
    const customer = `E2E Customer ${nonce}`;
    const subject = `Invoice sync fails ${nonce}`;
    const details = `Reproduced on dashboard session ${nonce}`;
    const note = `Escalated by UI flow ${nonce}`;

    await page.goto("/");

    await page.getByTestId("customer-name-input").fill(customer);
    await page.getByTestId("subject-input").fill(subject);
    await page.getByTestId("details-input").fill(details);
    await page.getByTestId("priority-select").selectOption("high");
    await page.getByTestId("open-ticket-button").click();

    const ticket = page.locator('[data-testid="ticket-card"]', {
      hasText: subject,
    });
    await expect(ticket).toBeVisible();
    await expect(ticket).toContainText("Fresh");
    await expect(ticket).toContainText("high");

    await ticket.getByRole("button", { name: "Start Work" }).click();
    await expect(ticket).toContainText("In Flight");

    await ticket.getByRole("button", { name: "Set Normal" }).click();
    await expect(ticket).toContainText("normal");

    await ticket.getByPlaceholder("Add latest internal note").fill(note);
    await ticket.getByRole("button", { name: "Save Note" }).click();
    await expect(ticket).toContainText(note);

    await ticket.getByRole("button", { name: "Resolve" }).click();
    await expect(ticket).toContainText("Closed");
  });
});
