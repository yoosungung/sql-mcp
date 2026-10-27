import { useState } from "react";
import { Badge, Flex, Heading, Text } from "@radix-ui/themes";
import { reorder, sourceLabel, type Doc } from "../model";
import type { TableCombine } from "./docs";
import { DragHandle } from "./DragHandle";

export function UnionCanvas({
  view,
  combine,
  onChange,
  onSelectTable,
  onTableDrop,
}: {
  view: Doc;
  combine: TableCombine;
  onChange: (view: Doc) => void;
  onSelectTable: (name: string) => void;
  onTableDrop?: (name: string) => void;
}) {
  const [from, setFrom] = useState<number | null>(null);
  const names = (view.sources || []).map((s) => sourceLabel(s));
  return (
    <Flex
      className="stack-canvas"
      direction="column"
      gap="2"
      aria-label={`${combine} canvas`}
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
      <Heading size="3" weight="medium">{combine}</Heading>
      <Text size="1" color="gray">Vertical stack. Drag to reorder sources.</Text>
      {names.map((name, i) => (
        <div
          key={`${name}-${i}`}
          className="stack-row"
          draggable
          onDragStart={() => setFrom(i)}
          onDragOver={(e) => e.preventDefault()}
          onDrop={() => {
            if (from !== null && from !== i) {
              onChange({ ...view, sources: reorder(view.sources || [], from, i) });
            }
            setFrom(null);
          }}
          onClick={() => onSelectTable(name)}
          role="button"
          tabIndex={0}
        >
          <DragHandle />
          <Text weight="medium">{i + 1}</Text>
          <Text>{name}</Text>
          {i === 0 && <Badge variant="soft" size="1">Base</Badge>}
        </div>
      ))}
      {!names.length && <Text size="2" color="gray">Add tables from the catalog.</Text>}
    </Flex>
  );
}
