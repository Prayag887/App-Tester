// Guards the Tauri IPC contract: command arguments bind in camelCase only
// (tauri-macros converts every arg key), while nested request/variable
// structs keep their serde snake_case fields. A snake_case top-level key
// (e.g. is_secret) silently fails to bind and the invoke rejects — this
// exact bug broke environment-variable saving.
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import * as api from "./api";
import type { ManualRequest, SendOptions } from "./types";

const mockInvoke = vi.mocked(invoke);

const request: ManualRequest = {
  method: "POST",
  url: "https://api.test/v1",
  query: [],
  headers: [],
  body: { kind: "none" },
  auth: { kind: "none" },
};

const getRequest: ManualRequest = {
  ...request,
  method: "GET",
};

describe("api IPC argument shapes", () => {
  beforeEach(() => mockInvoke.mockReset());

  it("save_variable: camelCase top-level args (isSecret), plain variable payload", async () => {
    mockInvoke.mockResolvedValue({});
    await api.saveVariable(null, "env-1", {
      name: "host",
      value: "api.test",
      is_secret: true,
    });
    expect(mockInvoke).toHaveBeenCalledWith("save_variable", {
      id: null,
      environmentId: "env-1",
      name: "host",
      value: "api.test",
      isSecret: true,
    });
  });

  it("save_request: camelCase collectionId, request serialized as-is", async () => {
    mockInvoke.mockResolvedValue({});
    await api.saveRequest(null, "c-1", "Create item", request);
    expect(mockInvoke).toHaveBeenCalledWith("save_request", {
      id: null,
      collectionId: "c-1",
      name: "Create item",
      request,
    });
  });

  it("list_variables: environmentId maps to environment_id", async () => {
    mockInvoke.mockResolvedValue([]);
    await api.listVariables("env-1");
    expect(mockInvoke).toHaveBeenCalledWith("list_variables", {
      environmentId: "env-1",
    });
    await api.listVariables(null);
    expect(mockInvoke).toHaveBeenCalledWith("list_variables", {
      environmentId: null,
    });
  });

  it("send_request: request/options/variables are single-word top-level args", async () => {
    mockInvoke.mockResolvedValue({});
    const options: SendOptions = {
      follow_redirects: true,
      max_redirects: 10,
      timeout_ms: 5000,
      verify_tls: true,
      proxy_url: null,
    };
    await api.sendRequest(getRequest, options, [
      { name: "host", value: "x", is_secret: false },
    ]);
    expect(mockInvoke).toHaveBeenCalledWith("send_request", {
      request: getRequest,
      options,
      variables: [{ name: "host", value: "x", is_secret: false }],
    });
  });
});
