import { cubeForView, forTarget, type Doc, type Join } from "../model";
import type { Pick, Tab } from "./types";

export type TableCombine = "join" | "union" | "minus";

export function draftView(combine: TableCombine, tables: Doc[], target: string): Doc | undefined {
  if (!tables.length) return undefined;
  if (combine !== "join" && tables.length < 2) return undefined;
  const names = combine === "join" ? unionNames(tables) : sharedNames(tables);
  if (!names.length) return undefined;
  const view: Doc = {
    kind: "view",
    name: `${combine}_${tables.map((t) => t.name).join("_")}`,
    target,
    combine,
    description: "",
    sources: tables.map((t) => ({ kind: "table", table: t.name })),
    columns: names.map((name) => ({
      name,
      mappings: tables
        .filter((t) => columnNames(t).includes(name))
        .map((t) => ({ source: t.name, column: name })),
    })),
  };
  if (combine === "join") view.joins = joinEdges(tables);
  return view;
}

function columnNames(table: Doc): string[] {
  const raw = table.columns as { name?: string }[] | undefined;
  if (Array.isArray(raw)) {
    const names = raw.map((col) => col.name).filter((name): name is string => Boolean(name));
    if (names.length) return names;
  }
  if (table.pk?.length) return [...table.pk];
  return [table.name];
}

function unionNames(tables: Doc[]): string[] {
  const names: string[] = [];
  for (const table of tables) {
    for (const name of columnNames(table)) if (!names.includes(name)) names.push(name);
  }
  return names;
}

function sharedNames(tables: Doc[]): string[] {
  const sets = tables.map((table) => new Set(columnNames(table)));
  return columnNames(tables[0]).filter((name) => sets.every((set) => set.has(name)));
}

function joinEdges(tables: Doc[]): Join[] {
  if (tables.length < 2) return [];
  return tables.slice(0, -1).map((left, i) => {
    const right = tables[i + 1];
    return {
      left: left.name,
      right: right.name,
      type: "inner",
      on: [{ left_column: columnNames(left)[0], right_column: columnNames(right)[0] }],
    };
  });
}

export function detailDoc(
  docs: Doc[],
  tab: Tab,
  table: Doc | undefined,
  view: Doc | undefined,
  member: Pick,
): Doc | undefined {
  if (member?.kind === "metric" || member?.kind === "segment") {
    return docs.find((d) => d.kind === member.kind && d.name === member.name);
  }
  if (member?.kind === "dimension" || member?.kind === "fact") {
    const cube = view ? cubeForView(forTarget(docs, "cube", view.target || ""), view.name) : undefined;
    const list = member.kind === "dimension" ? cube?.dimensions : cube?.facts;
    const found = list?.find((m) => m.name === member.name);
    return found ? { ...found, kind: member.kind, name: found.name } : undefined;
  }
  if (tab === "physical") return table;
  if (member?.kind === "view" || view) return view;
  return table;
}

export function uniqueName(base: string, docs: Doc[]): string {
  if (!docs.some((d) => d.name === base)) return base;
  let n = 2;
  while (docs.some((d) => d.name === `${base}_${n}`)) n += 1;
  return `${base}_${n}`;
}

export function mergedColumns(views: Doc[]): Doc["columns"] {
  const names = new Set<string>();
  for (const view of views) for (const col of view.columns || []) names.add(col.name);
  return [...names].map((name) => ({
    name,
    mappings: views.flatMap((view) =>
      (view.columns || []).some((c) => c.name === name) ? [{ source: view.name, column: name }] : [],
    ),
  })).filter((col) => col.mappings.length > 0);
}
