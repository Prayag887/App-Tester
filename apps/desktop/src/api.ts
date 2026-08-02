import { invoke } from "@tauri-apps/api/core";
import type { AndroidApp, AndroidCaChange, AndroidCertificateInstall, AndroidCaStatus, AndroidDevice, CompanionStatus, HttpTransaction, ProxyStatus, ReplaySummary } from "./types";
const native = () => "__TAURI_INTERNALS__" in window;
export const discoverDevices = async (): Promise<AndroidDevice[]> => native() ? invoke("discover_devices") : [];
export const listInstalledApps = async (serial: string): Promise<AndroidApp[]> => invoke("list_installed_apps", { serial });
export const launchInstalledApp = async (serial: string, packageName: string): Promise<void> => invoke("launch_installed_app", { serial, packageName });
export const getProxyStatus = async (): Promise<ProxyStatus> => native() ? invoke("get_proxy_status") : "stopped";
export const startProxy = async (): Promise<string> => invoke("start_proxy");
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
export const listTransactions = async ():Promise<HttpTransaction[]> => native()
  ? collectTransactionPages((limit, offset) => invoke("list_transactions", {limit, offset}))
  : [];
export const deleteAllTransactions = async ():Promise<void> => invoke("delete_all_transactions");
export const testYesterdaysApis = async ():Promise<ReplaySummary> => invoke("test_yesterdays_apis");
export const getCompanionStatus = async (serial:string):Promise<CompanionStatus> =>
  invoke("get_companion_status", { serial });
export const installCompanion = async (serial:string):Promise<CompanionStatus> =>
  invoke("install_companion", { serial });
export const openCompanion = async (serial:string, packageName?:string):Promise<void> =>
  invoke("open_companion", { serial, packageName });
export const removeUsbRelay = async (serial:string):Promise<void> =>
  invoke("remove_usb_relay", { serial });
export const prepareAndroidCertificateInstall = async (serial:string):Promise<AndroidCertificateInstall> =>
  invoke("prepare_android_certificate_install", { serial });
export const getAndroidCaStatus = async (serial:string, connectionType:string):Promise<AndroidCaStatus> =>
  invoke("get_android_ca_status", { serial, connectionType });
export const setAndroidCaUsage = async (serial:string, connectionType:string,useCa:boolean):Promise<AndroidCaChange> =>
  invoke("set_android_ca_usage", { serial, connectionType, useCa });
