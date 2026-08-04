import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return { ...actual, deleteAllTransactions: vi.fn().mockResolvedValue(undefined) };
});
import * as api from "./api";
import {
  captureScreen,
  choosePackage,
  getChangedCount,
  getErrorCount,
  getFailedCount,
  getMatchingApps,
  getRowStates,
  getSelectedTransaction,
  getStatusLabel,
  getVisibleTransactions,
  reconcileTransactions,
  requestDeleteAll,
  setMirrorOpen,
  ui,
  upsertIncident,
  upsertTransaction,
} from "./stores.svelte";
import type { AndroidApp, HttpTransaction, LogIncident } from "./types";

const transaction = (overrides: Partial<HttpTransaction> = {}): HttpTransaction =>
  ({
    id: "one",
    session_id: "session",
    state: "response_complete",
    request: {
      method: "GET",
      scheme: "https",
      host: "api.example.test",
      path: "/v1/items",
      query: [],
      headers: [],
      body: { storage: "empty" },
      http_version: "HTTP_1_1",
    },
    response: { status: 200, headers: [], body: { storage: "empty" }, decoded_size: 0, encoded_size: 0, http_version: "HTTP_1_1" },
    timing: { request_started_ms: 1_000 },
    capture_quality: "complete",
    correlated_incidents: [],
    created_at: "2026-07-24T00:00:00Z",
    updated_at: "2026-07-24T00:00:00Z",
    ...overrides,
  }) as HttpTransaction;

const incident = (signature: string, message: string): LogIncident =>
  ({
    id: `${signature}-id`,
    category: "error",
    signature,
    title: "Title",
    message,
    summary: "Summary",
    where_occurred: "Where",
    how_occurred: "How",
    likely_cause: "Cause",
    reproduction_steps: [],
    first_occurred_at: "2026-07-24T00:00:00Z",
    occurred_at: "2026-07-24T00:00:00Z",
    lines: [],
    occurrence_count: 1,
  }) as LogIncident;

const reset = () => {
  ui.transactions = [];
  ui.incidents = [];
  ui.query = "";
  ui.changedOnly = false;
  ui.errorsOnly = false;
  ui.selectedId = "";
  ui.packageSearch = "";
  ui.packagePickerOpen = false;
  ui.devicesOpen = false;
  ui.device = "";
  ui.apps = [];
  ui.capturing = false;
  ui.proxy = "stopped";
  ui.notice = "";
  ui.packageName = "";
  ui.mirrorOpen = false;
  ui.mirrorData = "";
  ui.mirrorError = "";
  ui.confirmDeleteAll = false;
};

beforeEach(reset);

describe("upsertTransaction", () => {
  it("prepends new transactions and replaces by id", () => {
    upsertTransaction(transaction({ id: "a", created_at: "2026-07-24T00:00:01Z" }));
    upsertTransaction(transaction({ id: "b", created_at: "2026-07-24T00:00:02Z" }));
    upsertTransaction(transaction({ id: "a", created_at: "2026-07-24T00:00:03Z" }));
    expect(ui.transactions.map(tx => tx.id)).toEqual(["a", "b"]);
    expect(ui.transactions[0].created_at).toBe("2026-07-24T00:00:03Z");
  });
});

describe("reconcileTransactions", () => {
  it("merges a fresh read without erasing recent event-delivered rows", () => {
    upsertTransaction(transaction({ id: "event", created_at: "2026-07-24T00:00:05Z" }));
    reconcileTransactions([transaction({ id: "db", created_at: "2026-07-24T00:00:01Z" })]);
    expect(ui.transactions.map(tx => tx.id).sort()).toEqual(["db", "event"]);
  });

  it("sorts the merged set newest first", () => {
    reconcileTransactions([
      transaction({ id: "old", created_at: "2026-07-24T00:00:01Z" }),
      transaction({ id: "new", created_at: "2026-07-24T00:00:09Z" }),
      transaction({ id: "mid", created_at: "2026-07-24T00:00:05Z" }),
    ]);
    expect(ui.transactions.map(tx => tx.id)).toEqual(["new", "mid", "old"]);
  });

  it("an empty database read never erases the live list", () => {
    upsertTransaction(transaction({ id: "live" }));
    reconcileTransactions([]);
    expect(ui.transactions.map(tx => tx.id)).toEqual(["live"]);
  });
});

describe("upsertIncident", () => {
  it("deduplicates by signature and increments the occurrence count", () => {
    upsertIncident(incident("sig-1", "first"));
    upsertIncident(incident("sig-1", "second"));
    expect(ui.incidents).toHaveLength(1);
    expect(ui.incidents[0].occurrence_count).toBe(2);
    expect(ui.incidents[0].message).toBe("second");
  });

  it("keeps distinct signatures separate and caps the list", () => {
    for (let i = 0; i < 120; i += 1) {
      upsertIncident(incident(`sig-${i}`, `message ${i}`));
    }
    expect(ui.incidents).toHaveLength(100);
    expect(ui.incidents[0].signature).toBe("sig-119");
  });
});

describe("getVisibleTransactions", () => {
  const base = [
    transaction({ id: "get", request: { ...transaction().request, method: "GET", host: "api.one.test", path: "/a" } }),
    transaction({ id: "post", request: { ...transaction().request, method: "POST", host: "api.two.test", path: "/b" } }),
    transaction({ id: "fail", request: { ...transaction().request, method: "DELETE", host: "api.three.test", path: "/c" }, response: { status: 500, headers: [], body: { storage: "empty" }, decoded_size: 0, encoded_size: 0, http_version: "HTTP_1_1" } }),
  ];

  it("filters by free-text query across method, host, and path", () => {
    ui.transactions = base;
    ui.query = "api.two";
    expect(getVisibleTransactions().map(tx => tx.id)).toEqual(["post"]);
  });

  it("filters changed-only and errors-only flags", () => {
    ui.transactions = base;
    ui.changedOnly = true;
    expect(getVisibleTransactions()).toEqual([]);
    ui.changedOnly = false;
    ui.errorsOnly = true;
    expect(getVisibleTransactions().map(tx => tx.id)).toEqual(["fail"]);
  });

  it("exposes correlated incidents under the errors filter", () => {
    ui.transactions = base.map(tx =>
      tx.id === "get" ? { ...tx, correlated_incidents: ["incident-id"] } : tx,
    );
    ui.errorsOnly = true;
    expect(getVisibleTransactions().map(tx => tx.id)).toEqual(["get", "fail"]);
  });

  it("never surfaces CONNECT tunnel rows", () => {
    ui.transactions = [
      ...base,
      transaction({ id: "connect", request: { ...transaction().request, method: "CONNECT" } }),
    ];
    expect(getVisibleTransactions().map(tx => tx.id)).toEqual(["get", "post", "fail"]);
  });
});

describe("getSelectedTransaction", () => {
  it("prefers the explicitly selected id and falls back to the first visible row", () => {
    ui.transactions = [
      transaction({ id: "first", created_at: "2026-07-24T00:00:01Z" }),
      transaction({ id: "second", created_at: "2026-07-24T00:00:02Z" }),
    ];
    expect(getSelectedTransaction()?.id).toBe("first");
    ui.selectedId = "first";
    expect(getSelectedTransaction()?.id).toBe("first");
    ui.selectedId = "missing";
    expect(getSelectedTransaction()?.id).toBe("first");
  });
});

describe("getMatchingApps", () => {
  const apps: AndroidApp[] = [
    { package_name: "com.example.alpha", version_name: "1.0", debuggable: true },
    { package_name: "com.example.beta", debuggable: false },
  ];

  it("searches package name and version and limits unscoped results", () => {
    ui.apps = apps;
    expect(getMatchingApps()).toHaveLength(2);
    ui.packageSearch = "BETA";
    expect(getMatchingApps().map(app => app.package_name)).toEqual(["com.example.beta"]);
    ui.packageSearch = "nomatch";
    expect(getMatchingApps()).toEqual([]);
  });
});

describe("row states and counters", () => {
  it("classifies each visible row once", () => {
    ui.transactions = [
      transaction({ id: "ok" }),
      transaction({ id: "bad", response: { status: 500, headers: [], body: { storage: "empty" }, decoded_size: 0, encoded_size: 0, http_version: "HTTP_1_1" } }),
    ];
    expect(getRowStates().get("ok")).toBe("Captured");
    expect(getRowStates().get("bad")).toBe("Failed");
    expect(getChangedCount()).toBe(0);
    expect(getFailedCount()).toBe(1);
  });

  it("counts incidents by crash/error/anr categories", () => {
    ui.incidents = [
      incident("a", "boom"),
      incident("b", "warn"),
      incident("c", "anr"),
    ];
    ui.incidents[1].category = "warning";
    expect(getErrorCount()).toBe(2);
  });
});

describe("status label", () => {
  it("reflects live capture, running proxy, and idle states", () => {
    expect(getStatusLabel()).toBe("Ready to capture");
    ui.proxy = "running";
    expect(getStatusLabel()).toBe("Proxy ready");
    ui.capturing = true;
    expect(getStatusLabel()).toBe("Capturing live");
  });
});

describe("choosePackage", () => {
  it("selects the package, closes the picker, and announces it", () => {
    ui.packagePickerOpen = true;
    choosePackage("com.example.alpha");
    expect(ui.packageName).toBe("com.example.alpha");
    expect(ui.packagePickerOpen).toBe(false);
    expect(ui.packageSearch).toBe("");
    expect(ui.notice).toContain("com.example.alpha");
  });
});

describe("requestDeleteAll", () => {
  it("arms the confirmation on the first click without deleting", async () => {
    ui.transactions = [transaction({ id: "keep" })];
    requestDeleteAll();
    expect(ui.confirmDeleteAll).toBe(true);
    expect(ui.transactions).toHaveLength(1);
  });

  it("deletes on the second click", async () => {
    ui.transactions = [transaction({ id: "gone" })];
    requestDeleteAll();
    requestDeleteAll();
    expect(ui.confirmDeleteAll).toBe(false);
    await vi.waitFor(() => {
      expect(ui.transactions).toEqual([]);
      expect(ui.notice).toContain("Deleted all");
    });
  });

  it("reports failures without clearing the list", async () => {
    vi.mocked(api.deleteAllTransactions).mockRejectedValueOnce(new Error("boom"));
    ui.transactions = [transaction({ id: "stays" })];
    requestDeleteAll();
    requestDeleteAll();
    await vi.waitFor(() => {
      expect(ui.transactions).toHaveLength(1);
      expect(ui.notice).toContain("Could not delete captures");
    });
  });
});

describe("device mirror", () => {
  it("opening the mirror keeps prior data and closing clears it", () => {
    ui.mirrorData = "data:image/png;base64,abc";
    setMirrorOpen(true);
    expect(ui.mirrorOpen).toBe(true);
    expect(ui.mirrorData).toBe("data:image/png;base64,abc");
    setMirrorOpen(false);
    expect(ui.mirrorOpen).toBe(false);
    expect(ui.mirrorData).toBe("");
  });

  it("capture without a selected device reports a clear error", async () => {
    ui.device = "";
    await captureScreen();
    expect(ui.mirrorError).toContain("Select an Android device");
  });
});
