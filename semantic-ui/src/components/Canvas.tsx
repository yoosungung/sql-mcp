import { useState } from "react";
import { Badge, Button, Flex, Heading, Text } from "@radix-ui/themes";
import { sourceLabel, type Doc } from "../model";
import type { TableCombine } from "./docs";
import type { Tab } from "./types";

export function Canvas({
  tab,
  selectMode,
  selectedView,
  stackNames,
  joinView,
  checked,
  onDrop,
  combine,
  onCombine,
  picked,
  draft,
  busy,
  onSaveView,
  onDeleteView,
  onDiscard,
  viewBar,
  onDropTable,
}: {
  tab: Tab;
  selectMode: boolean;
  selectedView?: Doc;
  stackNames?: string[];
  joinView?: Doc;
  checked: string[];
  onDrop: (from: number, to: number) => void;
  combine: TableCombine;
  onCombine: (combine: TableCombine) => void;
  picked: string[];
  draft?: Doc;
  busy: boolean;
  onSaveView: () => void;
  onDeleteView: () => void;
  onDiscard: () => void;
  viewBar?: { title: string; unsaved: boolean; canSave: boolean; canDelete: boolean; canDiscard: boolean };
  onDropTable: (combine: TableCombine, table: string) => void;
}) {
  const [over, setOver] = useState<TableCombine | null>(null);
  if (tab === "cubes") {
    return (
      <main className="canvas" aria-label="Canvas">
        <Flex direction="column" gap="3">
          {viewBar && (
            <ViewBar {...viewBar} busy={busy} onSave={onSaveView} onDelete={onDeleteView} onDiscard={onDiscard} />
          )}
          {selectMode && (stackNames || []).length > 1 ? (
            <Stack names={stackNames || []} onDrop={onDrop} />
          ) : selectedView && isTableCombine(selectedView.combine) ? (
            <TableDraft combine={selectedView.combine} names={(selectedView.sources || []).map((s) => sourceLabel(s))} joins={selectedView.joins} />
          ) : (
            <CombineDiagram names={checked} />
          )}
        </Flex>
      </main>
    );
  }
  return (
    <main className="canvas" aria-label="Canvas">
      <Flex direction="column" gap="3">
        {viewBar && (
          <ViewBar {...viewBar} busy={busy} onSave={onSaveView} onDelete={onDeleteView} onDiscard={onDiscard} />
        )}
        <div className="combine-switch" role="group" aria-label="Combine">
          {(["join", "union", "minus"] as const).map((kind) => (
            <div
              key={kind}
              className={over === kind ? "drop-target is-over" : "drop-target"}
              onDragOver={(event) => {
                event.preventDefault();
                event.dataTransfer.dropEffect = "copy";
                setOver(kind);
              }}
              onDragLeave={(event) => {
                if (!event.currentTarget.contains(event.relatedTarget as Node)) setOver(null);
              }}
              onDrop={(event) => {
                event.preventDefault();
                event.stopPropagation();
                setOver(null);
                const name = event.dataTransfer.getData("text/plain");
                if (name) onDropTable(kind, name);
              }}
            >
              <Button
                type="button"
                size="1"
                variant={combine === kind ? "solid" : "soft"}
                color="gray"
                aria-pressed={combine === kind}
                onClick={() => onCombine(kind)}
              >
                {kind}
              </Button>
            </div>
          ))}
        </div>
        {draft ? (
          <TableDraft combine={combine} names={picked} joins={draft.joins} />
        ) : joinView ? (
          <JoinDiagram view={joinView} />
        ) : (
          <Text size="2" color="gray">Drag a table onto join, union, or minus.</Text>
        )}
      </Flex>
    </main>
  );
}

function isTableCombine(value: string | undefined): value is TableCombine {
  return value === "join" || value === "union" || value === "minus";
}

function ViewBar({
  title,
  unsaved,
  canSave,
  canDelete,
  canDiscard,
  busy,
  onSave,
  onDelete,
  onDiscard,
}: {
  title: string;
  unsaved: boolean;
  canSave: boolean;
  canDelete: boolean;
  canDiscard: boolean;
  busy: boolean;
  onSave: () => void;
  onDelete: () => void;
  onDiscard: () => void;
}) {
  const [ask, setAsk] = useState(false);
  return (
    <Flex direction="column" gap="2" className="view-bar">
      <Flex align="center" justify="between" gap="3">
        <Flex align="center" gap="2">
          <Heading size="3" weight="medium">{title}</Heading>
          {unsaved && <Badge variant="soft">Unsaved</Badge>}
        </Flex>
        <Flex align="center" gap="2">
          {canDiscard && (
            <Button type="button" size="1" variant="soft" color="gray" disabled={busy} onClick={onDiscard}>Discard</Button>
          )}
          {canDelete && (
            <Button type="button" size="1" variant="soft" color="red" disabled={busy} onClick={() => setAsk(true)}>Delete view</Button>
          )}
          <Button type="button" size="1" disabled={!canSave || busy} onClick={onSave}>Save view</Button>
        </Flex>
      </Flex>
      {ask && (
        <Flex align="center" justify="between" gap="3" role="alertdialog" aria-label="Delete view">
          <Text size="2">Delete {title}? Its cube, metrics, and segments are deleted with it.</Text>
          <Flex gap="2">
            <Button type="button" size="1" variant="soft" color="gray" onClick={() => setAsk(false)}>Cancel</Button>
            <Button type="button" size="1" color="red" aria-label="Confirm delete" onClick={() => { setAsk(false); onDelete(); }}>Delete</Button>
          </Flex>
        </Flex>
      )}
    </Flex>
  );
}

function TableDraft({
  combine,
  names,
  joins,
}: {
  combine: TableCombine;
  names: string[];
  joins?: Doc["joins"];
}) {
  return (
    <Flex direction="column" gap="2">
      <Heading size="3" weight="medium">{combine}</Heading>
      <Text size="1" color="gray">
        {combine === "join" ? "Lines connect consecutive tables." : "First table is the base."}
      </Text>
      {names.map((name, i) => (
        <div className={combine === "join" ? "node" : "stack-row"} key={name}>
          {combine !== "join" && <Text weight="medium">{i + 1}</Text>}
          <span>{name}</span>
          {combine !== "join" && i === 0 && <Badge variant="soft" size="1">Base</Badge>}
        </div>
      ))}
      {(joins || []).map((join, i) => (
        <Text key={i} size="1" color="gray">
          {join.type} {join.left}.{join.on?.[0]?.left_column} = {join.right}.{join.on?.[0]?.right_column}
        </Text>
      ))}
    </Flex>
  );
}

function JoinDiagram({ view }: { view?: Doc }) {
  if (!view) return <Text size="2" color="gray">No join view for this table.</Text>;
  return (
    <Flex direction="column" gap="2">
      <Heading size="3" weight="medium">Existing join</Heading>
      <Text size="1" color="gray">{view.name}</Text>
      {(view.sources || []).map((s) => (
        <div className="node" key={sourceLabel(s)}>
          {sourceLabel(s)}
        </div>
      ))}
      {(view.joins || []).map((j, i) => (
        <Text key={i} size="1" color="gray">
          {j.type} {j.on?.[0]?.left_column}
        </Text>
      ))}
    </Flex>
  );
}

function CombineDiagram({ names }: { names: string[] }) {
  if (!names.length) return <Text size="2" color="gray">Check views to preview the combine.</Text>;
  return (
    <Flex direction="column" gap="2">
      <Heading size="3" weight="medium">Choice</Heading>
      <Text size="1" color="gray">Checked views become one cube. Last is the base.</Text>
      {names.map((n) => (
        <div className="node" key={n}>
          {n}
        </div>
      ))}
    </Flex>
  );
}

function Stack({ names, onDrop }: { names: string[]; onDrop: (from: number, to: number) => void }) {
  const [from, setFrom] = useState<number | null>(null);
  return (
    <Flex direction="column" gap="2">
      <Heading size="3" weight="medium">Priority</Heading>
      <Text size="1" color="gray">Top is summary first. Last is the cube base. Drag to reorder.</Text>
      {names.map((name, i) => (
        <div
          key={`${name}-${i}`}
          className="stack-row"
          draggable
          onDragStart={() => setFrom(i)}
          onDragOver={(e) => e.preventDefault()}
          onDrop={() => {
            if (from !== null) onDrop(from, i);
            setFrom(null);
          }}
        >
          <Text size="1" color="gray">⋮⋮</Text>
          <Text weight="medium">{i + 1}</Text>
          <Text>{name}</Text>
          {i === 0 && <Badge variant="soft" size="1">Priority 1</Badge>}
          {i === names.length - 1 && i > 0 && <Badge variant="soft" color="gray" size="1">Base</Badge>}
        </div>
      ))}
    </Flex>
  );
}
