import { Badge, Button, Flex, Heading, IconButton, Text } from "@radix-ui/themes";
import { addViewColumn, removeViewColumn, type Doc } from "../model";
import { columnMeta, mappingSourceColumns, tableColumnNames } from "./docs";
import type { Selection, Tab } from "./types";

export function Inspector({
  open,
  onToggle,
  tab,
  view,
  tables,
  cube,
  metrics,
  segments,
  selected,
  onSelect,
  onOpenViewMapping,
  onOpenCubeMapping,
  onChangeView,
}: {
  open: boolean;
  onToggle: () => void;
  tab: Tab;
  view?: Doc;
  tables: Doc[];
  cube?: Doc;
  metrics: Doc[];
  segments: Doc[];
  selected: Selection;
  onSelect: (pick: Selection) => void;
  onOpenViewMapping: () => void;
  onOpenCubeMapping: () => void;
  onChangeView?: (view: Doc) => void;
}) {
  return (
    <aside className={open ? "inspector" : "inspector is-collapsed"} aria-label="Inspector">
      <div className="pane-bar pane-bar-start pane-bar-wrap" role="toolbar" aria-label="Inspector header">
        <IconButton
          size="1"
          variant="ghost"
          color="gray"
          aria-expanded={open}
          aria-label={open ? "Collapse inspector" : "Expand inspector"}
          onClick={onToggle}
        >
          {open ? "›" : "‹"}
        </IconButton>
        {open && (
          <>
            <Heading size="3" weight="medium" className="pane-bar-title">
              {tab === "tables" ? "Columns" : "Members"}
            </Heading>
            {tab === "tables" && view && (
              <>
                <Button size="1" variant="soft" onClick={onOpenViewMapping}>Map columns</Button>
                <Button
                  size="1"
                  variant="soft"
                  disabled={!onChangeView || !(view.sources || []).length}
                  onClick={() => {
                    const sources = mappingSourceColumns(view, tables);
                    const first = sources[0];
                    if (!first || !onChangeView) return;
                    const phys = tableColumnNames(first.table)[0] || "col";
                    const added = addViewColumn(view, { source: first.label, column: phys });
                    onChangeView(added.view);
                    onSelect({ kind: "column", name: added.name });
                  }}
                >
                  Add column
                </Button>
                <Button
                  size="1"
                  variant="soft"
                  color="red"
                  disabled={!onChangeView || selected?.kind !== "column"}
                  onClick={() => {
                    if (!onChangeView || selected?.kind !== "column") return;
                    onChangeView(removeViewColumn(view, selected.name));
                    onSelect({ kind: "view", name: view.name });
                  }}
                >
                  Delete column
                </Button>
              </>
            )}
            {tab === "cubes" && cube && (
              <Button size="1" variant="soft" onClick={onOpenCubeMapping}>Map members</Button>
            )}
          </>
        )}
      </div>
      {open && (
        <>
          {tab === "tables" ? (
            <ColumnsPanel
              view={view}
              tables={tables}
              selected={selected}
              onSelect={onSelect}
            />
          ) : (
            <MembersPanel
              cube={cube}
              metrics={metrics}
              segments={segments}
              selected={selected}
              onSelect={onSelect}
            />
          )}
        </>
      )}
    </aside>
  );
}

function ColumnsPanel({
  view,
  tables,
  selected,
  onSelect,
}: {
  view?: Doc;
  tables: Doc[];
  selected: Selection;
  onSelect: (pick: Selection) => void;
}) {
  if (!view) return <Text size="2" color="gray">Open a view to list columns.</Text>;
  return (
    <Flex direction="column" gap="2">
      {(view.columns || []).map((col) => {
        const meta = columnMeta(view, col.name, tables);
        const on = selected?.kind === "column" && selected.name === col.name;
        return (
          <button
            key={col.name}
            type="button"
            className={on ? "col-row on" : "col-row"}
            onClick={() => onSelect({ kind: "column", name: col.name })}
          >
            <Badge size="1" variant={meta.pk ? "solid" : "soft"} color={meta.pk ? "indigo" : "gray"}>
              {meta.pk ? "PK" : "—"}
            </Badge>
            <Text size="1" color="gray">{meta.type}</Text>
            <Text size="2" weight="medium">{col.name}</Text>
            <Text size="1" color="gray">{meta.description || ""}</Text>
          </button>
        );
      })}
      {!(view.columns || []).length && <Text size="2" color="gray">No columns yet. Use Add column or Map columns.</Text>}
    </Flex>
  );
}

function MembersPanel({
  cube,
  metrics,
  segments,
  selected,
  onSelect,
}: {
  cube?: Doc;
  metrics: Doc[];
  segments: Doc[];
  selected: Selection;
  onSelect: (pick: Selection) => void;
}) {
  if (!cube) return <Text size="2" color="gray">Select or create a cube.</Text>;
  return (
    <Flex direction="column" gap="2" className="tree">
      <Text size="2">cube {cube.name}</Text>
      <Group title="Dimensions" items={cube.dimensions || []} kind="dimension" selected={selected} onSelect={onSelect} />
      <Group title="Facts" items={cube.facts || []} kind="fact" selected={selected} onSelect={onSelect} />
      <Group title="Metrics" items={metrics} kind="metric" selected={selected} onSelect={onSelect} />
      <Group title="Segments" items={segments} kind="segment" selected={selected} onSelect={onSelect} />
    </Flex>
  );
}

function Group({
  title,
  items,
  kind,
  selected,
  onSelect,
}: {
  title: string;
  items: { name: string; description?: string; type?: string }[];
  kind: "dimension" | "fact" | "metric" | "segment";
  selected: Selection;
  onSelect: (pick: Selection) => void;
}) {
  return (
    <Flex direction="column" gap="1">
      <Text size="1" weight="medium" color="gray" className="group-label">{title}</Text>
      {items.map((item) => {
        const on = selected?.kind === kind && selected.name === item.name;
        return (
          <button
            key={item.name}
            type="button"
            className={on ? "col-row on" : "col-row"}
            onClick={() => onSelect({ kind, name: item.name })}
          >
            <Text size="2" weight="medium">{item.name}</Text>
            <Text size="1" color="gray">{item.type || ""}</Text>
            <Text size="1" color="gray">{item.description || ""}</Text>
          </button>
        );
      })}
    </Flex>
  );
}
