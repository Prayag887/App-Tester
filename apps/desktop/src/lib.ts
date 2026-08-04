//! Pure presentation helpers for the desktop UI. No runes, no state, no IO —
//! every function is a deterministic mapping over plain values, which keeps
//! them unit-testable in isolation.

import type { BodyStorage, HttpTransaction } from "./types";

export type Screen = "traffic" | "logs";
export type Tab = "Overview" | "Request" | "Response" | "Compare" | "cURL" | "Timeline";
export type TransactionState = "Pending" | "Failed" | "Changed" | "Captured";

export const transactionState = (tx: HttpTransaction): TransactionState => {
  if (!tx.response) return "Pending";
  if (tx.response.status >= 400) return "Failed";
  if (tx.comparison?.differences.some(difference => !difference.ignored)) return "Changed";
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