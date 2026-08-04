<script lang="ts">
  // The manual request composer: build a request, send it through the native
  // engine, and inspect the response. Sent requests also appear in the
  // Traffic lab, because the engine records them like captured traffic.
  // Pasting a curl command (in the URL bar or the import panel) fills the
  // whole composer from the native parser.
  import { onMount } from "svelte";
  import { Plus, Send, TerminalSquare, Trash2 } from "lucide-svelte";
  import * as api from "../api";
  import { byteSizeLabel, elapsedLabel, prettyJson } from "../lib";
  import { ui } from "../stores.svelte";
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
  let curlTextarea: HTMLTextAreaElement | undefined;

  let busy = $state(false);
  let response = $state<SendResult | null>(null);
  let error = $state("");
  let responseTab = $state<"body" | "headers">("body");
  let pretty = $state(true);
  let urlInput: HTMLInputElement | undefined;

  onMount(() => urlInput?.focus());

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
      );
      responseTab = "body";
      pretty = true;
    } catch (cause) {
      response = null;
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (!(event.metaKey || event.ctrlKey) || event.key !== "Enter") return;
    event.preventDefault();
    if (event.target === curlTextarea) {
      void applyCurl(curlText);
      return;
    }
    void send();
  }

  /// Fills the composer from a parsed curl command. Every field the parser
  /// produces is mapped onto the tabs so the user sees exactly what will
  /// be sent; the imported transport options (`-k`, `-m`, …) are kept.
  async function applyCurl(text: string) {
    if (!text.trim()) return;
    try {
      const imported = await api.parseCurl(text);
      const request = imported.request;
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
      optionsOverride = imported.options;
      curlOpen = false;
      error = "";
      ui.notice = "Imported from curl — review, then send.";
    } catch (cause) {
      error = `Could not parse curl: ${String(cause)}`;
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
        placeholder="https://api.example.com/v1/items — or paste a curl command"
        oninput={(event) => (url = (event.target as HTMLInputElement).value)}
        onpaste={(event) => {
          const text = event.clipboardData?.getData("text") ?? "";
          if (text.trimStart().startsWith("curl")) {
            event.preventDefault();
            void applyCurl(text);
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
      <button class="primary" disabled={busy} onclick={() => void send()}>
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
                <input
                  value={field.value ?? ""}
                  placeholder="Value"
                  oninput={(event) =>
                    (multipartFields[index].value =
                      (event.target as HTMLInputElement).value)}
                />
              {/if}
              <button
                class="icon-button"
                aria-label="Remove field"
                onclick={() => multipartFields.splice(index, 1)}
              ><Trash2 size={13} /></button>
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
