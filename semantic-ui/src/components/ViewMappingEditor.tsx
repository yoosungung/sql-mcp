import { useEffect, useMemo, useState } from "react";
import { Button, Checkbox, Dialog, Flex, Heading, Text, TextField } from "@radix-ui/themes";
import type { Doc } from "../model";
import {
  applyMappingRows,
  mappingRowsFromView,
  mappingSourceColumns,
  tableColumnNames,
  type MappingRow,
  type MappingSource,
} from "./docs";

export function ViewMappingEditor({
  open,
  onOpenChange,
  view,
  tables,
  onApply,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  view: Doc;
  tables: Doc[];
  onApply: (view: Doc) => void;
}) {
  const sources = useMemo(() => mappingSourceColumns(view, tables), [view, tables]);
  const [rows, setRows] = useState<MappingRow[]>([]);

  useEffect(() => {
    if (open) setRows(mappingRowsFromView(view, tables));
  }, [open, view, tables]);

  function patch(i: number, next: MappingRow) {
    setRows((cur) => cur.map((r, idx) => (idx === i ? next : r)));
  }

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Content className="mapping-dialog" aria-label="View mapping editor">
        <Flex align="center" justify="between" mb="3">
          <Heading size="4">Map view columns · {view.name}</Heading>
          <Flex gap="2">
            <Dialog.Close>
              <Button variant="soft" color="gray">Cancel</Button>
            </Dialog.Close>
            <Button
              onClick={() => {
                onApply(applyMappingRows(view, rows));
                onOpenChange(false);
              }}
            >
              Apply
            </Button>
          </Flex>
        </Flex>
        <Text size="2" color="gray" as="p" mb="3">
          Checkbox is the source of truth. Unmapped physical columns sit at the bottom.
        </Text>
        <div className="mapping-grid" style={{ gridTemplateColumns: `32px minmax(140px, 1fr) repeat(${Math.max(sources.length, 1)}, minmax(120px, 1fr))` }}>
          <div />
          <Text size="1" weight="medium">View column</Text>
          {sources.map((s) => (
            <Text key={s.label} size="1" weight="medium">{s.label}</Text>
          ))}
          {rows.map((row, i) => (
            <Row
              key={`${row.name}-${i}-${row.unmappedHint ? "h" : "a"}`}
              row={row}
              sources={sources}
              onChange={(next) => patch(i, next)}
            />
          ))}
        </div>
      </Dialog.Content>
    </Dialog.Root>
  );
}

function Row({
  row,
  sources,
  onChange,
}: {
  row: MappingRow;
  sources: MappingSource[];
  onChange: (row: MappingRow) => void;
}) {
  return (
    <>
      <Checkbox
        checked={row.active}
        aria-label={`map ${row.name}`}
        onCheckedChange={(v) => onChange({ ...row, active: v === true })}
      />
      {row.active ? (
        <TextField.Root
          size="1"
          value={row.name}
          aria-label={`name ${row.name}`}
          onChange={(e) => onChange({ ...row, name: e.target.value })}
        />
      ) : (
        <Text size="2" color="gray">{row.name}</Text>
      )}
      {sources.map((s) => {
        const mapping = row.mappings.find((m) => m.source === s.label);
        const options = tableColumnNames(s.table);
        return (
          <select
            key={s.label}
            className="map-select"
            disabled={!row.active}
            aria-label={`${row.name} → ${s.label}`}
            value={mapping?.column || ""}
            onChange={(e) => {
              const column = e.target.value;
              const rest = row.mappings.filter((m) => m.source !== s.label);
              onChange({
                ...row,
                mappings: column ? [...rest, { source: s.label, column }] : rest,
              });
            }}
          >
            <option value="">—</option>
            {options.map((c) => (
              <option key={c} value={c}>{c}</option>
            ))}
          </select>
        );
      })}
    </>
  );
}
