import { useEffect, useState, type ReactNode } from "react";
import { Button, Flex, Heading, IconButton, Select, Text, TextField } from "@radix-ui/themes";
import {
  JOIN_ON_OPS,
  joinOnOp,
  setJoinRelationship,
  setSourceAlias,
  sourceLabel,
  type Doc,
  type JoinOn,
  type JoinOnOp,
  type Relationship,
} from "../model";
import { columnMeta, tableColumnNames } from "./docs";
import type { Selection } from "./types";

export function Detail({
  selection,
  table,
  view,
  tables,
  docs,
  cube,
  busy,
  onSave,
  onChangeView,
}: {
  selection: Selection;
  table?: Doc;
  view?: Doc;
  tables: Doc[];
  docs: Doc[];
  cube?: Doc;
  busy: boolean;
  onSave: (doc: Doc) => void;
  onChangeView?: (doc: Doc) => void;
}) {
  if (selection?.kind === "table" || (!selection && table && !view)) {
    const sourceKey = selection?.kind === "table" ? selection.name : undefined;
    const source = view?.sources?.find((s) => sourceLabel(s) === sourceKey);
    const doc =
      (source ? tables.find((t) => t.name === source.table) : undefined) ||
      table ||
      (sourceKey ? tables.find((t) => t.name === sourceKey) : undefined);
    return (
      <TableDetail
        doc={doc}
        view={view}
        sourceKey={source ? sourceKey : undefined}
        busy={busy}
        onSave={onSave}
        onChangeView={onChangeView}
      />
    );
  }
  if (selection?.kind === "join" && view) {
    const join = (view.joins || [])[selection.index];
    return <JoinDetail join={join} index={selection.index} view={view} tables={tables} onChangeView={onChangeView} />;
  }
  if (selection?.kind === "column" && view) {
    return <ColumnDetail view={view} columnName={selection.name} tables={tables} />;
  }
  if (selection?.kind === "dimension" || selection?.kind === "fact") {
    const list = selection.kind === "dimension" ? cube?.dimensions : cube?.facts;
    const found = list?.find((m) => m.name === selection.name);
    if (!found) return <Text size="2" color="gray">Member not found.</Text>;
    return (
      <MemberDetail
        draft={{ ...found, kind: selection.kind }}
        busy={busy}
        onSave={(d) => {
          if (!cube) return;
          const key = selection.kind === "dimension" ? "dimensions" : "facts";
          const next = ((cube[key] as { name: string }[]) || []).map((m) =>
            m.name === d.name ? { ...m, ...d } : m,
          );
          onSave({ ...cube, [key]: next });
        }}
      />
    );
  }
  if (selection?.kind === "metric" || selection?.kind === "segment") {
    const doc = docs.find((d) => d.kind === selection.kind && d.name === selection.name);
    return <EditableDocDetail doc={doc} busy={busy} onSave={onSave} />;
  }
  if (selection?.kind === "view" || view) {
    return <ViewDetail doc={view} onChangeView={onChangeView} />;
  }
  return <Text size="2" color="gray">Select a table, join, column, view, or cube member.</Text>;
}

type TableFk = { columns?: string[]; references?: { table?: string; columns?: string[] } };

function TableDetail({
  doc,
  view,
  sourceKey,
  busy,
  onSave,
  onChangeView,
}: {
  doc?: Doc;
  view?: Doc;
  sourceKey?: string;
  busy: boolean;
  onSave: (doc: Doc) => void;
  onChangeView?: (doc: Doc) => void;
}) {
  const [scope, setScope] = useState("");
  const [fallback, setFallback] = useState("");
  const [perms, setPerms] = useState<{ role: string; expression: string }[]>([]);
  const [aliasDraft, setAliasDraft] = useState("");
  useEffect(() => {
    if (!doc) return;
    setScope(filterExpression(doc.scope_filter));
    setFallback(filterExpression(doc.default_filter));
    const raw = Array.isArray(doc.permission_filters) ? doc.permission_filters : [];
    setPerms(
      raw.map((p) => ({
        role: String((p as { role?: string }).role || ""),
        expression: String((p as { expression?: string }).expression || ""),
      })),
    );
  }, [doc]);
  useEffect(() => {
    const found = view?.sources?.find((s) => sourceLabel(s) === sourceKey);
    setAliasDraft(found ? sourceLabel(found) : "");
  }, [view, sourceKey]);
  if (!doc) return <Text size="2" color="gray">Select a registered table.</Text>;
  const cols = (doc.columns as { name?: string; type?: string; nullable?: boolean }[]) || [];
  const name = String(doc.table || doc.name);
  const database = String(doc.target || doc.db || "");
  const fks = Array.isArray(doc.fk) ? (doc.fk as TableFk[]) : [];
  const source = view?.sources?.find((s) => sourceLabel(s) === sourceKey);

  function saveTable() {
    onSave({
      ...doc,
      scope_filter: toFilter(scope),
      default_filter: toFilter(fallback),
      permission_filters: perms.filter((p) => p.role.trim() && p.expression.trim()),
    });
  }

  return (
    <div className="table-detail">
      <div className="table-detail-col" role="region" aria-label="Table header">
        <Heading size="3" weight="medium" mb="3">table · {name}</Heading>
        <div className="fields">
          <Field label="Name"><Text size="2">{name}</Text></Field>
          {source && view && onChangeView ? (
            <Field label="Alias">
              <TextField.Root
                size="2"
                value={aliasDraft}
                onChange={(e) => {
                  const value = e.target.value;
                  setAliasDraft(value);
                  onChangeView(setSourceAlias(view, sourceLabel(source), value));
                }}
                aria-label="Source alias"
              />
            </Field>
          ) : null}
          <Field label="Database"><Text size="2">{database}</Text></Field>
          <Field label="PK"><Text size="2">{(doc.pk || []).join(", ") || "—"}</Text></Field>
        </div>
      </div>
      <div className="table-detail-col" role="region" aria-label="Table columns">
        <Heading size="2" mb="2">Columns</Heading>
        {cols.length ? (
          <table className="col-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Type</th>
                <th>Nullable</th>
                <th>FK</th>
              </tr>
            </thead>
            <tbody>
              {cols.map((c) => (
                <tr key={c.name}>
                  <td><Text size="2" weight="medium">{c.name}</Text></td>
                  <td><Text size="2" color="gray">{c.type || "?"}</Text></td>
                  <td><Text size="2" color="gray">{c.nullable ? "yes" : "no"}</Text></td>
                  <td><Text size="2" color="gray">{fkLabel(fks, c.name || "")}</Text></td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <Text size="2" color="gray">No columns.</Text>
        )}
      </div>
      <div className="table-detail-col" role="region" aria-label="Table filters">
        <Flex align="center" justify="between" mb="2">
          <Heading size="2">Filters</Heading>
          <Button size="1" disabled={busy} onClick={saveTable}>Save filters</Button>
        </Flex>
        <div className="filter-stack">
          <Flex className="filter-row" gap="2" align="center">
            <Text size="1" color="gray" className="filter-label">Scope</Text>
            <TextField.Root
              size="2"
              value={scope}
              onChange={(e) => setScope(e.target.value)}
              placeholder="expression"
              aria-label="Scope filter"
            />
            <IconButton size="1" variant="ghost" color="gray" onClick={() => setScope("")} aria-label="Remove scope filter">
              ×
            </IconButton>
          </Flex>
          <Flex className="filter-row" gap="2" align="center">
            <Text size="1" color="gray" className="filter-label">Default</Text>
            <TextField.Root
              size="2"
              value={fallback}
              onChange={(e) => setFallback(e.target.value)}
              placeholder="expression"
              aria-label="Default filter"
            />
            <IconButton size="1" variant="ghost" color="gray" onClick={() => setFallback("")} aria-label="Remove default filter">
              ×
            </IconButton>
          </Flex>
          {perms.map((p, i) => (
            <Flex key={i} className="filter-row" gap="2" align="center">
              <TextField.Root
                className="filter-role"
                size="2"
                value={p.role}
                onChange={(e) => setPerms(perms.map((row, j) => (j === i ? { ...row, role: e.target.value } : row)))}
                placeholder="role"
                aria-label="Permission role"
              />
              <TextField.Root
                size="2"
                value={p.expression}
                onChange={(e) => setPerms(perms.map((row, j) => (j === i ? { ...row, expression: e.target.value } : row)))}
                placeholder="expression"
                aria-label="Permission expression"
              />
              <IconButton
                size="1"
                variant="ghost"
                color="gray"
                onClick={() => setPerms(perms.filter((_, j) => j !== i))}
                aria-label="Remove permission filter"
              >
                ×
              </IconButton>
            </Flex>
          ))}
          <Button size="1" variant="soft" onClick={() => setPerms([...perms, { role: "", expression: "" }])}>
            Add permission filter
          </Button>
        </div>
      </div>
    </div>
  );
}

function filterExpression(value: unknown): string {
  if (!value || typeof value !== "object") return "";
  const expression = (value as { expression?: unknown }).expression;
  return typeof expression === "string" ? expression : "";
}

function toFilter(value: string): { expression: string } | null {
  const expression = value.trim();
  return expression ? { expression } : null;
}

function fkLabel(fks: TableFk[], column: string): string {
  for (const fk of fks) {
    const i = (fk.columns || []).indexOf(column);
    if (i < 0) continue;
    const table = fk.references?.table || "";
    const col = fk.references?.columns?.[i] || fk.references?.columns?.[0] || "";
    if (table && col) return `${table}.${col}`;
    if (table) return table;
  }
  return "—";
}

function JoinDetail({
  join,
  index,
  view,
  tables,
  onChangeView,
}: {
  join?: Doc["joins"] extends (infer J)[] | undefined ? J : never;
  index: number;
  view: Doc;
  tables: Doc[];
  onChangeView?: (doc: Doc) => void;
}) {
  if (!join) return <Text size="2" color="gray">Join not found.</Text>;
  const sources = (view.sources || []).map((s) => s.alias || s.table);
  const ons: JoinOn[] = join.on?.length ? join.on : [{ left_column: "", right_column: "" }];

  function columnsFor(sourceName: string): string[] {
    const src = (view.sources || []).find((s) => (s.alias || s.table) === sourceName);
    const table = tables.find((t) => t.name === sourceName || t.name === src?.table || t.table === src?.table);
    return table ? tableColumnNames(table) : [];
  }

  function patch(next: typeof join) {
    const joins = (view.joins || []).map((j, i) => (i === index ? next : j));
    onChangeView?.({ ...view, joins });
  }

  function firstColumn(sourceName: string, current: string): string {
    const cols = columnsFor(sourceName);
    return cols.includes(current) ? current : (cols[0] || current);
  }

  function patchOn(onIndex: number, next: Partial<JoinOn>) {
    const on = ons.map((item, i) => {
      if (i !== onIndex) return item;
      const merged: JoinOn = { ...item, ...next };
      if (!merged.op || merged.op === "=") {
        delete merged.op;
      }
      return merged;
    });
    patch({ ...join, on });
  }

  return (
    <div>
      <Heading size="3" weight="medium" mb="3">join · {join.left} → {join.right}</Heading>
      <div className="join-detail">
        <div className="join-detail-row pair" role="group" aria-label="Join start row">
          <JoinField label="Join start">
            <Select.Root
              size="2"
              value={join.left}
              onValueChange={(left) => {
                const on = ons.map((o) => ({ ...o, left_column: firstColumn(left, o.left_column) }));
                patch({ ...join, left, on });
              }}
            >
              <Select.Trigger aria-label="Join start" />
              <Select.Content>
                {sources.map((name) => (
                  <Select.Item key={name} value={name}>{name}</Select.Item>
                ))}
              </Select.Content>
            </Select.Root>
          </JoinField>
          {ons.map((o, onIndex) => (
            <JoinField key={`start-${onIndex}`} label="Join start column">
              <Select.Root
                size="2"
                value={o.left_column}
                onValueChange={(left_column) => patchOn(onIndex, { left_column })}
              >
                <Select.Trigger aria-label="Join start column" />
                <Select.Content>
                  {columnsFor(join.left).map((name) => (
                    <Select.Item key={name} value={name}>{name}</Select.Item>
                  ))}
                </Select.Content>
              </Select.Root>
            </JoinField>
          ))}
        </div>
        <div className="join-detail-row triple" role="group" aria-label="Join options row">
          {ons.map((o, onIndex) => (
            <JoinField key={`op-${onIndex}`} label="Join operator">
              <Select.Root
                size="2"
                value={joinOnOp(o)}
                onValueChange={(op) => patchOn(onIndex, { op: op as JoinOnOp })}
              >
                <Select.Trigger aria-label="Join operator" />
                <Select.Content>
                  {JOIN_ON_OPS.map((op) => (
                    <Select.Item key={op} value={op}>{op}</Select.Item>
                  ))}
                </Select.Content>
              </Select.Root>
            </JoinField>
          ))}
          <JoinField label="Join type">
            <Select.Root
              size="2"
              value={join.type || "inner"}
              onValueChange={(type) => patch({ ...join, type })}
            >
              <Select.Trigger aria-label="Join type" />
              <Select.Content>
                {(["inner", "left", "right", "full"] as const).map((t) => (
                  <Select.Item key={t} value={t}>{t}</Select.Item>
                ))}
              </Select.Content>
            </Select.Root>
          </JoinField>
          <JoinField label="Relationship">
            <Select.Root
              size="2"
              value={join.relationship || "none"}
              onValueChange={(value) => {
                onChangeView?.(
                  setJoinRelationship(
                    view,
                    index,
                    value === "none" ? undefined : (value as Relationship),
                  ),
                );
              }}
            >
              <Select.Trigger aria-label="Join relationship" />
              <Select.Content>
                <Select.Item value="none">unset</Select.Item>
                <Select.Item value="many_to_one">many_to_one</Select.Item>
                <Select.Item value="one_to_one">one_to_one</Select.Item>
                <Select.Item value="one_to_many">one_to_many</Select.Item>
              </Select.Content>
            </Select.Root>
          </JoinField>
        </div>
        <div className="join-detail-row pair" role="group" aria-label="Join end row">
          <JoinField label="Join end">
            <Select.Root
              size="2"
              value={join.right}
              onValueChange={(right) => {
                const on = ons.map((o) => ({ ...o, right_column: firstColumn(right, o.right_column) }));
                patch({ ...join, right, on });
              }}
            >
              <Select.Trigger aria-label="Join end" />
              <Select.Content>
                {sources.map((name) => (
                  <Select.Item key={name} value={name}>{name}</Select.Item>
                ))}
              </Select.Content>
            </Select.Root>
          </JoinField>
          {ons.map((o, onIndex) => (
            <JoinField key={`end-${onIndex}`} label="Join end column">
              <Select.Root
                size="2"
                value={o.right_column}
                onValueChange={(right_column) => patchOn(onIndex, { right_column })}
              >
                <Select.Trigger aria-label="Join end column" />
                <Select.Content>
                  {columnsFor(join.right).map((name) => (
                    <Select.Item key={name} value={name}>{name}</Select.Item>
                  ))}
                </Select.Content>
              </Select.Root>
            </JoinField>
          ))}
        </div>
      </div>
    </div>
  );
}

function ColumnDetail({
  view,
  columnName,
  tables,
}: {
  view: Doc;
  columnName: string;
  tables: Doc[];
}) {
  const meta = columnMeta(view, columnName, tables);
  return (
    <div>
      <Heading size="3" weight="medium" mb="3">column · {columnName}</Heading>
      <div className="fields">
        <Field label="PK"><Text size="2">{meta.pk ? "yes" : "no"}</Text></Field>
        <Field label="Type"><Text size="2">{meta.type}</Text></Field>
        <Field label="Description"><Text size="2">{meta.description || "—"}</Text></Field>
      </div>
      <Heading size="2" mt="4" mb="2">Mappings</Heading>
      {meta.mappings.map((m) => (
        <Text key={`${m.source}.${m.column}`} size="2" as="p">{m.source}.{m.column}</Text>
      ))}
    </div>
  );
}

function ViewDetail({ doc, onChangeView }: { doc?: Doc; onChangeView?: (doc: Doc) => void }) {
  if (!doc) return <Text size="2" color="gray">No view open.</Text>;
  return (
    <div>
      <Heading size="3" weight="medium" mb="3">view · {doc.name}</Heading>
      <div className="fields">
        <Field label="Combine"><Text size="2">{doc.combine}</Text></Field>
        <Field label="Description">
          <TextField.Root
            size="2"
            value={doc.description || ""}
            onChange={(e) => onChangeView?.({ ...doc, description: e.target.value })}
          />
        </Field>
      </div>
      <Text size="1" color="gray" as="p" mt="3">Save from the view header.</Text>
    </div>
  );
}

function MemberDetail({
  draft,
  busy,
  onSave,
}: {
  draft: Doc;
  busy: boolean;
  onSave: (doc: Doc) => void;
}) {
  const [local, setLocal] = useState(draft);
  useEffect(() => setLocal(draft), [draft]);
  return (
    <div>
      <Flex align="center" justify="between" mb="3">
        <Heading size="3" weight="medium">{local.kind} · {local.name}</Heading>
        <Button disabled={busy} onClick={() => onSave(local)}>Save</Button>
      </Flex>
      <div className="fields">
        <Field label="Description">
          <TextField.Root size="2" value={local.description || ""} onChange={(e) => setLocal({ ...local, description: e.target.value })} />
        </Field>
        <Field label="Type">
          <TextField.Root size="2" value={local.type || ""} onChange={(e) => setLocal({ ...local, type: e.target.value })} />
        </Field>
        <Field label="Column">
          <TextField.Root size="2" value={local.column || ""} onChange={(e) => setLocal({ ...local, column: e.target.value })} />
        </Field>
        <Field label="Expression">
          <TextField.Root size="2" value={local.expression || ""} onChange={(e) => setLocal({ ...local, expression: e.target.value })} />
        </Field>
      </div>
    </div>
  );
}

function EditableDocDetail({
  doc,
  busy,
  onSave,
}: {
  doc?: Doc;
  busy: boolean;
  onSave: (doc: Doc) => void;
}) {
  const [local, setLocal] = useState(doc);
  useEffect(() => setLocal(doc), [doc]);
  if (!local) return <Text size="2" color="gray">Not found.</Text>;
  return (
    <div>
      <Flex align="center" justify="between" mb="3">
        <Heading size="3" weight="medium">{local.kind} · {local.name}</Heading>
        <Button disabled={busy} onClick={() => onSave(local)}>Save</Button>
      </Flex>
      <div className="fields">
        <Field label="Description">
          <TextField.Root size="2" value={local.description || ""} onChange={(e) => setLocal({ ...local, description: e.target.value })} />
        </Field>
        {local.kind === "metric" && (
          <Field label="Expression">
            <TextField.Root size="2" value={local.expression || ""} onChange={(e) => setLocal({ ...local, expression: e.target.value })} />
          </Field>
        )}
        {local.kind === "segment" && (
          <Field label="Filter">
            <TextField.Root
              size="2"
              value={local.filter?.expression || ""}
              onChange={(e) => setLocal({ ...local, filter: { expression: e.target.value } })}
            />
          </Field>
        )}
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <Text size="1" color="gray">{label}</Text>
      <div>{children}</div>
    </>
  );
}

function JoinField({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="join-field">
      <Text size="1" color="gray">{label}</Text>
      {children}
    </div>
  );
}
