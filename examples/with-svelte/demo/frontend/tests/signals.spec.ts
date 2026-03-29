import { test, expect, API_URL } from "./fixtures";
import { randomUUID } from "crypto";

test.describe("signal ingestion endpoints", () => {
  test("signal/view accepts page view and returns session", async ({
    request,
  }) => {
    const res = await request.post(`${API_URL}/_api/signal/view`, {
      headers: { "Content-Type": "application/json" },
      data: { url: "/test-page", referrer: "https://google.com", title: "Test" },
    });

    expect(res.ok(), `signal/view returned ${res.status()} from ${API_URL}`).toBeTruthy();
    const body = await res.json();
    expect(body.ok).toBe(true);
    expect(body.session_id).toBeTruthy();
    // Session ID should be a valid UUID
    expect(body.session_id).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/,
    );
  });

  test("session persists across requests", async ({ request }) => {
    // First view: get a session
    const first = await request.post(`${API_URL}/_api/signal/view`, {
      headers: { "Content-Type": "application/json" },
      data: { url: "/page-1" },
    });
    const firstBody = await first.json();
    const sessionId = firstBody.session_id;

    // Second view: reuse the session
    const second = await request.post(`${API_URL}/_api/signal/view`, {
      headers: {
        "Content-Type": "application/json",
        "x-session-id": sessionId,
      },
      data: { url: "/page-2" },
    });
    const secondBody = await second.json();

    expect(secondBody.ok).toBe(true);
    expect(secondBody.session_id).toBe(sessionId);
  });

  test("signal/event accepts event batch", async ({ request }) => {
    const res = await request.post(`${API_URL}/_api/signal/event`, {
      headers: { "Content-Type": "application/json" },
      data: {
        events: [
          { event: "click", properties: { button: "signup" } },
          { event: "scroll", properties: { depth: 50 } },
          { event: "form_submit", properties: { form: "contact" } },
        ],
        context: { page_url: "/landing" },
      },
    });

    expect(res.ok()).toBeTruthy();
    const body = await res.json();
    expect(body.ok).toBe(true);
    expect(body.session_id).toBeTruthy();
  });

  test("signal/event rejects oversized batch", async ({ request }) => {
    const events = Array.from({ length: 51 }, (_, i) => ({
      event: `event_${i}`,
    }));

    const res = await request.post(`${API_URL}/_api/signal/event`, {
      headers: { "Content-Type": "application/json" },
      data: { events },
    });

    const body = await res.json();
    expect(body.ok).toBe(false);
  });

  test("signal/user accepts valid UUID", async ({ request }) => {
    const res = await request.post(`${API_URL}/_api/signal/user`, {
      headers: { "Content-Type": "application/json" },
      data: {
        user_id: randomUUID(),
        traits: { plan: "pro", company: "Acme" },
      },
    });

    expect(res.ok()).toBeTruthy();
    const body = await res.json();
    expect(body.ok).toBe(true);
  });

  test("signal/user rejects invalid UUID", async ({ request }) => {
    const res = await request.post(`${API_URL}/_api/signal/user`, {
      headers: { "Content-Type": "application/json" },
      data: { user_id: "not-a-uuid", traits: {} },
    });

    const body = await res.json();
    expect(body.ok).toBe(false);
  });

  test("signal/report accepts error reports", async ({ request }) => {
    const res = await request.post(`${API_URL}/_api/signal/report`, {
      headers: { "Content-Type": "application/json" },
      data: {
        errors: [
          {
            message: "TypeError: Cannot read property 'x' of null",
            stack: "at render (app.js:42)\nat mount (framework.js:100)",
            context: { component: "UserList" },
            page_url: "/users",
          },
        ],
      },
    });

    expect(res.ok()).toBeTruthy();
    const body = await res.json();
    expect(body.ok).toBe(true);
  });

  test("x-forge-platform header accepted", async ({ request }) => {
    const res = await request.post(`${API_URL}/_api/signal/view`, {
      headers: {
        "Content-Type": "application/json",
        "x-forge-platform": "desktop-macos",
      },
      data: { url: "/desktop-test" },
    });

    expect(res.ok()).toBeTruthy();
    const body = await res.json();
    expect(body.ok).toBe(true);
    expect(body.session_id).toBeTruthy();
  });
});
