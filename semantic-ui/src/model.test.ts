import { describe, expect, it } from "vitest";
import {
  applyCombine,
  combineName,
  joinOnOp,
  mappingFor,
  memberPayload,
  removeJoin,
  reorder,
  addViewColumn,
  removeViewColumn,
  renameViewColumn,
  setMapping,
  setSourceAlias,
  toolPayload,
  dropViewFromCube,
  uniqueSourceAlias,
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

  it("adds a uniquely named view column and can rename or remove it", () => {
    const added = addViewColumn(view, { source: "account_table", column: "name" });
    expect(added.name).toBe("col");
    expect(added.view.columns?.map((c) => c.name)).toEqual(["account_id", "col"]);
    const renamed = renameViewColumn(added.view, "col", "display_name");
    expect(renamed.columns?.map((c) => c.name)).toEqual(["account_id", "display_name"]);
    expect(removeViewColumn(renamed, "display_name").columns?.map((c) => c.name)).toEqual(["account_id"]);
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

  it("clears cube.view when the stack is empty", () => {
    const cube: Doc = { kind: "cube", name: "sales", view: "orders_customer" };
    const next = withChoiceStack(cube, []);
    expect(next.view).toBeUndefined();
    expect(next.choice).toBeUndefined();
  });

  it("drops a view from the cube stack and keeps the cube", () => {
    const cube: Doc = {
      kind: "cube",
      name: "sales",
      view: "accounts",
      choice: [{ view: "loans", summary: true }],
    };
    const next = dropViewFromCube(cube, "accounts");
    expect(next.name).toBe("sales");
    expect(next.view).toBe("loans");
    expect(next.choice).toBeUndefined();
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

describe("uniqueSourceAlias", () => {
  it("uses the table name when it is free, then table_2", () => {
    expect(uniqueSourceAlias([], "address")).toBe("address");
    expect(uniqueSourceAlias([{ kind: "table", table: "address" }], "address")).toBe("address_2");
  });
});

describe("joinOnOp", () => {
  it("defaults to equals when omitted", () => {
    expect(joinOnOp({ left_column: "id", right_column: "id" })).toBe("=");
    expect(joinOnOp({ left_column: "id", right_column: "id", op: ">" })).toBe(">");
  });
});

describe("setSourceAlias", () => {
  it("stores alias on the view source and rewrites joins", () => {
    const view: Doc = {
      kind: "view",
      name: "v",
      sources: [
        { kind: "table", table: "address" },
        { kind: "table", table: "address", alias: "address_2" },
      ],
      joins: [{ left: "address", right: "address_2", type: "inner", on: [{ left_column: "id", right_column: "id" }] }],
      columns: [{ name: "id", mappings: [{ source: "address_2", column: "id" }] }],
    };
    const next = setSourceAlias(view, "address_2", "bill_to");
    expect(next.sources).toEqual([
      { kind: "table", table: "address" },
      { kind: "table", table: "address", alias: "bill_to" },
    ]);
    expect(next.joins?.[0].right).toBe("bill_to");
    expect(next.columns?.[0].mappings?.[0].source).toBe("bill_to");
  });
});

describe("memberPayload", () => {
  it("drops kind and keeps schema fields", () => {
    const payload = memberPayload(
      {
        name: "id",
        description: "",
        type: "string",
        column: "id",
        encryption: false,
      },
      "dimension",
    );
    expect(payload).toEqual({
      name: "id",
      description: "",
      type: "string",
      column: "id",
      encryption: false,
    });
    expect("kind" in payload).toBe(false);
  });

  it("keeps expression instead of column when both were typed", () => {
    const payload = memberPayload(
      { name: "age", description: "", type: "quantity", column: "x", expression: "a - b", encryption: false },
      "fact",
    );
    expect(payload.expression).toBe("a - b");
    expect(payload.column).toBeUndefined();
  });
});

describe("removeJoin", () => {
  it("drops the join at the given index from the view document", () => {
    const view: Doc = {
      kind: "view",
      name: "v",
      combine: "join",
      joins: [
        { left: "a", right: "b", type: "inner", on: [{ left_column: "id", right_column: "id" }] },
        { left: "b", right: "c", type: "left", on: [{ left_column: "id", right_column: "id" }] },
      ],
    };
    const next = removeJoin(view, 0);
    expect(next.joins).toEqual([
      { left: "b", right: "c", type: "left", on: [{ left_column: "id", right_column: "id" }] },
    ]);
    expect(view.joins).toHaveLength(2);
  });
});

describe("applyCombine", () => {
  it("removes joins when switching to union", () => {
    const view: Doc = {
      kind: "view",
      name: "v",
      combine: "join",
      joins: [{ left: "a", right: "b", type: "inner", on: [{ left_column: "id", right_column: "id" }] }],
    };
    const next = applyCombine(view, "union");
    expect(next.combine).toBe("union");
    expect(next.joins).toBeUndefined();
  });
});
