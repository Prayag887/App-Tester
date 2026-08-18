// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";
import { ui } from "../stores.svelte";
import type { HttpTransaction } from "../types";
import RequestList from "./RequestList.svelte";

function transaction(index: number): HttpTransaction {
  return {
    id: `transaction-${index}`,
    session_id: "session",
    state: "response_complete",
    request: {
      method: "GET",
      scheme: "https",
      host: "api.test",
      path: `/items/${index}`,
      query: [],
      headers: [],
      body: { storage: "empty" },
      http_version: "HTTP/1.1",
    },
    response: {
      status: 400,
      headers: [],
      body: { storage: "empty" },
      decoded_size: 0,
      encoded_size: 0,
      http_version: "HTTP/1.1",
    },
    timing: { request_started_ms: index, response_complete_ms: index + 125 },
    capture_quality: "complete",
    correlated_incidents: [],
    created_at: "2026-08-16T00:00:00Z",
    updated_at: "2026-08-16T00:00:00Z",
  };
}

afterEach(() => {
  cleanup();
  ui.transactions = [];
  ui.selectedId = "";
  ui.transactionDetail = null;
  ui.demoMode = false;
  ui.query = "";
});

describe("RequestList", () => {
  it("keeps the DOM row count bounded for a full capture window", () => {
    ui.transactions = Array.from({ length: 250 }, (_, index) =>
      transaction(index),
    );

    const { container } = render(RequestList);

    expect(
      container.querySelectorAll(".request-row").length,
    ).toBeLessThanOrEqual(23);
    expect(container.querySelectorAll(".request-spacer")).toHaveLength(1);
    expect(container.textContent).toContain(
      "TimeMethodEndpointStatusDurationChanges",
    );
    expect(container.textContent).toContain("125 ms");
    expect(container.textContent).not.toContain("LIVE REQUESTS");
  });

  it("explains when filters hide an existing capture", () => {
    ui.transactions = [transaction(1)];
    ui.query = "does-not-match";

    const { getByText, queryByText } = render(RequestList);

    expect(getByText("No matching requests")).toBeTruthy();
    expect(getByText("Adjust the search or active filters.")).toBeTruthy();
    expect(queryByText("No traffic yet")).toBeNull();
    ui.query = "";
  });

  it("loads an explorable capture without an Android device", async () => {
    const { getByRole } = render(RequestList);

    await fireEvent.click(getByRole("button", { name: "Load demo traffic" }));

    expect(ui.demoMode).toBe(true);
    expect(ui.transactions).toHaveLength(5);
    expect(ui.incidents).toHaveLength(1);
  });
});
