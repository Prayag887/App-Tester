import { describe, expect, it } from "vitest";
import {
  bodyText,
  bodyTextPreview,
  bodyImagePreviews,
  byteSizeLabel,
  curlCommand,
  durationMs,
  elapsedLabel,
  endpointId,
  manualRequestFromTransaction,
  methodTone,
  prettyJson,
  textPreview,
  timeLabel,
  transactionState,
} from "./lib";
import type { BodyStorage, HttpTransaction } from "./types";

const transaction = (
  overrides: Partial<HttpTransaction> = {},
): HttpTransaction =>
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
    response: {
      status: 200,
      headers: [],
      body: { storage: "empty" },
      decoded_size: 0,
      encoded_size: 0,
      http_version: "HTTP_1_1",
    },
    timing: { request_started_ms: 1_000 },
    capture_quality: "complete",
    correlated_incidents: [],
    created_at: "2026-07-24T00:00:00Z",
    updated_at: "2026-07-24T00:00:00Z",
    ...overrides,
  }) as HttpTransaction;

describe("transactionState", () => {
  it("classifies pending, failed, changed, and captured transactions", () => {
    expect(transactionState(transaction({ response: undefined }))).toBe(
      "Pending",
    );
    expect(
      transactionState(
        transaction({
          response: {
            status: 500,
            headers: [],
            body: { storage: "empty" },
            decoded_size: 0,
            encoded_size: 0,
            http_version: "HTTP_1_1",
          },
        }),
      ),
    ).toBe("Failed");
    expect(
      transactionState(
        transaction({
          response: {
            status: 200,
            headers: [],
            body: { storage: "empty" },
            decoded_size: 0,
            encoded_size: 0,
            http_version: "HTTP_1_1",
          },
          comparison: {
            baseline_transaction_id: "base",
            compatibility: "exact",
            differences: [
              {
                kind: "key_added",
                path: "/user",
                severity: "critical",
                ignored: false,
                explanation: "x",
              },
            ],
          },
        }),
      ),
    ).toBe("Changed");
    expect(transactionState(transaction())).toBe("Captured");
  });

  it("ignores differences that are explicitly ignored", () => {
    expect(
      transactionState(
        transaction({
          comparison: {
            compatibility: "exact",
            differences: [
              {
                kind: "key_added",
                severity: "critical",
                ignored: true,
                explanation: "x",
              },
            ],
          },
        }),
      ),
    ).toBe("Captured");
  });

  it("keeps a row marked changed after a later unchanged daily snapshot", () => {
    expect(
      transactionState(
        transaction({ daily_changes: { count: 2 } }),
      ),
    ).toBe("Changed");
  });
});

describe("durationMs", () => {
  it("measures the gap between request start and response completion", () => {
    expect(
      durationMs(
        transaction({
          timing: { request_started_ms: 100, response_complete_ms: 340 },
        }),
      ),
    ).toBe(240);
  });

  it("is undefined while the request is still in flight", () => {
    expect(durationMs(transaction())).toBeUndefined();
  });
});

describe("bodyText", () => {
  const inline: BodyStorage = { storage: "inline", bytes: [104, 105] };
  const preview: BodyStorage = {
    storage: "truncated",
    preview: [104, 105],
    original_size: 42,
  };

  it("decodes inline and preview bytes as UTF-8 text", () => {
    expect(bodyText(inline)).toBe("hi");
    expect(bodyText(preview)).toBe("hi");
  });

  it("reports empty and unavailable storage without decoding", () => {
    expect(bodyText({ storage: "empty" })).toBe("No body");
    expect(bodyText({ storage: "unavailable", reason: "encrypted" })).toBe(
      "encrypted",
    );
  });
});

describe("bounded text previews", () => {
  it("decodes only the configured prefix of an inline body", () => {
    const preview = bodyTextPreview(
      { storage: "inline", bytes: [97, 98, 99, 100, 101] },
      3,
    );

    expect(preview).toEqual({
      text: "abc",
      truncated: true,
      shown: 3,
      total: 5,
    });
  });

  it("preserves the original size reported by truncated native storage", () => {
    const preview = bodyTextPreview(
      { storage: "truncated", preview: [97, 98, 99], original_size: 10_000 },
      2,
    );

    expect(preview).toEqual({
      text: "ab",
      truncated: true,
      shown: 2,
      total: 10_000,
    });
  });

  it("bounds generated text such as cURL commands", () => {
    expect(textPreview("12345", 3)).toEqual({
      text: "123",
      truncated: true,
      shown: 3,
      total: 5,
    });
  });
});

describe("bodyImagePreviews", () => {
  const multipart = (mediaType: string, body: number[]) => {
    const prefix = new TextEncoder().encode(
      `--preview-boundary\r\nContent-Disposition: form-data; name="photo"; filename="avatar.png"\r\nContent-Type: ${mediaType}\r\n\r\n`,
    );
    const suffix = new TextEncoder().encode(
      "\r\n--preview-boundary--\r\n",
    );
    return [...prefix, ...body, ...suffix];
  };

  it("extracts a complete raster image from multipart form data", () => {
    const bytes = [137, 80, 78, 71, 0, 255];
    const previews = bodyImagePreviews(
      { storage: "inline", bytes: multipart("image/png", bytes) },
      "multipart/form-data; boundary=preview-boundary",
    );

    expect(previews).toEqual([{
      name: "avatar.png",
      mediaType: "image/png",
      byteLength: bytes.length,
      dataUrl: "data:image/png;base64,iVBORwD/",
    }]);
  });

  it("does not render SVG or an incomplete multipart image part", () => {
    expect(
      bodyImagePreviews(
        { storage: "inline", bytes: multipart("image/svg+xml", [60, 115, 118, 103, 62]) },
        "multipart/form-data; boundary=preview-boundary",
      ),
    ).toEqual([]);
    expect(
      bodyImagePreviews(
        { storage: "truncated", preview: multipart("image/png", [1, 2]).slice(0, -10), original_size: 100_000 },
        "multipart/form-data; boundary=preview-boundary",
      ),
    ).toEqual([]);
  });
});

describe("prettyJson", () => {
  it("pretty-prints valid JSON and leaves other text untouched", () => {
    expect(prettyJson('{"a":1}')).toBe('{\n  "a": 1\n}');
    expect(prettyJson("not json")).toBe("not json");
  });
});

describe("endpointId", () => {
  it("joins method, host, and path template", () => {
    const tx = transaction({
      endpoint_identity: {
        method: "POST",
        host: "api.example.test",
        path_template: "/v1/{id}",
      },
    });
    expect(endpointId(tx)).toBe("POST api.example.test /v1/{id}");
  });

  it("is undefined when the transaction has no endpoint identity", () => {
    expect(endpointId(transaction())).toBeUndefined();
  });
});

describe("curlCommand", () => {
  it("prefers the multi-line generated cURL command", () => {
    const tx = transaction({
      curl: {
        compact: "curl https://api.example.test",
        multiline: "curl \\\n+  https://api.example.test",
        redacted: true,
      },
    });
    expect(curlCommand(tx)).toBe("curl \\\n+  https://api.example.test");
  });

  it("uses the compact cURL command when multi-line output is unavailable", () => {
    expect(
      curlCommand(
        transaction({
          curl: {
            compact: "curl https://api.example.test",
            multiline: "",
            redacted: true,
          },
        }),
      ),
    ).toBe("curl https://api.example.test");
    expect(curlCommand(undefined)).toBeUndefined();
  });

  it("preserves auth tokens while redacting other legacy capture secrets", () => {
    const command = curlCommand(
      transaction({
        curl: {
          compact:
            "curl -H 'Authorization: Bearer real-secret' -H 'Proxy-Authorization: Basic proxy-token' -H 'Cookie: sid=cookie-secret' -H 'X-Api-Key: api-secret' -H 'X-Trace: public' https://api.example.test",
          multiline: "",
          redacted: false,
        },
      }),
    );

    expect(command).toContain("Authorization: Bearer real-secret");
    expect(command).toContain("Proxy-Authorization: Basic proxy-token");
    expect(command).not.toContain("cookie-secret");
    expect(command).not.toContain("api-secret");
    expect(command).toContain("Cookie: <redacted>");
    expect(command).toContain("X-Api-Key: <redacted>");
    expect(command).toContain("X-Trace: public");
  });
});

describe("timeLabel", () => {
  it("renders an ISO timestamp as a local time-of-day label", () => {
    const label = timeLabel("2026-07-24T10:30:45Z");
    expect(label).toMatch(/^\d{1,2}:\d{2}:\d{2} (AM|PM)$/);
  });
});

describe("transactionState priority", () => {
  it("treats an HTTP error as failed even when a comparison exists", () => {
    expect(
      transactionState(
        transaction({
          response: {
            status: 503,
            headers: [],
            body: { storage: "empty" },
            decoded_size: 0,
            encoded_size: 0,
            http_version: "HTTP_1_1",
          },
          comparison: {
            compatibility: "exact",
            differences: [
              {
                kind: "key_added",
                severity: "critical",
                ignored: false,
                explanation: "x",
              },
            ],
          },
        }),
      ),
    ).toBe("Failed");
  });

  it("reports pending while a response is missing even with an identity", () => {
    expect(
      transactionState(
        transaction({
          response: undefined,
          endpoint_identity: {
            method: "GET",
            host: "api.example.test",
            path_template: "/v1/{id}",
          },
        }),
      ),
    ).toBe("Pending");
  });
});

describe("bodyText storage variants", () => {
  it("decodes artifact previews and reports truncated size context", () => {
    const artifact: BodyStorage = {
      storage: "artifact",
      artifact_id: "artifact-id",
      preview: [115, 116, 117],
      original_size: 500,
    };
    expect(bodyText(artifact)).toBe("stu");
  });

  it("handles undefined and truncated storage without crashing", () => {
    expect(bodyText(undefined)).toBe("No body");
    const truncated: BodyStorage = {
      storage: "truncated",
      preview: [],
      original_size: 1234,
    };
    expect(bodyText(truncated)).toBe("");
  });
});

describe("prettyJson edge cases", () => {
  it("pretty-prints nested structures and preserves non-json strings", () => {
    expect(prettyJson('{"a":{"b":[1,2]}}')).toBe(
      '{\n  "a": {\n    "b": [\n      1,\n      2\n    ]\n  }\n}',
    );
    expect(prettyJson("")).toBe("");
    expect(prettyJson("{not json")).toBe("{not json");
  });
});

describe("endpointId edge cases", () => {
  it("returns undefined when endpoint_identity is null or missing fields", () => {
    expect(
      endpointId(transaction({ endpoint_identity: undefined })),
    ).toBeUndefined();
    expect(
      endpointId(
        transaction({
          endpoint_identity: {
            method: "GET",
            host: "api.example.test",
            path_template: "",
          },
        }),
      ),
    ).toBe("GET api.example.test ");
  });
});

describe("timeLabel robustness", () => {
  it("handles timestamps without a timezone offset", () => {
    const label = timeLabel("2026-07-24T10:30:45");
    expect(label).toMatch(/^\d{1,2}:\d{2}:\d{2} (AM|PM)$/);
  });
});

describe("elapsedLabel", () => {
  it("uses milliseconds below one second and seconds above", () => {
    expect(elapsedLabel(0)).toBe("0 ms");
    expect(elapsedLabel(999)).toBe("999 ms");
    expect(elapsedLabel(1000)).toBe("1.0 s");
    expect(elapsedLabel(2500)).toBe("2.5 s");
  });
});

describe("byteSizeLabel", () => {
  it("scales bytes to KB and MB with one decimal", () => {
    expect(byteSizeLabel(0)).toBe("0 B");
    expect(byteSizeLabel(1023)).toBe("1023 B");
    expect(byteSizeLabel(1024)).toBe("1.0 KB");
    expect(byteSizeLabel(1536)).toBe("1.5 KB");
    expect(byteSizeLabel(1024 * 1024)).toBe("1.0 MB");
    expect(byteSizeLabel(3.5 * 1024 * 1024)).toBe("3.5 MB");
  });
});

describe("methodTone", () => {
  it("maps methods to row-tone classes", () => {
    expect(methodTone("GET")).toBe("get");
    expect(methodTone("HEAD")).toBe("get");
    expect(methodTone("post")).toBe("post");
    expect(methodTone("PUT")).toBe("post");
    expect(methodTone("PATCH")).toBe("post");
    expect(methodTone("DELETE")).toBe("delete");
    expect(methodTone("BREW")).toBe("get");
  });
});

describe("manualRequestFromTransaction", () => {
  const transaction = (body: BodyStorage): HttpTransaction =>
    ({
      id: "t1",
      session_id: "s1",
      state: "completed",
      started_at: "",
      request: {
        method: "POST",
        scheme: "https",
        host: "api.test",
        path: "/v1/items?page=2",
        query: [{ name: "page", value: "2" }],
        headers: [
          { name: "Content-Type", value: "application/json; charset=utf-8" },
          { name: "X-Tenant", value: "acme" },
        ],
        body,
        http_version: "HTTP/1.1",
      },
      response: {
        status: 200,
        headers: [],
        body: { storage: "empty" },
        decoded_size: 0,
        encoded_size: 0,
        http_version: "HTTP/1.1",
      },
      endpoint: "api.test",
      method: "POST",
    }) as HttpTransaction;

  it("converts an inline JSON request into an editable composer request", () => {
    const request = manualRequestFromTransaction(
      transaction({
        storage: "inline",
        bytes: Array.from(new TextEncoder().encode('{"a":1}')),
      }),
    );
    expect(request.method).toBe("POST");
    expect(request.url).toBe("https://api.test/v1/items?page=2");
    expect(request.query).toEqual([]); // the URL already carries the query
    expect(request.headers).toEqual([
      { name: "Content-Type", value: "application/json; charset=utf-8" },
      { name: "X-Tenant", value: "acme" },
    ]);
    expect(request.body).toEqual({
      kind: "raw",
      media_type: "application/json",
      text: '{"a":1}',
    });
    expect(request.auth).toEqual({ kind: "none" });
  });

  it("leaves offloaded and empty bodies as none", () => {
    expect(
      manualRequestFromTransaction(
        transaction({
          storage: "artifact",
          artifact_id: "a1",
          preview: [],
          original_size: 1024,
        }),
      ).body,
    ).toEqual({ kind: "none" });
    expect(
      manualRequestFromTransaction(transaction({ storage: "empty" })).body,
    ).toEqual({
      kind: "none",
    });
  });
});
