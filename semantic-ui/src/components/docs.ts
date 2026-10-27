import { cubeForView, forTarget, sourceLabel, type Doc, type Source, type ViewColumn } from "../model";
import type { Selection, Tab } from "./types";
import { joinGraphOrder } from "./joinLayout";

export type TableCombine = "join" | "union" | "minus";

export function tableColumnNames(table: Doc): string[] {
  const raw = table.columns as { name?: string }[] | undefined;
  if (Array.isArray(raw)) {
    const names = raw.map((col) => col.name).filter((name): name is string => Boolean(name));
    if (names.length) return names;
  }
  if (table.pk?.length) return [...table.pk];
  return [];
}

export function draftView(combine: TableCombine, tables: Doc[], target: string): Doc | undefined {
  if (!tables.length) return undefined;
  if (combine !== "join" && tables.length < 2) return undefined;
  const names = combine === "join" ? unionNames(tables) : sharedNames(tables);
  if (!names.length && combine !== "join") return undefined;
  const view: Doc = {
    kind: "view",
    name: `${combine}_${tables.map((t) => t.name).join("_")}`,
    target,
    combine,
    description: "",
    sources: tables.map((t) => ({ kind: "table", table: t.name })),
    columns: autoMatchColumns(tables, names.length ? names : undefined),
  };
  if (combine === "join") view.joins = [];
  return view;
}

function unionNames(tables: Doc[]): string[] {
  const names: string[] = [];
  for (const table of tables) {
    for (const name of tableColumnNames(table)) if (!names.includes(name)) names.push(name);
  }
  return names;
}

function sharedNames(tables: Doc[]): string[] {
  if (!tables.length) return [];
  const sets = tables.map((table) => new Set(tableColumnNames(table)));
  return tableColumnNames(tables[0]).filter((name) => sets.every((set) => set.has(name)));
}

/** Same-name columns across tables → one view column with multi-source mappings. */
const NAME_RE = /^[A-Za-z_][A-Za-z0-9_]*$/;

export function viewColumnName(physical: string, used: Set<string>): string {
  let name = NAME_RE.test(physical) ? physical : logicalTableName(physical);
  if (!used.has(name)) return name;
  let n = 2;
  while (used.has(`${name}_${n}`)) n += 1;
  return `${name}_${n}`;
}

export function autoMatchColumns(tables: Doc[], preferNames?: string[]): ViewColumn[] {
  const names = preferNames?.length ? preferNames : unionNames(tables);
  const used = new Set<string>();
  return names.map((physical) => {
    const name = viewColumnName(physical, used);
    used.add(name);
    return {
      name,
      mappings: tables
        .filter((t) => tableColumnNames(t).includes(physical))
        .map((t) => ({ source: t.name, column: physical })),
    };
  }).filter((col) => col.mappings.length > 0);
}

export function autoMatchColumnsForSources(sources: Source[], tables: Doc[]): ViewColumn[] {
  const names: string[] = [];
  for (const source of sources) {
    const table = tables.find((t) => t.name === source.table);
    if (!table) continue;
    for (const col of tableColumnNames(table)) if (!names.includes(col)) names.push(col);
  }
  const used = new Set<string>();
  return names
    .map((physical) => {
      const name = viewColumnName(physical, used);
      used.add(name);
      return {
        name,
        mappings: sources.flatMap((source) => {
          const table = tables.find((t) => t.name === source.table);
          if (!table || !tableColumnNames(table).includes(physical)) return [];
          return [{ source: sourceLabel(source), column: physical }];
        }),
      };
    })
    .filter((col) => col.mappings.length > 0);
}

export type MappingRow = {
  active: boolean;
  name: string;
  mappings: { source: string; column: string }[];
  /** true when this row is a candidate from an unmapped physical column */
  unmappedHint?: boolean;
};

export type MappingSource = { label: string; table: Doc };

/** Join: dagre left-to-right. Union/minus: `view.sources` drag order. Labels are aliases. */
export function mappingSourceColumns(view: Doc, tables: Doc[]): MappingSource[] {
  const cols: MappingSource[] = [];
  for (const source of view.sources || []) {
    const table = tables.find((t) => t.name === source.table);
    if (!table) continue;
    cols.push({ label: sourceLabel(source), table });
  }
  if (view.combine === "join") {
    const order = joinGraphOrder(cols.map((c) => c.label), view.joins);
    const rank = new Map(order.map((name, i) => [name, i]));
    cols.sort((a, b) => (rank.get(a.label) ?? 0) - (rank.get(b.label) ?? 0));
  }
  return cols;
}

export function mappingRowsFromView(view: Doc, tables: Doc[]): MappingRow[] {
  const active: MappingRow[] = (view.columns || []).map((col) => ({
    active: true,
    name: col.name,
    mappings: (col.mappings || []).map((m) => ({ ...m })),
  }));
  const mappedPhys = new Set(
    active.flatMap((row) => row.mappings.map((m) => `${m.source}.${m.column}`)),
  );
  const hints: MappingRow[] = [];
  for (const { label, table } of mappingSourceColumns(view, tables)) {
    for (const col of tableColumnNames(table)) {
      const key = `${label}.${col}`;
      if (mappedPhys.has(key)) continue;
      const existing = hints.find((h) => h.name === col);
      if (existing) {
        if (!existing.mappings.some((m) => m.source === label)) {
          existing.mappings.push({ source: label, column: col });
        }
        mappedPhys.add(key);
        continue;
      }
      hints.push({
        active: false,
        name: col,
        mappings: [{ source: label, column: col }],
        unmappedHint: true,
      });
      mappedPhys.add(key);
    }
  }
  return [...active, ...hints];
}

export function applyMappingRows(view: Doc, rows: MappingRow[]): Doc {
  return {
    ...view,
    columns: rows
      .filter((r) => r.active && r.name && r.mappings.length)
      .map((r) => ({ name: r.name, mappings: r.mappings })),
  };
}

export function columnMeta(
  view: Doc,
  columnName: string,
  tables: Doc[],
): { name: string; type: string; pk: boolean; mappings: ViewColumn["mappings"] } {
  const col = (view.columns || []).find((c) => c.name === columnName);
  const mappings = col?.mappings || [];
  let type = "";
  let pk = false;
  for (const m of mappings) {
    const src = (view.sources || []).find((s) => sourceLabel(s) === m.source);
    const table = tables.find((t) => t.name === (src?.table || m.source));
    if (!table) continue;
    const cols = (table.columns as { name?: string; type?: string }[] | undefined) || [];
    const phys = cols.find((c) => c.name === m.column);
    if (phys?.type && !type) type = phys.type;
    if ((table.pk || []).includes(m.column)) pk = true;
  }
  return {
    name: columnName,
    type: type || "—",
    pk,
    mappings,
  };
}

export function logicalTableName(physical: string): string {
  return physical.replace(/[^A-Za-z0-9_]/g, "_").replace(/^([^A-Za-z_])/, "_$1") || "table";
}

export function physicalTableDoc(
  target: string,
  physical: { schema: string; name: string },
  columns: { name: string; data_type?: string; nullable?: boolean }[],
): Doc {
  const logical = logicalTableName(physical.name);
  return {
    kind: "table",
    name: logical,
    target,
    description: "",
    db: target,
    schema: physical.schema || "main",
    table: physical.name,
    columns: columns.map((c) => ({
      name: c.name,
      type: c.data_type || "unknown",
      nullable: c.nullable ?? true,
    })),
    pk: [],
    fk: [],
    permission_filters: [],
    scope_filter: null,
    default_filter: null,
  };
}

export function viewsReferencingTable(views: Doc[], tableName: string): Doc[] {
  return views.filter((v) => (v.sources || []).some((s) => s.table === tableName));
}

export function cubesReferencingTable(cubes: Doc[], views: Doc[], tableName: string): Doc[] {
  const viewNames = new Set(viewsReferencingTable(views, tableName).map((v) => v.name));
  return cubes.filter(
    (c) =>
      (c.view && viewNames.has(c.view)) ||
      (c.choice || []).some((item) => viewNames.has(item.view)),
  );
}

export function detailDoc(
  docs: Doc[],
  tab: Tab,
  table: Doc | undefined,
  view: Doc | undefined,
  member: Selection,
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
  if (member?.kind === "column" && view) {
    return { kind: "column", name: member.name, view: view.name } as Doc;
  }
  if (member?.kind === "join" && view) {
    const join = (view.joins || [])[member.index];
    return join ? ({ kind: "join", name: `${join.left}-${join.right}`, ...join } as Doc) : undefined;
  }
  if (member?.kind === "table") return table;
  if (member?.kind === "view") return view;
  if (tab === "tables") return view || table;
  return view || table;
}

export function uniqueName(base: string, docs: Doc[]): string {
  if (!docs.some((d) => d.name === base)) return base;
  let n = 2;
  while (docs.some((d) => d.name === `${base}_${n}`)) n += 1;
  return `${base}_${n}`;
}
