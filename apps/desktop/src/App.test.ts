import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { App, bodyText, compactEndpoint, displayState, duration, endpointIsExcluded, endpointSuggestions, fullEndpoint, preferredDevice } from "./App";
import { collectTransactionPages } from "./api";
import type { AndroidDevice, HttpTransaction } from "./types";
const transaction = { response: undefined, timing: {request_started_ms:100}, comparison: undefined } as HttpTransaction;
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
  it("discovers USB-only devices and preserves a valid explicit selection", () => {
    const usb = {serial:"oneplus",connection_type:"usb",authorization_status:"authorized"} as AndroidDevice;
    const emulator = {serial:"emulator",connection_type:"emulator",authorization_status:"authorized"} as AndroidDevice;
    expect(preferredDevice("", [emulator, usb])).toBe("oneplus");
    expect(preferredDevice("emulator", [emulator, usb])).toBe("emulator");
    expect(preferredDevice("disconnected", [emulator, usb])).toBe("oneplus");
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
    expect(markup).toContain("without deleting saved comparison history");
    expect(markup).toContain("Inspect logs");
    expect(markup).toContain("Toolkit");
  });
});
