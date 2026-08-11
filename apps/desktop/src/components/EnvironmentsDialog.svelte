<script lang="ts">
  // The environments manager: create/rename/delete environments, edit their
  // variables (and the global scope) with secret masking. Edits accumulate
  // locally and are written in one batch on "Save changes".
  import { onMount } from "svelte";
  import { Check, Eye, EyeOff, Globe, Pencil, Plus, Trash2, X } from "lucide-svelte";
  import * as api from "../api";
  import type { EnvironmentSummary, VariableRecord } from "../types";

  let {
    open,
    onClose,
    onSaved,
    onNotice,
  }: {
    open: boolean;
    onClose: () => void;
    onSaved: () => void;
    onNotice: (message: string) => void;
  } = $props();

  interface Row extends VariableRecord {
    dirty: boolean;
    removed: boolean;
  }

  let environments = $state<EnvironmentSummary[]>([]);
  let selected = $state<string>(""); // "" = global scope
  let rows = $state<Row[]>([]);
  let creating = $state(false);
  let newName = $state("");
  let renaming = $state("");
  let renameValue = $state("");
  let confirmingDelete = $state("");
  let busy = $state(false);
  let revealSecrets = $state(false);

  async function load() {
    environments = await api.listEnvironments();
    await loadRows();
  }

  async function loadRows() {
    const scopeId = selected || null;
    rows = (await api.listVariables(scopeId)).map((variable) => ({
      ...variable,
      dirty: false,
      removed: false,
    }));
  }

  function selectScope(id: string) {
    selected = id;
    void loadRows();
  }

  function addRow() {
    rows = [
      ...rows,
      {
        id: "",
        environment_id: selected || null,
        name: "",
        value: "",
        is_secret: false,
        created_at: "",
        updated_at: "",
        dirty: true,
        removed: false,
      },
    ];
  }

  function markDirty(index: number) {
    rows[index].dirty = true;
  }

  async function create() {
    const name = newName.trim();
    if (!name) return;
    creating = false;
    newName = "";
    try {
      const created = await api.createEnvironment(name);
      environments = [...environments, created];
      selectScope(created.id);
      onNotice(`Created environment "${name}".`);
    } catch (error) {
      onNotice(`Could not create environment: ${String(error)}`);
    }
  }

  function startRename(environment: EnvironmentSummary) {
    renaming = environment.id;
    renameValue = environment.name;
  }

  async function finishRename(environment: EnvironmentSummary) {
    const name = renameValue.trim();
    renaming = "";
    if (!name || name === environment.name) return;
    try {
      await api.renameEnvironment(environment.id, name);
      environments = await api.listEnvironments();
    } catch (error) {
      onNotice(`Could not rename environment: ${String(error)}`);
    }
  }

  async function removeEnvironment(environment: EnvironmentSummary) {
    if (confirmingDelete !== environment.id) {
      confirmingDelete = environment.id;
      return;
    }
    confirmingDelete = "";
    try {
      await api.deleteEnvironment(environment.id);
      if (selected === environment.id) {
        selected = "";
        await loadRows();
      }
      environments = await api.listEnvironments();
    } catch (error) {
      onNotice(`Could not delete environment: ${String(error)}`);
    }
  }

  async function save() {
    const pending = rows.filter((row) => row.dirty);
    const removed = rows.filter((row) => row.removed);
    if (!pending.length && !removed.length) {
      onClose();
      return;
    }
    busy = true;
    try {
      for (const row of pending) {
        if (row.removed) {
          if (row.id) await api.deleteVariable(row.id);
          continue;
        }
        if (!row.name.trim()) continue;
        await api.saveVariable(row.id || null, selected || null, {
          name: row.name.trim(),
          value: row.value,
          is_secret: row.is_secret,
        });
      }
      for (const row of removed) {
        if (row.id) await api.deleteVariable(row.id);
      }
      await loadRows();
      onSaved();
      onNotice("Variables saved.");
      onClose();
    } catch (error) {
      onNotice(`Could not save variables: ${String(error)}`);
    } finally {
      busy = false;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") onClose();
  }

  onMount(() => {
    void load();
  });
</script>

<svelte:window onkeydown={onKeydown} />

<div class="modal-backdrop" role="presentation" onclick={(event) => {
  if (event.target === event.currentTarget) onClose();
}}>
  <div class="env-dialog" role="dialog" aria-label="Environments">
    <div class="env-heading">
      <h2><Globe size={16} /> Environments</h2>
      <button class="icon-button" aria-label="Close" onclick={onClose}><X size={15} /></button>
    </div>
    <div class="env-body">
      <div class="env-scopes">
        <button class:active={selected === ""} onclick={() => selectScope("")}>
          <Globe size={13} /><span>Global</span>
        </button>
        {#each environments as environment}
          <div class="env-scope-row">
            <button
              class:active={selected === environment.id}
              onclick={() => selectScope(environment.id)}
            >
              {#if renaming === environment.id}
                <input
                  class="env-rename"
                  value={renameValue}
                  oninput={(event) => (renameValue = (event.target as HTMLInputElement).value)}
                  onkeydown={(event) => {
                    if (event.key === "Enter") void finishRename(environment);
                    if (event.key === "Escape") renaming = "";
                  }}
                />
              {:else}
                <span>{environment.name}</span>
              {/if}
              <small>{environment.variable_count}</small>
            </button>
            <button
              class="icon-button"
              aria-label="Rename environment"
              onclick={() => startRename(environment)}
            ><Pencil size={11} /></button>
            <button
              class="icon-button destructive"
              class:confirming={confirmingDelete === environment.id}
              aria-label="Delete environment"
              onclick={() => void removeEnvironment(environment)}
            >{#if confirmingDelete === environment.id}<b>?</b>{:else}<Trash2 size={11} />{/if}</button>
          </div>
        {/each}
        {#if creating}
          <input
            class="env-create"
            value={newName}
            placeholder="Environment name"
            oninput={(event) => (newName = (event.target as HTMLInputElement).value)}
            onkeydown={(event) => {
              if (event.key === "Enter") void create();
              if (event.key === "Escape") creating = false;
            }}
          />
        {/if}
        <button class="env-add" onclick={() => (creating = !creating)}>
          <Plus size={13} /> New environment
        </button>
      </div>
      <div class="env-variables">
        <div class="env-variable-heading">
          <b>{selected ? environments.find((env) => env.id === selected)?.name ?? "Scope" : "Global"} variables</b>
          <button class="icon-button" title="Toggle secret visibility" aria-label="Toggle secret visibility" onclick={() => (revealSecrets = !revealSecrets)}>
            {#if revealSecrets}<EyeOff size={13} />{:else}<Eye size={13} />{/if}
          </button>
        </div>
        {#each rows as row, index (row.id || `${index}-new`)}
          <div class:removed={row.removed} class="env-row">
            <input
              class="env-name"
              value={row.name}
              placeholder="name"
              oninput={(event) => {
                rows[index].name = (event.target as HTMLInputElement).value;
                markDirty(index);
              }}
            />
            <input
              class="env-value"
              type={row.is_secret && !revealSecrets ? "password" : "text"}
              value={row.value}
              placeholder="value"
              oninput={(event) => {
                rows[index].value = (event.target as HTMLInputElement).value;
                markDirty(index);
              }}
            />
            <button
              class="icon-button env-secret"
              class:active={row.is_secret}
              title={row.is_secret ? "Secret (masked)" : "Plain value"}
              aria-label="Toggle secret"
              onclick={() => {
                rows[index].is_secret = !rows[index].is_secret;
                markDirty(index);
              }}
            >{#if row.is_secret}<EyeOff size={12} />{:else}<Eye size={12} />{/if}</button>
            <button
              class="icon-button destructive"
              aria-label="Remove variable"
              onclick={() => (rows[index].removed = !rows[index].removed)}
            ><Trash2 size={12} /></button>
          </div>
        {/each}
        {#if !rows.length}
          <div class="env-empty">No variables in this scope.</div>
        {/if}
        <button class="env-add" onclick={addRow}><Plus size={13} /> Add variable</button>
      </div>
    </div>
    <div class="env-actions">
      <span class="env-hint">Changes apply on save.</span>
      <button class="quiet" onclick={onClose}>Cancel</button>
      <button class="primary" disabled={busy} onclick={() => void save()}>
        {#if busy}<span class="spinner"></span>{/if}
        Save changes
      </button>
    </div>
  </div>
</div>
