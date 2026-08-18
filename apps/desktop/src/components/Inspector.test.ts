// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { UI_TEXT_PREVIEW_LIMIT } from "../lib";
import { ui } from "../stores.svelte";
import type { HttpTransaction } from "../types";
import Inspector from "./Inspector.svelte";

const largeText = "x".repeat(UI_TEXT_PREVIEW_LIMIT + 4_096);
const largeBytes = Array.from(new TextEncoder().encode(largeText));

const transaction: HttpTransaction = {
  id: "failed-request",
  session_id: "session",
  state: "response_complete",
  request: {
    method: "GET",
    scheme: "http",
    host: "127.0.0.1",
    path: "/bad-request",
    query: [],
    headers: [],
    body: { storage: "empty" },
    http_version: "HTTP/1.1",
  },
  response: {
    status: 400,
    headers: [
      { name: "X-Duplicate", value: "same" },
      { name: "X-Duplicate", value: "same" },
    ],
    body: { storage: "inline", bytes: largeBytes },
    decoded_size: largeBytes.length,
    encoded_size: largeBytes.length,
    http_version: "HTTP/1.1",
  },
  timing: { request_started_ms: 1, response_complete_ms: 2 },
  capture_quality: "complete",
  correlated_incidents: [],
  curl: {
    compact: largeText,
    multiline: largeText,
    redacted: true,
  },
  created_at: "2026-08-16T00:00:00Z",
  updated_at: "2026-08-16T00:00:00Z",
};

beforeEach(() => {
  ui.transactions = [transaction];
  ui.selectedId = transaction.id;
  ui.tab = "Response";
});

afterEach(() => {
  cleanup();
  ui.transactions = [];
  ui.selectedId = "";
  ui.tab = "Request";
});

describe("Inspector", () => {
  it("renders duplicate response headers without crashing and bounds a large body", async () => {
    ui.tab = "Request";
    const { container } = render(Inspector);
    expect(screen.queryByRole("tab", { name: "Overview" })).toBeNull();
    await fireEvent.click(screen.getByRole("tab", { name: "Response" }));

    expect(screen.getAllByText("same")).toHaveLength(2);
    expect(
      container.querySelector(".detail-panel pre")?.textContent,
    ).toHaveLength(UI_TEXT_PREVIEW_LIMIT);
    expect(
      screen.getByText(/The full body remains in the capture/),
    ).toBeTruthy();
  });

  it("bounds a large generated cURL command", async () => {
    const { container } = render(Inspector);
    await fireEvent.click(screen.getByRole("tab", { name: "cURL" }));

    expect(
      container.querySelector(".detail-panel pre")?.textContent,
    ).toHaveLength(UI_TEXT_PREVIEW_LIMIT);
    expect(screen.getByText(/65,536 of 69,632 characters/)).toBeTruthy();
  });

  it("closes the contextual inspector and clears its cached detail", async () => {
    ui.transactionDetail = transaction;
    render(Inspector);

    await fireEvent.click(
      screen.getByRole("button", { name: "Close request inspector" }),
    );

    expect(ui.selectedId).toBe("");
    expect(ui.transactionDetail).toBeNull();
  });

  it("renders an image uploaded in multipart form data", () => {
    const prefix = new TextEncoder().encode(
      '--image-boundary\r\nContent-Disposition: form-data; name="photo"; filename="photo.png"\r\nContent-Type: image/png\r\n\r\n',
    );
    const suffix = new TextEncoder().encode("\r\n--image-boundary--\r\n");
    ui.transactions = [{
      ...transaction,
      request: {
        ...transaction.request,
        method: "POST",
        content_type: "multipart/form-data; boundary=image-boundary",
        body: {
          storage: "inline",
          bytes: [...prefix, 137, 80, 78, 71, ...suffix],
        },
      },
    }];
    ui.tab = "Request";

    render(Inspector);

    const image = screen.getByRole("img", {
      name: "Multipart upload preview: photo.png",
    });
    expect(image.getAttribute("src")).toBe("data:image/png;base64,iVBORw==");
    expect(screen.getByText("image/png · 4 B")).toBeTruthy();
  });
});
