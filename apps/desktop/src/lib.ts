//! Pure presentation helpers for the desktop UI. No runes, no state, no IO —
//! every function is a deterministic mapping over plain values, which keeps
//! them unit-testable in isolation.

import type { BodyStorage, HttpTransaction, ManualBody, ManualRequest } from "./types";

export type Screen = "traffic" | "logs" | "composer";
export type Tab =
  "Overview" | "Request" | "Response" | "Compare" | "cURL" | "Timeline";
export type TransactionState = "Pending" | "Failed" | "Changed" | "Captured";

export const transactionState = (tx: HttpTransaction): TransactionState => {
  if (!tx.response) return "Pending";
  if (tx.response.status >= 400) return "Failed";
  if (tx.comparison?.differences.some((difference) => !difference.ignored))
    return "Changed";
  return "Captured";
};

export const durationMs = (tx: HttpTransaction): number | undefined =>
  tx.timing.response_complete_ms == null
    ? undefined
    : tx.timing.response_complete_ms - tx.timing.request_started_ms;

export const bodyText = (value: CapturedBody | undefined): string => {
  if (!value || value.storage === "empty") return "No body";
  if (value.storage === "unavailable") return value.reason;
  return new TextDecoder().decode(
    new Uint8Array(value.storage === "inline" ? value.bytes : value.preview),
  );
};

type CapturedBody = BodyStorage;

export const prettyJson = (value: string): string => {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
};

export const endpointId = (tx: HttpTransaction): string | undefined =>
  tx.endpoint_identity &&
  `${tx.endpoint_identity.method} ${tx.endpoint_identity.host} ${tx.endpoint_identity.path_template}`;

/** Returns the generated cURL command preferred for clipboard export. */
export const curlCommand = (
  tx: HttpTransaction | undefined,
): string | undefined => tx?.curl?.multiline || tx?.curl?.compact;

export const timeLabel = (iso: string): string =>
  new Date(iso).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });

export const copyToClipboard = (value: string, onDone: () => void): void => {
  void navigator.clipboard.writeText(value);
  onDone();
};

// ---- Composer presentation helpers ----

export const elapsedLabel = (ms: number): string =>
  ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;

export const byteSizeLabel = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
};

/** Maps an HTTP method to the existing row-tone CSS class. */
export const methodTone = (method: string): string => {
  switch (method.toUpperCase()) {
    case "POST":
    case "PUT":
    case "PATCH":
      return "post";
    case "DELETE":
      return "delete";
    default:
      return "get";
  }
};

const textDecoder = new TextDecoder();

/** Turns a captured transaction's request into an editable composer request. */
export const manualRequestFromTransaction = (transaction: HttpTransaction): ManualRequest => {
  const captured = transaction.request;
  const port = captured.port ? `:${captured.port}` : "";
  const url = `${captured.scheme}://${captured.host}${port}${captured.path}`;
  const contentTypes = captured.headers
    .filter((header) => header.name.toLowerCase() === "content-type")
    .map((header) => header.value.split(";")[0].trim().toLowerCase());
  const body: ManualBody = (() => {
    if (captured.body.storage === "inline") {
      const text = textDecoder.decode(new Uint8Array(captured.body.bytes));
      return { kind: "raw", media_type: contentTypes[0] ?? null, text };
    }
    if (captured.body.storage === "truncated") {
      const text = textDecoder.decode(new Uint8Array(captured.body.preview));
      return { kind: "raw", media_type: contentTypes[0] ?? null, text };
    }
    // Offloaded artifacts and unavailable bodies have nothing editable.
    return { kind: "none" };
  })();
  return {
    method: captured.method,
    url,
    query: [],
    headers: captured.headers.map((header) => ({ name: header.name, value: header.value })),
    body,
    auth: { kind: "none" },
  };
};
