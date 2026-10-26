import { useEffect, useMemo, useState } from "react";
import { Badge, Callout, Flex, Select, Separator, Text, Theme } from "@radix-ui/themes";
import { AuthorApi } from "./api";
import { Canvas } from "./components/Canvas";
import { Catalog } from "./components/Catalog";
import { Detail } from "./components/Detail";
import { detailDoc, draftView, uniqueName, type TableCombine } from "./components/docs";
import { Inspector } from "./components/Inspector";
import type { Pick, Tab } from "./components/types";
import {
  combineName,
  cubeForView,
  choiceStack,
  forTarget,
  joinViewForTable,
  metricsForCube,
  reorder,
  segmentsForCube,
  withChoiceStack,
  type Doc,
} from "./model";

export function App({ api }: { api?: AuthorApi }) {
  const client = useMemo(() => api ?? new AuthorApi(), [api]);
  const [targets, setTargets] = useState<{ name: string; engine?: string }[]>([]);
  const [target, setTarget] = useState("");
  const [docs, setDocs] = useState<Doc[]>([]);
  const [physical, setPhysical] = useState<Doc[]>([]);
  const [tab, setTab] = useState<Tab>("physical");
  const [tableName, setTableName] = useState("");
  const [picked, setPicked] = useState<string[]>([]);
  const [tableCombine, setTableCombine] = useState<TableCombine>("join");
  const [draft, setDraft] = useState<Doc>();
  const [draftKey, setDraftKey] = useState("");
  const [editing, setEditing] = useState<Doc>();
  const [editingStamp, setEditingStamp] = useState("");
  const [cubeDraft, setCubeDraft] = useState<Doc>();
  const [cubeStamp, setCubeStamp] = useState("");
  const [checked, setChecked] = useState<string[]>([]);
  const [viewName, setViewName] = useState("");
  const [member, setMember] = useState<Pick>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [navOpen, setNavOpen] = useState(true);
  const [inspectorOpen, setInspectorOpen] = useState(true);

  async function loadPhysical(database: string) {
    const items = await client.listPhysicalTables(database);
    const tables: Doc[] = [];
    for (const item of items) {
      const desc = await client.describePhysicalTable(database, item.name, item.schema);
      tables.push({
        kind: "table",
        name: item.name,
        target: database,
        columns: (desc.columns || []).map((col) => ({ name: col.name, mappings: [] })),
      });
    }
    setPhysical(tables);
  }

  async function reload(nextTarget = target) {
    const [dbs, all] = await Promise.all([client.listDatabases(), client.listDocs()]);
    setTargets(dbs);
    const chosen = nextTarget || dbs[0]?.name || "";
    if (!nextTarget && chosen) setTarget(chosen);
    setDocs(all);
    if (chosen) await loadPhysical(chosen);
  }

  useEffect(() => {
    reload("").catch((e: Error) => setError(e.message));
    // initial load only
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const tables = physical.length ? physical : forTarget(docs, "table", target);
  const views = forTarget(docs, "view", target);
  const cubes = forTarget(docs, "cube", target);
  const selectedTable = tables.find((t) => t.name === tableName) ?? tables[0];
  const joinView = selectedTable ? joinViewForTable(views, selectedTable.name) : views.find((v) => v.combine === "join");
  const selectedView = views.find((v) => v.name === viewName);
  const cube = selectedView ? cubeForView(cubes, selectedView.name) : undefined;
  const liveCube = cubeDraft && cube && cubeDraft.name === cube.name ? cubeDraft : cube;
  const selectMode = tab === "cubes" && choiceStack(liveCube).length > 1;
  const engine = targets.find((t) => t.name === target)?.engine;

  function chooseTarget(value: string) {
    setTarget(value);
    setTableName("");
    setViewName("");
    setChecked([]);
    setPicked([]);
    setMember(null);
    loadPhysical(value).catch((err: Error) => setError(err.message));
  }

  async function save(doc: Doc) {
    setBusy(true);
    setError("");
    try {
      if ((doc.kind === "dimension" || doc.kind === "fact") && cube) {
        const key = doc.kind === "dimension" ? "dimensions" : "facts";
        const list = ((cube[key] as { name: string }[]) || []).map((m) =>
          m.name === doc.name ? { ...m, description: doc.description, column: doc.column } : m,
        );
        await client.update("cube", cube.name, { ...cube, [key]: list });
      } else if (doc.kind === "dimension" || doc.kind === "fact") {
        throw new Error("select a cube view before editing a member");
      } else {
        await client.update(doc.kind, doc.name, doc);
      }
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function combine() {
    if (checked.length < 2) return;
    const canonical = checked[checked.length - 1];
    const existing = cubeForView(cubes, canonical);
    const cubeDoc = withChoiceStack(
      {
        kind: "cube",
        name: existing?.name ?? uniqueName(combineName(checked), cubes),
        target,
        view: canonical,
        description: existing?.description ?? "",
        dimensions: existing?.dimensions ?? [],
        facts: existing?.facts ?? [],
      },
      checked,
    );
    setBusy(true);
    setError("");
    try {
      if (existing) await client.update("cube", existing.name, cubeDoc);
      else await client.create(cubeDoc);
      setViewName(canonical);
      setMember(null);
      setChecked([]);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  function dropSource(from: number, to: number) {
    if (!liveCube) return;
    setCubeDraft(withChoiceStack(liveCube, reorder(choiceStack(liveCube), from, to)));
  }

  const pickedTables = picked
    .map((name) => tables.find((table) => table.name === name))
    .filter((table): table is Doc => Boolean(table));
  const nextDraftKey = `${target}:${tableCombine}:${picked.join(",")}`;
  if (nextDraftKey !== draftKey) {
    setDraftKey(nextDraftKey);
    const next = draftView(tableCombine, pickedTables, target);
    setDraft(next ? { ...next, name: uniqueName(next.name, views) } : undefined);
  }

  async function createPhysical() {
    if (!draft) return;
    setBusy(true);
    setError("");
    try {
      await client.create(draft);
      setPicked([]);
      setMember(null);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  function editView(doc: Doc) {
    if (draft && doc.name === draft.name) setDraft(doc);
    else if (editing && doc.name === editing.name) setEditing(doc);
    else void save(doc);
  }

  const opened = tab === "cubes" ? selectedView : draft ? undefined : joinView;
  const openedStamp = opened ? JSON.stringify(opened) : "";
  const dirty = Boolean(editing && opened && JSON.stringify(editing) !== openedStamp);
  if ((opened?.name || "") !== (editing?.name || "") || (openedStamp !== editingStamp && !dirty)) {
    setEditingStamp(openedStamp);
    setEditing(opened ? (JSON.parse(JSON.stringify(opened)) as Doc) : undefined);
  }
  const cubeOpenedStamp = cube ? JSON.stringify(cube) : "";
  const cubeDirty = Boolean(cubeDraft && cube && JSON.stringify(cubeDraft) !== cubeOpenedStamp);
  if ((cube?.name || "") !== (cubeDraft?.name || "") || (cubeOpenedStamp !== cubeStamp && !cubeDirty)) {
    setCubeStamp(cubeOpenedStamp);
    setCubeDraft(cube ? (JSON.parse(JSON.stringify(cube)) as Doc) : undefined);
  }

  async function saveOpen() {
    if (tab === "physical" && draft) {
      await createPhysical();
      return;
    }
    if (tab === "cubes" && cubeDirty && cubeDraft) {
      await save(cubeDraft);
      return;
    }
    if (editing) await save(editing);
  }

  async function deleteOpen() {
    if (!editing) return;
    const name = editing.name;
    const usedBy = views.filter((view) => view.name !== name && (view.sources || []).some((source) => source.table === name));
    if (usedBy.length) {
      setError(`${name} is used by ${usedBy.map((view) => view.name).join(", ")}`);
      return;
    }
    const choiceUsers = cubes.filter(
      (c) => c.view !== name && (c.choice || []).some((item) => item.view === name),
    );
    if (choiceUsers.length) {
      setError(`${name} is used by ${choiceUsers.map((c) => c.name).join(", ")}`);
      return;
    }
    const cubeDoc = cubeForView(cubes, name);
    const metrics = cubeDoc ? metricsForCube(docs, cubeDoc.name) : [];
    const segments = cubeDoc ? segmentsForCube(docs, cubeDoc.name) : [];
    setBusy(true);
    setError("");
    try {
      for (const doc of [...metrics, ...segments]) await client.remove(doc.kind, doc.name);
      if (cubeDoc) await client.remove("cube", cubeDoc.name);
      await client.remove("view", name);
      setViewName("");
      setMember(null);
      setEditing(undefined);
      setEditingStamp("");
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  const detail = detailDoc(docs, tab, selectedTable, selectedView, member);
  const viewBar = tab === "physical"
    ? {
        title: draft ? "New view" : editing?.name || "No view",
        unsaved: Boolean(draft) || dirty,
        canSave: Boolean(draft) || dirty,
        canDelete: Boolean(editing) && !draft,
        canDiscard: Boolean(draft),
      }
      : editing
      ? { title: editing.name, unsaved: dirty || cubeDirty, canSave: dirty || cubeDirty, canDelete: true, canDiscard: false }
      : undefined;

  return (
    <Theme appearance="light" accentColor="indigo" grayColor="slate" panelBackground="solid" radius="medium" scaling="95%">
      <div className={error ? "shell has-error" : "shell"}>
        <header className="topbar">
          <Flex align="center" gap="3">
            <Text size="3" weight="bold">Semantic</Text>
            <Separator orientation="vertical" />
            <Text size="2" color="gray">Target</Text>
            {target ? (
              <Select.Root size="2" value={target} onValueChange={chooseTarget}>
                <Select.Trigger aria-label="Target" />
                <Select.Content position="popper">
                  {targets.map((t) => (
                    <Select.Item key={t.name} value={t.name}>
                      {t.name}
                    </Select.Item>
                  ))}
                </Select.Content>
              </Select.Root>
            ) : (
              <Text size="2" color="gray">{error ? "No target" : "Loading targets"}</Text>
            )}
            {engine && <Badge variant="soft" color="gray">{engine}</Badge>}
          </Flex>
          <Flex align="center" gap="3">
            <Text size="1" color="gray" className="topbar-note">One target. No cross-target joins.</Text>
            {busy && <Badge variant="soft">Saving</Badge>}
          </Flex>
        </header>
        {error && (
          <div className="alert">
            <Callout.Root color="red" size="1" variant="soft">
              <Callout.Text>{error}</Callout.Text>
            </Callout.Root>
          </div>
        )}
        <div className={["workspace", selectMode ? "select-mode" : "", navOpen ? "" : "nav-collapsed", inspectorOpen ? "" : "inspector-collapsed"].filter(Boolean).join(" ")}>
          <Catalog
            open={navOpen}
            onToggle={() => setNavOpen((open) => !open)}
            tab={tab}
            onTab={setTab}
            tables={tables}
            selectedTable={selectedTable}
            onSelectTable={(name) => {
              setTableName(name);
              setMember({ kind: "table", name });
            }}
            picked={picked}
            onTogglePick={(name) =>
              setPicked((cur) => (cur.includes(name) ? cur.filter((n) => n !== name) : [...cur, name]))
            }
            views={views}
            viewName={viewName}
            checked={checked}
            onToggleCheck={(name) =>
              setChecked((cur) => (cur.includes(name) ? cur.filter((n) => n !== name) : [...cur, name]))
            }
            onSelectView={(name) => {
              setViewName(name);
              setMember({ kind: "view", name });
            }}
            busy={busy}
            onCombine={combine}
          />
          <Canvas
            selectMode={selectMode}
            tab={tab}
            selectedView={tab === "cubes" ? editing ?? selectedView : selectedView}
            stackNames={choiceStack(liveCube)}
            joinView={joinView}
            checked={checked}
            onDrop={dropSource}
            combine={tableCombine}
            onCombine={setTableCombine}
            picked={picked}
            draft={draft}
            busy={busy}
            onSaveView={() => void saveOpen()}
            onDeleteView={() => void deleteOpen()}
            onDiscard={() => setPicked([])}
            viewBar={viewBar}
            onDropTable={(kind, name) => {
              setTableCombine(kind);
              setPicked((cur) => (cur.includes(name) ? cur : [...cur, name]));
            }}
          />
          <Inspector
            open={inspectorOpen}
            onToggle={() => setInspectorOpen((open) => !open)}
            tab={tab}
            mapping={draft ?? (tab === "physical" ? editing ?? joinView : undefined)}
            onSaveMapping={editView}
            source={draft ? picked[0] : selectedTable?.name}
            selectedView={tab === "cubes" ? editing ?? selectedView : selectedView}
            cube={liveCube}
            metrics={liveCube ? metricsForCube(docs, liveCube.name) : []}
            segments={liveCube ? segmentsForCube(docs, liveCube.name) : []}
            selected={member}
            onSelect={setMember}
          />
        </div>
        <section className="editor" aria-label="Detail">
          <Detail
            doc={detail?.kind === "view" && editing ? editing : detail}
            busy={busy}
            onSave={save}
            onChange={detail?.kind === "view" ? setEditing : undefined}
          />
        </section>
      </div>
    </Theme>
  );
}
