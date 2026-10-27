export type Doc = {
  kind: string;
  name: string;
  target?: string;
  cube?: string;
  view?: string;
  combine?: string;
  description?: string;
  pk?: string[];
  sources?: Source[];
  joins?: Join[];
  /** view columns (mappings) or table columns (type/nullable) */
  columns?: Array<{ name: string; mappings?: Mapping[]; type?: string; nullable?: boolean }>;
  dimensions?: Member[];
  facts?: Member[];
  choice?: CubeChoice[];
  prefer?: string;
  grain?: string[];
  role?: Role;
  group_by?: string[];
  encryption?: boolean;
  additive?: boolean;
  expression?: string;
  aggregation?: string;
  filter?: { expression?: string } | null;
  type?: string;
  column?: string;
  db?: string;
  schema?: string;
  table?: string;
  [key: string]: unknown;
};

export type Source = {
  kind: string;
  table: string;
  alias?: string;
};

export type CubeChoice = { view: string; summary?: boolean };

export type Relationship = "many_to_one" | "one_to_one" | "one_to_many";

export const JOIN_ON_OPS = ["=", "<>", "!=", "<", ">", "<=", ">="] as const;
export type JoinOnOp = (typeof JOIN_ON_OPS)[number];

export type JoinOn = {
  left_column: string;
  right_column: string;
  op?: JoinOnOp;
};

export type Join = {
  left: string;
  right: string;
  type: string;
  relationship?: Relationship;
  on?: JoinOn[];
};

export function joinOnOp(on: JoinOn): JoinOnOp {
  return on.op && (JOIN_ON_OPS as readonly string[]).includes(on.op) ? on.op : "=";
}

export type Mapping = { source: string; column: string };

export type ViewColumn = { name: string; mappings: Mapping[] };

export type AsCode = {
  table: string;
  key_column: string;
  value_column: string;
  where?: string;
};

export type Role = "all" | string[];

export type Member = {
  name: string;
  description?: string;
  type?: string;
  column?: string;
  expression?: string;
  encryption?: boolean;
  as_code?: AsCode;
  additive?: boolean;
  role?: Role;
};

export const DIM_TYPES = ["code", "date", "time", "datetime", "string"] as const;
export const FACT_TYPES = ["amount", "fx", "quantity"] as const;
export const AGGREGATIONS = ["group", "rollup", "cube", "grouping_sets"] as const;

export function roleText(role?: Role): string {
  if (!role || role === "all") return "all";
  return role.join(", ");
}

export function parseRole(text: string): Role | undefined {
  const trimmed = text.trim();
  if (!trimmed || trimmed === "all") return undefined;
  return trimmed.split(/[,\s]+/).filter(Boolean);
}

export function memberPayload(member: Member, kind: "dimension" | "fact"): Member {
  const next: Member = {
    name: member.name,
    description: member.description ?? "",
    type: member.type || (kind === "fact" ? "quantity" : "string"),
    encryption: member.encryption ?? false,
  };
  if ((member.expression || "").trim()) next.expression = member.expression!.trim();
  else if ((member.column || "").trim()) next.column = member.column!.trim();
  if (kind === "dimension" && member.as_code?.table && member.as_code.key_column && member.as_code.value_column) {
    next.as_code = { ...member.as_code };
  }
  if (kind === "fact" && member.additive === false) next.additive = false;
  if (member.role && member.role !== "all") next.role = member.role;
  return next;
}

export function applyCombine(view: Doc, combine: "join" | "union" | "minus"): Doc {
  const next: Doc = { ...view, combine };
  if (combine === "join") {
    next.joins = view.joins || [];
    return next;
  }
  delete next.joins;
  return next;
}

export function forTarget(docs: Doc[], kind: string, target: string): Doc[] {
  return docs
    .filter((d) => d.kind === kind && d.target === target)
    .sort((a, b) => a.name.localeCompare(b.name));
}

export function reorder<T>(items: T[], from: number, to: number): T[] {
  if (from === to || from < 0 || to < 0 || from >= items.length || to >= items.length) {
    return items.slice();
  }
  const next = items.slice();
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}

export function sourceLabel(source: Source): string {
  return source.alias || source.table;
}

export function uniqueSourceAlias(sources: Source[], table: string): string {
  const used = new Set(sources.map(sourceLabel));
  if (!used.has(table)) return table;
  let n = 2;
  while (used.has(`${table}_${n}`)) n += 1;
  return `${table}_${n}`;
}

export function setSourceAlias(view: Doc, current: string, nextRaw: string): Doc {
  const sources = view.sources || [];
  const idx = sources.findIndex((s) => sourceLabel(s) === current);
  if (idx < 0) return view;
  const table = sources[idx].table;
  const next = nextRaw.trim() || table;
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(next)) return view;
  if (sources.some((s, i) => i !== idx && sourceLabel(s) === next)) return view;
  const nextSources = sources.map((s, i) => {
    if (i !== idx) return s;
    return next === s.table ? { kind: s.kind, table: s.table } : { kind: s.kind, table: s.table, alias: next };
  });
  const joins = (view.joins || []).map((j) => ({
    ...j,
    left: j.left === current ? next : j.left,
    right: j.right === current ? next : j.right,
  }));
  const columns = (view.columns || []).map((c) => ({
    ...c,
    mappings: (c.mappings || []).map((m) => (m.source === current ? { ...m, source: next } : m)),
  }));
  return { ...view, sources: nextSources, joins, columns };
}

export function joinViewForTable(views: Doc[], table: string): Doc | undefined {
  return views.find(
    (v) =>
      v.combine === "join" &&
      (v.sources || []).some((s) => s.kind === "table" && (s.alias || s.table) === table || s.table === table),
  );
}

export function mappingFor(column: { mappings?: Mapping[] }, source: string): string {
  return (column.mappings || []).find((m) => m.source === source)?.column ?? "";
}

export function setMapping(view: Doc, viewColumn: string, source: string, physical: string): Doc {
  const columns = (view.columns || []).map((col) => {
    if (col.name !== viewColumn) return col;
    const current = col.mappings || [];
    const rest = current.filter((m) => m.source !== source);
    const mappings = physical ? [...rest, { source, column: physical }] : rest;
    return { ...col, mappings: mappings.length ? mappings : current };
  });
  return { ...view, columns };
}

export function addViewColumn(view: Doc, mapping: Mapping, base = "col"): { view: Doc; name: string } {
  const used = new Set((view.columns || []).map((c) => c.name));
  let name = base;
  let n = 2;
  while (used.has(name)) {
    name = `${base}_${n}`;
    n += 1;
  }
  return {
    view: { ...view, columns: [...(view.columns || []), { name, mappings: [mapping] }] },
    name,
  };
}

export function removeViewColumn(view: Doc, name: string): Doc {
  return { ...view, columns: (view.columns || []).filter((c) => c.name !== name) };
}

export function renameViewColumn(view: Doc, current: string, next: string): Doc {
  const name = next.trim();
  if ((view.columns || []).some((c) => c.name === name && c.name !== current)) return view;
  return {
    ...view,
    columns: (view.columns || []).map((c) => (c.name === current ? { ...c, name } : c)),
  };
}

export function withSourceOrder(view: Doc, sources: Source[]): Doc {
  return { ...view, sources };
}

export function choiceStack(cube?: Doc): string[] {
  if (!cube) return [];
  const names = (cube.choice || []).map((item) => item.view);
  if (cube.view) names.push(cube.view);
  return names;
}

export function withChoiceStack(cube: Doc, names: string[]): Doc {
  if (names.length === 0) {
    const next = { ...cube };
    delete next.view;
    delete next.choice;
    delete next.prefer;
    return next;
  }
  if (names.length === 1) {
    const next = { ...cube, view: names[0] };
    delete next.choice;
    delete next.prefer;
    return next;
  }
  return {
    ...cube,
    view: names[names.length - 1],
    choice: names.slice(0, -1).map((view) => ({ view, summary: true as const })),
    prefer: "summary",
  };
}

export function dropViewFromCube(cube: Doc, viewName: string): Doc {
  return withChoiceStack(cube, choiceStack(cube).filter((n) => n !== viewName));
}

export function cubeForView(cubes: Doc[], viewName: string): Doc | undefined {
  return cubes.find(
    (c) => c.view === viewName || (c.choice || []).some((item) => item.view === viewName),
  );
}

export function metricsForCube(docs: Doc[], cube: string): Doc[] {
  return docs
    .filter((d) => d.kind === "metric" && d.cube === cube)
    .sort((a, b) => a.name.localeCompare(b.name));
}

export function segmentsForCube(docs: Doc[], cube: string): Doc[] {
  return docs
    .filter((d) => d.kind === "segment" && d.cube === cube)
    .sort((a, b) => a.name.localeCompare(b.name));
}

export function combineName(names: string[]): string {
  const body = names
    .join("_")
    .replace(/[^A-Za-z0-9_]/g, "_")
    .replace(/^[^A-Za-z_]+/, "")
    .slice(0, 36);
  return `sel_${body || "view"}`;
}

/** @deprecated MCP leftover; prefer AuthorApi JSON errors */
export function toolPayload(rpc: {
  error?: { message?: string };
  result?: { isError?: boolean; content?: { text?: string }[] };
}): unknown {
  if (rpc.error) throw new Error(rpc.error.message || "mcp error");
  const result = rpc.result;
  if (result?.isError) {
    throw new Error(result.content?.[0]?.text || "tool error");
  }
  const text = result?.content?.[0]?.text;
  if (typeof text !== "string") return result ?? null;
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

export function setJoinRelationship(view: Doc, index: number, relationship?: Relationship): Doc {
  const joins = (view.joins || []).map((j, i) => {
    if (i !== index) return j;
    if (!relationship) {
      const next = { ...j };
      delete (next as { relationship?: string }).relationship;
      return next;
    }
    return { ...j, relationship };
  });
  return { ...view, joins };
}

export function addJoin(
  view: Doc,
  left: string,
  right: string,
  on?: JoinOn[],
): Doc {
  const joins = [...(view.joins || [])];
  if (joins.some((j) => (j.left === left && j.right === right) || (j.left === right && j.right === left))) {
    return view;
  }
  joins.push({
    left,
    right,
    type: "inner",
    on: on || [{ left_column: "id", right_column: "id" }],
  });
  return { ...view, joins };
}

export function reorderSources(view: Doc, from: number, to: number): Doc {
  const sources = reorder(view.sources || [], from, to);
  return { ...view, sources };
}
