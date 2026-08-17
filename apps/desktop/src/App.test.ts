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

  it("stops at the configured transaction budget", async () => {
    const transaction = (id: string) => ({ id }) as HttpTransaction;
    const fetchPage = async (limit: number, offset: number) =>
      Array.from({ length: limit }, (_, index) => transaction(String(offset + index)));

    const result = await collectTransactionPages(fetchPage, 100, 250);

    expect(result).toHaveLength(250);
    expect(result.at(-1)?.id).toBe("249");
  });
});
