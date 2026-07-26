import { describe, expect, it } from "vitest";
import { bodyText, displayState, duration, endpointIsExcluded, fullEndpoint } from "./App";
import type { HttpTransaction } from "./types";
const transaction = { response: undefined, timing: {request_started_ms:100}, comparison: undefined } as HttpTransaction;
describe("traffic presentation", () => {
  it("shows pending rows immediately", () => expect(displayState(transaction)).toBe("Pending"));
  it("calculates completed duration", () => expect(duration({...transaction,timing:{request_started_ms:100,response_complete_ms:538}})).toBe(438));
  it("decodes inline bodies", () => expect(bodyText({storage:"inline",bytes:[123,125]})).toBe("{}"));
  it("marks schema comparison changes", () => expect(displayState({...transaction,response:{status:200},comparison:{
    baseline_transaction_id:"baseline",compatibility:"exact",differences:[{kind:"field_removed",severity:"critical",ignored:false,explanation:"Field was removed"}]}} as unknown as HttpTransaction)).toBe("Changed"));
  it("marks a first response as new", () => expect(displayState({...transaction,response:{status:200},comparison:{
    compatibility:"exact",differences:[]}} as unknown as HttpTransaction)).toBe("New"));
  it("matches only the complete endpoint in the negative filter", () => {
    const tx = {...transaction,request:{scheme:"https",host:"api.example.com",path:"/v1/users?active=true"}} as HttpTransaction;
    expect(fullEndpoint(tx)).toBe("https://api.example.com/v1/users?active=true");
    expect(endpointIsExcluded(tx, ["https://api.example.com/v1/users?active=true"])).toBe(true);
    expect(endpointIsExcluded(tx, ["api.example.com", "https://api.example.com/v1/users"])).toBe(false);
  });
});
