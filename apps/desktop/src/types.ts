export interface KeyValue {
  key: string;
  value: string;
  enabled: boolean;
}
export interface ApiRequest {
  id: string;
  collection_id: string;
  folder_path: string[];
  name: string;
  method: string;
  url: string;
  headers: KeyValue[];
  query: KeyValue[];
  body_kind: string;
  body?: string;
  auth: { type: string; [key: string]: unknown };
  assertions: unknown[];
  extractions: unknown[];
  disabled: boolean;
}
export interface Collection {
  id: string;
  name: string;
  requests: ApiRequest[];
  variables: KeyValue[];
  imported_at: string;
  import_warnings: string[];
}
export interface Environment {
  id: string;
  name: string;
  variables: KeyValue[];
}
export interface Difference {
  kind: string;
  path: string;
  baseline?: unknown;
  current?: unknown;
  message: string;
}
export interface ResponseComparison {
  changed: boolean;
  differences: Difference[];
}
export interface ResponseSnapshot {
  status: number;
  headers: KeyValue[];
  content_type?: string;
  body: string;
  body_size: number;
  duration_ms: number;
  truncated: boolean;
}
export interface Execution {
  id: string;
  request_id: string;
  request_name: string;
  state: string;
  response?: ResponseSnapshot;
  error?: string;
  comparison?: ResponseComparison;
  assertions: { name: string; passed: boolean; message: string }[];
  extractions: { name: string; value: string; source: string }[];
}
export interface Run {
  id: string;
  collection_id: string;
  collection_name: string;
  environment_name?: string;
  started_at: string;
  completed_at?: string;
  state: string;
  baseline_run_id?: string;
  executions: Execution[];
  pinned: boolean;
}
export interface RetentionPolicy {
  days: number;
  max_bytes?: number;
}
export interface CleanupResult {
  deleted_runs: number;
  deleted_blobs: number;
  reclaimed_bytes: number;
}
export interface AndroidDevice {
  serial: string;
  connection_type: "usb" | "wireless" | "emulator";
  authorization_status: string;
  model?: string;
  android_version?: string;
  api_level?: number;
}
export interface AndroidApp {
  package_name: string;
  version_name?: string;
  debuggable: boolean;
}
export interface CapturedBody {
  text: string;
  original_size: number;
  truncated: boolean;
}
export interface HttpTransaction {
  id: string;
  started_at_ms: number;
  request: {
    method: string;
    url: string;
    headers: Record<string, string>;
    body: CapturedBody;
  };
  response?: {
    status: number;
    headers: Record<string, string>;
    body: CapturedBody;
  };
}
export interface LogLine {
  timestamp_ms: number;
  level: string;
  tag: string;
  message: string;
}
export interface Diagnostic {
  signature: string;
  category: string;
  title: string;
  count: number;
  lines: LogLine[];
}
