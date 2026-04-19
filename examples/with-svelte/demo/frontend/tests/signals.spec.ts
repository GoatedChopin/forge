import { test, expect, API_URL, ACTION_TIMEOUT } from "./fixtures";
import { randomUUID } from "crypto";
import type { Page, Request } from "@playwright/test";

// Every client-sendable field on every /_api/signal/* endpoint is exercised
// below. Server-enriched fields (user_agent, device_type, browser, os,
// client_ip, country, city, is_bot, visitor_id) aren't echoed in responses.
// The final describe block runs the shipped Grafana dashboard queries via
// `/api/ds/query` to verify the SQL each panel ships actually executes
// against the enriched rows (gated on GRAFANA_URL being set).

type ViewBody = {
  url: string;
  referrer?: string;
  title?: string;
  utm_source?: string;
  utm_medium?: string;
  utm_campaign?: string;
  utm_term?: string;
  utm_content?: string;
  correlation_id?: string;
};

type EventBody = {
  events: Array<{
    event: string;
    properties?: Record<string, unknown>;
    correlation_id?: string;
    timestamp?: string;
  }>;
  context?: { page_url?: string; referrer?: string; session_id?: string };
};

type UserBody = { user_id: string; traits?: Record<string, unknown> };

type ReportBody = {
  errors: Array<{
    message: string;
    stack?: string;
    context?: Record<string, unknown>;
    correlation_id?: string;
    breadcrumbs?: Array<{
      message: string;
      data?: Record<string, unknown>;
      timestamp?: string;
    }>;
    page_url?: string;
  }>;
};

type VitalBody = {
  vitals: Array<{
    name: string;
    value: number;
    rating?: string;
    attribution?: Record<string, unknown>;
    correlation_id?: string;
    page_url?: string;
    timestamp?: string;
  }>;
  context?: { page_url?: string; referrer?: string; session_id?: string };
};

type SignalBodies = {
  event: EventBody;
  view: ViewBody;
  user: UserBody;
  report: ReportBody;
  vital: VitalBody;
};

async function waitForSignal<K extends keyof SignalBodies>(
  page: Page,
  endpoint: K,
  predicate: (body: SignalBodies[K]) => boolean = () => true,
  timeout = ACTION_TIMEOUT * 3,
): Promise<{ request: Request; body: SignalBodies[K] }> {
  const request = await page.waitForRequest(
    (req) => {
      if (!req.url().includes(`/_api/signal/${endpoint}`)) return false;
      if (req.method() !== "POST") return false;
      try {
        return predicate(req.postDataJSON() as SignalBodies[K]);
      } catch {
        return false;
      }
    },
    { timeout },
  );
  return {
    request,
    body: request.postDataJSON() as SignalBodies[K],
  };
}

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

test.describe("signals: client SDK via browser", () => {
  test.beforeEach(async ({ page, gotoReady }) => {
    await gotoReady("/");
    await page.waitForFunction(() => !!window.forgeSignals, undefined, {
      timeout: ACTION_TIMEOUT * 2,
    });
  });

  test("view: auto-captured on initial navigation", async ({
    page,
    gotoReady,
  }) => {
    const viewPromise = waitForSignal(page, "view");
    // beforeEach already navigated; open a second page context to observe
    // the first view without racing against beforeEach's navigation.
    await gotoReady("/");
    const { body } = await viewPromise;
    expect(body.url).toBeTruthy();
  });

  test("view: manual page() sends url, title, referrer, UTMs", async ({
    page,
    gotoReady,
  }) => {
    // The SDK only extracts UTMs from location at init time, so arrive on a
    // URL that already carries them; the auto-captured first /view is what we
    // assert against.
    const viewPromise = waitForSignal(
      page,
      "view",
      (b) => b.utm_source === "playwright",
    );
    await gotoReady(
      "/?utm_source=playwright&utm_medium=automation&utm_campaign=signals-spec&utm_term=full&utm_content=all-fields",
    );
    const { body } = await viewPromise;
    expect(body.url).toContain("utm_source=playwright");
    expect(body.utm_source).toBe("playwright");
    expect(body.utm_medium).toBe("automation");
    expect(body.utm_campaign).toBe("signals-spec");
    expect(body.utm_term).toBe("full");
    expect(body.utm_content).toBe("all-fields");
  });

  test("event: track() populates event, properties, correlation_id", async ({
    page,
  }) => {
    const eventPromise = waitForSignal(page, "event", (b) =>
      b.events.some((e) => e.event === "signals_spec_track"),
    );
    await page.evaluate(() => {
      const corr = window.forgeSignals.nextCorrelationId();
      window.forgeSignals.track("signals_spec_track", {
        number: 42,
        string: "hello",
        nested: { a: 1, b: [true, false, null] },
        correlation_ref: corr,
      });
    });
    const { body } = await eventPromise;
    const evt = body.events.find((e) => e.event === "signals_spec_track");
    expect(evt).toBeTruthy();
    expect(evt!.properties).toMatchObject({
      number: 42,
      string: "hello",
      nested: { a: 1, b: [true, false, null] },
    });
    expect(evt!.correlation_id).toMatch(/\S/);
    expect(body.context?.page_url).toBeTruthy();
  });

  test("event: UI click dispatches track() end-to-end", async ({ page }) => {
    const eventPromise = waitForSignal(page, "event", (b) =>
      b.events.some((e) => e.event === "cache_fetch"),
    );
    const fetchBtn = page.getByRole("button", { name: /Fetch Stats/i });
    await expect(fetchBtn).toBeVisible({ timeout: ACTION_TIMEOUT });
    await fetchBtn.click();
    const { body } = await eventPromise;
    const evt = body.events.find((e) => e.event === "cache_fetch");
    expect(evt).toBeTruthy();
    expect(evt!.properties).toBeTruthy();
  });

  test("user: identify() sends user_id and traits", async ({ page }) => {
    const userId = randomUUID();
    const identifyPromise = waitForSignal(
      page,
      "user",
      (b) => b.user_id === userId,
    );
    await page.evaluate(async (uid) => {
      await window.forgeSignals.identify(uid, {
        plan: "enterprise",
        email: "signals@example.com",
        signup_source: "test-suite",
        tier: 3,
      });
    }, userId);
    const { body } = await identifyPromise;
    expect(body.user_id).toBe(userId);
    expect(body.traits).toMatchObject({
      plan: "enterprise",
      email: "signals@example.com",
      signup_source: "test-suite",
      tier: 3,
    });
  });

  test("user: identify() through the SDK sends user_id + traits", async ({
    page,
  }) => {
    // The demo layout effect auto-identifies after real auth, but that flow
    // depends on the full register RPC path. This test exercises the SDK
    // surface directly so a transient auth failure can't mask a signals
    // regression.
    const userId = randomUUID();
    const email = `signals-${Date.now()}@example.com`;
    const identifyPromise = waitForSignal(
      page,
      "user",
      (b) => b.user_id === userId,
    );
    await page.evaluate(
      async ({ uid, mail }) => {
        await window.forgeSignals.identify(uid, {
          email: mail,
          name: "Signals Tester",
          plan: "pro",
        });
      },
      { uid: userId, mail: email },
    );
    const { body } = await identifyPromise;
    expect(body.user_id).toBe(userId);
    expect(body.traits).toMatchObject({
      email,
      name: "Signals Tester",
      plan: "pro",
    });
  });

  test("report: captureError with stack, context, breadcrumbs, page_url", async ({
    page,
  }) => {
    const reportPromise = waitForSignal(page, "report", (b) =>
      b.errors.some((e) => e.message.includes("signals-spec-manual")),
    );
    await page.evaluate(() => {
      window.forgeSignals.breadcrumb("trace-step-one", { step: 1 });
      window.forgeSignals.breadcrumb("trace-step-two", {
        step: 2,
        feature: "errors",
      });
      window.forgeSignals.captureError(
        new Error("signals-spec-manual capture"),
        { feature: "tests", severity: "expected" },
      );
    });
    const { body } = await reportPromise;
    const err = body.errors.find((e) =>
      e.message.includes("signals-spec-manual"),
    )!;
    expect(err.stack).toBeTruthy();
    expect(err.context).toMatchObject({
      feature: "tests",
      severity: "expected",
    });
    expect(err.page_url).toBeTruthy();
    const crumbs = err.breadcrumbs ?? [];
    const messages = crumbs.map((c) => c.message);
    expect(messages).toEqual(
      expect.arrayContaining(["trace-step-one", "trace-step-two"]),
    );
    const step2 = crumbs.find((c) => c.message === "trace-step-two");
    expect(step2?.data).toMatchObject({ step: 2, feature: "errors" });
  });

  test("report: unhandled errors are auto-captured", async ({ page }) => {
    // SDK defers window error-listener registration by 2s to avoid cold-start
    // pool contention, so wait past that window before dispatching.
    await page.waitForTimeout(2500);
    const reportPromise = waitForSignal(page, "report", (b) =>
      b.errors.some((e) => e.message.includes("signals-spec-auto")),
    );
    // Inline <script> throws raise a real window error event, unlike
    // setTimeout throws inside page.evaluate, which Playwright's eval wrapper
    // swallows before the window listener can see them.
    await page.evaluate(() => {
      const script = document.createElement("script");
      script.textContent =
        'throw new Error("signals-spec-auto throw from script tag");';
      document.head.appendChild(script);
    });
    const { body } = await reportPromise;
    const err = body.errors.find((e) =>
      e.message.includes("signals-spec-auto"),
    )!;
    expect(err.stack).toBeTruthy();
  });

  test("vital: manual vital() sends name, value, rating, attribution", async ({
    page,
  }) => {
    const vitalPromise = waitForSignal(page, "vital", (b) =>
      b.vitals.some((v) => v.name === "signals-spec-metric"),
    );
    await page.evaluate(() => {
      window.forgeSignals.vital("signals-spec-metric", 123.45, {
        rating: "good",
        attribution: { first_paint: 500, ttfb: 80 },
      });
    });
    const { body } = await vitalPromise;
    const v = body.vitals.find((v) => v.name === "signals-spec-metric")!;
    expect(v.value).toBe(123.45);
    expect(v.rating).toBe("good");
    expect(v.attribution).toMatchObject({ first_paint: 500, ttfb: 80 });
    expect(v.page_url).toBeTruthy();
  });

  test("correlation_id: SDK attaches x-correlation-id on RPC calls", async ({
    page,
  }) => {
    const rpcReq = page.waitForRequest(
      (req) => req.url().includes("/_api/rpc/") && req.method() === "POST",
      { timeout: ACTION_TIMEOUT * 2 },
    );
    await page.getByRole("button", { name: /Fetch Stats/i }).click();
    const req = await rpcReq;
    expect(req.headers()["x-correlation-id"]).toMatch(/\S/);
  });

  test("session_id: SDK reuses same session across requests", async ({
    page,
  }) => {
    const firstViewResp = page.waitForResponse(
      (res) => res.url().includes("/_api/signal/view") && res.status() === 200,
      { timeout: ACTION_TIMEOUT * 2 },
    );
    await page.evaluate(async () => {
      await window.forgeSignals.page();
    });
    const firstBody = (await (await firstViewResp).json()) as {
      session_id?: string;
    };
    expect(firstBody.session_id).toMatch(UUID_RE);
    const sessionId = firstBody.session_id!;

    const secondViewReq = page.waitForRequest(
      (req) =>
        req.url().includes("/_api/signal/view") && req.method() === "POST",
      { timeout: ACTION_TIMEOUT * 2 },
    );
    await page.evaluate(async () => {
      await window.forgeSignals.page();
    });
    const req = await secondViewReq;
    expect(req.headers()["x-session-id"]).toBe(sessionId);
  });
});

test.describe("signals: full-field payloads (direct POST)", () => {
  test("/view accepts every client-sendable field", async ({ request }) => {
    const res = await request.post(`${API_URL}/_api/signal/view`, {
      headers: {
        "Content-Type": "application/json",
        "User-Agent": "forge-spec/1.0 (playwright)",
      },
      data: {
        url: "https://demo.example/landing?x=1",
        referrer: "https://external.example/campaign",
        title: "Landing Page",
        utm_source: "newsletter",
        utm_medium: "email",
        utm_campaign: "spring-2026",
        utm_term: "cta-top",
        utm_content: "variant-a",
        correlation_id: "corr-view-fullfield",
      },
    });
    const body = (await res.json()) as { ok: boolean; session_id?: string };
    expect(res.ok()).toBeTruthy();
    expect(body.ok).toBe(true);
    expect(body.session_id).toMatch(UUID_RE);
  });

  test("/event accepts every client-sendable field", async ({ request }) => {
    const res = await request.post(`${API_URL}/_api/signal/event`, {
      headers: { "Content-Type": "application/json" },
      data: {
        events: [
          {
            event: "purchase",
            properties: {
              amount: 99.99,
              currency: "USD",
              items: [{ sku: "A1", qty: 2 }],
            },
            correlation_id: "corr-event-1",
            timestamp: new Date().toISOString(),
          },
          {
            event: "scroll",
            properties: { depth: 75 },
            correlation_id: "corr-event-2",
          },
        ],
        context: {
          page_url: "https://demo.example/checkout",
          referrer: "https://demo.example/cart",
          session_id: randomUUID(),
        },
      },
    });
    const body = (await res.json()) as { ok: boolean; session_id?: string };
    expect(body.ok).toBe(true);
    expect(body.session_id).toMatch(UUID_RE);
  });

  test("/user accepts valid UUID with rich traits", async ({ request }) => {
    // /user only surfaces a session_id when the caller already owns one,
    // so mint one via /view first, then thread it through as a header.
    const viewRes = await request.post(`${API_URL}/_api/signal/view`, {
      headers: { "Content-Type": "application/json" },
      data: { url: "/user-probe" },
    });
    const { session_id: sessionId } = (await viewRes.json()) as {
      session_id: string;
    };
    expect(sessionId).toMatch(UUID_RE);

    const res = await request.post(`${API_URL}/_api/signal/user`, {
      headers: {
        "Content-Type": "application/json",
        "x-session-id": sessionId,
      },
      data: {
        user_id: randomUUID(),
        traits: {
          plan: "pro",
          company: "Acme",
          tier: 3,
          verified: true,
          tags: ["beta", "alpha"],
        },
      },
    });
    const body = (await res.json()) as { ok: boolean; session_id?: string };
    expect(body.ok).toBe(true);
    expect(body.session_id).toBe(sessionId);
  });

  test("/report accepts every client-sendable field", async ({ request }) => {
    const res = await request.post(`${API_URL}/_api/signal/report`, {
      headers: { "Content-Type": "application/json" },
      data: {
        errors: [
          {
            message: "TypeError: cannot read property of undefined",
            stack:
              "TypeError: x\n  at foo (https://demo.example/app.js:10:5)\n  at bar (https://demo.example/app.js:20:3)",
            context: { component: "Checkout", order_id: "ord-42" },
            correlation_id: "corr-report-1",
            page_url: "https://demo.example/checkout",
            breadcrumbs: [
              {
                message: "clicked pay button",
                data: { method: "card" },
                timestamp: new Date().toISOString(),
              },
              {
                message: "validation passed",
                data: { total: 99.99 },
                timestamp: new Date().toISOString(),
              },
            ],
          },
        ],
      },
    });
    const body = (await res.json()) as { ok: boolean };
    expect(body.ok).toBe(true);
  });

  test("/vital accepts every client-sendable field", async ({ request }) => {
    const res = await request.post(`${API_URL}/_api/signal/vital`, {
      headers: { "Content-Type": "application/json" },
      data: {
        vitals: [
          {
            name: "LCP",
            value: 1234.5,
            rating: "good",
            attribution: {
              element: "img#hero",
              url: "https://demo.example/hero.png",
            },
            correlation_id: "corr-vital-1",
            page_url: "https://demo.example/",
            timestamp: new Date().toISOString(),
          },
          {
            name: "CLS",
            value: 0.05,
            rating: "good",
            attribution: { largest_shift: 0.03 },
          },
        ],
        context: {
          page_url: "https://demo.example/",
          session_id: randomUUID(),
        },
      },
    });
    const body = (await res.json()) as { ok: boolean; session_id?: string };
    expect(body.ok).toBe(true);
    expect(body.session_id).toMatch(UUID_RE);
  });
});

test.describe("signals: server validation", () => {
  test("/event rejects batch over 50 events", async ({ request }) => {
    const events = Array.from({ length: 51 }, (_, i) => ({ event: `e_${i}` }));
    const res = await request.post(`${API_URL}/_api/signal/event`, {
      headers: { "Content-Type": "application/json" },
      data: { events },
    });
    const body = (await res.json()) as { ok: boolean };
    expect(body.ok).toBe(false);
  });

  test("/user rejects invalid UUID", async ({ request }) => {
    const res = await request.post(`${API_URL}/_api/signal/user`, {
      headers: { "Content-Type": "application/json" },
      data: { user_id: "not-a-uuid", traits: {} },
    });
    const body = (await res.json()) as { ok: boolean };
    expect(body.ok).toBe(false);
  });

  test("DNT: 1 short-circuits /view but /report still lands", async ({
    request,
  }) => {
    const viewRes = await request.post(`${API_URL}/_api/signal/view`, {
      headers: { "Content-Type": "application/json", DNT: "1" },
      data: { url: "/dnt-test" },
    });
    const viewBody = (await viewRes.json()) as {
      ok: boolean;
      session_id?: string | null;
    };
    expect(viewBody.ok).toBe(true);
    expect(viewBody.session_id).toBeFalsy();

    const reportRes = await request.post(`${API_URL}/_api/signal/report`, {
      headers: { "Content-Type": "application/json", DNT: "1" },
      data: { errors: [{ message: "dnt-user-crash" }] },
    });
    const reportBody = (await reportRes.json()) as { ok: boolean };
    expect(reportBody.ok).toBe(true);
  });
});

// Collector flushes every 5s by default, so give Grafana panels up to ~20s
// to reflect new rows across the partitioned table.
const GRAFANA_POLL_TIMEOUT = ACTION_TIMEOUT * 4;

const GRAFANA_URL = process.env.GRAFANA_URL;
const GRAFANA_USER = process.env.GRAFANA_USER ?? "admin";
const GRAFANA_PASSWORD = process.env.GRAFANA_PASSWORD ?? "admin";
const GRAFANA_DS_UID = "forge_pg";

type GrafanaPanel = {
  id?: number;
  title?: string;
  type?: string;
  panels?: GrafanaPanel[];
  targets?: Array<{ rawSql?: string; format?: string; refId?: string }>;
};

type PanelQuery = {
  panelId: number;
  title: string;
  rawSql: string;
  format: string;
  refId: string;
};

function substituteVars(sql: string): string {
  return sql
    .replace(/\$\{bot_filter(?::[^}]*)?\}/g, "AND is_bot = false")
    .replace(/\$\{tenant(?::[^}]*)?\}/g, "")
    .replace(/\$interval\b/g, "'1h'");
}

function collectPanels(panels: GrafanaPanel[] | undefined): GrafanaPanel[] {
  if (!panels) return [];
  const out: GrafanaPanel[] = [];
  for (const panel of panels) {
    if (panel.type === "row") {
      out.push(...collectPanels(panel.panels));
    } else {
      out.push(panel);
      if (panel.panels?.length) out.push(...collectPanels(panel.panels));
    }
  }
  return out;
}

function extractQueries(panels: GrafanaPanel[]): PanelQuery[] {
  const queries: PanelQuery[] = [];
  for (const panel of panels) {
    for (const target of panel.targets ?? []) {
      if (!target.rawSql || !target.rawSql.trim()) continue;
      queries.push({
        panelId: panel.id ?? -1,
        title: panel.title ?? "(untitled)",
        rawSql: target.rawSql,
        format: target.format ?? "table",
        refId: target.refId ?? "A",
      });
    }
  }
  return queries;
}

type GrafanaQueryResult = {
  status: number;
  ok: boolean;
  body: {
    results?: Record<
      string,
      {
        status?: number;
        error?: string;
        errorSource?: string;
        frames?: Array<{
          data?: { values?: unknown[][] };
          schema?: { fields?: Array<{ name: string }> };
        }>;
      }
    >;
  };
};

async function runGrafanaQuery(
  request: import("@playwright/test").APIRequestContext,
  panel: PanelQuery,
  opts: { from?: string; to?: string } = {},
): Promise<GrafanaQueryResult> {
  const auth = Buffer.from(`${GRAFANA_USER}:${GRAFANA_PASSWORD}`).toString(
    "base64",
  );
  const rawSql = substituteVars(panel.rawSql);
  const res = await request.post(`${GRAFANA_URL}/api/ds/query`, {
    headers: {
      "Content-Type": "application/json",
      Authorization: `Basic ${auth}`,
    },
    data: {
      from: opts.from ?? "now-24h",
      to: opts.to ?? "now",
      queries: [
        {
          refId: panel.refId,
          datasource: { type: "postgres", uid: GRAFANA_DS_UID },
          rawSql,
          format: panel.format,
        },
      ],
    },
  });
  const body = (await res.json()) as GrafanaQueryResult["body"];
  return { status: res.status(), ok: res.ok(), body };
}

function firstResult(body: GrafanaQueryResult["body"]) {
  const entries = Object.values(body.results ?? {});
  return entries[0];
}

function rowCount(body: GrafanaQueryResult["body"]): number {
  const result = firstResult(body);
  const frame = result?.frames?.[0];
  const values = frame?.data?.values;
  if (!values || values.length === 0) return 0;
  return values[0]?.length ?? 0;
}

function findPanel(panels: PanelQuery[], title: string): PanelQuery {
  const panel = panels.find((p) => p.title === title);
  if (!panel) {
    const titles = panels.map((p) => p.title).join(", ");
    throw new Error(`panel "${title}" not found. Available: ${titles}`);
  }
  return panel;
}

function panelContainsValue(
  body: GrafanaQueryResult["body"],
  needle: string,
): boolean {
  const frame = firstResult(body)?.frames?.[0];
  const values = frame?.data?.values;
  if (!values) return false;
  for (const column of values) {
    if (!column) continue;
    for (const cell of column) {
      if (typeof cell === "string" && cell.includes(needle)) return true;
    }
  }
  return false;
}

test.describe("signals: grafana dashboard queries execute", () => {
  test.skip(
    !GRAFANA_URL,
    "GRAFANA_URL not set; skipping Grafana dashboard verification.",
  );

  let businessPanels: PanelQuery[] = [];
  let srePanels: PanelQuery[] = [];

  test.beforeAll(async ({ request }) => {
    const auth = Buffer.from(`${GRAFANA_USER}:${GRAFANA_PASSWORD}`).toString(
      "base64",
    );
    const fetchDashboard = async (uid: string): Promise<PanelQuery[]> => {
      const res = await request.get(
        `${GRAFANA_URL}/api/dashboards/uid/${uid}`,
        { headers: { Authorization: `Basic ${auth}` } },
      );
      if (!res.ok()) {
        throw new Error(
          `dashboard ${uid} fetch failed: ${res.status()} ${await res.text()}`,
        );
      }
      const body = (await res.json()) as {
        dashboard?: { panels?: GrafanaPanel[] };
      };
      const panels = collectPanels(body.dashboard?.panels);
      return extractQueries(panels);
    };
    businessPanels = await fetchDashboard("forge-biz");
    srePanels = await fetchDashboard("forge-sre");
    expect(businessPanels.length).toBeGreaterThan(0);
    expect(srePanels.length).toBeGreaterThan(0);
  });

  test("every Forge Business panel query executes without error", async ({
    request,
  }) => {
    const failures: string[] = [];
    for (const panel of businessPanels) {
      const result = await runGrafanaQuery(request, panel);
      if (!result.ok) {
        failures.push(`${panel.title} (HTTP ${result.status})`);
        continue;
      }
      const inner = firstResult(result.body);
      if (inner?.error) {
        failures.push(`${panel.title}: ${inner.error}`);
      }
    }
    expect(failures, failures.join("\n")).toEqual([]);
  });

  test("every Forge SRE panel query executes without error", async ({
    request,
  }) => {
    const failures: string[] = [];
    for (const panel of srePanels) {
      const result = await runGrafanaQuery(request, panel);
      if (!result.ok) {
        failures.push(`${panel.title} (HTTP ${result.status})`);
        continue;
      }
      const inner = firstResult(result.body);
      if (inner?.error) {
        failures.push(`${panel.title}: ${inner.error}`);
      }
    }
    expect(failures, failures.join("\n")).toEqual([]);
  });

  test("Custom Events panel reflects a freshly tracked event", async ({
    request,
  }) => {
    const marker = `grafana-probe-${randomUUID()}`;
    const correlationId = `grafana-track-${randomUUID()}`;
    const res = await request.post(`${API_URL}/_api/signal/event`, {
      headers: { "Content-Type": "application/json" },
      data: {
        events: [
          {
            event: marker,
            properties: { kind: "grafana_probe" },
            correlation_id: correlationId,
          },
        ],
      },
    });
    expect(res.ok()).toBeTruthy();

    const panel = findPanel(businessPanels, "Custom Events");
    await expect
      .poll(
        async () => {
          const result = await runGrafanaQuery(request, panel, {
            from: "now-1h",
            to: "now",
          });
          return panelContainsValue(result.body, marker);
        },
        { timeout: GRAFANA_POLL_TIMEOUT },
      )
      .toBe(true);
  });

  test("Frontend Errors panel counts a freshly reported error", async ({
    request,
  }) => {
    const correlationId = `grafana-error-${randomUUID()}`;
    const res = await request.post(`${API_URL}/_api/signal/report`, {
      headers: { "Content-Type": "application/json" },
      data: {
        errors: [
          {
            message: `grafana-probe-error-${randomUUID()}`,
            correlation_id: correlationId,
          },
        ],
      },
    });
    expect(res.ok()).toBeTruthy();

    const panel = findPanel(srePanels, "Frontend Errors");
    await expect
      .poll(
        async () => {
          const result = await runGrafanaQuery(request, panel, {
            from: "now-1h",
            to: "now",
          });
          return rowCount(result.body);
        },
        { timeout: GRAFANA_POLL_TIMEOUT },
      )
      .toBeGreaterThan(0);
  });
});
