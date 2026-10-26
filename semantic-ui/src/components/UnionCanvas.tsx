import { useState } from "react";
import { Badge, Flex, Heading, Text } from "@radix-ui/themes";
import { reorder, sourceLabel, type Doc } from "../model";
import type { TableCombine } from "./docs";

export function UnionCanvas({
  view,
  combine,
  onChange,
  onSelectTable,
}: {
  view: Doc;
  combine: TableCombine;
  onChange: (view: Doc) => void;
  onSelectTable: (name: string) => void;
}) {
  const [from, setFrom] = useState<number | null>(null);
  const names = (view.sources || []).map((s) => sourceLabel(s));
  return (
    <Flex direction="column" gap="2" aria-label={`${combine} canvas`}>
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
          <Text size="1" color="gray">⋮⋮</Text>
          <Text weight="medium">{i + 1}</Text>
          <Text>{name}</Text>
          {i === 0 && <Badge variant="soft" size="1">Base</Badge>}
        </div>
      ))}
      {!names.length && <Text size="2" color="gray">Add tables from the catalog.</Text>}
    </Flex>
  );
}
