// @vitest-environment jsdom
// UI behaviour tests for the composer's library pane: creating collections,
// expanding to load saved requests, opening history entries, and the
// environments manager entry point.
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ComposerLibrary from "./ComposerLibrary.svelte";
import * as api from "../api";
import type { ManualRequest } from "../types";

vi.mock("../api", () => ({
  listCollections: vi.fn(),
  listEnvironments: vi.fn(),
  listHistory: vi.fn(),
  listRequests: vi.fn(),
  getRequest: vi.fn(),
  createCollection: vi.fn(),
  renameCollection: vi.fn(),
  deleteCollection: vi.fn(),
  deleteRequest: vi.fn(),
  getHistoryRequest: vi.fn(),
  deleteHistory: vi.fn(),
  clearHistory: vi.fn(),
  listVariables: vi.fn(),
  saveVariable: vi.fn(),
  deleteVariable: vi.fn(),
  createEnvironment: vi.fn(),
  renameEnvironment: vi.fn(),
  deleteEnvironment: vi.fn(),
}));

const mockedApi = vi.mocked(api);

function renderLibrary() {
  const onLoadRequest = vi.fn();
  const onActiveEnvironmentChange = vi.fn();
  const onVariablesSaved = vi.fn();
  const onNotice = vi.fn();
  render(ComposerLibrary, {
    props: {
      loadedRequestId: "",
      activeEnvironmentId: "",
      refreshToken: 0,
      onLoadRequest,
      onActiveEnvironmentChange,
      onVariablesSaved,
      onNotice,
    },
  });
  return { onLoadRequest, onActiveEnvironmentChange, onVariablesSaved, onNotice };
}

const collection = {
  id: "c1",
  name: "Payments",
  description: "",
  color: "",
  request_count: 2,
  created_at: "",
  updated_at: "",
};

const summary = {
  id: "r1",
  collection_id: "c1",
  name: "Create payment",
  method: "POST",
  url: "https://api.test/payments",
  created_at: "",
  updated_at: "",
};

const request: ManualRequest = {
  method: "POST",
  url: "https://api.test/payments",
  query: [],
  headers: [],
  body: { kind: "none" },
  auth: { kind: "none" },
};

beforeEach(() => {
  vi.clearAllMocks();
  mockedApi.listCollections.mockResolvedValue([]);
  mockedApi.listEnvironments.mockResolvedValue([]);
  mockedApi.listHistory.mockResolvedValue([]);
  mockedApi.listVariables.mockResolvedValue([]);
});

describe("ComposerLibrary", () => {
  it("creates a collection and shows it in the list", async () => {
    const user = userEvent.setup();
    mockedApi.createCollection.mockResolvedValue({ ...collection, request_count: 0 });
    renderLibrary();
    await waitFor(() => expect(mockedApi.listCollections).toHaveBeenCalled());

    await user.click(screen.getByRole("button", { name: "New collection" }));
    await user.type(screen.getByPlaceholderText("Collection name"), "Payments");
    await user.keyboard("{Enter}");

    await waitFor(() =>
      expect(mockedApi.createCollection).toHaveBeenCalledWith("Payments"),
    );
    expect(screen.getByText("Payments")).toBeTruthy();
  });

  it("expanding a collection loads its requests and opens one on click", async () => {
    const user = userEvent.setup();
    mockedApi.listCollections.mockResolvedValue([collection]);
    mockedApi.listRequests.mockResolvedValue([summary]);
    mockedApi.getRequest.mockResolvedValue({
      ...summary,
      request,
    });
    const { onLoadRequest } = renderLibrary();
    await waitFor(() => expect(mockedApi.listCollections).toHaveBeenCalled());

    await user.click(screen.getByText("Payments"));
    await waitFor(() => expect(mockedApi.listRequests).toHaveBeenCalledWith("c1"));

    await user.click(screen.getByText("Create payment"));
    await waitFor(() => expect(mockedApi.getRequest).toHaveBeenCalledWith("r1"));
    expect(onLoadRequest).toHaveBeenCalledWith(request, "r1", "c1");
  });

  it("opens a history entry into the composer", async () => {
    const user = userEvent.setup();
    mockedApi.listHistory.mockResolvedValue([
      { id: "h1", method: "GET", url: "https://api.test/v1", status: 200, sent_at: "2026-08-05T10:00:00Z" },
    ]);
    mockedApi.getHistoryRequest.mockResolvedValue(request);
    const { onLoadRequest } = renderLibrary();
    await waitFor(() => expect(mockedApi.listHistory).toHaveBeenCalled());

    await user.click(screen.getByText("https://api.test/v1"));
    await waitFor(() => expect(mockedApi.getHistoryRequest).toHaveBeenCalledWith("h1"));
    expect(onLoadRequest).toHaveBeenCalledWith(request, "", "");
  });

  it("clears history from the heading action", async () => {
    const user = userEvent.setup();
    mockedApi.listHistory.mockResolvedValue([
      { id: "h1", method: "GET", url: "https://api.test/v1", status: 200, sent_at: "2026-08-05T10:00:00Z" },
    ]);
    renderLibrary();
    await waitFor(() => expect(mockedApi.listHistory).toHaveBeenCalled());

    await user.click(screen.getByRole("button", { name: "Clear history" }));
    await waitFor(() => expect(mockedApi.clearHistory).toHaveBeenCalled());
  });

  it("opens the environments manager from the settings button", async () => {
    const user = userEvent.setup();
    renderLibrary();
    await waitFor(() => expect(mockedApi.listEnvironments).toHaveBeenCalled());

    await user.click(screen.getByRole("button", { name: "Manage environments" }));
    expect(screen.getByRole("dialog", { name: "Environments" })).toBeTruthy();
  });
});
