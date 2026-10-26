import { useState } from "react";
import { Badge, Flex, Heading, Text } from "@radix-ui/themes";
import type { Doc } from "../model";
import type { TableCombine } from "./docs";
import { JoinCanvas } from "./JoinCanvas";
import { UnionCanvas } from "./UnionCanvas";
import { CubeHeader, ViewHeader } from "./ViewHeader";
import type { Tab } from "./types";

export function Canvas({
  tab,
  selectedView,
  stackNames,
  checked,
  onDropStack,
  combine,
  onCombine,
  picked,
  draft,
  editing,
  busy,
  views,
  cube,
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
  onSaveCube,
  onDeleteCube,
  onRenameCube,
}: {
  tab: Tab;
  selectedView?: Doc;
  stackNames?: string[];
  checked: string[];
  onDropStack: (from: number, to: number) => void;
  combine: TableCombine;
  onCombine: (combine: TableCombine) => void;
  picked: string[];
  draft?: Doc;
  editing?: Doc;
  busy: boolean;
  views: Doc[];
  cube?: Doc;
  cubeDirty: boolean;
  onSaveView: () => void;
  onDeleteView: () => void;
  onNewView: (combine: TableCombine) => void;
  onOpenView: (name: string) => void;
  onRenameView: (name: string) => void;
  onChangeView: (view: Doc) => void;
  onDropTable: (combine: TableCombine, table: string) => void;
  onSelectTable: (name: string) => void;
  onSelectJoin: (index: number) => void;
  selectedJoinIndex?: number;
  onNewCube: () => void;
  onSaveCube: () => void;
  onDeleteCube: () => void;
  onRenameCube: (name: string) => void;
}) {
  const [over, setOver] = useState<TableCombine | null>(null);
  const live = draft || editing;

  if (tab === "cubes") {
    return (
      <main className="canvas" aria-label="Canvas">
        <Flex direction="column" gap="3" height="100%">
          <CubeHeader
            title={cube?.name || ""}
            unsaved={cubeDirty}
            busy={busy}
            canSave={cubeDirty && Boolean(cube)}
            canDelete={Boolean(cube)}
            onNew={onNewCube}
            onRename={onRenameCube}
            onSave={onSaveCube}
            onDelete={onDeleteCube}
          />
          <ChoiceStack
            names={stackNames?.length ? stackNames : checked}
            onDrop={onDropStack}
            onSelectView={(name) => onOpenView(name)}
          />
        </Flex>
      </main>
    );
  }

  const viewDirty = Boolean(draft) || (editing && selectedView && JSON.stringify(editing) !== JSON.stringify(selectedView));

  return (
    <main className="canvas" aria-label="Canvas">
      <Flex direction="column" gap="3" height="100%">
        <ViewHeader
          title={live?.name || "No view"}
          combine={(live?.combine as TableCombine) || combine}
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
            if (live) onChangeView({ ...live, combine: c });
          }}
        />
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
              <button
                type="button"
                className={combine === kind ? "combine-btn on" : "combine-btn"}
                aria-pressed={combine === kind}
                aria-label={kind}
                onClick={() => onCombine(kind)}
              >
                {kind}
              </button>
            </div>
          ))}
        </div>
        {live && ((live.combine as TableCombine) || combine) === "join" ? (
          <JoinCanvas
            view={live}
            onChange={onChangeView}
            onSelectJoin={onSelectJoin}
            onSelectTable={onSelectTable}
            selectedJoinIndex={selectedJoinIndex}
          />
        ) : live ? (
          <UnionCanvas
            view={live}
            combine={((live.combine as TableCombine) || combine) === "minus" ? "minus" : "union"}
            onChange={onChangeView}
            onSelectTable={onSelectTable}
          />
        ) : (
          <Text size="2" color="gray">New or Open a view, then drag tables onto join, union, or minus.</Text>
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
  onSelectView,
}: {
  names: string[];
  onDrop: (from: number, to: number) => void;
  onSelectView: (name: string) => void;
}) {
  const [from, setFrom] = useState<number | null>(null);
  return (
    <Flex direction="column" gap="2" aria-label="Choice stack">
      <Heading size="3" weight="medium">Choice stack</Heading>
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
          onClick={() => onSelectView(name)}
          role="button"
          tabIndex={0}
        >
          <Text size="1" color="gray">⋮⋮</Text>
          <Text weight="medium">{i + 1}</Text>
          <Text>{name}</Text>
          {i === 0 && names.length > 1 && <Badge variant="soft" size="1">Priority 1</Badge>}
          {i === names.length - 1 && <Badge variant="soft" color="gray" size="1">Base</Badge>}
        </div>
      ))}
      {!names.length && <Text size="2" color="gray">Check views on the left to build a stack.</Text>}
    </Flex>
  );
}
