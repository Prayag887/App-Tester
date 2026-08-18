<script lang="ts">
  // The manual request composer: build a request, send it through the native
  // engine, and inspect the response. Sent requests also appear in the
  // Traffic lab, because the engine records them like captured traffic.
  // Pasting a curl command (in the URL bar or the import panel) fills the
  // whole composer from the native parser.
  import { onMount } from "svelte";
  import { Check, Copy, Plus, Send, TerminalSquare, Trash2 } from "lucide-svelte";
  import * as api from "../api";
  import { bodyTextPreview, byteSizeLabel, elapsedLabel, prettyJson } from "../lib";
  import {
    beginHorizontalResize,
    clampPanelSize,
    readPanelSize,
    storePanelSize,
  } from "../panel-resize";
  import { ui } from "../stores.svelte";
  import PanelResizeHandle from "./PanelResizeHandle.svelte";
  import type {
    AuthSpec,
    HeaderEntry,
    ManualBody,
    ManualRequest,
    MultipartField,
    QueryParameter,
    SendOptions,
    SendResult,
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
  let curlOpen = $state(false);
  let curlText = $state("");
  let curlTextarea = $state<HTMLTextAreaElement | undefined>();
  let curlCopied = $state(false);

  // Flashes green on the Send button after a successful send.
  let sendFlash = $state(false);

  let busy = $state(false);
  let response = $state<SendResult | null>(null);
  let error = $state("");
  let responseTab = $state<"body" | "headers">("body");
  let pretty = $state(true);
  let urlInput: HTMLInputElement | undefined;
  let composerShell: HTMLElement;
  let composerRequestWidth = $state(readPanelSize("app-tester.composer-request-width", 700));

  function resizeComposerPanels(event: PointerEvent) {
    const bounds = composerShell.getBoundingClientRect();
    beginHorizontalResize(
      event,
      clientX => clampPanelSize(clientX - bounds.left, 460, Math.max(460, bounds.width - 360)),
      value => composerRequestWidth = value,
      value => storePanelSize("app-tester.composer-request-width", value),
    );
  }

  function resizeComposerPanelsWithKeyboard(event: KeyboardEvent) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    composerRequestWidth = clampPanelSize(composerRequestWidth + (event.key === "ArrowRight" ? 16 : -16), 460, 900);
    storePanelSize("app-tester.composer-request-width", composerRequestWidth);
  }

  onMount(() => {
    // A request handed over from another screen ("Send in Composer").
    if (ui.composerDraft) {
      fillRequest(ui.composerDraft);
      ui.composerDraft = null;
      ui.notice = "Opened in the composer — review, then send.";
    }
    urlInput?.focus();
  });

  const responsePreview = $derived.by(() => bodyTextPreview(response?.body));
  const responseText = $derived(responsePreview.text);
  const responsePretty = $derived(responsePreview.truncated ? responseText : prettyJson(responseText));

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
      response = await api.sendRequest(wireRequest(), optionsOverride ?? DEFAULT_OPTIONS);
      responseTab = "body";
      pretty = true;
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

  async function copyCurl() {
    if (!url.trim()) {
      error = "Enter a URL to copy as cURL.";
      return;
    }
    error = "";
    try {
      const command = await api.generateComposerCurl(
        wireRequest(),
        optionsOverride ?? DEFAULT_OPTIONS,
      );
      await navigator.clipboard.writeText(command);
      curlCopied = true;
      ui.notice = "Composer cURL copied.";
      window.setTimeout(() => (curlCopied = false), 1200);
    } catch (cause) {
      error = `Could not copy cURL: ${String(cause)}`;
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
  }

  /// Maps a parsed request onto every composer tab.
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
      curlOpen = false;
      error = "";
      ui.notice = "Imported from curl — review, then send.";
    } catch (cause) {
      error = `Could not parse curl: ${String(cause)}`;
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

<style>
  .composer-tabs button {
    position: relative;
    border-bottom: 2px solid transparent;
  }
  .composer-tabs button::after {
    position: absolute;
    right: 12px;
    bottom: -2px;
    left: 12px;
    height: 3px;
    content: "";
    border-radius: 999px;
    background: var(--shell-accent);
    opacity: 0;
    transform: scaleX(.08);
    transform-origin: center;
    will-change: transform,opacity;
    transition: transform 280ms cubic-bezier(.16,1,.3,1),opacity 160ms ease,box-shadow 240ms ease;
  }
  .composer-tabs button:hover::after { opacity: .58; transform: scaleX(.42); }
  .composer-tabs button.active::after {
    opacity: 1;
    transform: scaleX(1);
    box-shadow: 0 0 8px color-mix(in srgb,var(--shell-accent) 42%,transparent);
    animation: composer-tab-indicator-lock 480ms linear(0,.28 15%,.76 42%,1.12 65%,.97 84%,1) both;
  }
  @keyframes composer-tab-indicator-lock {
    0% { opacity: .58; transform: scaleX(.42); box-shadow: 0 0 0 transparent; }
    64% { opacity: 1; transform: scaleX(1.12); box-shadow: 0 0 14px color-mix(in srgb,var(--shell-accent) 58%,transparent); }
    84% { transform: scaleX(.97); }
    100% { opacity: 1; transform: scaleX(1); box-shadow: 0 0 8px color-mix(in srgb,var(--shell-accent) 42%,transparent); }
  }
</style>

<section class:has-response={Boolean(response) || Boolean(error) || busy} class="composer" bind:this={composerShell} style:--composer-request-width={`${composerRequestWidth}px`}>
  <div class="composer-request">
    <div class="composer-bar">
      <select
        value={method}
        aria-label="HTTP method"
        onchange={(event) => (method = (event.target as HTMLSelectElement).value)}
      >
        {#each METHODS as entry (entry)}
          <option value={entry}>{entry}</option>
        {/each}
      </select>
      <input
        class="url-input"
        bind:this={urlInput}
        value={url}
        placeholder="https://api.example.com/v1/items — paste a curl command to import it"
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
        class:copied={curlCopied}
        class="icon-button"
        title={curlCopied ? "cURL copied" : "Copy request as cURL"}
        aria-label={curlCopied ? "cURL copied" : "Copy request as cURL"}
        onclick={() => void copyCurl()}
      >{#if curlCopied}<Check size={15} />{:else}<Copy size={15} />{/if}</button>
      <button class="primary" class:success={sendFlash} disabled={busy} onclick={() => void send()}>
        {#if busy}<span class="spinner"></span>{:else}<Send size={15} />{/if}
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
          placeholder="curl 'https://api.example.com/v1' \\\n  -H 'Authorization: Bearer …'"
          oninput={(event) => (curlText = (event.target as HTMLTextAreaElement).value)}
        ></textarea>
        <div class="curl-actions">
          <span>Paste or type a curl command — it fills the composer. ⌘↵ applies.</span>
          <button class="primary" onclick={() => void applyCurl(curlText)}>Apply</button>
        </div>
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
              {#each MEDIA_TYPES as mediaType (mediaType)}
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
          ></textarea>
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
            <span></span>
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
            <span></span>
          </div>
        {/if}
      {/if}
    </div>
  </div>

  <PanelResizeHandle
    label="Resize request and response panels"
    hidden={!response && !error && !busy}
    onpointerdown={resizeComposerPanels}
    onkeydown={resizeComposerPanelsWithKeyboard}
  />

  <div class:response-empty={!response && !error && !busy} class="composer-response">
    {#if busy && !response}
      <div class="empty"><span class="spinner"></span><strong>Sending…</strong></div>
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
        <span class="meta-spacer"></span>
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
            {#if responsePreview.truncated}
              <div class="content-truncated">
                Showing {byteSizeLabel(responsePreview.shown)} of {byteSizeLabel(responsePreview.total)}. The full body remains in the capture.
              </div>
            {/if}
          {:else}
            <div class="empty compact"><strong>No body</strong></div>
          {/if}
        </div>
      {:else}
        <div class="panel">
          {#if response.headers.length}
            <div class="headers">
              {#each response.headers as header, index (`${index}:${header.name}:${header.value}`)}
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
