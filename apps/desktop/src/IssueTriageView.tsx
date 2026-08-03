import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, Check, ChevronRight, FileDiff } from "lucide-react";
import type { Execution, Run } from "./types";

interface Issue {
  execution: Execution;
  run: Run;
}

export function IssueTriageView({
  runs,
  activeRun,
  activeExecution,
  onInspect,
}: {
  runs: Run[];
  activeRun?: Run;
  activeExecution?: Execution;
  onInspect: (run: Run, execution: Execution) => void;
}) {
  const issues = useMemo(
    () =>
      runs.flatMap((run) =>
        run.executions
          .filter((execution) => execution.state !== "passed")
          .map((execution) => ({ run, execution })),
      ),
    [runs],
  );
  const [selected, setSelected] = useState<string>();
  useEffect(() => {
    if (!selected) setSelected(issues[0]?.execution.id);
  }, [issues, selected]);
  const current =
    issues.find((item) => item.execution.id === selected) ?? issues[0];
  const detailed =
    activeExecution?.id === current?.execution.id ? activeExecution : undefined;
  const regressions = issues.filter(
    (item) => item.execution.state === "changed",
  ).length;
  const failed = issues.filter(
    (item) => item.execution.state !== "changed",
  ).length;
  return (
    <section className="issues-view">
      <header className="screen-hero">
        <div>
          <p className="eyebrow">QUALITY REVIEW</p>
          <h1>Issue triage</h1>
          <p>
            Review response changes and failed checks separately from composing
            requests.
          </p>
        </div>
      </header>
      <section className="issue-metrics">
        <Metric
          label="Needs review"
          value={issues.length}
          detail="Changed or failed endpoints"
          bad
        />
        <Metric
          label="Regressions"
          value={regressions}
          detail="Response difference detected"
        />
        <Metric
          label="Failed checks"
          value={failed}
          detail="Transport or assertion failure"
        />
        <Metric label="Runs" value={runs.length} detail="Available history" />
      </section>
      <section className="issue-layout">
        <aside className="issue-list panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">OPEN ITEMS</p>
              <h2>Failures & changes</h2>
            </div>
            <span>{issues.length}</span>
          </div>
          {issues.length ? (
            issues.map((item) => (
              <button
                key={`${item.run.id}-${item.execution.id}`}
                className={
                  item.execution.id === current?.execution.id
                    ? "issue-row active"
                    : "issue-row"
                }
                onClick={() => {
                  setSelected(item.execution.id);
                  onInspect(item.run, item.execution);
                }}
              >
                <IssueState execution={item.execution} />
                <div>
                  <strong>{item.execution.request_name}</strong>
                  <small>
                    {item.execution.error ??
                      item.execution.comparison?.differences[0]?.message ??
                      "Response changed from baseline"}
                  </small>
                  <em>{new Date(item.run.started_at).toLocaleString()}</em>
                </div>
                <ChevronRight />
              </button>
            ))
          ) : (
            <div className="mini-empty">
              <Check />
              <strong>Nothing needs review</strong>
              <p>Run a collection against a baseline to surface regressions.</p>
            </div>
          )}
        </aside>
        <ComparisonPanel
          execution={detailed ?? current?.execution}
          run={activeRun?.id === current?.run.id ? activeRun : current?.run}
        />
      </section>
    </section>
  );
}

function Metric({
  label,
  value,
  detail,
  bad,
}: {
  label: string;
  value: number;
  detail: string;
  bad?: boolean;
}) {
  return (
    <article className={bad ? "issue-metric bad" : "issue-metric"}>
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{detail}</small>
    </article>
  );
}

function IssueState({ execution }: { execution: Execution }) {
  return execution.state === "changed" ? (
    <span className="issue-icon changed">
      <FileDiff />
    </span>
  ) : (
    <span className="issue-icon failed">
      <AlertTriangle />
    </span>
  );
}

function ComparisonPanel({
  execution,
  run,
}: {
  execution?: Execution;
  run?: Run;
}) {
  const differences = execution?.comparison?.differences ?? [];
  return (
    <article className="comparison-panel panel">
      {execution ? (
        <>
          <div className="panel-title">
            <div>
              <p className="eyebrow">RESPONSE COMPARE</p>
              <h2>{execution.request_name}</h2>
              <small>
                {run?.collection_name} · {execution.state.replaceAll("_", " ")}
              </small>
            </div>
            <IssueState execution={execution} />
          </div>
          {execution.error && (
            <div className="error-copy">{execution.error}</div>
          )}
          {differences.length ? (
            <>
              <p className="comparison-intro">
                Baseline and current response differ in {differences.length}{" "}
                place{differences.length === 1 ? "" : "s"}.
              </p>
              <div className="comparison-diffs">
                {differences.map((difference, index) => (
                  <section
                    className="comparison-diff"
                    key={`${difference.path}-${index}`}
                  >
                    <header>
                      <code>{difference.path}</code>
                      <span>{difference.kind.replaceAll("_", " ")}</span>
                    </header>
                    <p>{difference.message}</p>
                    <div>
                      <pre>
                        {JSON.stringify(difference.baseline, null, 2) ||
                          "Not present"}
                      </pre>
                      <ChevronRight />
                      <pre>
                        {JSON.stringify(difference.current, null, 2) ||
                          "Not present"}
                      </pre>
                    </div>
                  </section>
                ))}
              </div>
            </>
          ) : (
            <ResponseSnapshot execution={execution} />
          )}
        </>
      ) : (
        <div className="mini-empty">
          <FileDiff />
          <strong>Select an issue</strong>
          <p>The full baseline-to-current response comparison appears here.</p>
        </div>
      )}
    </article>
  );
}

function ResponseSnapshot({ execution }: { execution: Execution }) {
  return (
    <>
      <p className="comparison-intro">
        No structured baseline difference is available for this result.
      </p>
      {execution.response && (
        <pre className="response-body">
          {prettyBody(execution.response.body)}
        </pre>
      )}
    </>
  );
}
function prettyBody(body: string) {
  try {
    return JSON.stringify(JSON.parse(body), null, 2);
  } catch {
    return body;
  }
}
