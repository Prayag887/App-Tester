// @vitest-environment jsdom
// UI behaviour tests for the composer itself: sending (Enter, ⌘↵),
// the save dialog (including creating a collection on the fly), and
// curl paste-to-import.
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ComposerView from "./ComposerView.svelte";
import * as api from "../api";
import { ui } from "../stores.svelte";
import type { ManualRequest, SendResult } from "../types";

vi.mock("../api", () => ({
  sendRequest: vi.fn(),
  parseCurl: vi.fn(),
  listCollections: vi.fn(),
  listVariables: vi.fn(),
  listEnvironments: vi.fn(),
  listHistory: vi.fn(),
  listRequests: vi.fn(),
  getRequest: vi.fn(),
  saveRequest: vi.fn(),
  createCollection: vi.fn(),
  renameCollection: vi.fn(),
  deleteCollection: vi.fn(),
  deleteRequest: vi.fn(),
  getHistoryRequest: vi.fn(),
  deleteHistory: vi.fn(),
  clearHistory: vi.fn(),
  saveVariable: vi.fn(),
  deleteVariable: vi.fn(),
  createEnvironment: vi.fn(),
  renameEnvironment: vi.fn(),
  deleteEnvironment: vi.fn(),
  pickFile: vi.fn(),
}));

const mockedApi = vi.mocked(api);

const sendResult: SendResult = {
  transaction_id: "t1",
  state: "completed",
  status: 200,
  reason: "OK",
  elapsed_ms: 12,
  total_bytes: 2,
  body: { storage: "inline", bytes: [123, 125] },
  content_type: "application/json",
  headers: [{ name: "Content-Type", value: "application/json" }],
  http_version: "HTTP/1.1",
};

const collection = {
  id: "c1",
  name: "Payments",
  description: "",
  color: "",
  request_count: 0,
  created_at: "",
  updated_at: "",
};

beforeEach(() => {
  vi.clearAllMocks();
  ui.notice = "";
  ui.composerDraft = null;
  mockedApi.listCollections.mockResolvedValue([collection]);
  mockedApi.listVariables.mockResolvedValue([]);
  mockedApi.listEnvironments.mockResolvedValue([]);
  mockedApi.listHistory.mockResolvedValue([]);
  mockedApi.sendRequest.mockResolvedValue(sendResult);
});

function urlInput(): HTMLInputElement {
  return screen.getByLabelText("Request URL") as HTMLInputElement;
}

describe("ComposerView", () => {
  it("sends the request when Enter is pressed in the URL bar", async () => {
    const user = userEvent.setup();
    render(ComposerView);
    await waitFor(() => expect(mockedApi.listVariables).toHaveBeenCalled());

    await user.type(urlInput(), "https://api.test/v1");
    await user.keyboard("{Enter}");

    await waitFor(() => expect(mockedApi.sendRequest).toHaveBeenCalled());
    const request = mockedApi.sendRequest.mock.calls[0][0];
    expect(request.url).toBe("https://api.test/v1");
    expect(request.method).toBe("GET");
  });

  it("sends the request with ⌘↵ (Ctrl+Enter) from the URL bar", async () => {
    const user = userEvent.setup();
    render(ComposerView);
    await waitFor(() => expect(mockedApi.listVariables).toHaveBeenCalled());

    await user.type(urlInput(), "https://api.test/v1");
    await user.keyboard("{Control>}{Enter}{/Control}");

    await waitFor(() => expect(mockedApi.sendRequest).toHaveBeenCalled());
  });

  it("opens the save dialog with ⌘S and saves into the chosen collection", async () => {
    const user = userEvent.setup();
    mockedApi.saveRequest.mockImplementation(async (_id, _collectionId, name, _request) => ({
      id: "r1",
      collection_id: "c1",
      name,
      request: { method: "GET", url: "https://api.test/v1", query: [], headers: [], body: { kind: "none" }, auth: { kind: "none" } },
      created_at: "",
      updated_at: "",
    }));
    render(ComposerView);
    await waitFor(() => expect(mockedApi.listVariables).toHaveBeenCalled());

    await user.type(urlInput(), "https://api.test/v1");
    await user.keyboard("{Control>}s{/Control}");

    const dialog = screen.getByRole("dialog", { name: "Save request" });
    expect(dialog).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(mockedApi.saveRequest).toHaveBeenCalled());
    const [id, collectionId, name, request] = mockedApi.saveRequest.mock.calls[0];
    expect(id).toBeNull();
    expect(collectionId).toBe("c1");
    expect(name).toBe("v1");
    expect((request as ManualRequest).url).toBe("https://api.test/v1");
  });

  it("creates a new collection from the save dialog when none exists", async () => {
    const user = userEvent.setup();
    mockedApi.listCollections.mockResolvedValue([]);
    mockedApi.createCollection.mockResolvedValue({ ...collection, id: "c2", name: "API" });
    mockedApi.saveRequest.mockResolvedValue({
      id: "r1",
      collection_id: "c2",
      name: "v1",
      request: { method: "GET", url: "https://api.test/v1", query: [], headers: [], body: { kind: "none" }, auth: { kind: "none" } },
      created_at: "",
      updated_at: "",
    });
    render(ComposerView);
    await waitFor(() => expect(mockedApi.listVariables).toHaveBeenCalled());

    await user.type(urlInput(), "https://api.test/v1");
    await user.click(screen.getByRole("button", { name: "Save request" }));
    await user.selectOptions(screen.getByLabelText("Collection"), "__new__");
    await user.type(screen.getByPlaceholderText("Collection name"), "API");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(mockedApi.createCollection).toHaveBeenCalledWith("API"));
    await waitFor(() => expect(mockedApi.saveRequest).toHaveBeenCalled());
    const [, collectionId] = mockedApi.saveRequest.mock.calls[0];
    expect(collectionId).toBe("c2");
  });

  it("imports a pasted curl command into the composer", async () => {
    const user = userEvent.setup();
    mockedApi.parseCurl.mockResolvedValue({
      request: {
        method: "POST",
        url: "https://api.test/items",
        query: [],
        headers: [{ name: "X-Tenant", value: "acme" }],
        body: { kind: "raw", media_type: "application/json", text: "{\"a\":1}" },
        auth: { kind: "bearer", token: "tok" },
      },
      options: {
        follow_redirects: true,
        max_redirects: 10,
        timeout_ms: 5000,
        verify_tls: true,
        proxy_url: null,
      },
    });
    render(ComposerView);
    await waitFor(() => expect(mockedApi.listVariables).toHaveBeenCalled());

    const input = urlInput();
    fireEvent.paste(input, {
      clipboardData: { getData: () => "curl -X POST https://api.test/items" },
    });

    await waitFor(() => expect(mockedApi.parseCurl).toHaveBeenCalled());
    await waitFor(() => expect(input.value).toBe("https://api.test/items"));
  });

  it("shows a clear error when sending without a URL", async () => {
    const user = userEvent.setup();
    render(ComposerView);
    await waitFor(() => expect(mockedApi.listVariables).toHaveBeenCalled());

    await user.click(screen.getByRole("button", { name: "Send" }));
    expect(screen.getByText("Enter a URL to send.")).toBeTruthy();
    expect(mockedApi.sendRequest).not.toHaveBeenCalled();
  });
});
