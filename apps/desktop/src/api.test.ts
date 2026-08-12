import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import * as api from "./api";
import type { ManualRequest, SendOptions } from "./types";

const mockInvoke = vi.mocked(invoke);

describe("composer IPC argument shapes", () => {
  beforeEach(() => mockInvoke.mockReset());

  it("sends only the request and transport options", async () => {
    mockInvoke.mockResolvedValue({});
    const request: ManualRequest = {
      method: "GET",
      url: "https://api.test/v1",
      query: [],
      headers: [],
      body: { kind: "none" },
      auth: { kind: "none" },
    };
    const options: SendOptions = {
      follow_redirects: true,
      max_redirects: 10,
      timeout_ms: 5000,
      verify_tls: true,
      proxy_url: null,
    };

    await api.sendRequest(request, options);

    expect(mockInvoke).toHaveBeenCalledWith("send_request", { request, options });
  });

  it("generates cURL with the request and transport options", async () => {
    mockInvoke.mockResolvedValue("curl");
    const request: ManualRequest = {
      method: "GET",
      url: "https://api.test/v1",
      query: [],
      headers: [],
      body: { kind: "none" },
      auth: { kind: "none" },
    };
    const options: SendOptions = {
      follow_redirects: true,
      max_redirects: 5,
      timeout_ms: 30_000,
      verify_tls: true,
      proxy_url: null,
    };

    await api.generateComposerCurl(request, options);

    expect(mockInvoke).toHaveBeenCalledWith("generate_composer_curl", { request, options });
  });
});
