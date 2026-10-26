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
  type Node,
  type NodeProps,
} from "@xyflow/react";
import dagre from "@dagrejs/dagre";
import "@xyflow/react/dist/style.css";
import { Badge, Flex, Select, Text } from "@radix-ui/themes";
import type { Doc, Relationship } from "../model";

function layoutNodes(names: string[], joins: Doc["joins"]): { nodes: Node[]; edges: Edge[] } {
  const g = new dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir: "LR", nodesep: 40, ranksep: 80 });
  for (const name of names) g.setNode(name, { width: 140, height: 48 });
  for (const j of joins || []) {
    if (names.includes(j.left) && names.includes(j.right)) g.setEdge(j.left, j.right);
  }
  dagre.layout(g);
  const nodes: Node[] = names.map((name) => {
    const pos = g.node(name);
    return {
      id: name,
      type: "table",
      position: { x: (pos?.x || 0) - 70, y: (pos?.y || 0) - 24 },
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

export function JoinCanvas({
  view,
  onChange,
  onSelectJoin,
  onSelectTable,
  selectedJoinIndex,
}: {
  view: Doc;
  onChange: (view: Doc) => void;
  onSelectJoin: (index: number) => void;
  onSelectTable: (name: string) => void;
  selectedJoinIndex?: number;
}) {
  const names = useMemo(
    () => (view.sources || []).map((s) => s.alias || s.table),
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
    <div className="join-flow" aria-label="Join canvas">
      <ReactFlow
        nodes={nodes}
        edges={edges.map((e) => ({
          ...e,
          selected: e.data?.index === selectedJoinIndex,
        }))}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        onEdgeClick={(_, edge) => {
          const index = (edge.data as { index?: number } | undefined)?.index;
          if (typeof index === "number") onSelectJoin(index);
        }}
        onNodeClick={(_, node) => onSelectTable(node.id)}
        nodeTypes={nodeTypes}
        fitView
        proOptions={{ hideAttribution: true }}
      >
        <Background />
        <Controls showInteractive={false} />
      </ReactFlow>
      {typeof selectedJoinIndex === "number" && view.joins?.[selectedJoinIndex] && (
        <Flex align="center" gap="2" className="join-rel" mt="2">
          <Badge size="1">relationship</Badge>
          <Select.Root
            size="1"
            value={view.joins[selectedJoinIndex].relationship || "none"}
            onValueChange={(value) => {
              const joins = (view.joins || []).map((j, i) => {
                if (i !== selectedJoinIndex) return j;
                if (value === "none") {
                  const next = { ...j };
                  delete next.relationship;
                  return next;
                }
                return { ...j, relationship: value as Relationship };
              });
              onChange({ ...view, joins });
            }}
          >
            <Select.Trigger aria-label="Join relationship" />
            <Select.Content>
              <Select.Item value="none">unset</Select.Item>
              <Select.Item value="many_to_one">many_to_one</Select.Item>
              <Select.Item value="one_to_one">one_to_one</Select.Item>
              <Select.Item value="one_to_many">one_to_many</Select.Item>
            </Select.Content>
          </Select.Root>
        </Flex>
      )}
    </div>
  );
}
