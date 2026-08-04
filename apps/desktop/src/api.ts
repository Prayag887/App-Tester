import { invoke } from "@tauri-apps/api/core";
import type { AndroidApp, AndroidCaChange, AndroidCertificateInstall, AndroidCaStatus, AndroidDevice, ComparisonRules, CompanionApp, CompanionConnection, CompanionInstall, HttpTransaction, ManualRequest, ProxyConfiguration, ProxyStatus, QrPairingChallenge, QrPairingResult, ReplaySummary, SendOptions, SendResult, UsbCompanionConnection } from "./types";
const native = () => "__TAURI_INTERNALS__" in window;
export const discoverDevices = async (): Promise<AndroidDevice[]> => native() ? invoke("discover_devices") : [];
export const listInstalledApps = async (serial: string): Promise<AndroidApp[]> => invoke("list_installed_apps", { serial });
export const launchInstalledApp = async (serial: string, packageName: string): Promise<void> => invoke("launch_installed_app", { serial, packageName });
export const getProxyStatus = async (): Promise<ProxyStatus> => native() ? invoke("get_proxy_status") : "stopped";
export const startProxy = async (): Promise<string> => invoke("start_proxy");
export const getProxyConfiguration = async (): Promise<ProxyConfiguration> => invoke("get_proxy_configuration");
export const stopProxy = async (): Promise<void> => invoke("stop_proxy");
export const startLogcatCapture = async (serial:string, packageName:string):Promise<void> =>
  invoke("start_logcat_capture", { serial, packageName });
export const generateCa = async (): Promise<{certificate_path:string;fingerprint_sha256:string}> => invoke("generate_ca_certificate");
export const configureAndroidProxy = async (serial:string, host:string,port:number):Promise<void> => invoke("configure_android_proxy",{serial,host,port});
export const clearAndroidProxy = async (serial:string):Promise<void> => invoke("clear_android_proxy",{serial});
export const getProxyHost = async (connectionType:string):Promise<string> =>
  invoke("get_proxy_host", { connectionType });
export const collectTransactionPages = async (
  fetchPage: (limit:number, offset:number) => Promise<HttpTransaction[]>,
  pageSize = 500,
): Promise<HttpTransaction[]> => {
  const transactions = new Map<string, HttpTransaction>();
  for (let offset = 0;; offset += pageSize) {
    const page = await fetchPage(pageSize, offset);
    page.forEach(transaction => transactions.set(transaction.id, transaction));
    if (page.length < pageSize) break;
  }
  return [...transactions.values()];
};
export const listTransactions = async (sessionId?: string):Promise<HttpTransaction[]> => native()
  ? collectTransactionPages((limit, offset) => invoke("list_transactions", {sessionId, limit, offset}))
  : [];
export const deleteAllTransactions = async ():Promise<void> => invoke("delete_all_transactions");
export const exportCapture = async ():Promise<string> => invoke("export_capture");
export const exportCaptureToFile = async ():Promise<string> => invoke("export_capture_to_file");
export const importCapture = async (payload:string):Promise<number> => invoke("import_capture", {payload});
export const testYesterdaysApis = async ():Promise<ReplaySummary> => invoke("test_yesterdays_apis");
export const approveBaseline = async (endpointId:string, transactionId:string):Promise<void> =>
  invoke("approve_baseline", { endpointId, transactionId });
export const deleteBaseline = async (endpointId:string):Promise<boolean> =>
  invoke("delete_baseline", { endpointId });
export const getComparisonRules = async (endpointId:string):Promise<ComparisonRules> =>
  invoke("get_comparison_rules", { endpointId });
export const saveComparisonRules = async (endpointId:string, rules:ComparisonRules):Promise<void> =>
  invoke("save_comparison_rules", { endpointId, ignoredJsonPointers:rules.ignored_json_pointers, volatileKeys:rules.volatile_keys });
export const beginQrPairing = async ():Promise<QrPairingChallenge> => invoke("begin_qr_pairing");
export const prepareCompanionInstall = async ():Promise<CompanionInstall> => invoke("prepare_companion_install");
export const prepareCompanionConnection = async (host:string):Promise<CompanionConnection> =>
  invoke("prepare_companion_connection", { host });
export const listCompanionApps = async (token:string):Promise<CompanionApp[]> => invoke("list_companion_apps", {token});
export const selectCompanionPackage = async (token:string, packageName:string):Promise<void> =>
  invoke("select_companion_package", {token, packageName});
export const startUsbCompanionCapture = async (serial:string, packageName:string):Promise<CompanionApp[]> =>
  invoke("start_usb_companion_capture", { serial, packageName });
export const openUsbCompanion = async (serial:string, packageName:string):Promise<UsbCompanionConnection> =>
  invoke("open_usb_companion", { serial, packageName });
export const stopUsbCompanionCapture = async (serial:string):Promise<void> =>
  invoke("stop_usb_companion_capture", { serial });
export const installCompanion = async (serial:string):Promise<string> => invoke("install_companion", { serial });
export const finishQrPairing = async (pairingId:string):Promise<QrPairingResult> => invoke("finish_qr_pairing",{pairingId});
export const pairWithCode = async (host:string, port:number, pairingCode:string):Promise<QrPairingResult> =>
  invoke("pair_with_code", { host, port, pairingCode });
export const enableUsbWifi = async (serial:string, port=5555):Promise<QrPairingResult> =>
  invoke("enable_usb_wifi", { serial, port });
export const captureScreen = async (serial:string):Promise<string> =>
  invoke("capture_screen", { serial });
export const sendRequest = async (request: ManualRequest, options: SendOptions): Promise<SendResult> =>
  invoke("send_request", { request, options });
export const prepareAndroidCertificateInstall = async (serial:string):Promise<AndroidCertificateInstall> =>
  invoke("prepare_android_certificate_install", { serial });
export const getAndroidCaStatus = async (serial:string, connectionType:string):Promise<AndroidCaStatus> =>
  invoke("get_android_ca_status", { serial, connectionType });
export const setAndroidCaUsage = async (serial:string, connectionType:string,useCa:boolean):Promise<AndroidCaChange> =>
  invoke("set_android_ca_usage", { serial, connectionType, useCa });
