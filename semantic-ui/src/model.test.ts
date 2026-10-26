import { describe, expect, it } from "vitest";
import {
  combineName,
  mappingFor,
  reorder,
  setMapping,
  toolPayload,
  withChoiceStack,
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

describe("withChoiceStack", () => {
  it("puts later names on cube.view and earlier names on summary choice", () => {
    const cube: Doc = { kind: "cube", name: "sales", view: "orders_customer" };
    const next = withChoiceStack(cube, ["loan_view", "financial_all"]);
    expect(next.view).toBe("financial_all");
    expect(next.prefer).toBe("summary");
    expect(next.choice).toEqual([{ view: "loan_view", summary: true }]);
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
