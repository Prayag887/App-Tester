//! Pure presentation helpers for the desktop UI. No runes, no state, no IO —
//! every function is a deterministic mapping over plain values, which keeps
//! them unit-testable in isolation.

import type {
  BodyStorage,
  HttpTransaction,
  ManualBody,
  ManualRequest,
} from "./types";

export type Screen = "traffic" | "logs" | "composer";
export type Tab = "Request" | "Response" | "Compare" | "cURL" | "Timeline";
export type TransactionState = "Pending" | "Failed" | "Changed" | "Captured";

export const transactionState = (tx: HttpTransaction): TransactionState => {
  if (!tx.response) return "Pending";
  if (tx.response.status >= 400) return "Failed";
  if ((tx.daily_changes?.count ?? 0) > 0) return "Changed";
  if (tx.comparison?.differences.some((difference) => !difference.ignored))
    return "Changed";
  return "Captured";
};

export const durationMs = (tx: HttpTransaction): number | undefined =>
  tx.timing.response_complete_ms == null
    ? undefined
    : tx.timing.response_complete_ms - tx.timing.request_started_ms;

export const UI_TEXT_PREVIEW_LIMIT = 64 * 1024;

export interface TextPreview {
  text: string;
  truncated: boolean;
  shown: number;
  total: number;
}

export interface ImagePreview {
  name: string;
  mediaType: string;
  byteLength: number;
  dataUrl: string;
}

const textDecoder = new TextDecoder();

export const bodyText = (value: CapturedBody | undefined): string => {
  if (!value || value.storage === "empty") return "No body";
  if (value.storage === "unavailable") return value.reason;
  return textDecoder.decode(
    new Uint8Array(value.storage === "inline" ? value.bytes : value.preview),
  );
};

type CapturedBody = BodyStorage;

/**
 * Decodes only the prefix that is safe to place in the WebView DOM. Captured
 * bodies remain intact in native storage; this limit applies only to visible
 * text so a large response cannot make WebKit lay out millions of characters.
 */
export const bodyTextPreview = (
  value: CapturedBody | undefined,
  limit = UI_TEXT_PREVIEW_LIMIT,
): TextPreview => {
  if (!value || value.storage === "empty") {
    return { text: "", truncated: false, shown: 0, total: 0 };
  }
  if (value.storage === "unavailable") {
    return {
      text: value.reason,
      truncated: false,
      shown: value.reason.length,
      total: value.reason.length,
    };
  }
  const bytes = value.storage === "inline" ? value.bytes : value.preview;
  const total =
    value.storage === "inline"
      ? bytes.length
      : (value.original_size ?? bytes.length);
  const shownBytes = bytes.slice(0, limit);
  return {
    text: textDecoder.decode(new Uint8Array(shownBytes)),
    truncated: bytes.length > shownBytes.length || total > shownBytes.length,
    shown: shownBytes.length,
    total,
  };
};

const SAFE_IMAGE_MEDIA_TYPE =
  /^image\/(?:png|jpeg|gif|webp|avif|bmp|x-icon|vnd\.microsoft\.icon)$/i;
const byteEncoder = new TextEncoder();

function bodyBytes(value: CapturedBody | undefined): number[] | undefined {
  if (!value || value.storage === "empty" || value.storage === "unavailable")
    return undefined;
  return value.storage === "inline" ? value.bytes : value.preview;
}

function bytesIndexOf(source: Uint8Array, target: Uint8Array, from = 0): number {
  if (!target.length) return from;
  outer: for (let index = from; index <= source.length - target.length; index += 1) {
    for (let offset = 0; offset < target.length; offset += 1) {
      if (source[index + offset] !== target[offset]) continue outer;
    }
    return index;
  }
  return -1;
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let index = 0; index < bytes.length; index += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(index, index + 0x8000));
  }
  return btoa(binary);
}

/**
 * Extracts complete, browser-safe raster image parts from a bounded body
 * preview. SVG is deliberately excluded because captured markup must never
 * become active content in the Inspector.
 */
export function bodyImagePreviews(
  value: CapturedBody | undefined,
  contentType: string | null | undefined,
): ImagePreview[] {
  const storedBytes = bodyBytes(value);
  if (!storedBytes?.length || !contentType) return [];
  const bytes = new Uint8Array(storedBytes);
  const mediaType = contentType.split(";", 1)[0].trim().toLowerCase();

  if (SAFE_IMAGE_MEDIA_TYPE.test(mediaType)) {
    const complete =
      value?.storage === "inline" ||
      ((value?.storage === "artifact" || value?.storage === "truncated") &&
        value.original_size === bytes.length);
    if (!complete) return [];
    return [{
      name: "Image body",
      mediaType,
      byteLength: bytes.length,
      dataUrl: `data:${mediaType};base64,${bytesToBase64(bytes)}`,
    }];
  }

  if (mediaType !== "multipart/form-data") return [];
  const boundaryMatch = contentType.match(
    /(?:^|;)\s*boundary=(?:"([^"]+)"|([^;\s]+))/i,
  );
  const boundary = boundaryMatch?.[1] ?? boundaryMatch?.[2];
  if (!boundary) return [];

  const delimiter = byteEncoder.encode(`--${boundary}`);
  const nextDelimiter = byteEncoder.encode(`\r\n--${boundary}`);
  const headerEndMarker = byteEncoder.encode("\r\n\r\n");
  const previews: ImagePreview[] = [];
  let partStart = bytesIndexOf(bytes, delimiter);

  while (partStart >= 0) {
    const headersStart = partStart + delimiter.length + 2;
    const headersEnd = bytesIndexOf(bytes, headerEndMarker, headersStart);
    if (headersEnd < 0) break;
    const bodyStart = headersEnd + headerEndMarker.length;
    const bodyEnd = bytesIndexOf(bytes, nextDelimiter, bodyStart);
    if (bodyEnd < 0) break;

    const headers = textDecoder.decode(bytes.subarray(headersStart, headersEnd));
    const partType = headers.match(/^content-type:\s*([^;\r\n]+)/im)?.[1]?.trim().toLowerCase();
    if (partType && SAFE_IMAGE_MEDIA_TYPE.test(partType)) {
      const disposition = headers.match(/^content-disposition:\s*([^\r\n]+)/im)?.[1] ?? "";
      const filename = disposition.match(/filename="([^"]*)"/i)?.[1];
      const fieldName = disposition.match(/name="([^"]*)"/i)?.[1];
      const imageBytes = bytes.subarray(bodyStart, bodyEnd);
      previews.push({
        name: filename || fieldName || `Image ${previews.length + 1}`,
        mediaType: partType,
        byteLength: imageBytes.length,
        dataUrl: `data:${partType};base64,${bytesToBase64(imageBytes)}`,
      });
    }
    partStart = bodyEnd + 2;
  }
  return previews;
}

/** Bounds already-decoded content such as generated cURL commands. */
export const textPreview = (
  value: string,
  limit = UI_TEXT_PREVIEW_LIMIT,
): TextPreview => ({
  text: value.slice(0, limit),
  truncated: value.length > limit,
  shown: Math.min(value.length, limit),
  total: value.length,
});

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

const SENSITIVE_CURL_HEADER =
  /((?:cookie|set-cookie|x-api-key|api-key)\s*:\s*)([^'"\r\n]+)/gi;

/** Keeps non-auth session secrets out of imported or legacy cURL commands. */
export const redactCurlSecrets = (command: string): string =>
  command.replace(SENSITIVE_CURL_HEADER, "$1<redacted>");

/** Returns the generated cURL command, including locally captured auth tokens. */
export const curlCommand = (
  tx: HttpTransaction | undefined,
): string | undefined => {
  const command = tx?.curl?.multiline || tx?.curl?.compact;
  return command ? redactCurlSecrets(command) : undefined;
};

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

/** Turns a captured transaction's request into an editable composer request. */
export const manualRequestFromTransaction = (
  transaction: HttpTransaction,
): ManualRequest => {
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
    headers: captured.headers.map((header) => ({
      name: header.name,
      value: header.value,
    })),
    body,
    auth: { kind: "none" },
  };
};
