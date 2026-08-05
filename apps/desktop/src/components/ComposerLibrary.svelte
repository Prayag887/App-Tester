<script lang="ts">
  // The composer's collections sidebar: create/rename/delete collections,
  // expand them to list saved requests, and load one into the composer.
  // Owns its own data fetching; the composer only learns about loads.
  // Also hosts the environment switcher and the variables manager.
  import { onMount } from "svelte";
  import { Check, ChevronRight, FolderOpen, History, Pencil, Plus, Settings2, Trash2 } from "lucide-svelte";
  import * as api from "../api";
  import EnvironmentsDialog from "./EnvironmentsDialog.svelte";
  import { timeLabel } from "../lib";
  import type {
    CollectionSummary,
    EnvironmentSummary,
    HistorySummary,
    ManualRequest,
    SavedRequestSummary,
  } from "../types";

  let {
    loadedRequestId,
    activeEnvironmentId,
    refreshToken,
    onLoadRequest,
    onActiveEnvironmentChange,
    onVariablesSaved,
    onNotice,
  }: {
    loadedRequestId: string;
    activeEnvironmentId: string;
    refreshToken: number;
    onLoadRequest: (request: ManualRequest, id: string, collectionId: string) => void;
    onActiveEnvironmentChange: (id: string) => void;
    onVariablesSaved: () => void;
    onNotice: (message: string) => void;
  } = $props();

  let collections = $state<CollectionSummary[]>([]);
  let expanded = $state<Set<string>>(new Set());
  let requestsByCollection = $state<Record<string, SavedRequestSummary[]>>({});
  let history = $state<HistorySummary[]>([]);
  let creating = $state(false);
  let newName = $state("");
  let renaming = $state("");
  let renameValue = $state("");
  let confirmingDelete = $state("");
  let busy = $state(false);

  let environments = $state<EnvironmentSummary[]>([]);
  let envDialogOpen = $state(false);

  $effect(() => {
    // Bumped by the composer after every send so the list stays fresh.
    if (refreshToken > 0) void loadHistory();
  });

  async function refreshEnvironments() {
    try {
      environments = await api.listEnvironments();
    } catch (error) {
      onNotice(`Could not load environments: ${String(error)}`);
    }
  }

  function switchEnvironment(id: string) {
    onActiveEnvironmentChange(id);
    onNotice(id ? "Environment switched." : "No environment selected.");
  }

  async function refresh() {
    try {
      collections = await api.listCollections();
    } catch (error) {
      onNotice(`Could not load collections: ${String(error)}`);
    }
  }

  async function toggle(collection: CollectionSummary) {
    const next = new Set(expanded);
    if (next.has(collection.id)) {
      next.delete(collection.id);
      expanded = next;
      return;
    }
    next.add(collection.id);
    expanded = next;
    if (!requestsByCollection[collection.id]) {
      try {
        requestsByCollection = {
          ...requestsByCollection,
          [collection.id]: await api.listRequests(collection.id),
        };
      } catch (error) {
        onNotice(`Could not load requests: ${String(error)}`);
      }
    }
  }

  async function create() {
    const name = newName.trim();
    if (!name) return;
    creating = false;
    newName = "";
    try {
      const created = await api.createCollection(name);
      collections = [...collections, created];
      expanded = new Set([...expanded, created.id]);
      requestsByCollection = { ...requestsByCollection, [created.id]: [] };
      onNotice(`Created collection "${name}".`);
    } catch (error) {
      onNotice(`Could not create collection: ${String(error)}`);
    }
  }

  function startRename(collection: CollectionSummary) {
    renaming = collection.id;
    renameValue = collection.name;
  }

  async function finishRename(collection: CollectionSummary) {
    const name = renameValue.trim();
    renaming = "";
    if (!name || name === collection.name) return;
    try {
      await api.renameCollection(collection.id, name);
      await refresh();
    } catch (error) {
      onNotice(`Could not rename collection: ${String(error)}`);
    }
  }

  async function removeCollection(collection: CollectionSummary) {
    if (confirmingDelete !== collection.id) {
      confirmingDelete = collection.id;
      return;
    }
    confirmingDelete = "";
    busy = true;
    try {
      await api.deleteCollection(collection.id);
      await refresh();
      onNotice(`Deleted collection "${collection.name}".`);
    } catch (error) {
      onNotice(`Could not delete collection: ${String(error)}`);
    } finally {
      busy = false;
    }
  }

  async function removeRequest(collectionId: string, request: SavedRequest) {
    try {
      await api.deleteRequest(request.id);
      requestsByCollection = {
        ...requestsByCollection,
        [collectionId]: (requestsByCollection[collectionId] ?? []).filter(
          (item) => item.id !== request.id,
        ),
      };
      await refresh();
    } catch (error) {
      onNotice(`Could not delete request: ${String(error)}`);
    }
  }

  async function loadHistory() {
    try {
      history = await api.listHistory();
    } catch (error) {
      onNotice(`Could not load history: ${String(error)}`);
    }
  }

  async function openHistoryEntry(entry: HistorySummary) {
    try {
      const request = await api.getHistoryRequest(entry.id);
      onLoadRequest(request, "", "");
      onNotice("Loaded from history — review, then send.");
    } catch (error) {
      onNotice(`Could not load the request: ${String(error)}`);
    }
  }

  async function removeHistoryEntry(id: string) {
    try {
      await api.deleteHistory(id);
      await loadHistory();
    } catch (error) {
      onNotice(`Could not delete history entry: ${String(error)}`);
    }
  }

  async function clearAllHistory() {
    try {
      await api.clearHistory();
      history = [];
    } catch (error) {
      onNotice(`Could not clear history: ${String(error)}`);
    }
  }

  function load(request: SavedRequestSummary) {
    void (async () => {
      try {
        const full = await api.getRequest(request.id);
        onLoadRequest(full.request, full.id, full.collection_id);
      } catch (error) {
        onNotice(`Could not load the request: ${String(error)}`);
      }
    })();
  }

  onMount(() => {
    void refresh();
    void refreshEnvironments();
    void loadHistory();
  });
</script>

<aside class="composer-library">
  <div class="library-heading">
    <b>Environment</b>
    <button
      class="icon-button"
      title="Manage environments & variables"
      aria-label="Manage environments"
      onclick={() => (envDialogOpen = true)}
    ><Settings2 size={13} /></button>
  </div>
  <div class="library-environment">
    <select
      value={activeEnvironmentId}
      aria-label="Active environment"
      onchange={(event) =>
        switchEnvironment((event.target as HTMLSelectElement).value)}
    >
      <option value="">No environment</option>
      {#each environments as environment}
        <option value={environment.id}>{environment.name}</option>
      {/each}
    </select>
  </div>

  <div class="library-heading">
    <b>Collections</b>
    <button
      class="icon-button"
      title="New collection"
      aria-label="New collection"
      onclick={() => (creating = !creating)}
    ><Plus size={14} /></button>
  </div>

  {#if creating}
    <div class="library-create">
      <input
        value={newName}
        placeholder="Collection name"
        oninput={(event) => (newName = (event.target as HTMLInputElement).value)}
        onkeydown={(event) => {
          if (event.key === "Enter") void create();
          if (event.key === "Escape") creating = false;
        }}
      />
      <button class="icon-button" aria-label="Create collection" onclick={() => void create()}><Check size={14} /></button>
    </div>
  {/if}

  <div class="library-list">
    {#if !collections.length}
      <div class="empty compact">
        <FolderOpen size={22} />
        <span>No collections yet.<br />Save a request to start one.</span>
      </div>
    {/if}
    {#each collections as collection}
      <div class="library-collection">
        <div class="library-row">
          <button class="library-toggle" onclick={() => void toggle(collection)}>
            <ChevronRight size={13} class={expanded.has(collection.id) ? "rotated" : ""} />
            {#if renaming === collection.id}
              <input
                class="library-rename"
                value={renameValue}
                oninput={(event) => (renameValue = (event.target as HTMLInputElement).value)}
                onkeydown={(event) => {
                  if (event.key === "Enter") void finishRename(collection);
                  if (event.key === "Escape") renaming = "";
                }}
              />
            {:else}
              <span class="library-name">{collection.name}</span>
            {/if}
            <small>{collection.request_count}</small>
          </button>
          <button class="icon-button" title="Rename" aria-label="Rename collection" onclick={() => startRename(collection)}><Pencil size={12} /></button>
          <button
            class="icon-button destructive"
            class:confirming={confirmingDelete === collection.id}
            title={confirmingDelete === collection.id ? "Confirm delete" : "Delete collection"}
            aria-label="Delete collection"
            disabled={busy}
            onclick={() => void removeCollection(collection)}
          >{#if confirmingDelete === collection.id}<b>?</b>{:else}<Trash2 size={12} />{/if}</button>
        </div>
        {#if expanded.has(collection.id)}
          <div class="library-requests">
            {#each requestsByCollection[collection.id] ?? [] as request}
              <button
                class:loaded={request.id === loadedRequestId}
                class="library-request"
                onclick={() => load(request)}
              >
                <span class="method-tag">{request.method}</span>
                <span class="library-request-name" title={request.name}>{request.name}</span>
                <span
                  class="icon-button library-remove"
                  role="button"
                  aria-label="Delete request"
                  onclick={(event) => {
                    event.stopPropagation();
                    void removeRequest(collection.id, request);
                  }}
                ><Trash2 size={11} /></span>
              </button>
            {/each}
            {#if !(requestsByCollection[collection.id] ?? []).length}
              <div class="library-empty">No saved requests</div>
            {/if}
          </div>
        {/if}
      </div>
    {/each}
  </div>

  <div class="library-heading">
    <b>History</b>
    {#if history.length}
      <button
        class="icon-button"
        title="Clear history"
        aria-label="Clear history"
        onclick={() => void clearAllHistory()}
      ><Trash2 size={13} /></button>
    {/if}
  </div>
  <div class="library-list">
    {#each history as entry}
      <div class="history-row">
        <button class="library-request" onclick={() => void openHistoryEntry(entry)}>
          <span class="method-tag">{entry.method}</span>
          <span class="library-request-name" title={entry.url}>{entry.url}</span>
          <small class="history-time">{timeLabel(entry.sent_at)}</small>
        </button>
        <button
          class="icon-button library-remove"
          aria-label="Delete history entry"
          onclick={() => void removeHistoryEntry(entry.id)}
        ><Trash2 size={11} /></button>
      </div>
    {/each}
    {#if !history.length}
      <div class="library-empty">
        <History size={13} />
        Sent requests appear here for one-click re-sending.
      </div>
    {/if}
  </div>
</aside>

{#if envDialogOpen}
  <EnvironmentsDialog
    open={envDialogOpen}
    onClose={() => (envDialogOpen = false)}
    onSaved={() => {
      void refreshEnvironments();
      onVariablesSaved();
    }}
    onNotice={onNotice}
  />
{/if}
