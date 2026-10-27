import { useEffect, useState, type ReactNode } from "react";
import { Button, Checkbox, Flex, Heading, IconButton, Select, Text, TextField } from "@radix-ui/themes";
import {
  AGGREGATIONS,
  DIM_TYPES,
  FACT_TYPES,
  JOIN_ON_OPS,
  joinOnOp,
  mappingFor,
  memberPayload,
  parseRole,
  renameViewColumn,
  roleText,
  setJoinRelationship,
  setMapping,
  setSourceAlias,
  sourceLabel,
  type Doc,
  type Join,
  type JoinOn,
  type JoinOnOp,
  type Member,
  type Relationship,
} from "../model";
import { columnMeta, mappingSourceColumns, tableColumnNames } from "./docs";
import type { Selection, Tab } from "./types";

export function Detail({
  selection,
  tab,
  table,
  view,
  tables,
  docs,
  cube,
  metrics,
  segments,
  busy,
  onSave,
  onSelect,
  onChangeView,
}: {
  selection: Selection;
  tab?: Tab;
  table?: Doc;
  view?: Doc;
  tables: Doc[];
  docs: Doc[];
  cube?: Doc;
  metrics?: Doc[];
  segments?: Doc[];
  busy: boolean;
  onSave: (doc: Doc, previousName?: string) => void;
  onSelect?: (pick: Selection) => void;
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
    return (
      <ColumnDetail
        view={view}
        columnName={selection.name}
        tables={tables}
        onChangeView={onChangeView}
      />
    );
  }
  if (selection?.kind === "dimension" || selection?.kind === "fact") {
    const memberKind = selection.kind;
    const list = memberKind === "dimension" ? cube?.dimensions : cube?.facts;
    const found = list?.find((m) => m.name === selection.name);
    if (!found) return <Text size="2" color="gray">Member not found.</Text>;
    return (
      <MemberDetail
        kind={memberKind}
        draft={found}
        tables={tables}
        busy={busy}
        onSave={(member) => {
          if (!cube) return;
          const key = memberKind === "dimension" ? "dimensions" : "facts";
          const payload = memberPayload(member, memberKind);
          const next = ((cube[key] as Member[]) || []).map((m) =>
            m.name === found.name ? payload : m,
          );
          onSave({ ...cube, [key]: next });
          onSelect?.({ kind: memberKind, name: payload.name });
        }}
      />
    );
  }
  if (selection?.kind === "metric" || selection?.kind === "segment") {
    const pool = selection.kind === "metric" ? metrics : segments;
    const doc =
      pool?.find((d) => d.name === selection.name) ||
      docs.find((d) => d.kind === selection.kind && d.name === selection.name);
    return (
      <EditableDocDetail
        doc={doc}
        view={view}
        busy={busy}
        onSave={(next) => onSave(next, doc?.name)}
        onSelect={onSelect}
      />
    );
  }
  if (selection?.kind === "cube" && cube) {
    return <CubeDetail cube={cube} view={view} tables={tables} busy={busy} onSave={onSave} />;
  }
  if (selection?.kind === "view" || view) {
    return (
      <ViewDetail
        doc={view}
        tables={tables}
        showColumns={tab === "cubes"}
        onChangeView={onChangeView}
      />
    );
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
  const [role, setRole] = useState("all");
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
    setRole(roleText(doc.role as Member["role"]));
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

  function applyFilters() {
    const parsed = parseRole(role);
    const next = {
      ...doc,
      scope_filter: toFilter(scope),
      default_filter: toFilter(fallback),
      permission_filters: perms.filter((p) => p.role.trim() && p.expression.trim()),
    };
    if (parsed) next.role = parsed;
    else delete next.role;
    onSave(next as Doc);
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
          <Field label="Role">
            <TextField.Root size="2" value={role} onChange={(e) => setRole(e.target.value)} aria-label="Table role" />
          </Field>
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
          <Button size="1" disabled={busy} onClick={applyFilters}>Apply</Button>
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
  join?: Join;
  index: number;
  view: Doc;
  tables: Doc[];
  onChangeView?: (doc: Doc) => void;
}) {
  if (!join) return <Text size="2" color="gray">Join not found.</Text>;
  const current = join;
  const sources = (view.sources || []).map((s) => s.alias || s.table);
  const ons: JoinOn[] = current.on?.length ? current.on : [{ left_column: "", right_column: "" }];

  function columnsFor(sourceName: string): string[] {
    const src = (view.sources || []).find((s) => (s.alias || s.table) === sourceName);
    const table = tables.find((t) => t.name === sourceName || t.name === src?.table || t.table === src?.table);
    return table ? tableColumnNames(table) : [];
  }

  function patch(next: Join) {
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
    patch({ ...current, on });
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
                patch({ ...current, left, on });
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
              onValueChange={(type) => patch({ ...current, type })}
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
                patch({ ...current, right, on });
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
  onChangeView,
}: {
  view: Doc;
  columnName: string;
  tables: Doc[];
  onChangeView?: (doc: Doc) => void;
}) {
  const col = (view.columns || []).find((c) => c.name === columnName);
  if (!col) return <Text size="2" color="gray">Column not found.</Text>;
  const meta = columnMeta(view, columnName, tables);
  const sources = mappingSourceColumns(view, tables);
  return (
    <div className="column-detail">
      <div className="table-detail-col" role="region" aria-label="Column">
        <Heading size="3" weight="medium" mb="3">column · {columnName}</Heading>
        <div className="fields">
          <Field label="Name">
            <TextField.Root
              size="2"
              value={col.name}
              aria-label="Column name"
              onChange={(e) => onChangeView?.(renameViewColumn(view, columnName, e.target.value))}
            />
          </Field>
          <Field label="Type"><Text size="2">{meta.type}</Text></Field>
          <Field label="PK"><Text size="2">{meta.pk ? "yes" : "no"}</Text></Field>
        </div>
      </div>
      <div className="table-detail-col" role="region" aria-label="Column mappings">
        <Heading size="2" mb="2">Mappings</Heading>
        <div className="fields column-map-fields">
          {sources.map((s) => (
            <Field key={s.label} label={s.label}>
              <Select.Root
                size="2"
                value={mappingFor(col, s.label) || "none"}
                onValueChange={(value) => {
                  onChangeView?.(setMapping(view, columnName, s.label, value === "none" ? "" : value));
                }}
              >
                <Select.Trigger aria-label={`Map ${s.label}`} />
                <Select.Content>
                  <Select.Item value="none">—</Select.Item>
                  {tableColumnNames(s.table).map((name) => (
                    <Select.Item key={name} value={name}>{name}</Select.Item>
                  ))}
                </Select.Content>
              </Select.Root>
            </Field>
          ))}
          {!sources.length && <Text size="2" color="gray">No sources.</Text>}
        </div>
      </div>
    </div>
  );
}

function ViewDetail({
  doc,
  tables,
  showColumns,
  onChangeView,
}: {
  doc?: Doc;
  tables: Doc[];
  showColumns: boolean;
  onChangeView?: (doc: Doc) => void;
}) {
  if (!doc) return <Text size="2" color="gray">No view open.</Text>;
  const fields = (
    <>
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
    </>
  );
  if (!showColumns) return <div>{fields}</div>;
  const columns = (doc.columns || []).map((c) => columnMeta(doc, c.name, tables));
  return (
    <div className="table-detail view-detail">
      <div className="table-detail-col" role="region" aria-label="View">
        {fields}
      </div>
      <div className="table-detail-col" role="region" aria-label="View columns">
        <Heading size="2" mb="2">Columns</Heading>
        {columns.length ? (
          <table className="col-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Type</th>
                <th>PK</th>
              </tr>
            </thead>
            <tbody>
              {columns.map((c) => (
                <tr key={c.name}>
                  <td><Text size="2" weight="medium">{c.name}</Text></td>
                  <td><Text size="2" color="gray">{c.type}</Text></td>
                  <td><Text size="2" color="gray">{c.pk ? "yes" : "no"}</Text></td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <Text size="2" color="gray">No columns.</Text>
        )}
      </div>
    </div>
  );
}

function MemberDetail({
  kind,
  draft,
  tables,
  busy,
  onSave,
}: {
  kind: "dimension" | "fact";
  draft: Member;
  tables: Doc[];
  busy: boolean;
  onSave: (member: Member) => void;
}) {
  const [local, setLocal] = useState(draft);
  useEffect(() => setLocal(draft), [draft]);
  const types = kind === "dimension" ? DIM_TYPES : FACT_TYPES;
  return (
    <div>
      <Flex align="center" justify="between" mb="3">
        <Heading size="3" weight="medium">{kind} · {local.name}</Heading>
        <Button disabled={busy} onClick={() => onSave(local)}>Apply</Button>
      </Flex>
      <div className="fields">
        <Field label="Name">
          <TextField.Root
            size="2"
            value={local.name}
            onChange={(e) => setLocal({ ...local, name: e.target.value })}
            aria-label="Member name"
          />
        </Field>
        <Field label="Description">
          <TextField.Root
            size="2"
            value={local.description || ""}
            onChange={(e) => setLocal({ ...local, description: e.target.value })}
            aria-label="Member description"
          />
        </Field>
        <Field label="Type">
          <Select.Root
            size="2"
            value={local.type && (types as readonly string[]).includes(local.type) ? local.type : types[types.length - 1]}
            onValueChange={(type) => setLocal({ ...local, type })}
          >
            <Select.Trigger aria-label="Member type" />
            <Select.Content>
              {types.map((t) => (
                <Select.Item key={t} value={t}>{t}</Select.Item>
              ))}
            </Select.Content>
          </Select.Root>
        </Field>
        <Field label="Column">
          <TextField.Root
            size="2"
            value={local.column || ""}
            onChange={(e) => {
              const column = e.target.value;
              const next = { ...local, column };
              delete next.expression;
              setLocal(next);
            }}
          />
        </Field>
        <Field label="Expression">
          <TextField.Root
            size="2"
            value={local.expression || ""}
            onChange={(e) => {
              const expression = e.target.value;
              const next = { ...local, expression };
              delete next.column;
              setLocal(next);
            }}
          />
        </Field>
        <Field label="Encryption">
          <Checkbox
            checked={Boolean(local.encryption)}
            aria-label="Encryption"
            onCheckedChange={(v) => setLocal({ ...local, encryption: v === true })}
          />
        </Field>
        {kind === "fact" && (
          <Field label="Additive">
            <Checkbox
              checked={local.additive !== false}
              aria-label="Additive"
              onCheckedChange={(v) => setLocal({ ...local, additive: v === true })}
            />
          </Field>
        )}
        {kind === "dimension" && (
          <>
            <Field label="Code table">
              <Select.Root
                size="2"
                value={local.as_code?.table || "none"}
                onValueChange={(value) => {
                  if (value === "none") {
                    const next = { ...local };
                    delete next.as_code;
                    setLocal(next);
                    return;
                  }
                  setLocal({
                    ...local,
                    as_code: {
                      table: value,
                      key_column: local.as_code?.key_column || "",
                      value_column: local.as_code?.value_column || "",
                      where: local.as_code?.where,
                    },
                  });
                }}
              >
                <Select.Trigger aria-label="Code table" />
                <Select.Content>
                  <Select.Item value="none">—</Select.Item>
                  {tables.map((t) => (
                    <Select.Item key={t.name} value={t.name}>{t.name}</Select.Item>
                  ))}
                </Select.Content>
              </Select.Root>
            </Field>
            {local.as_code && (
              <>
                <Field label="Key column">
                  <TextField.Root
                    size="2"
                    value={local.as_code.key_column}
                    onChange={(e) => setLocal({ ...local, as_code: { ...local.as_code!, key_column: e.target.value } })}
                  />
                </Field>
                <Field label="Value column">
                  <TextField.Root
                    size="2"
                    value={local.as_code.value_column}
                    onChange={(e) => setLocal({ ...local, as_code: { ...local.as_code!, value_column: e.target.value } })}
                  />
                </Field>
                <Field label="Where">
                  <TextField.Root
                    size="2"
                    value={local.as_code.where || ""}
                    onChange={(e) => setLocal({ ...local, as_code: { ...local.as_code!, where: e.target.value || undefined } })}
                  />
                </Field>
              </>
            )}
          </>
        )}
        <Field label="Role">
          <TextField.Root
            size="2"
            value={roleText(local.role)}
            onChange={(e) => setLocal({ ...local, role: parseRole(e.target.value) })}
            aria-label="Member role"
          />
        </Field>
      </div>
    </div>
  );
}

function CubeDetail({
  cube,
  view,
  tables,
  busy,
  onSave,
}: {
  cube: Doc;
  view?: Doc;
  tables: Doc[];
  busy: boolean;
  onSave: (doc: Doc) => void;
}) {
  const [local, setLocal] = useState(cube);
  useEffect(() => setLocal(cube), [cube]);
  const grainOptions = grainCandidates(view, tables);
  const grain = Array.isArray(local.grain) ? (local.grain as string[]) : [];
  return (
    <div>
      <Flex align="center" justify="between" mb="3">
        <Heading size="3" weight="medium">cube · {local.name}</Heading>
        <Button disabled={busy} onClick={() => onSave(stripEmptyGrain(local))}>Apply</Button>
      </Flex>
      <div className="fields">
        <Field label="Description">
          <TextField.Root
            size="2"
            value={local.description || ""}
            onChange={(e) => setLocal({ ...local, description: e.target.value })}
            aria-label="Cube description"
          />
        </Field>
        <Field label="Grain">
          <Flex gap="2" wrap="wrap">
            {grainOptions.map((name) => {
              const on = grain.includes(name);
              return (
                <Button
                  key={name}
                  size="1"
                  variant={on ? "solid" : "soft"}
                  onClick={() => {
                    const next = on ? grain.filter((g) => g !== name) : [...grain, name];
                    setLocal({ ...local, grain: next });
                  }}
                >
                  {name}
                </Button>
              );
            })}
            {!grainOptions.length && <Text size="2" color="gray">No PK on the base view source.</Text>}
          </Flex>
        </Field>
        <Field label="Role">
          <TextField.Root
            size="2"
            value={roleText(local.role as Doc["role"])}
            onChange={(e) => {
              const parsed = parseRole(e.target.value);
              const next = { ...local };
              if (parsed) next.role = parsed;
              else delete next.role;
              setLocal(next);
            }}
            aria-label="Cube role"
          />
        </Field>
      </div>
    </div>
  );
}

function grainCandidates(view: Doc | undefined, tables: Doc[]): string[] {
  const source = view?.sources?.[0];
  const table = tables.find((t) => t.name === source?.table);
  return table?.pk || [];
}

function stripEmptyGrain(cube: Doc): Doc {
  const grain = Array.isArray(cube.grain) ? (cube.grain as string[]) : [];
  if (grain.length) return cube;
  const next = { ...cube };
  delete next.grain;
  return next;
}

function EditableDocDetail({
  doc,
  view,
  busy,
  onSave,
  onSelect,
}: {
  doc?: Doc;
  view?: Doc;
  busy: boolean;
  onSave: (doc: Doc) => void;
  onSelect?: (pick: Selection) => void;
}) {
  const [local, setLocal] = useState(doc);
  useEffect(() => setLocal(doc), [doc]);
  if (!local) return <Text size="2" color="gray">Not found.</Text>;
  const viewCols = (view?.columns || []).map((c) => c.name);
  const aggregation = local.aggregation || "group";
  const groupBy = Array.isArray(local.group_by) ? (local.group_by as string[]) : [];
  return (
    <div>
      <Flex align="center" justify="between" mb="3">
        <Heading size="3" weight="medium">{local.kind} · {local.name}</Heading>
        <Button
          disabled={busy}
          onClick={() => {
            const next = metricOrSegmentPayload(local);
            onSave(next);
            if (next.kind === "metric" || next.kind === "segment") {
              onSelect?.({ kind: next.kind, name: next.name });
            }
          }}
        >
          Save
        </Button>
      </Flex>
      <div className="fields">
        <Field label="Name">
          <TextField.Root
            size="2"
            value={local.name}
            onChange={(e) => setLocal({ ...local, name: e.target.value })}
            aria-label={local.kind === "metric" ? "Metric name" : "Segment name"}
          />
        </Field>
        <Field label="Description">
          <TextField.Root size="2" value={local.description || ""} onChange={(e) => setLocal({ ...local, description: e.target.value })} />
        </Field>
        {local.kind === "metric" && (
          <>
            <Field label="Expression">
              <TextField.Root size="2" value={local.expression || ""} onChange={(e) => setLocal({ ...local, expression: e.target.value })} />
            </Field>
            <Field label="Aggregation">
              <Select.Root
                size="2"
                value={aggregation}
                onValueChange={(value) => {
                  const next = { ...local, aggregation: value };
                  if (value !== "group") delete next.group_by;
                  else if (!next.group_by && viewCols[0]) next.group_by = [viewCols[0]];
                  setLocal(next);
                }}
              >
                <Select.Trigger aria-label="Aggregation" />
                <Select.Content>
                  {AGGREGATIONS.map((a) => (
                    <Select.Item key={a} value={a}>{a}</Select.Item>
                  ))}
                </Select.Content>
              </Select.Root>
            </Field>
            {aggregation === "group" && (
              <Field label="Group by">
                <Select.Root
                  size="2"
                  value={groupBy[0] || viewCols[0] || "none"}
                  onValueChange={(value) => setLocal({ ...local, group_by: value === "none" ? undefined : [value] })}
                >
                  <Select.Trigger aria-label="Group by" />
                  <Select.Content>
                    <Select.Item value="none">—</Select.Item>
                    {viewCols.map((name) => (
                      <Select.Item key={name} value={name}>{name}</Select.Item>
                    ))}
                  </Select.Content>
                </Select.Root>
              </Field>
            )}
            <Field label="Filter">
              <TextField.Root
                size="2"
                value={local.filter?.expression || ""}
                onChange={(e) => setLocal({ ...local, filter: e.target.value.trim() ? { expression: e.target.value } : null })}
                aria-label="Metric filter"
              />
            </Field>
          </>
        )}
        {local.kind === "segment" && (
          <Field label="Filter">
            <TextField.Root
              size="2"
              value={local.filter?.expression || ""}
              onChange={(e) => setLocal({ ...local, filter: { expression: e.target.value } })}
              aria-label="Segment filter"
            />
          </Field>
        )}
        <Field label="Role">
          <TextField.Root
            size="2"
            value={roleText(local.role as Doc["role"])}
            onChange={(e) => {
              const parsed = parseRole(e.target.value);
              const next = { ...local };
              if (parsed) next.role = parsed;
              else delete next.role;
              setLocal(next);
            }}
            aria-label={`${local.kind} role`}
          />
        </Field>
      </div>
    </div>
  );
}

function metricOrSegmentPayload(doc: Doc): Doc {
  const next = { ...doc };
  if (next.kind === "metric") {
    if (!next.aggregation) next.aggregation = "group";
    if (next.aggregation === "group") {
      const gb = Array.isArray(next.group_by) ? (next.group_by as string[]).filter(Boolean) : [];
      if (gb.length) next.group_by = gb;
    } else {
      delete next.group_by;
    }
    if (!next.filter?.expression) next.filter = null;
  }
  return next;
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
