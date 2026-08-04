<script lang="ts">
  // The manual request composer: build a request, send it through the native
  // engine, and inspect the response. Sent requests also appear in the
  // Traffic lab, because the engine records them like captured traffic.
  import { onMount } from "svelte";
  import { Plus, Send, Trash2 } from "lucide-svelte";
  import * as api from "../api";
  import { byteSizeLabel, elapsedLabel, prettyJson } from "../lib";
  import type {
    AuthSpec,
    HeaderEntry,
    ManualBody,
    ManualRequest,
    QueryParameter,
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
  let bodyKind = $state<"none" | "form" | "raw">("none");
  let formFields = $state<QueryParameter[]>([{ name: "", value: "" }]);
  let rawText = $state("");
  let rawMediaType = $state("application/json");
  let authKind = $state<AuthSpec["kind"]>("none");
  let bearerToken = $state("");
  let basicUsername = $state("");
  let basicPassword = $state("");
  let apiKeyName = $state("");
  let apiKeyValue = $state("");

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
      response = await api.sendRequest(wireRequest(), DEFAULT_OPTIONS);
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
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      void send();
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
        placeholder="https://api.example.com/v1/items"
        oninput={(event) => (url = (event.target as HTMLInputElement).value)}
      />
      <button class="primary" disabled={busy} onclick={() => void send()}>
        {#if busy}<span class="spinner" />{:else}<Send size={15} />{/if}
        Send
      </button>
    </div>

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
