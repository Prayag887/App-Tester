import { describe, expect, it } from "vitest";
import { createDemoCapture } from "./demo-data";
import { bodyImagePreviews, bodyText } from "./lib";

describe("offline demo capture", () => {
  it("covers success, change, failure, image, cURL, and diagnostics", () => {
    const demo = createDemoCapture(Date.parse("2026-08-18T12:00:00Z"));

    expect(demo.transactions).toHaveLength(5);
    expect(demo.transactions.some((item) => item.response?.status === 500)).toBe(true);
    expect(demo.transactions.some((item) => item.daily_changes?.count)).toBe(true);
    expect(demo.transactions.every((item) => item.curl?.redacted === false)).toBe(true);
    expect(demo.incidents).toHaveLength(1);

    const image = demo.transactions.find((item) => item.id === "demo-image")!;
    expect(bodyImagePreviews(image.response?.body, image.response?.content_type)).toHaveLength(1);

    const profile = demo.transactions.find((item) => item.id === "demo-profile")!;
    expect(bodyText(profile.response?.body)).toContain('"name": "Maya"');
  });
});
