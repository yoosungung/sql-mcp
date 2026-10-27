import { useState, type DragEvent } from "react";
import { Badge, Flex, Heading, IconButton, Text } from "@radix-ui/themes";
import { applyCombine, type Doc } from "../model";
import type { TableCombine } from "./docs";
import { JoinCanvas } from "./JoinCanvas";
import { UnionCanvas } from "./UnionCanvas";
import { CubeHeader, ViewHeader } from "./ViewHeader";
import { DragHandle } from "./DragHandle";
import type { Tab } from "./types";

export function Canvas({
  tab,
  selectedView,
  stackNames,
  onDropStack,
  onRemoveFromStack,
  onViewDrop,
  combine,
  onCombine,
  picked,
  draft,
  editing,
  busy,
  views,
  cube,
  cubes,
  cubeDirty,
  onSaveView,
  onDeleteView,
  onNewView,
  onOpenView,
  onRenameView,
  onChangeView,
  onDropTable,
  onSelectTable,
  onSelectJoin,
  selectedJoinIndex,
  onNewCube,
  onOpenCube,
  onSaveCube,
  onDeleteCube,
  onRenameCube,
}: {
  tab: Tab;
  selectedView?: Doc;
  stackNames?: string[];
  onDropStack: (from: number, to: number) => void;
  onRemoveFromStack: (name: string) => void;
  onViewDrop: (name: string) => void;
  combine: TableCombine;
  onCombine: (combine: TableCombine) => void;
  picked: string[];
  draft?: Doc;
  editing?: Doc;
  busy: boolean;
  views: Doc[];
  cube?: Doc;
  cubes: Doc[];
  cubeDirty: boolean;
  onSaveView: () => void;
  onDeleteView: () => void;
  onNewView: (combine: TableCombine, name: string) => void;
  onOpenView: (name: string) => void;
  onRenameView: (name: string) => void;
  onChangeView: (view: Doc) => void;
  onDropTable: (combine: TableCombine, table: string) => void;
  onSelectTable: (name: string) => void;
  onSelectJoin: (index: number) => void;
  selectedJoinIndex?: number;
  onNewCube: () => void;
  onOpenCube: (name: string) => void;
  onSaveCube: () => void;
  onDeleteCube: () => void;
  onRenameCube: (name: string) => void;
}) {
  const live = draft || editing;
  const kind: TableCombine = ((live?.combine as TableCombine) || combine);

  function dropTable(event: DragEvent) {
    event.preventDefault();
    event.stopPropagation();
    const name = event.dataTransfer.getData("text/plain");
    if (name) onDropTable(kind, name);
  }

  if (tab === "cubes") {
    return (
      <main
        className="canvas"
        aria-label="Canvas"
        onDragOver={(event) => {
          event.preventDefault();
          event.dataTransfer.dropEffect = "copy";
        }}
        onDrop={(event) => {
          const name = event.dataTransfer.getData("text/plain");
          if (name) {
            event.preventDefault();
            onViewDrop(name);
          }
        }}
      >
        <Flex direction="column" gap="3" height="100%">
          <CubeHeader
            title={cube?.name || ""}
            unsaved={cubeDirty}
            busy={busy}
            canSave={cubeDirty && Boolean(cube)}
            canDelete={Boolean(cube)}
            cubes={cubes}
            onNew={onNewCube}
            onOpen={onOpenCube}
            onRename={onRenameCube}
            onSave={onSaveCube}
            onDelete={onDeleteCube}
          />
          <ChoiceStack
            names={stackNames || []}
            onDrop={onDropStack}
            onRemove={onRemoveFromStack}
            onSelectView={(name) => onOpenView(name)}
            onViewDrop={onViewDrop}
          />
        </Flex>
      </main>
    );
  }

  const viewDirty = Boolean(draft) || (editing && selectedView && JSON.stringify(editing) !== JSON.stringify(selectedView));

  return (
    <main
      className="canvas"
      aria-label="Canvas"
      onDragOver={(event) => {
        event.preventDefault();
        event.dataTransfer.dropEffect = "copy";
      }}
      onDrop={dropTable}
    >
      <Flex className="canvas-body" direction="column" gap="3" height="100%">
        <ViewHeader
          title={live?.name || "No view"}
          combine={kind}
          unsaved={Boolean(viewDirty)}
          busy={busy}
          canSave={Boolean(live)}
          canDelete={Boolean(editing) && !draft}
          views={views}
          onNew={onNewView}
          onOpen={onOpenView}
          onRename={onRenameView}
          onSave={onSaveView}
          onDelete={onDeleteView}
          onCombineChange={(c) => {
            onCombine(c);
            if (live) onChangeView(applyCombine(live, c));
          }}
        />
        {live && kind === "join" ? (
          <JoinCanvas
            view={live}
            onChange={onChangeView}
            onSelectJoin={onSelectJoin}
            onSelectTable={onSelectTable}
            selectedJoinIndex={selectedJoinIndex}
            onTableDrop={(name) => onDropTable(kind, name)}
          />
        ) : live ? (
          <UnionCanvas
            view={live}
            combine={kind === "minus" ? "minus" : "union"}
            onChange={onChangeView}
            onSelectTable={onSelectTable}
            onTableDrop={(name) => onDropTable(kind, name)}
          />
        ) : (
          <Text size="2" color="gray">Drag tables onto the canvas. View type is the selector in the header.</Text>
        )}
        {!live && picked.length > 0 && (
          <Text size="1" color="gray">Picked: {picked.join(", ")}</Text>
        )}
      </Flex>
    </main>
  );
}

function ChoiceStack({
  names,
  onDrop,
  onRemove,
  onSelectView,
  onViewDrop,
}: {
  names: string[];
  onDrop: (from: number, to: number) => void;
  onRemove: (name: string) => void;
  onSelectView: (name: string) => void;
  onViewDrop: (name: string) => void;
}) {
  const [from, setFrom] = useState<number | null>(null);
  return (
    <Flex
      className="stack-canvas"
      direction="column"
      gap="2"
      aria-label="Choice stack"
      onDragOver={(event) => {
        event.preventDefault();
        event.dataTransfer.dropEffect = "copy";
      }}
      onDrop={(event) => {
        if (from !== null) return;
        const name = event.dataTransfer.getData("text/plain");
        if (name) {
          event.preventDefault();
          event.stopPropagation();
          onViewDrop(name);
        }
      }}
    >
      <Heading size="3" weight="medium">Choice stack</Heading>
      <Text size="1" color="gray">Top is summary first. Last is the cube base. Drag views from the catalog. × removes a view.</Text>
      {names.map((name, i) => (
        <div
          key={`${name}-${i}`}
          className="stack-row"
          draggable
          onDragStart={() => setFrom(i)}
          onDragOver={(e) => e.preventDefault()}
          onDrop={(event) => {
            if (from !== null) {
              event.preventDefault();
              event.stopPropagation();
              if (from !== i) onDrop(from, i);
              setFrom(null);
              return;
            }
          }}
          onClick={() => onSelectView(name)}
          role="button"
          tabIndex={0}
        >
          <DragHandle />
          <Text weight="medium">{i + 1}</Text>
          <Text>{name}</Text>
          {i === 0 && names.length > 1 && <Badge variant="soft" size="1">Priority 1</Badge>}
          {i === names.length - 1 && <Badge variant="soft" color="gray" size="1">Base</Badge>}
          <IconButton
            size="1"
            variant="ghost"
            color="gray"
            aria-label={`Remove ${name} from stack`}
            onPointerDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              onRemove(name);
            }}
          >
            ×
          </IconButton>
        </div>
      ))}
      {!names.length && <Text size="2" color="gray">Drag views from the catalog.</Text>}
    </Flex>
  );
}
