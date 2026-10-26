import { Badge, Button, Flex, Heading, IconButton, Text } from "@radix-ui/themes";
import type { Doc } from "../model";
import { columnMeta } from "./docs";
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
}) {
  return (
    <aside className={open ? "inspector" : "inspector is-collapsed"} aria-label="Inspector">
      <div className="pane-bar pane-bar-start">
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
      </div>
      {open && (
        <>
          {tab === "tables" ? (
            <ColumnsPanel
              view={view}
              tables={tables}
              selected={selected}
              onSelect={onSelect}
              onOpenMapping={onOpenViewMapping}
            />
          ) : (
            <MembersPanel
              cube={cube}
              metrics={metrics}
              segments={segments}
              selected={selected}
              onSelect={onSelect}
              onOpenMapping={onOpenCubeMapping}
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
  onOpenMapping,
}: {
  view?: Doc;
  tables: Doc[];
  selected: Selection;
  onSelect: (pick: Selection) => void;
  onOpenMapping: () => void;
}) {
  if (!view) return <Text size="2" color="gray">Open a view to list columns.</Text>;
  return (
    <Flex direction="column" gap="2">
      <Flex align="center" justify="between" gap="2">
        <Heading size="3" weight="medium">Columns</Heading>
        <Button size="1" variant="soft" onClick={onOpenMapping}>Map columns</Button>
      </Flex>
      <Text size="1" color="gray">{view.name}</Text>
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
      {!(view.columns || []).length && <Text size="2" color="gray">No columns yet. Use Map columns.</Text>}
    </Flex>
  );
}

function MembersPanel({
  cube,
  metrics,
  segments,
  selected,
  onSelect,
  onOpenMapping,
}: {
  cube?: Doc;
  metrics: Doc[];
  segments: Doc[];
  selected: Selection;
  onSelect: (pick: Selection) => void;
  onOpenMapping: () => void;
}) {
  if (!cube) return <Text size="2" color="gray">Select or create a cube.</Text>;
  return (
    <Flex direction="column" gap="2" className="tree">
      <Flex align="center" justify="between" gap="2">
        <Heading size="3" weight="medium">Members</Heading>
        <Button size="1" variant="soft" onClick={onOpenMapping}>Map members</Button>
      </Flex>
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
