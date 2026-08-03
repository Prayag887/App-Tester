import { useMemo, useState } from "react";
import {
  Activity,
  RefreshCw,
  Shield,
  Smartphone,
  Usb,
  Wifi,
} from "lucide-react";
import { useCaptureSession } from "./useCaptureSession";
import type { HttpTransaction } from "./types";

export function ApiHitsView({ onError }: { onError: (error: string) => void }) {
  const session = useCaptureSession(onError);
  const [selectedId, setSelectedId] = useState<string>();
  const selected =
    session.transactions.find((item) => item.id === selectedId) ??
    session.transactions.at(-1);
  const errors = session.transactions.filter(
    (item) => (item.response?.status ?? 0) >= 400,
  ).length;
  const p95 = useMemo(
    () => percentile(session.transactions, 0.95),
    [session.transactions],
  );

  return (
    <section className="hits-view">
      <header className="screen-hero hits-hero">
        <div>
          <p className="eyebrow">LIVE CAPTURE</p>
          <h1>API Hits</h1>
          <p>
            Inspect live Android traffic without leaving your request workspace.
          </p>
        </div>
        <span className={`capture-state ${session.status}`}>
          {session.status}
        </span>
      </header>
      <section className="capture-command-bar">
        <div className="capture-step">
          <span>
            <Usb /> 1. Device
          </span>
          <select
            value={session.device?.serial ?? ""}
            disabled={session.active || session.busy}
            onChange={(event) => void session.selectDevice(event.target.value)}
          >
            <option value="">Connect USB or select device</option>
            {session.devices.map((device) => (
              <option
                key={device.serial}
                value={device.serial}
                disabled={device.authorization_status !== "authorized"}
              >
                {device.model ?? device.serial} · {device.connection_type} ·{" "}
                {device.authorization_status}
              </option>
            ))}
          </select>
        </div>
        <button
          className="secondary"
          disabled={session.active || session.busy}
          onClick={() =>
            void session.refresh().catch((error) => onError(String(error)))
          }
        >
          <RefreshCw /> Refresh devices
        </button>
        {session.device?.connection_type === "usb" && (
          <button
            className="secondary"
            disabled={session.active || session.busy}
            title="Keep ADB available after unplugging USB"
            onClick={() => void session.switchUsbToWifi()}
          >
            <Wifi /> USB to Wi‑Fi
          </button>
        )}
        <div className="capture-step">
          <span>
            <Smartphone /> 2. App
          </span>
          <select
            value={session.selectedApp ?? ""}
            disabled={!session.device || session.active || session.busy}
            onChange={(event) =>
              session.selectApp(event.target.value || undefined)
            }
          >
            <option value="">Select a debuggable app</option>
            {session.apps.map((app) => (
              <option key={app.package_name}>{app.package_name}</option>
            ))}
          </select>
        </div>
        <button
          className="secondary"
          disabled={!session.device || session.active || session.busy}
          onClick={() => void session.installCertificate()}
        >
          <Shield /> Install HTTPS CA
        </button>
        {session.active ? (
          <button
            className="danger"
            disabled={session.busy}
            onClick={() => void session.stop()}
          >
            Stop capture
          </button>
        ) : (
          <button
            className="primary"
            disabled={session.busy || !session.device || !session.selectedApp}
            onClick={() => void session.start()}
          >
            <Activity /> Start capture
          </button>
        )}
      </section>
      <section className="capture-setup-summary">
        <span className={session.device ? "ready" : ""}>
          {session.device ? <Shield /> : <Usb />}{" "}
          {session.device
            ? `${session.device.connection_type.toUpperCase()} connected`
            : "Connect an authorized Android device"}
        </span>
        <span className={session.selectedApp ? "ready" : ""}>
          {session.selectedApp ? <Shield /> : <Smartphone />}{" "}
          {session.selectedApp ?? "Choose a debuggable app"}
        </span>
        <small>
          HTTPS traffic requires the one-time CA installation above. APIQA
          captures only the selected device session.
        </small>
      </section>
      <section className="hit-metrics">
        <Metric
          label="Requests"
          value={session.transactions.length}
          detail="This capture session"
        />
        <Metric
          label="Errors"
          value={errors}
          detail={errors ? "Needs review" : "No failures detected"}
          bad={errors > 0}
        />
        <Metric
          label="P95 response size"
          value={p95 ? formatBytes(p95) : "—"}
          detail="Captured responses"
        />
        <Metric
          label="Log activity"
          value={session.logs.length}
          detail="Recent app log lines"
        />
      </section>
      <section className="hits-content">
        <div className="hit-table panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">TRAFFIC</p>
              <h2>Captured requests</h2>
            </div>
            <span>{session.transactions.length} retained</span>
          </div>
          {session.transactions.length ? (
            <div className="hit-rows">
              {session.transactions
                .slice()
                .reverse()
                .map((transaction) => (
                  <button
                    key={transaction.id}
                    className={
                      transaction.id === selected?.id
                        ? "hit-row active"
                        : "hit-row"
                    }
                    onClick={() => setSelectedId(transaction.id)}
                  >
                    <span>
                      {new Date(transaction.started_at_ms).toLocaleTimeString()}
                    </span>
                    <code
                      className={`method method-${transaction.request.method.toLowerCase()}`}
                    >
                      {transaction.request.method}
                    </code>
                    <strong title={transaction.request.url}>
                      {transaction.request.url}
                    </strong>
                    <span
                      className={
                        (transaction.response?.status ?? 0) >= 400
                          ? "hit-status error"
                          : "hit-status"
                      }
                    >
                      {transaction.response?.status ?? "…"}
                    </span>
                  </button>
                ))}
            </div>
          ) : (
            <EmptyHits active={session.active} />
          )}
        </div>
        <HitInspector transaction={selected} />
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
  value: string | number;
  detail: string;
  bad?: boolean;
}) {
  return (
    <article className={bad ? "hit-metric bad" : "hit-metric"}>
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{detail}</small>
    </article>
  );
}

function EmptyHits({ active }: { active: boolean }) {
  return (
    <div className="hits-empty">
      <Activity />
      <strong>
        {active ? "Waiting for API activity" : "Start a capture session"}
      </strong>
      <p>
        {active
          ? "Requests from the selected Android app will appear here in real time."
          : "Choose an authorized device and debuggable app above."}
      </p>
    </div>
  );
}

function HitInspector({ transaction }: { transaction?: HttpTransaction }) {
  return (
    <aside className="hit-inspector panel">
      {transaction ? (
        <>
          <div className="panel-title">
            <div>
              <p className="eyebrow">SELECTED HIT</p>
              <h2>{transaction.request.method} request</h2>
            </div>
            <span className="status-code">
              {transaction.response?.status ?? "…"}
            </span>
          </div>
          <dl>
            <div>
              <dt>URL</dt>
              <dd>{transaction.request.url}</dd>
            </div>
            <div>
              <dt>Started</dt>
              <dd>{new Date(transaction.started_at_ms).toLocaleString()}</dd>
            </div>
            <div>
              <dt>Response body</dt>
              <dd>{transaction.response?.body.original_size ?? 0} B</dd>
            </div>
          </dl>
          <div className="hit-body">
            <span>Response preview</span>
            <pre>
              {transaction.response?.body.text || "Waiting for the response…"}
            </pre>
          </div>
        </>
      ) : (
        <div className="mini-empty">
          <Activity />
          <strong>Select a request</strong>
          <p>Its request and response details appear here.</p>
        </div>
      )}
    </aside>
  );
}

function percentile(transactions: HttpTransaction[], quantile: number) {
  const values = transactions
    .flatMap((item) =>
      item.response ? [item.response.body.original_size] : [],
    )
    .sort((left, right) => left - right);
  return values.length
    ? values[Math.min(values.length - 1, Math.floor(values.length * quantile))]
    : 0;
}
function formatBytes(value: number) {
  return value < 1024 ? `${value} B` : `${(value / 1024).toFixed(1)} KiB`;
}
