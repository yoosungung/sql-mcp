export type Tab = "tables" | "cubes";

export type Selection =
  | { kind: "table"; name: string }
  | { kind: "view"; name: string }
  | { kind: "join"; index: number }
  | { kind: "column"; name: string }
  | { kind: "cube"; name: string }
  | { kind: "dimension" | "fact" | "metric" | "segment"; name: string }
  | null;

/** @deprecated use Selection */
export type Pick = Selection;

export type MappingEditor =
  | { mode: "view" }
  | { mode: "cube" }
  | null;
