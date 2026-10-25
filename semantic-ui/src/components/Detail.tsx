import { useEffect, useState, type ReactNode } from "react";
import { Button, Flex, Heading, Text, TextField } from "@radix-ui/themes";
import type { Doc } from "../model";

export function Detail({
  doc,
  busy,
  onSave,
  onChange,
}: {
  doc: Doc | undefined;
  busy: boolean;
  onSave: (doc: Doc) => void;
  onChange?: (doc: Doc) => void;
}) {
  const [draft, setDraft] = useState<Doc | undefined>(doc);
  if ((doc?.name || "") !== (draft?.name || "") || (doc?.kind || "") !== (draft?.kind || "")) setDraft(doc);
  useEffect(() => setDraft(doc), [doc]);
  if (!draft) return <Text size="2" color="gray">Select a table, view, or cube member.</Text>;
  const patch = (key: string, value: unknown) => {
    const next = { ...draft, [key]: value };
    setDraft(next);
    if (draft.kind === "view") onChange?.(next);
  };
  return (
    <div>
      <Flex align="center" justify="between" mb="3" gap="3">
        <Heading size="3" weight="medium">{draft.kind} · {draft.name}</Heading>
        {draft.kind === "view" ? null : (
          <Button disabled={busy} onClick={() => onSave(draft)}>Save</Button>
        )}
      </Flex>
      {draft.kind === "table" ? (
        <div className="fields">
          <Field label="Name"><Text size="2">{draft.name}</Text></Field>
          <Field label="Target"><Text size="2">{String(draft.target || "")}</Text></Field>
          <Field label="Description">
            <TextField.Root size="2" value={draft.description || ""} onChange={(e) => patch("description", e.target.value)} />
          </Field>
          <Field label="Primary key">
            <TextField.Root
              size="2"
              value={(draft.pk || []).join(", ")}
              onChange={(e) =>
                patch(
                  "pk",
                  e.target.value.split(",").map((s) => s.trim()).filter(Boolean),
                )
              }
            />
          </Field>
        </div>
      ) : draft.kind === "dimension" || draft.kind === "fact" ? (
        <MemberFields draft={draft} onChange={setDraft} />
      ) : (
        <div className="fields">
          <Field label="Name"><Text size="2">{draft.name}</Text></Field>
          <Field label="Description">
            <TextField.Root size="2" value={draft.description || ""} onChange={(e) => patch("description", e.target.value)} />
          </Field>
          {draft.kind === "view" && <Field label="Combine"><Text size="2">{draft.combine}</Text></Field>}
          {draft.kind === "metric" && (
            <Field label="Expression">
              <TextField.Root size="2" value={draft.expression || ""} onChange={(e) => patch("expression", e.target.value)} />
            </Field>
          )}
          {draft.kind === "segment" && (
            <Field label="Filter">
              <TextField.Root
                size="2"
                value={draft.filter?.expression || ""}
                onChange={(e) => patch("filter", { expression: e.target.value })}
              />
            </Field>
          )}
        </div>
      )}
      {draft.kind === "table" && (
        <Text size="1" color="gray" as="p" mt="3">No column list here. Columns are in the right mapping pane.</Text>
      )}
      <Text size="1" color="gray" as="p" mt="2">Writes meta JSON. semantic-map.md is not an editor.</Text>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <Text size="2" color="gray">{label}</Text>
      {children}
    </>
  );
}

function MemberFields({ draft, onChange }: { draft: Doc; onChange: (doc: Doc) => void }) {
  return (
    <div className="fields">
      <Field label="Name"><Text size="2">{draft.name}</Text></Field>
      <Field label="Type"><Text size="2">{draft.type}</Text></Field>
      <Field label="Description">
        <TextField.Root
          size="2"
          value={draft.description || ""}
          onChange={(e) => onChange({ ...draft, description: e.target.value })}
        />
      </Field>
      <Field label="View column">
        <TextField.Root
          size="2"
          value={draft.column || ""}
          onChange={(e) => onChange({ ...draft, column: e.target.value })}
        />
      </Field>
    </div>
  );
}
