export type Tab = "physical" | "cubes";

export type Pick = {
  kind: "table" | "view" | "cube" | "metric" | "segment" | "dimension" | "fact";
  name: string;
} | null;
