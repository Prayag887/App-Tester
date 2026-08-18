import type { BodyStorage, HttpTransaction, LogIncident } from "./types";

const encoder = new TextEncoder();
const textBody = (value: unknown) => ({
  storage: "inline" as const,
  bytes: Array.from(encoder.encode(JSON.stringify(value, null, 2))),
});

const emptyBody = { storage: "empty" as const };
const headers = (contentType = "application/json") => [
  { name: "content-type", value: contentType },
  { name: "x-request-id", value: "demo_req_8f2a" },
];

// A complete 1 × 1 violet PNG. Small on purpose, but it exercises the same
// binary-to-base64 image preview path as a real captured response.
const DEMO_PNG = [
  137,80,78,71,13,10,26,10,0,0,0,13,73,72,68,82,0,0,0,1,0,0,0,1,8,6,0,0,0,
  31,21,196,137,0,0,0,13,73,68,65,84,8,215,99,168,100,248,207,240,31,0,5,84,2,
  127,151,85,51,143,0,0,0,0,73,69,78,68,174,66,96,130,
];

type DemoTransactionOptions = {
  id: string;
  offsetMs: number;
  method: string;
  path: string;
  status: number;
  durationMs: number;
  requestBody?: ReturnType<typeof textBody>;
  responseBody?: BodyStorage;
  responseContentType?: string;
  comparison?: HttpTransaction["comparison"];
  dailyChanges?: HttpTransaction["daily_changes"];
  incidents?: string[];
};

function transaction(options: DemoTransactionOptions, now: number): HttpTransaction {
  const createdAt = new Date(now - options.offsetMs).toISOString();
  const url = `https://api.demo-shop.test${options.path}`;
  const requestBody = options.requestBody ?? emptyBody;
  return {
    id: options.id,
    session_id: "demo-session",
    state: "response_complete",
    request: {
      method: options.method,
      scheme: "https",
      host: "api.demo-shop.test",
      path: options.path,
      query: options.path === "/v1/catalog" ? [{ name: "page", value: "1" }] : [],
      headers: [
        { name: "accept", value: "application/json" },
        { name: "authorization", value: "Bearer demo_token_not_real" },
        ...(options.requestBody ? headers() : []),
      ],
      body: requestBody,
      content_type: options.requestBody ? "application/json" : undefined,
      http_version: "HTTP/2",
    },
    response: {
      status: options.status,
      reason: options.status >= 500 ? "Internal Server Error" : undefined,
      headers: headers(options.responseContentType),
      body: options.responseBody ?? emptyBody,
      content_type: options.responseContentType ?? "application/json",
      decoded_size: JSON.stringify(options.responseBody ?? {}).length,
      encoded_size: JSON.stringify(options.responseBody ?? {}).length,
      http_version: "HTTP/2",
    },
    timing: {
      request_started_ms: now - options.offsetMs,
      request_complete_ms: now - options.offsetMs + 14,
      response_started_ms: now - options.offsetMs + Math.max(28, options.durationMs - 22),
      response_complete_ms: now - options.offsetMs + options.durationMs,
    },
    endpoint_identity: {
      method: options.method,
      host: "api.demo-shop.test",
      path_template: options.path.replace(/\/\d+(?=\/|$)/g, "/:id"),
    },
    curl: {
      compact: `curl '${url}' -H 'authorization: Bearer demo_token_not_real'`,
      multiline: `curl '${url}' \\\n  -H 'accept: application/json' \\\n  -H 'authorization: Bearer demo_token_not_real'`,
      redacted: false,
    },
    capture_quality: "complete",
    comparison: options.comparison,
    daily_changes: options.dailyChanges,
    correlated_incidents: options.incidents ?? [],
    created_at: createdAt,
    updated_at: createdAt,
  };
}

export function createDemoCapture(now = Date.now()): {
  transactions: HttpTransaction[];
  incidents: LogIncident[];
} {
  const crashId = "demo-incident-checkout";
  const transactions = [
    transaction({
      id: "demo-profile",
      offsetMs: 4_000,
      method: "GET",
      path: "/v1/profile",
      status: 200,
      durationMs: 86,
      responseBody: textBody({ id: 42, name: "Maya", plan: "pro", notifications: true }),
    }, now),
    transaction({
      id: "demo-order",
      offsetMs: 8_500,
      method: "POST",
      path: "/v1/orders",
      status: 201,
      durationMs: 214,
      requestBody: textBody({ sku: "LAMP-04", quantity: 1 }),
      responseBody: textBody({ order_id: "ord_demo_1042", state: "confirmed", total: 79.95 }),
    }, now),
    transaction({
      id: "demo-catalog-changed",
      offsetMs: 13_000,
      method: "GET",
      path: "/v1/catalog",
      status: 200,
      durationMs: 142,
      responseBody: textBody({ items: [{ id: 7, title: "Aurora Lamp", price: 79.95 }], next_cursor: null }),
      dailyChanges: { count: 2, last_changed_at: new Date(now - 13_000).toISOString() },
      comparison: {
        baseline_transaction_id: "demo-baseline-catalog",
        compatibility: "potentially_breaking",
        differences: [{
          kind: "field_removed",
          path: "/items/*/inventory_count",
          previous: "number",
          current: "—",
          severity: "warning",
          ignored: false,
          explanation: "The inventory_count field disappeared from catalog items.",
        }],
      },
    }, now),
    transaction({
      id: "demo-checkout-error",
      offsetMs: 19_000,
      method: "POST",
      path: "/v1/checkout",
      status: 500,
      durationMs: 681,
      requestBody: textBody({ order_id: "ord_demo_1042", payment_method: "demo_card" }),
      responseBody: textBody({ error: "payment_provider_timeout", retryable: true }),
      incidents: [crashId],
    }, now),
    transaction({
      id: "demo-image",
      offsetMs: 24_000,
      method: "GET",
      path: "/v1/avatar.png",
      status: 200,
      durationMs: 61,
      responseBody: { storage: "inline", bytes: DEMO_PNG },
      responseContentType: "image/png",
    }, now),
  ];

  const incidents: LogIncident[] = [{
    id: crashId,
    category: "error",
    signature: "demo.checkout.payment-timeout",
    title: "Checkout failed after payment timeout",
    message: "PaymentGatewayTimeout: provider did not respond within 500 ms",
    occurrence_count: 1,
    summary: "The checkout request returned HTTP 500 after its payment provider timed out.",
    root_cause: "The payment client timeout is shorter than the provider's observed response time.",
    foreground_activity: "com.example.shop/.CheckoutActivity",
    first_app_frame: "com.example.shop.checkout.PaymentRepository.submit",
    where_occurred: "PaymentRepository.submit",
    how_occurred: "A provider request exceeded the configured 500 ms deadline.",
    likely_cause: "An overly aggressive upstream timeout without a retry policy.",
    reproduction_steps: ["Open the cart", "Tap Checkout", "Submit the demo payment"],
    first_occurred_at: new Date(now - 19_000).toISOString(),
    occurred_at: new Date(now - 19_000).toISOString(),
    lines: [{
      timestamp_ms: now - 19_000,
      level: "E",
      tag: "Checkout",
      message: "PaymentGatewayTimeout: provider did not respond within 500 ms",
    }],
  }];
  return { transactions, incidents };
}
