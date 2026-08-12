// @vitest-environment jsdom
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ComposerView from "./ComposerView.svelte";
import * as api from "../api";
import { ui } from "../stores.svelte";
import type { SendResult } from "../types";

vi.mock("../api", () => ({
  sendRequest: vi.fn(),
  generateComposerCurl: vi.fn(),
  parseCurl: vi.fn(),
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

beforeEach(() => {
  vi.clearAllMocks();
  ui.notice = "";
  ui.composerDraft = null;
  mockedApi.sendRequest.mockResolvedValue(sendResult);
  mockedApi.generateComposerCurl.mockResolvedValue("curl --request GET --url 'https://api.test/v1'");
});

const urlInput = () => screen.getByLabelText("Request URL") as HTMLInputElement;

describe("ComposerView", () => {
  it("sends the request when Enter is pressed in the URL bar", async () => {
    const user = userEvent.setup();
    render(ComposerView);
    await user.type(urlInput(), "https://api.test/v1");
    await user.keyboard("{Enter}");

    await waitFor(() => expect(mockedApi.sendRequest).toHaveBeenCalled());
    const [request, options] = mockedApi.sendRequest.mock.calls[0];
    expect(request.url).toBe("https://api.test/v1");
    expect(request.method).toBe("GET");
    expect(options.timeout_ms).toBe(30_000);
  });

  it("sends the request with Ctrl+Enter", async () => {
    const user = userEvent.setup();
    render(ComposerView);
    await user.type(urlInput(), "https://api.test/v1");
    await user.keyboard("{Control>}{Enter}{/Control}");
    await waitFor(() => expect(mockedApi.sendRequest).toHaveBeenCalled());
  });

  it("imports a pasted curl command into the composer", async () => {
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
    fireEvent.paste(urlInput(), {
      clipboardData: { getData: () => "curl -X POST https://api.test/items" },
    });

    await waitFor(() => expect(mockedApi.parseCurl).toHaveBeenCalled());
    await waitFor(() => expect(urlInput().value).toBe("https://api.test/items"));
  });

  it("shows a clear error when sending without a URL", async () => {
    const user = userEvent.setup();
    render(ComposerView);
    await user.click(screen.getByRole("button", { name: "Send" }));
    expect(screen.getByText("Enter a URL to send.")).toBeTruthy();
    expect(mockedApi.sendRequest).not.toHaveBeenCalled();
  });

  it("copies the current request as cURL", async () => {
    const user = userEvent.setup();
    const writeText = vi.spyOn(navigator.clipboard, "writeText");
    render(ComposerView);
    await user.type(urlInput(), "https://api.test/v1");
    await user.click(screen.getByRole("button", { name: "Copy request as cURL" }));

    await waitFor(() => expect(mockedApi.generateComposerCurl).toHaveBeenCalled());
    expect(mockedApi.generateComposerCurl.mock.calls[0][0].url).toBe("https://api.test/v1");
    expect(writeText).toHaveBeenCalledWith("curl --request GET --url 'https://api.test/v1'");
    expect(screen.getByRole("button", { name: "cURL copied" })).toBeTruthy();
  });

  it("does not expose collections, environments, or request history", () => {
    render(ComposerView);
    expect(screen.queryByText("Collections")).toBeNull();
    expect(screen.queryByText("History")).toBeNull();
    expect(screen.queryByLabelText("Manage environments")).toBeNull();
    expect(screen.queryByLabelText("Save request")).toBeNull();
  });
});
