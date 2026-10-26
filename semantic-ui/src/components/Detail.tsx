import { useEffect, useState, type ReactNode } from "react";
import { Button, Flex, Heading, Text, TextField } from "@radix-ui/themes";
import type { Doc } from "../model";
import { columnMeta } from "./docs";
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
    const doc = table || (selection?.kind === "table" ? tables.find((t) => t.name === selection.name) : undefined);
    return <TableDetail doc={doc} />;
  }
  if (selection?.kind === "join" && view) {
    const join = (view.joins || [])[selection.index];
    return <JoinDetail join={join} index={selection.index} view={view} onChangeView={onChangeView} />;
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

function TableDetail({ doc }: { doc?: Doc }) {
  if (!doc) return <Text size="2" color="gray">Select a registered table.</Text>;
  const cols = (doc.columns as { name?: string; type?: string; nullable?: boolean }[]) || [];
  return (
    <div>
      <Heading size="3" weight="medium" mb="3">table · {doc.name}</Heading>
      <Text size="1" color="gray" as="p" mb="3">Read-only. Register/Delete only — no table edits.</Text>
      <div className="fields">
        <Field label="Name"><Text size="2">{doc.name}</Text></Field>
        <Field label="Target"><Text size="2">{String(doc.target || "")}</Text></Field>
        <Field label="Physical"><Text size="2">{String(doc.schema || "")}.{String(doc.table || doc.name)}</Text></Field>
        <Field label="PK"><Text size="2">{(doc.pk || []).join(", ") || "—"}</Text></Field>
      </div>
      <Heading size="2" mt="4" mb="2">Columns</Heading>
      {cols.map((c) => (
        <Text key={c.name} size="2" as="p">{c.name} · {c.type || "?"}{c.nullable ? " nullable" : ""}</Text>
      ))}
      <Heading size="2" mt="4" mb="2">Filters</Heading>
      <Text size="2" as="p">scope: {JSON.stringify(doc.scope_filter ?? null)}</Text>
      <Text size="2" as="p">default: {JSON.stringify(doc.default_filter ?? null)}</Text>
      <Text size="2" as="p">permission: {JSON.stringify(doc.permission_filters ?? [])}</Text>
    </div>
  );
}

function JoinDetail({
  join,
  index,
  view,
  onChangeView,
}: {
  join?: Doc["joins"] extends (infer J)[] | undefined ? J : never;
  index: number;
  view: Doc;
  onChangeView?: (doc: Doc) => void;
}) {
  if (!join) return <Text size="2" color="gray">Join not found.</Text>;
  return (
    <div>
      <Heading size="3" weight="medium" mb="3">join · {join.left} → {join.right}</Heading>
      <div className="fields">
        <Field label="Type"><Text size="2">{join.type}</Text></Field>
        <Field label="Relationship"><Text size="2">{join.relationship || "unset"}</Text></Field>
        <Field label="ON">
          <Text size="2">
            {(join.on || []).map((o) => `${o.left_column} = ${o.right_column}`).join(", ")}
          </Text>
        </Field>
      </div>
      {onChangeView && (
        <Flex gap="2" mt="3">
          {(["inner", "left", "right", "full"] as const).map((t) => (
            <Button
              key={t}
              size="1"
              variant={join.type === t ? "solid" : "soft"}
              onClick={() => {
                const joins = (view.joins || []).map((j, i) => (i === index ? { ...j, type: t } : j));
                onChangeView({ ...view, joins });
              }}
            >
              {t}
            </Button>
          ))}
        </Flex>
      )}
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
