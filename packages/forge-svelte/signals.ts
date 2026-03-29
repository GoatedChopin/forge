
import type { ForgeClient } from "./client.js";

export interface SignalsConfig {
  /** Enable signals collection (default: true) */
  enabled?: boolean;
  /** Auto-track page views on navigation (default: true) */
  autoPageViews?: boolean;
  /** Auto-capture frontend errors (default: true) */
  autoCaptureErrors?: boolean;
  /** Flush interval in ms (default: 5000) */
  flushInterval?: number;
  /** Max events per batch (default: 20) */
  maxBatchSize?: number;
}

interface SignalEvent {
  event: string;
  properties?: Record<string, unknown>;
  correlation_id?: string;
  timestamp?: string;
}

interface Breadcrumb {
  message: string;
  data?: Record<string, unknown>;
  timestamp: string;
}

const DEFAULT_FLUSH_INTERVAL = 5000;
const DEFAULT_MAX_BATCH = 20;
const MAX_BREADCRUMBS = 20;
const MAX_QUEUE_SIZE = 1000;

/** Generate a short unique ID for correlation (nanoid-style) */
function generateId(): string {
  const chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  let id = "";
  const bytes = crypto.getRandomValues(new Uint8Array(21));
  for (const byte of bytes) {
    id += chars[byte % chars.length];
  }
  return id;
}

export class ForgeSignals {
  private queue: SignalEvent[] = [];
  private breadcrumbs: Breadcrumb[] = [];
  private sessionId: string | null = null;
  private lastCorrelationId: string | null = null;
  private lastPageUrl: string | null = null;
  private client: ForgeClient;
  private config: Required<SignalsConfig>;
  private flushTimer: ReturnType<typeof setInterval> | null = null;
  private destroyed = false;
  private utmParams: Record<string, string> | null = null;
  private originalPushState: typeof history.pushState | null = null;
  private originalReplaceState: typeof history.replaceState | null = null;
  private boundListeners: Array<[EventTarget, string, EventListener]> = [];

  constructor(client: ForgeClient, config?: SignalsConfig) {
    this.client = client;
    this.config = {
      enabled: config?.enabled ?? true,
      autoPageViews: config?.autoPageViews ?? true,
      autoCaptureErrors: config?.autoCaptureErrors ?? true,
      flushInterval: config?.flushInterval ?? DEFAULT_FLUSH_INTERVAL,
      maxBatchSize: config?.maxBatchSize ?? DEFAULT_MAX_BATCH,
    };

    if (!this.config.enabled) return;

    this.utmParams = this.extractUtm();
    this.startFlushTimer();
    // Defer auto-capture setup to avoid competing with the SSE connection
    // for DB pool connections on cold start.
    setTimeout(() => {
      if (!this.destroyed) this.setupAutoCapture();
    }, 2000);
    this.setupUnloadFlush();
  }

  /** Track a custom event. */
  track(event: string, properties?: Record<string, unknown>): void {
    if (!this.config.enabled) return;
    this.enqueue({
      event,
      properties,
      correlation_id: this.lastCorrelationId ?? undefined,
    });
  }

  /** Identify the current user (links anonymous session to user). */
  async identify(userId: string, traits?: Record<string, unknown>): Promise<void> {
    if (!this.config.enabled) return;
    try {
      await fetch(`${this.client.getUrl()}/_api/signal/user`, {
        method: "POST",
        ...this.signalFetchOptions(),
        body: JSON.stringify({ user_id: userId, traits: traits ?? {} }),
      });
    } catch {
      // Silently fail, analytics should never break the app
    }
  }

  /** Track a page view. Called automatically on navigation when autoPageViews is enabled. */
  async page(properties?: Record<string, unknown>): Promise<void> {
    if (!this.config.enabled) return;
    try {
      const payload: Record<string, unknown> = {
        url: location.href,
        referrer: document.referrer || undefined,
        title: document.title || undefined,
        ...this.utmParams,
        ...properties,
      };

      const response = await fetch(`${this.client.getUrl()}/_api/signal/view`, {
        method: "POST",
        ...this.signalFetchOptions(),
        body: JSON.stringify(payload),
      });

      const result = await response.json();
      if (result.session_id && !this.sessionId) {
        this.sessionId = result.session_id;
      }
      // UTM only matters on first page view
      this.utmParams = null;
    } catch {
      // Silent
    }
  }

  /** Capture a frontend error with optional context. */
  captureError(error: Error | string, context?: Record<string, unknown>): void {
    if (!this.config.enabled) return;
    const message = typeof error === "string" ? error : error.message;
    const stack = typeof error === "string" ? undefined : error.stack;

    this.reportErrors([{
      message,
      stack,
      context,
      correlation_id: this.lastCorrelationId ?? undefined,
      breadcrumbs: [...this.breadcrumbs],
      page_url: typeof location !== "undefined" ? location.href : undefined,
    }]);
  }

  /** Add a breadcrumb for error reproduction context. */
  breadcrumb(message: string, data?: Record<string, unknown>): void {
    if (!this.config.enabled) return;
    this.breadcrumbs.push({
      message,
      data,
      timestamp: new Date().toISOString(),
    });
    if (this.breadcrumbs.length > MAX_BREADCRUMBS) {
      this.breadcrumbs.shift();
    }
  }

  /** Generate a correlation ID for the next RPC call. */
  nextCorrelationId(): string {
    this.lastCorrelationId = generateId();
    return this.lastCorrelationId;
  }

  /** Get the current session ID (null until first server response). */
  getSessionId(): string | null {
    return this.sessionId;
  }

  /** Clean up timers and event listeners. */
  destroy(): void {
    this.destroyed = true;
    if (this.flushTimer) clearInterval(this.flushTimer);
    this.flushBeacon();
    this.teardownAutoCapture();
  }

  // -- Internal --

  private signalFetchOptions(): { headers: Record<string, string>; credentials: RequestCredentials } {
    return {
      headers: {
        "Content-Type": "application/json",
        "x-forge-platform": "web",
        ...(this.sessionId ? { "x-session-id": this.sessionId } : {}),
      },
      credentials: "include",
    };
  }

  private enqueue(event: SignalEvent): void {
    event.timestamp = new Date().toISOString();
    this.queue.push(event);
    if (this.queue.length > MAX_QUEUE_SIZE) {
      this.queue.splice(0, this.queue.length - MAX_QUEUE_SIZE);
    }
    if (this.queue.length >= this.config.maxBatchSize) {
      this.flush();
    }
  }

  private async flush(): Promise<void> {
    if (this.queue.length === 0) return;
    const events = this.queue.splice(0, this.config.maxBatchSize);
    try {
      const response = await fetch(`${this.client.getUrl()}/_api/signal/event`, {
        method: "POST",
        ...this.signalFetchOptions(),
        body: JSON.stringify({
          events,
          context: {
            page_url: typeof location !== "undefined" ? location.href : undefined,
            session_id: this.sessionId,
          },
        }),
      });
      const result = await response.json();
      if (result.session_id && !this.sessionId) {
        this.sessionId = result.session_id;
      }
    } catch {
      this.queue.unshift(...events);
      if (this.queue.length > MAX_QUEUE_SIZE) {
        this.queue.length = MAX_QUEUE_SIZE;
      }
    }
  }

  private flushBeacon(): void {
    if (this.queue.length === 0) return;
    const events = this.queue.splice(0);
    const body = JSON.stringify({
      events,
      context: {
        page_url: typeof location !== "undefined" ? location.href : undefined,
        session_id: this.sessionId,
      },
    });
    try {
      navigator.sendBeacon(`${this.client.getUrl()}/_api/signal/event`, body);
    } catch {
      // Last resort, just drop
    }
  }

  private async reportErrors(errors: Array<Record<string, unknown>>): Promise<void> {
    try {
      await fetch(`${this.client.getUrl()}/_api/signal/report`, {
        method: "POST",
        ...this.signalFetchOptions(),
        body: JSON.stringify({ errors }),
      });
    } catch {
      // Silent
    }
  }

  private startFlushTimer(): void {
    this.flushTimer = setInterval(() => {
      if (!this.destroyed) this.flush();
    }, this.config.flushInterval);
  }

  private addEventListener(target: EventTarget, event: string, handler: EventListener): void {
    target.addEventListener(event, handler);
    this.boundListeners.push([target, event, handler]);
  }

  private setupAutoCapture(): void {
    if (typeof window === "undefined") return;

    if (this.config.autoPageViews) {
      this.lastPageUrl = location.href;
      this.page();

      this.originalPushState = history.pushState.bind(history);
      this.originalReplaceState = history.replaceState.bind(history);

      const onNavigation = () => {
        const current = location.href;
        if (current !== this.lastPageUrl) {
          this.lastPageUrl = current;
          this.page();
        }
      };

      history.pushState = (...args: Parameters<typeof history.pushState>) => {
        this.originalPushState!(...args);
        onNavigation();
      };
      history.replaceState = (...args: Parameters<typeof history.replaceState>) => {
        this.originalReplaceState!(...args);
        onNavigation();
      };

      this.addEventListener(window, "popstate", () => onNavigation());
    }

    if (this.config.autoCaptureErrors) {
      this.addEventListener(window, "error", ((e: ErrorEvent) => {
        if (e.error) {
          this.captureError(e.error);
        } else {
          this.captureError(e.message || "Unknown error");
        }
      }) as EventListener);

      this.addEventListener(window, "unhandledrejection", ((e: PromiseRejectionEvent) => {
        const reason = e.reason;
        if (reason instanceof Error) {
          this.captureError(reason);
        } else {
          this.captureError(String(reason || "Unhandled promise rejection"));
        }
      }) as EventListener);
    }
  }

  private teardownAutoCapture(): void {
    // Restore monkey-patched history methods
    if (this.originalPushState) {
      history.pushState = this.originalPushState;
    }
    if (this.originalReplaceState) {
      history.replaceState = this.originalReplaceState;
    }
    // Remove all event listeners
    for (const [target, event, handler] of this.boundListeners) {
      target.removeEventListener(event, handler);
    }
    this.boundListeners = [];
  }

  private setupUnloadFlush(): void {
    if (typeof document === "undefined") return;

    this.addEventListener(document, "visibilitychange", () => {
      if (document.visibilityState === "hidden") {
        this.flushBeacon();
      }
    });
  }

  private extractUtm(): Record<string, string> | null {
    if (typeof location === "undefined") return null;
    const params = new URLSearchParams(location.search);
    const utm: Record<string, string> = {};
    for (const key of ["utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content"]) {
      const value = params.get(key);
      if (value) utm[key] = value;
    }
    return Object.keys(utm).length > 0 ? utm : null;
  }
}
