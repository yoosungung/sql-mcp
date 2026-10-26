import { Button, Flex, Heading, IconButton, Table, Text, TextField } from "@radix-ui/themes";
import { mappingFor, setMapping, type Doc } from "../model";
import type { Pick, Tab } from "./types";

export function Inspector({
  open,
  onToggle,
  tab,
  mapping,
  onSaveMapping,
  source,
  selectedView,
  cube,
  metrics,
  segments,
  selected,
  onSelect,
}: {
  open: boolean;
  onToggle: () => void;
  tab: Tab;
  mapping?: Doc;
  onSaveMapping: (doc: Doc) => void;
  source?: string;
  selectedView?: Doc;
  cube?: Doc;
  metrics: Doc[];
  segments: Doc[];
  selected: Pick;
  onSelect: (pick: Pick) => void;
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
          {tab === "physical" && mapping ? (
            <JoinMapping view={mapping} source={source} onSave={onSaveMapping} />
          ) : tab === "physical" ? (
            <Text size="2" color="gray">Check tables to map columns.</Text>
          ) : (
            <CubeTree
              view={selectedView}
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

function JoinMapping({
  view,
  source,
  onSave,
}: {
  view: Doc;
  source?: string;
  onSave: (doc: Doc) => void;
}) {
  const src = source || view.sources?.find((s) => s.kind === "table")?.table || "";
  return (
    <Flex direction="column" gap="2">
      <Heading size="3" weight="medium">Column mapping</Heading>
      <Text size="1" color="gray">{view.name} · {src}</Text>
      <Table.Root size="1" variant="surface">
        <Table.Header>
          <Table.Row>
            <Table.ColumnHeaderCell>View column</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell>Physical column</Table.ColumnHeaderCell>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {(view.columns || []).map((col) => (
            <Table.Row key={col.name}>
              <Table.RowHeaderCell>{col.name}</Table.RowHeaderCell>
              <Table.Cell>
                <TextField.Root
                  size="1"
                  aria-label={col.name}
                  defaultValue={mappingFor(col, src)}
                  onBlur={(e) => {
                    if (e.target.value !== mappingFor(col, src)) {
                      onSave(setMapping(view, col.name, src, e.target.value));
                    }
                  }}
                />
              </Table.Cell>
            </Table.Row>
          ))}
        </Table.Body>
      </Table.Root>
    </Flex>
  );
}

function CubeTree({
  view,
  cube,
  metrics,
  segments,
  selected,
  onSelect,
}: {
  view?: Doc;
  cube?: Doc;
  metrics: Doc[];
  segments: Doc[];
  selected: Pick;
  onSelect: (pick: Pick) => void;
}) {
  if (!view) return <Text size="2" color="gray">Select a view.</Text>;
  return (
    <Flex direction="column" gap="2" className="tree">
      <Heading size="3" weight="medium">Cube</Heading>
      <Text size="2">view {view.name}</Text>
      {cube ? (
        <>
          <Text size="2">cube {cube.name}</Text>
          <Group title="Dimensions" items={cube.dimensions || []} kind="dimension" selected={selected} onSelect={onSelect} />
          <Group title="Facts" items={cube.facts || []} kind="fact" selected={selected} onSelect={onSelect} />
          <Group title="Metrics" items={metrics} kind="metric" selected={selected} onSelect={onSelect} />
          <Group title="Segments" items={segments} kind="segment" selected={selected} onSelect={onSelect} />
          <Text size="1" color="gray">Metrics and segments attach after dimensions and facts.</Text>
        </>
      ) : (
        <Text size="2" color="gray">No cube for this view yet.</Text>
      )}
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
  items: { name: string }[];
  kind: "dimension" | "fact" | "metric" | "segment";
  selected: Pick;
  onSelect: (pick: Pick) => void;
}) {
  return (
    <Flex direction="column" gap="1">
      <Text size="1" weight="medium" color="gray" className="group-label">{title}</Text>
      {items.map((item) => {
        const on = selected?.kind === kind && selected.name === item.name;
        return (
          <Button
            key={item.name}
            type="button"
            size="1"
            variant={on ? "soft" : "ghost"}
            color={on ? "indigo" : "gray"}
            style={{ justifyContent: "flex-start" }}
            onClick={() => onSelect({ kind, name: item.name })}
          >
            {item.name}
          </Button>
        );
      })}
    </Flex>
  );
}
