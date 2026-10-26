import { describe, expect, it } from "vitest";
import {
  applyMappingRows,
  autoMatchColumns,
  autoMatchColumnsForSources,
  draftView,
  mappingRowsFromView,
  mappingSourceColumns,
  physicalTableDoc,
} from "./docs";
import type { Doc } from "../model";

const account = {
  kind: "table",
  name: "account",
  pk: ["id"],
  columns: [
    { name: "id", type: "int", nullable: false },
    { name: "name", type: "text", nullable: true },
  ],
} as Doc;
const order = {
  kind: "table",
  name: "order",
  pk: ["id"],
  columns: [
    { name: "id", type: "int", nullable: false },
    { name: "name", type: "text", nullable: true },
    { name: "account_id", type: "int", nullable: true },
  ],
} as Doc;

describe("draftView", () => {
  it("builds an identity join from one table", () => {
    const view = draftView("join", [account], "bird");
    expect(view?.combine).toBe("join");
    expect(view?.sources).toEqual([{ kind: "table", table: "account" }]);
    expect(view?.joins).toEqual([]);
  });

  it("does not auto-create joins when drafting a join view", () => {
    const view = draftView("join", [account, order], "bird");
    expect(view?.sources?.map((s) => s.table)).toEqual(["account", "order"]);
    expect(view?.joins).toEqual([]);
  });

  it("stacks union and minus on shared columns", () => {
    for (const combine of ["union", "minus"] as const) {
      const view = draftView(combine, [account, order], "bird");
      expect(view?.combine).toBe(combine);
      expect(view?.joins).toBeUndefined();
      expect(view?.columns?.map((c) => c.name)).toEqual(["id", "name"]);
    }
  });
});

describe("autoMatch / mapping rows", () => {
  it("auto-matches same names", () => {
    expect(autoMatchColumns([account, order]).map((c) => c.name)).toEqual(["id", "name", "account_id"]);
  });

  it("maps duplicate table sources by alias", () => {
    const cols = autoMatchColumnsForSources(
      [
        { kind: "table", table: "account" },
        { kind: "table", table: "account", alias: "bill_to" },
      ],
      [account],
    );
    expect(cols.find((c) => c.name === "id")?.mappings).toEqual([
      { source: "account", column: "id" },
      { source: "bill_to", column: "id" },
    ]);
  });

  it("lists unmapped physical columns with checkbox off", () => {
    const view: Doc = {
      kind: "view",
      name: "v",
      sources: [
        { kind: "table", table: "account" },
        { kind: "table", table: "order" },
      ],
      columns: [{ name: "id", mappings: [{ source: "account", column: "id" }] }],
    };
    const rows = mappingRowsFromView(view, [account, order]);
    expect(rows.filter((r) => r.active).map((r) => r.name)).toEqual(["id"]);
    expect(rows.some((r) => !r.active && r.name === "account_id")).toBe(true);
    const applied = applyMappingRows(view, rows.map((r) => (r.name === "name" ? { ...r, active: true } : r)));
    expect(applied.columns?.some((c) => c.name === "name")).toBe(true);
  });

  it("keeps union mapping columns in sources drag order", () => {
    const view: Doc = {
      kind: "view",
      name: "v",
      combine: "union",
      sources: [
        { kind: "table", table: "order" },
        { kind: "table", table: "account" },
      ],
    };
    expect(mappingSourceColumns(view, [account, order]).map((c) => c.label)).toEqual([
      "order",
      "account",
    ]);
  });

  it("orders join mapping columns by the join graph", () => {
    const view: Doc = {
      kind: "view",
      name: "v",
      combine: "join",
      sources: [
        { kind: "table", table: "order" },
        { kind: "table", table: "account" },
      ],
      joins: [{ left: "account", right: "order", type: "inner" }],
    };
    expect(mappingSourceColumns(view, [account, order]).map((c) => c.label)).toEqual([
      "account",
      "order",
    ]);
  });

  it("uses source aliases for unmapped mapping hints", () => {
    const view: Doc = {
      kind: "view",
      name: "v",
      sources: [
        { kind: "table", table: "account" },
        { kind: "table", table: "account", alias: "bill_to" },
      ],
      columns: [{
        name: "id",
        mappings: [
          { source: "account", column: "id" },
          { source: "bill_to", column: "id" },
        ],
      }],
    };
    const rows = mappingRowsFromView(view, [account]);
    const nameHint = rows.find((r) => !r.active && r.name === "name");
    expect(nameHint?.mappings).toEqual([
      { source: "account", column: "name" },
      { source: "bill_to", column: "name" },
    ]);
  });
});

describe("physicalTableDoc", () => {
  it("builds schema-shaped table metadata", () => {
    const doc = physicalTableDoc("bird", { schema: "main", name: "account" }, [
      { name: "id", data_type: "INTEGER", nullable: false },
    ]);
    expect(doc.kind).toBe("table");
    expect(doc.name).toBe("account");
    expect(doc.table).toBe("account");
    expect(doc.target).toBe("bird");
    expect(doc.db).toBe("bird");
    expect(doc.columns).toEqual([{ name: "id", type: "INTEGER", nullable: false }]);
  });
});
