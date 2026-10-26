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
  columns?: ViewColumn[];
  dimensions?: Member[];
  facts?: Member[];
  choice?: CubeChoice[];
  prefer?: string;
  expression?: string;
  aggregation?: string;
  filter?: { expression?: string } | null;
  type?: string;
  column?: string;
  [key: string]: unknown;
};

export type Source = {
  kind: string;
  table: string;
  alias?: string;
};

export type CubeChoice = { view: string; summary?: boolean };

export type Join = {
  left: string;
  right: string;
  type: string;
  on?: { left_column: string; right_column: string }[];
};

export type Mapping = { source: string; column: string };

export type ViewColumn = { name: string; mappings: Mapping[] };

export type Member = {
  name: string;
  description?: string;
  type?: string;
  column?: string;
  expression?: string;
};

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

export function joinViewForTable(views: Doc[], table: string): Doc | undefined {
  return views.find(
    (v) =>
      v.combine === "join" &&
      (v.sources || []).some((s) => s.kind === "table" && (s.alias || s.table) === table || s.table === table),
  );
}

export function mappingFor(column: ViewColumn, source: string): string {
  return column.mappings.find((m) => m.source === source)?.column ?? "";
}

export function setMapping(view: Doc, viewColumn: string, source: string, physical: string): Doc {
  const columns = (view.columns || []).map((col) => {
    if (col.name !== viewColumn) return col;
    const rest = col.mappings.filter((m) => m.source !== source);
    const mappings = physical ? [...rest, { source, column: physical }] : rest;
    return { ...col, mappings: mappings.length ? mappings : col.mappings };
  });
  return { ...view, columns };
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
  if (names.length <= 1) {
    const next = { ...cube, view: names[0] || cube.view };
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
