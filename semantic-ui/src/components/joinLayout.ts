import dagre from "@dagrejs/dagre";
import type { Join } from "../model";

export const JOIN_NODE = { width: 140, height: 48 };

export function runJoinLayout(names: string[], joins?: Join[]) {
  const g = new dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir: "LR", nodesep: 40, ranksep: 80 });
  for (const name of names) g.setNode(name, { width: JOIN_NODE.width, height: JOIN_NODE.height });
  for (const j of joins || []) {
    if (names.includes(j.left) && names.includes(j.right)) g.setEdge(j.left, j.right);
  }
  dagre.layout(g);
  return g;
}

/** Left-to-right source labels matching JoinCanvas dagre ranks. */
export function joinGraphOrder(names: string[], joins?: Join[]): string[] {
  const g = runJoinLayout(names, joins);
  return names.slice().sort((a, b) => {
    const pa = g.node(a);
    const pb = g.node(b);
    const dx = (pa?.x ?? 0) - (pb?.x ?? 0);
    if (dx !== 0) return dx;
    return (pa?.y ?? 0) - (pb?.y ?? 0);
  });
}
