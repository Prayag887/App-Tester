import { describe, expect, it } from "vitest";
import { collectTransactionPages } from "./api";
import type { HttpTransaction } from "./types";

describe("desktop capture pagination", () => {
  it("collects every page and deduplicates transactions by id", async () => {
    const transaction = (id: string) => ({ id }) as HttpTransaction;
    const pages = [
      [transaction("first"), transaction("second")],
      [transaction("second"), transaction("third")],
      [],
    ];
    let request = 0;
    const result = await collectTransactionPages(async () => pages[request++] ?? [], 2);
    expect(result.map(item => item.id)).toEqual(["first", "second", "third"]);
  });
});
