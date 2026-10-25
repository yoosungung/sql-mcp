import { describe, expect, it } from "vitest";
import { draftView } from "./docs";
import type { Doc } from "../model";

const account = {
  kind: "table",
  name: "account",
  pk: ["id"],
  columns: [{ name: "id" }, { name: "name" }],
} as Doc;
const order = {
  kind: "table",
  name: "order",
  pk: ["id"],
  columns: [{ name: "id" }, { name: "name" }, { name: "account_id" }],
} as Doc;

describe("draftView", () => {
  it("builds an identity join from one table", () => {
    const view = draftView("join", [account], "bird");
    expect(view?.combine).toBe("join");
    expect(view?.sources).toEqual([{ kind: "table", table: "account" }]);
    expect(view?.joins).toEqual([]);
    expect(view?.prefer).toBeUndefined();
  });

  it("joins consecutive tables on their first column", () => {
    const view = draftView("join", [account, order], "bird");
    expect(view?.joins).toEqual([
      { left: "account", right: "order", type: "inner", on: [{ left_column: "id", right_column: "id" }] },
    ]);
    expect(view?.columns?.find((c) => c.name === "name")?.mappings).toEqual([
      { source: "account", column: "name" },
      { source: "order", column: "name" },
    ]);
  });

  it("stacks union and minus on every source without joins", () => {
    for (const combine of ["union", "minus"] as const) {
      const view = draftView(combine, [account, order], "bird");
      expect(view?.combine).toBe(combine);
      expect(view?.joins).toBeUndefined();
      expect(view?.columns?.map((c) => c.name)).toEqual(["id", "name"]);
      expect(view?.columns?.[0].mappings).toEqual([
        { source: "account", column: "id" },
        { source: "order", column: "id" },
      ]);
    }
  });

  it("refuses a set view with one table", () => {
    expect(draftView("union", [account], "bird")).toBeUndefined();
    expect(draftView("minus", [account], "bird")).toBeUndefined();
  });
});
