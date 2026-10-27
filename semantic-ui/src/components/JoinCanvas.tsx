import { useCallback, useEffect, useMemo } from "react";
import {
  Background,
  Controls,
  Handle,
  MarkerType,
  Position,
  ReactFlow,
  addEdge,
  useEdgesState,
  useNodesState,
  type Connection,
  type Edge,
  type EdgeChange,
  type Node,
  type NodeChange,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Text } from "@radix-ui/themes";
import { sourceLabel, type Doc } from "../model";
import { JOIN_NODE, runJoinLayout } from "./joinLayout";

function layoutNodes(names: string[], joins: Doc["joins"]): { nodes: Node[]; edges: Edge[] } {
  const g = runJoinLayout(names, joins);
  const nodes: Node[] = names.map((name) => {
    const pos = g.node(name);
    return {
      id: name,
      type: "table",
      position: { x: (pos?.x || 0) - JOIN_NODE.width / 2, y: (pos?.y || 0) - JOIN_NODE.height / 2 },
      data: { label: name },
      sourcePosition: Position.Right,
      targetPosition: Position.Left,
    };
  });
  const edges: Edge[] = (joins || [])
    .filter((j) => names.includes(j.left) && names.includes(j.right))
    .map((j, i) => ({
      id: `e-${i}-${j.left}-${j.right}`,
      source: j.left,
      target: j.right,
      label: j.relationship || j.type,
      data: { index: i },
      markerEnd: { type: MarkerType.ArrowClosed },
    }));
  return { nodes, edges };
}

function TableNode({ data }: NodeProps) {
  return (
    <div className="rf-table-node">
      <Handle type="target" position={Position.Left} />
      <Text size="2" weight="medium">{String((data as { label?: string }).label || "")}</Text>
      <Handle type="source" position={Position.Right} />
    </div>
  );
}

const nodeTypes = { table: TableNode };

/** Initial fit-to-view never zooms in past 1:1 (UI font size). Manual Controls can still zoom. */
export const JOIN_FIT_VIEW = { maxZoom: 1 } as const;

export function JoinCanvas({
  view,
  onChange,
  onSelectJoin,
  onSelectTable,
  selectedJoinIndex,
  onRequestDeleteJoin,
  onTableDrop,
}: {
  view: Doc;
  onChange: (view: Doc) => void;
  onSelectJoin: (index: number) => void;
  onSelectTable: (name: string) => void;
  selectedJoinIndex?: number;
  onRequestDeleteJoin?: (index: number) => void;
  onTableDrop?: (name: string) => void;
}) {
  const names = useMemo(
    () => (view.sources || []).map(sourceLabel),
    [view.sources],
  );
  const seed = useMemo(() => layoutNodes(names, view.joins), [names, view.joins]);
  const [nodes, setNodes, onNodesChange] = useNodesState(seed.nodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(seed.edges);

  useEffect(() => {
    const next = layoutNodes(names, view.joins);
    setNodes(next.nodes);
    setEdges(next.edges);
  }, [names, view.joins, setNodes, setEdges]);

  useEffect(() => {
    if (typeof selectedJoinIndex !== "number") return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== "Delete" && event.key !== "Backspace") return;
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, [contenteditable='true'], [role='combobox'], [role='listbox'], [role='textbox']")) {
        return;
      }
      event.preventDefault();
      onRequestDeleteJoin?.(selectedJoinIndex);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [selectedJoinIndex, onRequestDeleteJoin]);

  const handleNodesChange = useCallback(
    (changes: NodeChange[]) => {
      onNodesChange(changes.filter((change) => change.type !== "remove"));
    },
    [onNodesChange],
  );
  const handleEdgesChange = useCallback(
    (changes: EdgeChange[]) => {
      onEdgesChange(changes.filter((change) => change.type !== "remove"));
    },
    [onEdgesChange],
  );
  const onBeforeDelete = useCallback(
    async ({ edges }: { nodes: Node[]; edges: Edge[] }) => {
      const indexes = edges
        .map((edge) => (edge.data as { index?: number } | undefined)?.index)
        .filter((index): index is number => typeof index === "number");
      const index = indexes[0] ?? selectedJoinIndex;
      if (typeof index === "number") onRequestDeleteJoin?.(index);
      return false;
    },
    [onRequestDeleteJoin, selectedJoinIndex],
  );

  const onConnect = useCallback(
    (connection: Connection) => {
      if (!connection.source || !connection.target) return;
      setEdges((eds) => addEdge(connection, eds));
      const joins = [...(view.joins || [])];
      if (!joins.some((j) => j.left === connection.source && j.right === connection.target)) {
        joins.push({
          left: connection.source,
          right: connection.target,
          type: "inner",
          on: [{ left_column: "id", right_column: "id" }],
        });
        onChange({ ...view, joins });
      }
    },
    [view, onChange, setEdges],
  );

  return (
    <div
      className="join-flow"
      aria-label="Join canvas"
      onDragOver={(event) => {
        if (!onTableDrop) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = "copy";
      }}
      onDrop={(event) => {
        const name = event.dataTransfer.getData("text/plain");
        if (name && onTableDrop) {
          event.preventDefault();
          event.stopPropagation();
          onTableDrop(name);
        }
      }}
    >
      <ReactFlow
        nodes={nodes}
        edges={edges.map((e) => ({
          ...e,
          selected: e.data?.index === selectedJoinIndex,
        }))}
        onNodesChange={handleNodesChange}
        onEdgesChange={handleEdgesChange}
        onBeforeDelete={onBeforeDelete}
        onConnect={onConnect}
        onEdgeClick={(_, edge) => {
          const index = (edge.data as { index?: number } | undefined)?.index;
          if (typeof index === "number") onSelectJoin(index);
        }}
        onNodeClick={(_, node) => onSelectTable(node.id)}
        nodeTypes={nodeTypes}
        fitView
        fitViewOptions={JOIN_FIT_VIEW}
        proOptions={{ hideAttribution: true }}
      >
        <Background />
        <Controls showInteractive={false} />
      </ReactFlow>
    </div>
  );
}
