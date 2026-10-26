import { describe, expect, it } from "vitest";
import {
  applyMappingRows,
  autoMatchColumns,
  draftView,
  mappingRowsFromView,
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

  it("joins consecutive tables on their first column", () => {
    const view = draftView("join", [account, order], "bird");
    expect(view?.joins).toEqual([
      { left: "account", right: "order", type: "inner", on: [{ left_column: "id", right_column: "id" }] },
    ]);
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
});

describe("physicalTableDoc", () => {
  it("builds schema-shaped table metadata", () => {
    const doc = physicalTableDoc("bird", { schema: "main", name: "account" }, [
      { name: "id", data_type: "INTEGER", nullable: false },
    ]);
    expect(doc.kind).toBe("table");
    expect(doc.table).toBe("account");
    expect(doc.columns).toEqual([{ name: "id", type: "INTEGER", nullable: false, mappings: [] }]);
  });
});
