<script lang="ts">
  import { AlertCircle, ChevronDown, Copy, Play, ShieldCheck } from "lucide-svelte";
  import {
    copy,
    getErrorCount,
    start,
    ui,
  } from "../stores.svelte";
  const errorCount = $derived(getErrorCount());
  const capturing = $derived(ui.capturing);
  const expandedIssue = $derived(ui.expandedIssue);
  const incidents = $derived(ui.incidents);
  const packageName = $derived(ui.packageName);
  const setExpandedIssue = (next: string) => ui.expandedIssue = next;
  import { timeLabel } from "../lib";
</script>

<section class="hero logs-hero"><div><span>Log inspector</span><h1>Every actionable error, in context.</h1><p>Live diagnostic evidence for <b>{packageName || "your selected package"}</b>.</p></div>{#if !capturing}<button class="primary" onclick={() => void start()}><Play/>Start monitoring</button>{/if}</section>
<section class="log-metrics"><article><small>Detected</small><b>{incidents.length}</b></article><article><small>Errors</small><b class="danger-text">{errorCount}</b></article><article><small>Monitoring</small><b>{capturing ? "Live" : "Paused"}</b></article></section>
<section class="log-feed">
  {#each incidents as issue (issue.signature)}
    {@const expanded = expandedIssue === issue.signature}
    <article class:expanded class="issue-card">
      <div class="issue-kind"><AlertCircle size={18}/>{issue.category.replaceAll("_", " ")}</div>
      <div class="issue-body">
        <button class="issue-toggle" aria-expanded={expanded} onclick={() => setExpandedIssue(expanded ? "" : issue.signature)}>
          <div class="issue-title"><div><h2>{issue.title}</h2><p>{issue.summary}</p></div><div class="issue-meta">{#if issue.occurrence_count > 1}<b class="occurrences">×{issue.occurrence_count}</b>{/if}<time>{timeLabel(issue.occurred_at)}</time><ChevronDown class="issue-chevron"/></div></div>
          <div class="issue-context">{issue.where_occurred || issue.foreground_activity || "App context unavailable"}</div>
        </button>
      </div>
    </article>
    {#if expanded}
      <section class="issue-expanded-content">
        <div class="issue-summary"><div><small>Summary</small><p>{issue.summary}</p></div><div><small>Root cause</small><p>{issue.root_cause || issue.likely_cause || "Still being classified from the captured evidence."}</p></div><div><small>How it happened</small><p>{issue.how_occurred || issue.likely_cause || "No additional execution context was recorded."}</p></div></div>
        <div class="issue-evidence"><div><small>Reproduction / context</small><ol>{#each issue.reproduction_steps as step, i (i)}<li>{step}</li>{/each}</ol></div><div><small>Captured logs</small><pre>{issue.lines.map(line => `${line.timestamp_ms}  ${line.level.padEnd(5)}  ${line.tag}: ${line.message}`).join("\n") || "No raw Logcat lines were retained for this issue."}</pre></div></div>
        <button class="quiet" onclick={() => copy(issue.lines.map(line => `${line.level} ${line.tag}: ${line.message}`).join("\n"))}><Copy/>Copy logs</button>
      </section>
    {/if}
  {/each}
  {#if !incidents.length}<div class="empty-state log-empty"><ShieldCheck size={32}/><b>No issues detected</b><span>{capturing ? "App Tester is watching Logcat for actionable problems." : "Start monitoring to see errors and warnings here."}</span></div>{/if}
</section>
