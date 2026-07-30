import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { App, baselineKey, bodyText, captureCleanupDevice, captureStartupPlan, compactEndpoint, developerIncidentReport, displayState, duration, endpointIsExcluded, endpointSuggestions, fullEndpoint, incidentLocation, logEvidence, preferredDevice, redactLogMessage, usbWifiHandoff } from "./App";
import { collectTransactionPages } from "./api";
import type { AndroidDevice, HttpTransaction, LogIncident } from "./types";
const transaction = { response: undefined, timing: {request_started_ms:100}, comparison: undefined } as HttpTransaction;
const incident = {title:"Checkout crash",category:"crash",occurrence_count:2,first_occurred_at:"2026-07-27T23:59:00Z",occurred_at:"2026-07-28T00:00:00Z",where_occurred:"at com.example.Checkout.submit",summary:"App crashed",how_occurred:"Tap led to null access",likely_cause:"NullPointerException",reproduction_steps:["Open checkout","Tap Pay"],lines:[{timestamp_ms:1,level:"E",tag:"AndroidRuntime",message:"FATAL EXCEPTION"}]} as LogIncident;
describe("traffic presentation", () => {
  it("shows pending rows immediately", () => expect(displayState(transaction)).toBe("Pending"));
  it("calculates completed duration", () => expect(duration({...transaction,timing:{request_started_ms:100,response_complete_ms:538}})).toBe(438));
  it("decodes inline bodies", () => expect(bodyText({storage:"inline",bytes:[123,125]})).toBe("{}"));
  it("marks schema comparison changes", () => expect(displayState({...transaction,response:{status:200},comparison:{
    baseline_transaction_id:"baseline",compatibility:"exact",differences:[{kind:"field_removed",severity:"critical",ignored:false,explanation:"Field was removed"}]}} as unknown as HttpTransaction)).toBe("Changed"));
  it("marks a first response as new", () => expect(displayState({...transaction,response:{status:200},comparison:{
    compatibility:"exact",differences:[]}} as unknown as HttpTransaction)).toBe("New"));
  it("matches full endpoints, host names, and host path prefixes in the negative filter", () => {
    const tx = {...transaction,request:{scheme:"https",host:"api.example.com",path:"/v1/users?active=true"}} as HttpTransaction;
    expect(fullEndpoint(tx)).toBe("https://api.example.com/v1/users?active=true");
    expect(endpointIsExcluded(tx, ["https://api.example.com/v1/users?active=true"])).toBe(true);
    expect(endpointIsExcluded(tx, ["example.com"])).toBe(true);
    expect(endpointIsExcluded(tx, ["api.example.com/v1"])).toBe(true);
    expect(endpointIsExcluded(tx, ["example.org"])).toBe(false);
  });
  it("suggests captured endpoints as the exclusion is typed", () => {
    const google = {...transaction,request:{scheme:"https",host:"google.com",path:"/search?q=api"}} as HttpTransaction;
    const github = {...transaction,request:{scheme:"https",host:"api.github.com",path:"/repos"}} as HttpTransaction;
    expect(endpointSuggestions([google, github, google], "goo")).toEqual(["https://google.com/search?q=api"]);
  });
  it("shortens excluded endpoints without losing their identifying path", () => {
    expect(compactEndpoint("https://www.api.example.com/v1/users")).toBe("api.example/v1/users");
    expect(compactEndpoint("http://service.dev:8080/health")).toBe("service:8080/health");
  });
  it("uses the normalized endpoint identity for a persistent baseline", () => {
    const tx = {...transaction, endpoint_identity:{method:"GET",host:"api.example.com",path_template:"/users/{id}"}} as HttpTransaction;
    expect(baselineKey(tx)).toBe("GET api.example.com /users/{id}");
  });
  it("prefers a USB device while preserving a valid explicit selection", () => {
    const usb = {serial:"oneplus",connection_type:"usb",authorization_status:"authorized"} as AndroidDevice;
    const emulator = {serial:"emulator",connection_type:"emulator",authorization_status:"authorized"} as AndroidDevice;
    expect(preferredDevice("", [emulator, usb])).toBe("oneplus");
    expect(preferredDevice("emulator", [emulator, usb])).toBe("emulator");
    expect(preferredDevice("disconnected", [emulator, usb])).toBe("oneplus");
  });
  it("keeps an active capture connected when USB hands off to Wi-Fi", () => {
    expect(usbWifiHandoff("192.168.1.44:5555", "com.example.app", true)).toEqual({
      endpoint: "192.168.1.44:5555", refreshProxyOwnership: true, restartLogcat: true,
      cleanupDevice: "192.168.1.44:5555",
    });
    expect(usbWifiHandoff("192.168.1.44:5555", "", true).restartLogcat).toBe(false);
    expect(usbWifiHandoff("192.168.1.44:5555", "", false).cleanupDevice).toBeUndefined();
  });
  it("keeps Logcat enabled when the Companion provides per-app traffic capture", () => {
    expect(captureStartupPlan("oneplus", true)).toEqual({
      configureSystemProxy: false,
      startLogcat: true,
    });
  });
  it("cleans up the device that was configured even if selection changes", () => {
    expect(captureCleanupDevice("192.168.1.44:5555", "emulator-5554")).toBe("192.168.1.44:5555");
    expect(captureCleanupDevice(undefined, "emulator-5554")).toBe("emulator-5554");
  });
  it("loads every transaction page instead of truncating the display at 250 hits", async () => {
    const hits = Array.from({length:620}, (_, index) => ({...transaction,id:`tx-${index}`} as HttpTransaction));
    const fetchPage = vi.fn(async (limit:number, offset:number) => hits.slice(offset, offset + limit));
    expect(await collectTransactionPages(fetchPage)).toHaveLength(620);
    expect(fetchPage).toHaveBeenCalledTimes(2);
  });
  it("renders a Delete all control for clearing captured APIs", () => {
    const markup = renderToStaticMarkup(createElement(App));
    expect(markup).toContain("Delete all");
    expect(markup).toContain("Export redacted");
    expect(markup).toContain("Import capture");
    expect(markup).toContain("Permanently delete all saved capture data from this computer");
    expect(markup).toContain("Inspect logs");
    expect(markup).toContain("Toolkit");
    expect(markup).toContain("Desktop host:");
    expect(markup).toContain('aria-label="Settings"');
    expect(markup).toContain('aria-label="Search captured traffic"');
    expect(markup).toContain('aria-label="Download Android companion"');
    expect(markup).not.toContain("Connect via QR");
    expect(markup).not.toContain("Pair with code");
    expect(markup).not.toContain("USB to Wi-Fi");
    expect(markup).toContain('<select aria-label="Package"');
    expect(markup).not.toContain('<select aria-label="Package" disabled');
  });
  it("shows an application frame as the incident location with a Logcat fallback", () => {
    const incident = {first_app_frame:"at com.example.Home.load(Home.kt:42)",foreground_activity:"com.example/.HomeActivity",lines:[
      {tag:"Home",level:"E",message:"failed",timestamp_ms:1},
    ]} as LogIncident;
    expect(incidentLocation(incident, "com.example")).toBe("com.example/.HomeActivity");
    expect(incidentLocation({...incident,first_app_frame:undefined,foreground_activity:undefined}, "com.example")).toBe("Home · Logcat");
  });
  it("creates one copyable evidence block with sensitive values redacted", () => {
    const lines = [{timestamp_ms:1,level:"D",tag:"Event",message:"firebaseAuthenticationToken=eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.signature"}];
    expect(redactLogMessage(lines[0].message)).not.toContain("eyJhbGci");
    expect(logEvidence(lines)).toBe("D Event: firebaseAuthenticationToken=[REDACTED]");
  });
});
describe("developer incident report", () => {
  it("includes diagnosis, reproduction, and evidence", () => {
    const report = developerIncidentReport(incident, "com.example");
    expect(report).toContain("Where: at com.example.Checkout.submit");
    expect(report).toContain("2. Tap Pay");
    expect(report).toContain("E AndroidRuntime: FATAL EXCEPTION");
  });
});
