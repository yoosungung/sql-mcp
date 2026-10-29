import { useEffect, useMemo, useState } from "react";
import { Button, Checkbox, Dialog, Flex, Heading, IconButton, Text, TextField } from "@radix-ui/themes";
import type { Doc } from "../model";
import {
  applyMappingRows,
  duplicateActiveNames,
  mappingDisplayOrder,
  mappingRowsFromView,
  mappingSourceColumns,
  splitMappingRow,
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
  const [order, setOrder] = useState<number[]>([]);

  useEffect(() => {
    if (!open) return;
    const next = mappingRowsFromView(view, tables);
    setRows(next);
    setOrder(mappingDisplayOrder(next));
    // Seed only when the dialog opens so renaming a column does not remount or jump.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  function patch(i: number, next: MappingRow) {
    setRows((cur) => cur.map((r, idx) => (idx === i ? next : r)));
  }

  function split(i: number) {
    const row = rows[i];
    if (!row) return;
    const source =
      [...sources].reverse().find((s) => row.mappings.some((m) => m.source === s.label))?.label
      ?? row.mappings[row.mappings.length - 1]?.source;
    if (!source) return;
    const next = splitMappingRow(rows, i, source);
    if (next.length === rows.length) return;
    const added = rows.length;
    setRows(next);
    setOrder((ord) => {
      if (ord.includes(added)) return ord;
      const at = ord.indexOf(i);
      if (at < 0) return [...ord, added];
      return [...ord.slice(0, at + 1), added, ...ord.slice(at + 1)];
    });
  }

  const dups = duplicateActiveNames(rows);

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
          Checkbox is the source of truth. Rows are sorted by name when opened (mapped, then unmapped). Duplicate names are marked. The split icon after the checkbox peels the last source mapping onto a new column.
        </Text>
        <div className="mapping-grid" style={{ gridTemplateColumns: `32px 28px minmax(140px, 1fr) repeat(${Math.max(sources.length, 1)}, minmax(120px, 1fr))` }}>
          <div />
          <div />
          <Text size="1" weight="medium">View column</Text>
          {sources.map((s) => (
            <Text key={s.label} size="1" weight="medium">{s.label}</Text>
          ))}
          {order.map((i) => {
            const row = rows[i];
            if (!row) return null;
            return (
              <Row
                key={i}
                row={row}
                sources={sources}
                duplicate={row.active && dups.has(row.name)}
                onChange={(next) => patch(i, next)}
                onSplit={() => split(i)}
              />
            );
          })}
        </div>
      </Dialog.Content>
    </Dialog.Root>
  );
}

function Row({
  row,
  sources,
  duplicate,
  onChange,
  onSplit,
}: {
  row: MappingRow;
  sources: MappingSource[];
  duplicate: boolean;
  onChange: (row: MappingRow) => void;
  onSplit: () => void;
}) {
  const canSplit = row.mappings.length > 1;
  return (
    <>
      <Checkbox
        checked={row.active}
        aria-label={`map ${row.name}`}
        onCheckedChange={(v) => onChange({ ...row, active: v === true })}
      />
      {canSplit ? (
        <IconButton
          size="1"
          variant="ghost"
          color="gray"
          type="button"
          aria-label={`split ${row.name}`}
          onClick={onSplit}
        >
          <SplitGlyph />
        </IconButton>
      ) : (
        <div />
      )}
      {row.active ? (
        <TextField.Root
          size="1"
          color={duplicate ? "red" : undefined}
          value={row.name}
          aria-label={`name ${row.name}`}
          aria-invalid={duplicate || undefined}
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

function SplitGlyph() {
  return (
    <svg className="split-glyph" viewBox="0 0 16 16" width="14" height="14" aria-hidden="true" fill="currentColor">
      <rect x="1.5" y="3" width="6" height="10" rx="1" />
      <rect x="8.5" y="3" width="6" height="4.5" rx="1" />
      <rect x="8.5" y="8.5" width="6" height="4.5" rx="1" />
    </svg>
  );
}
