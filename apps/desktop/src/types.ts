export type ProxyStatus = "stopped" | "starting" | "running" | "certificate_required" |
  "device_not_configured" | "partially_available" | "blocked_by_pinning" | "failed";
export interface ProxyConfiguration { bind_address:string; port:number; ca_certificate_path:string; ca_fingerprint_sha256?:string }
export interface HeaderEntry { name: string; value: string }
export interface QueryParameter { name: string; value: string }
export type BodyStorage =
  | { storage: "empty" }
  | { storage: "inline"; bytes: number[] }
  | { storage: "artifact"; artifact_id: string; preview: number[]; original_size: number }
  | { storage: "truncated"; preview: number[]; original_size?: number }
  | { storage: "unavailable"; reason: string };
export interface CapturedRequest {
  method: string; scheme: string; host: string; port?: number; path: string;
  query: QueryParameter[]; headers: HeaderEntry[]; body: BodyStorage;
  content_type?: string; http_version: string;
}
export interface CapturedResponse {
  status: number; reason?: string; headers: HeaderEntry[]; body: BodyStorage;
  content_type?: string; decoded_size: number; encoded_size: number; http_version: string;
}
export interface Difference {
  kind: string; path?: string; previous?: string; current?: string;
  severity: "critical" | "warning" | "informational"; ignored: boolean; explanation: string;
}
export interface HttpTransaction {
  id: string; session_id: string; state: string; request: CapturedRequest;
  response?: CapturedResponse; timing: {
    request_started_ms: number; request_complete_ms?: number;
    response_started_ms?: number; response_complete_ms?: number;
  };
  endpoint_identity?: { method: string; host: string; path_template: string };
  curl?: { compact: string; multiline: string; redacted: boolean };
  capture_quality: string;
  comparison?: { baseline_transaction_id?: string; compatibility: string; differences: Difference[] };
  correlated_incidents: string[]; created_at: string; updated_at: string;
}
export interface AndroidDevice {
  serial: string; connection_type: "usb" | "wireless" | "emulator";
  authorization_status: "authorized" | "unauthorized" | "offline" | "unknown";
  model?: string; android_version?: string; api_level?: number;
}
export interface AndroidApp {
  package_name: string; version_name?: string; version_code?: number; debuggable: boolean;
}
export interface QrPairingChallenge {
  id: string; service_name: string; qr_payload: string; qr_svg: string; expires_at: string;
}
export interface QrPairingResult { endpoint: string; adb_output: string }
export interface CompanionInstall { install_url: string; qr_svg: string }
export interface CompanionConnection { payload: string; qr_svg: string; token: string }
export interface UsbCompanionConnection { session_id: string; port: number }
export interface CompanionApp { package_name: string; label: string }
export interface AndroidCertificateInstall { remote_path: string; installer_output: string }
export interface AndroidCaStatus {
  state: "installed" | "not_installed" | "unknown";
  can_manage_automatically: boolean; detail: string;
}
export interface AndroidCaChange {
  status: AndroidCaStatus; requires_user_confirmation: boolean; rebooting: boolean;
}
export interface LogIncident {
  id: string; category: string; signature:string; title: string; message: string; occurrence_count: number;
  summary: string; root_cause?: string; foreground_activity?: string;
  first_app_frame?: string; where_occurred:string; how_occurred:string; likely_cause:string;
  reproduction_steps:string[];
  first_occurred_at:string; occurred_at: string; lines: { timestamp_ms:number; level:string; tag:string; message:string }[];
}
export interface ReplaySummary { attempted:number; completed:number; changed:number; skipped:number; failed:number }
export interface ComparisonRules { ignored_json_pointers:string[]; volatile_keys:string[] }
// ---- Composer (manual requests) ----
export type ManualBody =
  | { kind: "none" }
  | { kind: "form"; fields: [string, string][] }
  | { kind: "multipart"; fields: MultipartField[] }
  | { kind: "raw"; media_type?: string | null; text: string }
  | { kind: "binary"; bytes: number[] };
export interface MultipartField { name: string; value?: string | null; file?: string | null; media_type?: string | null }
export type AuthSpec =
  | { kind: "none" }
  | { kind: "bearer"; token: string }
  | { kind: "basic"; username: string; password: string }
  | { kind: "api_key"; key: string; value: string; in_query: boolean };
export interface ManualRequest {
  method: string; url: string; query: QueryParameter[]; headers: HeaderEntry[];
  body: ManualBody; auth: AuthSpec;
}
export interface SendOptions {
  timeout_ms: number; follow_redirects: boolean; max_redirects: number;
  verify_tls: boolean; proxy_url: string | null;
}
export interface SendResult {
  transaction_id: string; state: string; status: number; reason?: string | null;
  elapsed_ms: number; total_bytes: number; body: BodyStorage;
  content_type?: string | null; headers: HeaderEntry[]; http_version: string;
}
// ---- Collections & saved requests ----
export interface CollectionSummary {
  id: string; name: string; description: string; color: string;
  request_count: number; created_at: string; updated_at: string;
}
export interface SavedRequest {
  id: string; collection_id: string; name: string; request: ManualRequest;
  created_at: string; updated_at: string;
}
