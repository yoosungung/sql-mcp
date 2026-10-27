import { useEffect, useMemo, useState } from "react";
import { Button, Checkbox, Dialog, Flex, Heading, Text } from "@radix-ui/themes";
import { choiceStack, type Doc, type Member } from "../model";
import { columnMeta, suggestMemberKind, suggestMemberType } from "./docs";

type Row = {
  kind: "dimension" | "fact";
  name: string;
  active: boolean;
  /** viewName -> view column */
  byView: Record<string, string>;
  unmapped?: boolean;
};

export function CubeMappingEditor({
  open,
  onOpenChange,
  cube,
  views,
  tables = [],
  onApply,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  cube: Doc;
  views: Doc[];
  tables?: Doc[];
  onApply: (cube: Doc) => void;
}) {
  const stack = useMemo(() => choiceStack(cube), [cube]);
  const viewDocs = useMemo(
    () => stack.map((n) => views.find((v) => v.name === n)).filter((v): v is Doc => Boolean(v)),
    [stack, views],
  );

  const [rows, setRows] = useState<Row[]>([]);
  useEffect(() => {
    if (open) setRows(buildRows(cube, viewDocs, tables));
    // Seed only when the dialog opens so checkbox/kind edits are not reset.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const [drag, setDrag] = useState<{ view: string; column: string } | null>(null);

  function apply() {
    const dimensions: Member[] = [];
    const facts: Member[] = [];
    for (const row of rows) {
      if (!row.active) continue;
      // Prefer base view (last in stack) column as member.column
      const base = stack[stack.length - 1];
      const column = row.byView[base] || Object.values(row.byView).find(Boolean) || "";
      if (!column) continue;
      const viewDoc = viewDocs.find((v) => v.name === base) || viewDocs.find((v) => row.byView[v.name]);
      const meta = viewDoc ? columnMeta(viewDoc, column, tables) : { type: "", pk: false };
      const member: Member = {
        name: row.name,
        description: "",
        type: suggestMemberType(row.kind, meta.type === "—" ? "" : meta.type, meta.pk),
        column,
        encryption: false,
      };
      if (row.kind === "dimension") dimensions.push(member);
      else facts.push(member);
    }
    onApply({ ...cube, dimensions, facts });
    onOpenChange(false);
  }

  const mapped = rows.filter((r) => r.active && Object.values(r.byView).some(Boolean));
  const unmapped = rows.filter((r) => !r.active || !Object.values(r.byView).some(Boolean));

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Content className="mapping-dialog" aria-label="Cube mapping editor">
        <Flex align="center" justify="between" mb="3">
          <Heading size="4">Map cube members · {cube.name}</Heading>
          <Flex gap="2">
            <Dialog.Close>
              <Button variant="soft" color="gray">Cancel</Button>
            </Dialog.Close>
            <Button onClick={apply}>Apply</Button>
          </Flex>
        </Flex>
        <Text size="2" color="gray" as="p" mb="3">
          Drag a view column onto a dimension/fact row to remap. Unmapped members sit under 미매핑.
        </Text>
        <div className="mapping-grid" style={{ gridTemplateColumns: `32px 140px minmax(120px, 1fr) repeat(${Math.max(viewDocs.length, 1)}, minmax(120px, 1fr))` }}>
          <div />
          <Text size="1" weight="medium">Kind</Text>
          <Text size="1" weight="medium">Member</Text>
          {viewDocs.map((v) => (
            <Text key={v.name} size="1" weight="medium">{v.name}</Text>
          ))}
          {mapped.map((row, i) => (
            <MemberRow
              key={`m-${row.kind}-${row.name}-${i}`}
              row={row}
              views={viewDocs}
              drag={drag}
              setDrag={setDrag}
              onChange={(next) => setRows((cur) => cur.map((r) => (r === row ? next : r)))}
            />
          ))}
          <Text size="1" weight="medium" style={{ gridColumn: "1 / -1" }}>미매핑</Text>
          {unmapped.map((row, i) => (
            <MemberRow
              key={`u-${row.kind}-${row.name}-${i}`}
              row={row}
              views={viewDocs}
              drag={drag}
              setDrag={setDrag}
              onChange={(next) => setRows((cur) => cur.map((r) => (r === row ? next : r)))}
            />
          ))}
          {!unmapped.length && (
            <Text size="1" color="gray" style={{ gridColumn: "1 / -1" }}>없음</Text>
          )}
        </div>
        <Heading size="2" mt="4" mb="2">View columns (drag)</Heading>
        <Flex gap="2" wrap="wrap">
          {viewDocs.flatMap((v) =>
            (v.columns || []).map((c) => (
              <button
                key={`${v.name}.${c.name}`}
                type="button"
                className="chip"
                draggable
                onDragStart={() => setDrag({ view: v.name, column: c.name })}
                onDragEnd={() => setDrag(null)}
              >
                {v.name}.{c.name}
              </button>
            )),
          )}
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}

function MemberRow({
  row,
  views,
  drag,
  setDrag,
  onChange,
}: {
  row: Row;
  views: Doc[];
  drag: { view: string; column: string } | null;
  setDrag: (d: { view: string; column: string } | null) => void;
  onChange: (row: Row) => void;
}) {
  return (
    <>
      <Checkbox
        checked={row.active}
        aria-label={`map ${row.name}`}
        onCheckedChange={(v) => onChange({ ...row, active: v === true })}
      />
      <select
        className="map-select"
        aria-label={`kind ${row.name}`}
        value={row.kind}
        onChange={(e) => onChange({ ...row, kind: e.target.value as Row["kind"] })}
      >
        <option value="dimension">dimension</option>
        <option value="fact">fact</option>
      </select>
      <Text size="2">{row.name}</Text>
      {views.map((v) => (
        <div
          key={v.name}
          className="drop-cell"
          onDragOver={(e) => e.preventDefault()}
          onDrop={() => {
            if (!drag) return;
            onChange({
              ...row,
              active: true,
              byView: { ...row.byView, [v.name]: drag.column },
            });
            setDrag(null);
          }}
        >
          {row.byView[v.name] || "—"}
        </div>
      ))}
    </>
  );
}

function buildRows(cube: Doc, views: Doc[], tables: Doc[]): Row[] {
  const rows: Row[] = [];
  for (const d of cube.dimensions || []) {
    const byView: Record<string, string> = {};
    for (const v of views) {
      if ((v.columns || []).some((c) => c.name === d.column)) byView[v.name] = d.column || "";
    }
    rows.push({
      kind: "dimension",
      name: d.name,
      active: Boolean(d.column),
      byView,
    });
  }
  for (const f of cube.facts || []) {
    const byView: Record<string, string> = {};
    for (const v of views) {
      if ((v.columns || []).some((c) => c.name === f.column)) byView[v.name] = f.column || "";
    }
    rows.push({
      kind: "fact",
      name: f.name,
      active: Boolean(f.column),
      byView,
    });
  }
  // Candidate unmapped view columns as suggested dim/fact names
  const used = new Set(rows.map((r) => r.name));
  for (const v of views) {
    for (const c of v.columns || []) {
      if (used.has(c.name)) continue;
      used.add(c.name);
      const meta = columnMeta(v, c.name, tables);
      rows.push({
        kind: suggestMemberKind(meta.type === "—" ? "" : meta.type, meta.pk),
        name: c.name,
        active: false,
        byView: { [v.name]: c.name },
        unmapped: true,
      });
    }
  }
  return rows;
}
