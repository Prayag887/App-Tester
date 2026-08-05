<script lang="ts">
  // The manual request composer: build a request, send it through the native
  // engine, and inspect the response. Sent requests also appear in the
  // Traffic lab, because the engine records them like captured traffic.
  // Pasting a curl command (in the URL bar or the import panel) fills the
  // whole composer from the native parser.
  import { onMount } from "svelte";
  import { Check, Plus, Save, Send, TerminalSquare, Trash2 } from "lucide-svelte";
  import * as api from "../api";
  import { byteSizeLabel, elapsedLabel, prettyJson, unresolvedVariables } from "../lib";
  import { ui } from "../stores.svelte";
  import ComposerLibrary from "./ComposerLibrary.svelte";
  import type {
    AuthSpec,
    CollectionSummary,
    HeaderEntry,
    ManualBody,
    ManualRequest,
    MultipartField,
    QueryParameter,
    SendOptions,
    SendResult,
    Variable,
  } from "../types";

  const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
  const MEDIA_TYPES = [
    "application/json",
    "application/xml",
    "text/plain",
    "application/x-www-form-urlencoded",
  ];
  const DEFAULT_OPTIONS = {
    timeout_ms: 30_000,
    follow_redirects: true,
    max_redirects: 5,
    verify_tls: true,
    proxy_url: null,
  };

  let method = $state("GET");
  let url = $state("");
  let params = $state<QueryParameter[]>([{ name: "", value: "" }]);
  let headers = $state<HeaderEntry[]>([{ name: "", value: "" }]);
  let tab = $state<"params" | "headers" | "body" | "auth">("params");
  let bodyKind = $state<"none" | "form" | "raw" | "multipart">("none");
  let formFields = $state<QueryParameter[]>([{ name: "", value: "" }]);
  let multipartFields = $state<MultipartField[]>([{ name: "", value: "" }]);
  let rawText = $state("");
  let rawMediaType = $state("application/json");
  let authKind = $state<AuthSpec["kind"]>("none");
  let bearerToken = $state("");
  let basicUsername = $state("");
  let basicPassword = $state("");
  let apiKeyName = $state("");
  let apiKeyValue = $state("");
  // Transport settings carried by an imported curl command (`-k`, `-m`, …).
  let optionsOverride = $state<SendOptions | null>(null);
  // The saved request currently loaded in the composer (update on save).
  let loadedRequestId = $state("");
  let loadedCollectionId = $state("");

  let curlOpen = $state(false);
  let curlText = $state("");
  let curlTextarea: HTMLTextAreaElement | undefined;

  let saveOpen = $state(false);
  let saveName = $state("");
  let saveCollectionId = $state("");
  let saveCollections = $state<CollectionSummary[]>([]);
  let saveNewName = $state("");
  let saveBusy = $state(false);

  // Environment variables: global + active environment, environment wins.
  let activeEnvironmentId = $state("");
  let variables = $state<Variable[]>([]);
  let pickerOpen = $state(false);
  // Bumped after every send so the library's history list stays fresh.
  let historyRefresh = $state(0);
  // Flashes green on the Send button after a successful send.
  let sendFlash = $state(false);

  let busy = $state(false);
  let response = $state<SendResult | null>(null);
  let error = $state("");
  let responseTab = $state<"body" | "headers">("body");
  let pretty = $state(true);
  let urlInput: HTMLInputElement | undefined;

  onMount(() => {
    // A request handed over from another screen ("Send in Composer").
    if (ui.composerDraft) {
      fillRequest(ui.composerDraft);
      ui.composerDraft = null;
      loadedRequestId = "";
      loadedCollectionId = "";
      ui.notice = "Opened in the composer — review, then send.";
    }
    urlInput?.focus();
    void loadVariables();
  });

  async function loadVariables() {
    try {
      const [globalVariables, environmentVariables] = await Promise.all([
        api.listVariables(null),
        api.listVariables(activeEnvironmentId || null),
      ]);
      const byName = new Map<string, Variable>();
      for (const variable of globalVariables) {
        byName.set(variable.name, variable);
      }
      for (const variable of environmentVariables) {
        byName.set(variable.name, variable); // environment wins
      }
      variables = [...byName.values()];
    } catch (cause) {
      error = `Could not load variables: ${String(cause)}`;
    }
  }

  async function changeEnvironment(id: string) {
    activeEnvironmentId = id;
    await loadVariables();
  }

  /// Every `{{name}}` used anywhere in the request that no variable
  /// satisfies, deduplicated — shown as a warning strip before sending.
  const unresolved = $derived.by(() => {
    const known = variables.map((variable) => variable.name);
    const texts = [
      url,
      ...params.map((entry) => `${entry.name} ${entry.value}`),
      ...headers.map((entry) => `${entry.name} ${entry.value}`),
      rawText,
      ...formFields.map((field) => `${field.name} ${field.value}`),
      ...multipartFields.map((field) => `${field.name} ${field.value ?? ""} ${field.file ?? ""}`),
      bearerToken,
      `${basicUsername} ${basicPassword}`,
      `${apiKeyName} ${apiKeyValue}`,
    ];
    return [...new Set(texts.flatMap((text) => unresolvedVariables(text, known)))];
  });

  function insertVariable(name: string) {
    const element = document.activeElement;
    if (!(element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement)) {
      ui.notice = "Click a request field first, then pick a variable.";
      return;
    }
    const start = element.selectionStart ?? element.value.length;
    const end = element.selectionEnd ?? start;
    const token = `{{${name}}}`;
    element.value = element.value.slice(0, start) + token + element.value.slice(end);
    const caret = start + token.length;
    element.setSelectionRange(caret, caret);
    element.dispatchEvent(new Event("input", { bubbles: true }));
    pickerOpen = false;
  }

  const responseText = $derived.by(() => {
    const body = response?.body;
    if (!body || body.storage === "empty") return "";
    if (body.storage === "unavailable") return body.reason;
    const bytes = body.storage === "inline" ? body.bytes : body.preview;
    return new TextDecoder().decode(new Uint8Array(bytes));
  });
  const responsePretty = $derived(prettyJson(responseText));

  function wireBody(): ManualBody {
    if (bodyKind === "form") {
      return {
        kind: "form",
        fields: formFields
          .filter((field) => field.name.trim() !== "")
          .map((field) => [field.name.trim(), field.value]),
      };
    }
    if (bodyKind === "multipart") {
      return {
        kind: "multipart",
        fields: multipartFields.filter((field) => field.name.trim() !== ""),
      };
    }
    if (bodyKind === "raw") {
      return { kind: "raw", media_type: rawMediaType, text: rawText };
    }
    return { kind: "none" };
  }

  function wireAuth(): AuthSpec {
    switch (authKind) {
      case "bearer":
        return { kind: "bearer", token: bearerToken };
      case "basic":
        return { kind: "basic", username: basicUsername, password: basicPassword };
      case "api_key":
        return { kind: "api_key", key: apiKeyName, value: apiKeyValue, in_query: false };
      default:
        return { kind: "none" };
    }
  }

  function wireRequest(): ManualRequest {
    return {
      method,
      url: url.trim(),
      query: params.filter((entry) => entry.name.trim() !== ""),
      headers: headers.filter((entry) => entry.name.trim() !== ""),
      body: wireBody(),
      auth: wireAuth(),
    };
  }

  async function send() {
    if (busy) return;
    if (!url.trim()) {
      error = "Enter a URL to send.";
      return;
    }
    busy = true;
    error = "";
    try {
      response = await api.sendRequest(
        wireRequest(),
        optionsOverride ?? DEFAULT_OPTIONS,
        variables,
      );
      responseTab = "body";
      pretty = true;
      historyRefresh += 1;
      // Brief success flash on the Send button.
      sendFlash = true;
      window.setTimeout(() => (sendFlash = false), 600);
    } catch (cause) {
      response = null;
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (!(event.metaKey || event.ctrlKey)) return;
    if (event.key === "Enter") {
      event.preventDefault();
      if (event.target === curlTextarea) {
        void applyCurl(curlText);
        return;
      }
      void send();
      return;
    }
    if (event.key.toLowerCase() === "s") {
      event.preventDefault();
      void openSaveDialog();
    }
  }

  /// Maps a parsed request onto every composer tab. Shared by curl imports
  /// and saved-request loads so both paths behave identically.
  function fillRequest(request: ManualRequest) {
    method = request.method;
    url = request.url;
    params = request.query.length
      ? request.query.map((entry) => ({ ...entry }))
      : [{ name: "", value: "" }];
    headers = request.headers.length
      ? request.headers.map((entry) => ({ ...entry }))
      : [{ name: "", value: "" }];
    const body = request.body;
    if (body.kind === "form") {
      bodyKind = "form";
      formFields = body.fields.map(([name, value]) => ({ name, value }));
      multipartFields = [{ name: "", value: "" }];
      rawText = "";
    } else if (body.kind === "multipart") {
      bodyKind = "multipart";
      multipartFields = body.fields.map((field) => ({ ...field }));
      formFields = [{ name: "", value: "" }];
      rawText = "";
    } else if (body.kind === "raw") {
      bodyKind = "raw";
      rawText = body.text;
      rawMediaType = body.media_type ?? "text/plain";
      formFields = [{ name: "", value: "" }];
      multipartFields = [{ name: "", value: "" }];
    } else {
      bodyKind = "none";
      formFields = [{ name: "", value: "" }];
      multipartFields = [{ name: "", value: "" }];
      rawText = "";
    }
    const auth = request.auth;
    if (auth.kind === "bearer") {
      authKind = "bearer";
      bearerToken = auth.token;
    } else if (auth.kind === "basic") {
      authKind = "basic";
      basicUsername = auth.username;
      basicPassword = auth.password;
    } else if (auth.kind === "api_key") {
      authKind = "api_key";
      apiKeyName = auth.key;
      apiKeyValue = auth.value;
    } else {
      authKind = "none";
      bearerToken = "";
      basicUsername = "";
      basicPassword = "";
      apiKeyName = "";
      apiKeyValue = "";
    }
  }

  /// Fills the composer from a parsed curl command. Every field the parser
  /// produces is mapped onto the tabs so the user sees exactly what will
  /// be sent; the imported transport options (`-k`, `-m`, …) are kept.
  async function applyCurl(text: string) {
    if (!text.trim()) return;
    try {
      const imported = await api.parseCurl(text);
      fillRequest(imported.request);
      optionsOverride = imported.options;
      loadedRequestId = "";
      loadedCollectionId = "";
      curlOpen = false;
      error = "";
      ui.notice = "Imported from curl — review, then send.";
    } catch (cause) {
      error = `Could not parse curl: ${String(cause)}`;
    }
  }

  function loadSaved(request: ManualRequest, id: string, collectionId: string) {
    fillRequest(request);
    loadedRequestId = id;
    loadedCollectionId = collectionId;
    error = "";
    ui.notice = "Loaded — review, then send.";
  }

  async function openSaveDialog() {
    if (!url.trim()) {
      error = "Enter a URL before saving.";
      return;
    }
    try {
      saveCollections = await api.listCollections();
      saveName =
        url
          .split("?")[0]
          .split("/")
          .filter(Boolean)
          .pop() ?? `${method} ${url}`;
      saveCollectionId =
        loadedCollectionId ||
        saveCollections[0]?.id ||
        "";
      saveOpen = true;
    } catch (cause) {
      error = `Could not load collections: ${String(cause)}`;
    }
  }

  async function saveCurrent() {
    const name = saveName.trim();
    if (!name) {
      error = "Enter a name for the request.";
      return;
    }
    let collectionId = saveCollectionId;
    if (collectionId === "__new__") {
      const collectionName = saveNewName.trim();
      if (!collectionName) {
        error = "Enter a name for the new collection.";
        return;
      }
      saveBusy = true;
      try {
        const created = await api.createCollection(collectionName);
        collectionId = created.id;
      } catch (cause) {
        error = `Could not create the collection: ${String(cause)}`;
        saveBusy = false;
        return;
      }
    }
    if (!collectionId) {
      error = "Choose a collection to save into.";
      return;
    }
    saveBusy = true;
    try {
      const saved = await api.saveRequest(
        loadedRequestId || null,
        collectionId,
        name,
        wireRequest(),
      );
      loadedRequestId = saved.id;
      loadedCollectionId = saved.collection_id;
      saveOpen = false;
      error = "";
      ui.notice = `Saved "${saved.name}".`;
    } catch (cause) {
      error = `Could not save: ${String(cause)}`;
    } finally {
      saveBusy = false;
    }
  }

  async function pickMultipartFile(index: number) {
    try {
      const path = await api.pickFile();
      if (path) {
        multipartFields[index].file = path;
        multipartFields[index].value = undefined;
      }
    } catch (cause) {
      error = `Could not pick a file: ${String(cause)}`;
    }
  }

  const setTab = (next: typeof tab) => (tab = next);
  const setResponseTab = (next: typeof responseTab) => (responseTab = next);
</script>

<svelte:window onkeydown={onKeydown} />

{#snippet kvRows(rows: QueryParameter[])}
  {#each rows as row, index (index)}
    <div class="kv-row">
      <input
        value={row.name}
        placeholder="Name"
        oninput={(event) =>
          (rows[index].name = (event.target as HTMLInputElement).value)}
      />
      <input
        value={row.value}
        placeholder="Value"
        oninput={(event) =>
          (rows[index].value = (event.target as HTMLInputElement).value)}
      />
      <button
        class="icon-button"
        aria-label="Remove row"
        onclick={() => rows.splice(index, 1)}
      ><Trash2 size={13} /></button>
    </div>
  {/each}
  <button class="add-row" onclick={() => rows.push({ name: "", value: "" })}>
    <Plus size={13} /> Add
  </button>
{/snippet}

<section class="hero composer-hero">
  <div>
    <span>Composer</span>
    <h1>Build and send requests.</h1>
    <p>Manual requests land in the Traffic lab like captured ones.</p>
  </div>
  <div class="hero-host"><small>SHORTCUT</small><b>⌘ ↵ to send</b></div>
</section>

<section class="composer">
  <ComposerLibrary
    loadedRequestId={loadedRequestId}
    activeEnvironmentId={activeEnvironmentId}
    refreshToken={historyRefresh}
    onLoadRequest={loadSaved}
    onActiveEnvironmentChange={(id) => void changeEnvironment(id)}
    onVariablesSaved={() => void loadVariables()}
    onNotice={(message) => (ui.notice = message)}
  />
  <div class="composer-request">
    <div class="composer-bar">
      <select
        value={method}
        aria-label="HTTP method"
        onchange={(event) => (method = (event.target as HTMLSelectElement).value)}
      >
        {#each METHODS as entry}
          <option value={entry}>{entry}</option>
        {/each}
      </select>
      <input
        class="url-input"
        bind:this={urlInput}
        value={url}
        placeholder={"https://api.example.com/v1/items — paste a curl command to import it"}
        aria-label="Request URL"
        oninput={(event) => (url = (event.target as HTMLInputElement).value)}
        onpaste={(event) => {
          const text = event.clipboardData?.getData("text") ?? "";
          if (text.trimStart().startsWith("curl")) {
            event.preventDefault();
            void applyCurl(text);
          }
        }}
        onkeydown={(event) => {
          if (event.key === "Enter" && !(event.metaKey || event.ctrlKey)) {
            event.preventDefault();
            void send();
          }
        }}
      />
      <button
        class="icon-button curl-toggle"
        class:active={curlOpen}
        title="Import from curl"
        aria-label="Import from curl"
        onclick={() => (curlOpen = !curlOpen)}
      ><TerminalSquare size={15} /></button>
      <button
        class="icon-button curl-toggle"
        title="Save request (⌘S)"
        aria-label="Save request"
        onclick={() => void openSaveDialog()}
      ><Save size={15} /></button>
      <div class="picker var-picker">
        <button
          class="icon-button curl-toggle"
          class:active={pickerOpen}
          title="Insert a variable ({{name}})"
          aria-label="Insert a variable"
          onclick={() => (pickerOpen = !pickerOpen)}
        >{"{{ }}"}</button>
        {#if pickerOpen}
          <div class="var-picker-menu">
            {#if variables.length}
              {#each variables as variable}
                <button onclick={() => insertVariable(variable.name)}>
                  <span>{`{{${variable.name}}}`}</span>
                  {#if variable.is_secret}<small>secret</small>{/if}
                </button>
              {/each}
            {:else}
              <span class="var-picker-empty">No variables yet — manage them in the library.</span>
            {/if}
          </div>
        {/if}
      </div>
      <button class="primary" class:success={sendFlash} disabled={busy} onclick={() => void send()}>
        {#if busy}<span class="spinner" />{:else}<Send size={15} />{/if}
        Send
      </button>
    </div>

    {#if curlOpen}
      <div class="curl-import">
        <textarea
          bind:this={curlTextarea}
          class="curl-input"
          rows={4}
          value={curlText}
          placeholder={"curl 'https://api.example.com/v1' \\\n  -H 'Authorization: Bearer …'"}
          oninput={(event) => (curlText = (event.target as HTMLTextAreaElement).value)}
        />
        <div class="curl-actions">
          <span>Paste or type a curl command — it fills the composer. ⌘↵ applies.</span>
          <button class="primary" onclick={() => void applyCurl(curlText)}>Apply</button>
        </div>
      </div>
    {/if}

    {#if unresolved.length}
      <div class="var-warning">
        <span>
          Unknown {#if unresolved.length === 1}variable{:else}variables{/if}:
          {unresolved.map((name) => `{{${name}}}`).join(", ")}
        </span>
        <button class="quiet" onclick={() => (pickerOpen = true)}>Insert variable</button>
      </div>
    {/if}

    <nav class="composer-tabs" aria-label="Request sections">
      <button class:active={tab === "params"} onclick={() => setTab("params")}>Params</button>
      <button class:active={tab === "headers"} onclick={() => setTab("headers")}>Headers</button>
      <button class:active={tab === "body"} onclick={() => setTab("body")}>Body</button>
      <button class:active={tab === "auth"} onclick={() => setTab("auth")}>Auth</button>
    </nav>

    <div class="composer-fields">
      {#if tab === "params"}
        {@render kvRows(params)}
      {:else if tab === "headers"}
        {@render kvRows(headers)}
      {:else if tab === "body"}
        <div class="composer-row">
          <select
            value={bodyKind}
            aria-label="Body type"
            onchange={(event) =>
              (bodyKind = (event.target as HTMLSelectElement).value as typeof bodyKind)}
          >
            <option value="none">None</option>
            <option value="form">Form data</option>
            <option value="multipart">Multipart</option>
            <option value="raw">Raw</option>
          </select>
          {#if bodyKind === "raw"}
            <select
              value={rawMediaType}
              aria-label="Raw media type"
              onchange={(event) =>
                (rawMediaType = (event.target as HTMLSelectElement).value)}
            >
              {#each MEDIA_TYPES as mediaType}
                <option value={mediaType}>{mediaType}</option>
              {/each}
            </select>
          {/if}
        </div>
        {#if bodyKind === "form"}
          {@render kvRows(formFields)}
        {:else if bodyKind === "multipart"}
          {#each multipartFields as field, index (index)}
            <div class="kv-row">
              <input
                value={field.name}
                placeholder="Name"
                oninput={(event) =>
                  (multipartFields[index].name = (event.target as HTMLInputElement).value)}
              />
              {#if field.file}
                <span class="multipart-file" title={field.file ?? ""}>
                  📎 {field.file}
                  {field.media_type ? `(${field.media_type})` : ""}
                </span>
              {:else}
                <div class="multipart-value">
                  <input
                    value={field.value ?? ""}
                    placeholder="Value"
                    oninput={(event) =>
                      (multipartFields[index].value =
                        (event.target as HTMLInputElement).value)}
                  />
                  <button
                    class="file-pick"
                    title="Choose a file for this field"
                    onclick={() => void pickMultipartFile(index)}
                  >📎 File</button>
                </div>
              {/if}
              {#if field.file}
                <button
                  class="file-pick"
                  title="Choose a different file"
                  onclick={() => void pickMultipartFile(index)}
                >Choose…</button>
              {:else}
                <button
                  class="icon-button"
                  aria-label="Remove field"
                  onclick={() => multipartFields.splice(index, 1)}
                ><Trash2 size={13} /></button>
              {/if}
            </div>
          {/each}
          <button
            class="add-row"
            onclick={() => multipartFields.push({ name: "", value: "" })}
          >
            <Plus size={13} /> Add field
          </button>
        {:else if bodyKind === "raw"}
          <textarea
            class="raw-body"
            rows={12}
            value={rawText}
            placeholder={"{\"hello\": \"world\"}"}
            oninput={(event) => (rawText = (event.target as HTMLTextAreaElement).value)}
          />
        {/if}
      {:else if tab === "auth"}
        <div class="composer-row">
          <select
            value={authKind}
            aria-label="Auth type"
            onchange={(event) =>
              (authKind = (event.target as HTMLSelectElement).value as AuthSpec["kind"])}
          >
            <option value="none">No auth</option>
            <option value="bearer">Bearer token</option>
            <option value="basic">Basic auth</option>
            <option value="api_key">API key</option>
          </select>
        </div>
        {#if authKind === "bearer"}
          <input
            class="auth-input"
            value={bearerToken}
            placeholder="Token"
            oninput={(event) => (bearerToken = (event.target as HTMLInputElement).value)}
          />
        {:else if authKind === "basic"}
          <div class="kv-row">
            <input
              value={basicUsername}
              placeholder="Username"
              oninput={(event) => (basicUsername = (event.target as HTMLInputElement).value)}
            />
            <input
              type="password"
              value={basicPassword}
              placeholder="Password"
              oninput={(event) => (basicPassword = (event.target as HTMLInputElement).value)}
            />
            <span />
          </div>
        {:else if authKind === "api_key"}
          <div class="kv-row">
            <input
              value={apiKeyName}
              placeholder="Header name"
              oninput={(event) => (apiKeyName = (event.target as HTMLInputElement).value)}
            />
            <input
              value={apiKeyValue}
              placeholder="Value"
              oninput={(event) => (apiKeyValue = (event.target as HTMLInputElement).value)}
            />
            <span />
          </div>
        {/if}
      {/if}
    </div>
  </div>

  <div class="composer-response">
    {#if busy && !response}
      <div class="empty"><span class="spinner" /><strong>Sending…</strong></div>
    {:else if error}
      <div class="composer-error">
        <b>Request failed</b>
        <span>{error}</span>
      </div>
    {:else if response}
      <div class="composer-meta">
        <b
          class:status-ok={response.status < 400}
          class:status-bad={response.status >= 400}
        >{response.status} {response.reason ?? ""}</b>
        <span>{elapsedLabel(response.elapsed_ms)}</span>
        <span>{byteSizeLabel(response.total_bytes)}</span>
        <span>{response.http_version}</span>
        <span class="meta-spacer" />
        {#if responseText && (responseText !== responsePretty || response.body.storage === "truncated")}
          <button class:active={pretty} onclick={() => (pretty = !pretty)}>
            {pretty ? "Formatted" : "Raw"}
          </button>
        {/if}
      </div>
      <nav class="composer-tabs" aria-label="Response sections">
        <button class:active={responseTab === "body"} onclick={() => setResponseTab("body")}>Body</button>
        <button class:active={responseTab === "headers"} onclick={() => setResponseTab("headers")}>Headers</button>
      </nav>
      {#if responseTab === "body"}
        <div class="composer-body">
          {#if responseText}
            <pre>{pretty ? responsePretty : responseText}</pre>
          {:else}
            <div class="empty compact"><strong>No body</strong></div>
          {/if}
        </div>
      {:else}
        <div class="panel">
          {#if response.headers.length}
            <div class="headers">
              {#each response.headers as header}
                <div><b>{header.name}</b><span>{header.value}</span></div>
              {/each}
            </div>
          {:else}
            <div class="empty compact"><strong>No headers</strong></div>
          {/if}
        </div>
      {/if}
    {:else}
      <div class="empty">
        <Send size={26} />
        <strong>Send a request to see the response</strong>
        <span>Tip: ⌘↵ sends from anywhere in the composer.</span>
      </div>
    {/if}
  </div>
</section>

{#if saveOpen}
  <div class="modal-backdrop" role="presentation" onclick={(event) => {
    if (event.target === event.currentTarget) saveOpen = false;
  }}>
    <div class="save-dialog" role="dialog" aria-label="Save request">
      <h2><Save size={16} /> Save request</h2>
      <label>
        <span>Name</span>
        <input
          value={saveName}
          placeholder="e.g. Create item"
          oninput={(event) => (saveName = (event.target as HTMLInputElement).value)}
          onkeydown={(event) => {
            if (event.key === "Enter") void saveCurrent();
            if (event.key === "Escape") saveOpen = false;
          }}
        />
      </label>
      <label>
        <span>Collection</span>
        <select
          value={saveCollectionId}
          onchange={(event) =>
            (saveCollectionId = (event.target as HTMLSelectElement).value)}
        >
          {#if !saveCollections.length}
            <option value="" disabled>No collections yet</option>
          {/if}
          {#each saveCollections as collection}
            <option value={collection.id}>{collection.name}</option>
          {/each}
          <option value="__new__">＋ New collection…</option>
        </select>
        {#if saveCollectionId === "__new__"}
          <input
            value={saveNewName}
            placeholder="Collection name"
            oninput={(event) => (saveNewName = (event.target as HTMLInputElement).value)}
            onkeydown={(event) => {
              if (event.key === "Enter") void saveCurrent();
              if (event.key === "Escape") saveCollectionId = saveCollections[0]?.id ?? "";
            }}
          />
        {/if}
      </label>
      <div class="save-actions">
        <span class="save-hint">
          {loadedRequestId ? "Updates the loaded request." : "Saves as a new request."}
        </span>
        <button class="quiet" onclick={() => (saveOpen = false)}>Cancel</button>
        <button class="primary" disabled={saveBusy} onclick={() => void saveCurrent()}>
          {#if saveBusy}<span class="spinner" />{/if}
          Save
        </button>
      </div>
    </div>
  </div>
{/if}
