import { describe, expect, it } from "vitest";
import {
  combineName,
  mappingFor,
  reorder,
  setMapping,
  toolPayload,
  withSourceOrder,
  type Doc,
} from "./model";

describe("reorder", () => {
  it("moves the dragged row and keeps the top as priority 1", () => {
    expect(reorder(["a", "b", "c"], 2, 0)).toEqual(["c", "a", "b"]);
  });
});

describe("setMapping", () => {
  const view: Doc = {
    kind: "view",
    name: "v",
    columns: [
      { name: "account_id", mappings: [{ source: "account_table", column: "account_id" }] },
    ],
  };

  it("writes a physical column onto the view column", () => {
    const next = setMapping(view, "account_id", "disp", "account_id");
    const col = next.columns![0];
    expect(mappingFor(col, "disp")).toBe("account_id");
    expect(mappingFor(col, "account_table")).toBe("account_id");
  });
});

describe("withSourceOrder", () => {
  it("marks only the first source as summary", () => {
    const view: Doc = { kind: "view", name: "s", combine: "select" };
    const next = withSourceOrder(view, [
      { kind: "view", table: "loan_view", summary: true },
      { kind: "view", table: "financial_all", summary: false },
    ]);
    expect(next.sources?.map((s) => s.table)).toEqual(["loan_view", "financial_all"]);
    expect(next.sources?.[0].summary).toBe(true);
    expect(next.sources?.[1].summary).toBe(false);
  });
});

describe("toolPayload", () => {
  it("parses MCP tool text", () => {
    expect(
      toolPayload({ result: { content: [{ text: '{"items":[1]}' }] } }),
    ).toEqual({ items: [1] });
  });
});

describe("combineName", () => {
  it("stays a semantic identifier", () => {
    expect(combineName(["financial_all", "loan_view"])).toMatch(/^[A-Za-z_][A-Za-z0-9_]*$/);
  });
});
