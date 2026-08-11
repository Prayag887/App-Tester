// @vitest-environment jsdom
// UI behaviour tests for the environments manager: adding variables,
// secret masking, creating environments, and the batch-save flow. These
// cover the reported "can't add env variables / not saving" regressions.
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import EnvironmentsDialog from "./EnvironmentsDialog.svelte";
import * as api from "../api";

vi.mock("../api", () => ({
  listEnvironments: vi.fn(),
  listVariables: vi.fn(),
  saveVariable: vi.fn(),
  deleteVariable: vi.fn(),
  createEnvironment: vi.fn(),
  renameEnvironment: vi.fn(),
  deleteEnvironment: vi.fn(),
}));

const mockedApi = vi.mocked(api);

function renderDialog() {
  const onClose = vi.fn();
  const onSaved = vi.fn();
  const onNotice = vi.fn();
  render(EnvironmentsDialog, {
    props: { open: true, onClose, onSaved, onNotice },
  });
  return { onClose, onSaved, onNotice };
}

const variableRow = { id: "v1", environment_id: null, name: "host", value: "api.test", is_secret: false, created_at: "", updated_at: "" };

beforeEach(() => {
  vi.clearAllMocks();
  mockedApi.listEnvironments.mockResolvedValue([]);
  mockedApi.listVariables.mockResolvedValue([]);
  mockedApi.saveVariable.mockImplementation(async (_id, _env, variable) => ({
    id: "new-id",
    environment_id: null,
    ...variable,
    created_at: "",
    updated_at: "",
  }));
});

describe("EnvironmentsDialog", () => {
  it("adds a variable and saves it into the global scope", async () => {
    const user = userEvent.setup();
    const { onClose, onSaved } = renderDialog();
    await waitFor(() => expect(mockedApi.listVariables).toHaveBeenCalled());

    await user.click(screen.getByRole("button", { name: "Add variable" }));
    await user.type(screen.getByPlaceholderText("name"), "host");
    await user.type(screen.getByPlaceholderText("value"), "api.test");
    await user.click(screen.getByRole("button", { name: "Save changes" }));

    await waitFor(() =>
      expect(mockedApi.saveVariable).toHaveBeenCalledWith(null, null, {
        name: "host",
        value: "api.test",
        is_secret: false,
      }),
    );
    expect(onSaved).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("persists the secret flag when the eye toggle is used", async () => {
    const user = userEvent.setup();
    renderDialog();
    await waitFor(() => expect(mockedApi.listVariables).toHaveBeenCalled());

    await user.click(screen.getByRole("button", { name: "Add variable" }));
    await user.type(screen.getByPlaceholderText("name"), "token");
    await user.type(screen.getByPlaceholderText("value"), "s3cret");
    await user.click(screen.getByRole("button", { name: "Toggle secret" }));
    await user.click(screen.getByRole("button", { name: "Save changes" }));

    await waitFor(() =>
      expect(mockedApi.saveVariable).toHaveBeenCalledWith(null, null, {
        name: "token",
        value: "s3cret",
        is_secret: true,
      }),
    );
  });

  it("creates an environment and switches to its scope", async () => {
    const user = userEvent.setup();
    mockedApi.createEnvironment.mockResolvedValue({
      id: "env-1",
      name: "Dev",
      variable_count: 0,
      created_at: "",
      updated_at: "",
    });
    renderDialog();
    await waitFor(() => expect(mockedApi.listEnvironments).toHaveBeenCalled());

    await user.click(screen.getByRole("button", { name: "New environment" }));
    await user.type(screen.getByPlaceholderText("Environment name"), "Dev");
    await user.keyboard("{Enter}");

    await waitFor(() => expect(mockedApi.createEnvironment).toHaveBeenCalledWith("Dev"));
    await waitFor(() =>
      expect(mockedApi.listVariables).toHaveBeenCalledWith("env-1"),
    );
    expect(screen.getByText("Dev")).toBeTruthy();
    expect(screen.getByText("Dev variables")).toBeTruthy();
  });

  it("deletes an existing variable only after saving", async () => {
    const user = userEvent.setup();
    mockedApi.listVariables.mockResolvedValue([variableRow]);
    const { onNotice } = renderDialog();
    await waitFor(() => expect(mockedApi.listVariables).toHaveBeenCalled());

    await user.click(screen.getByRole("button", { name: "Remove variable" }));
    // Row is marked for removal but nothing reaches the backend yet.
    expect(mockedApi.deleteVariable).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Save changes" }));

    await waitFor(() => expect(mockedApi.deleteVariable).toHaveBeenCalledWith("v1"));
    expect(onNotice).toHaveBeenCalledWith("Variables saved.");
  });
});
