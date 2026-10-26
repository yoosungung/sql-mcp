import { useEffect, useMemo, useState } from "react";
import { Badge, Callout, Flex, Select, Separator, Text, Theme } from "@radix-ui/themes";
import { AuthorApi } from "./api";
import { Canvas } from "./components/Canvas";
import { Catalog } from "./components/Catalog";
import { CubeMappingEditor } from "./components/CubeMappingEditor";
import { Detail } from "./components/Detail";
import {
  cubesReferencingTable,
  draftView,
  physicalTableDoc,
  uniqueName,
  viewsReferencingTable,
  type TableCombine,
} from "./components/docs";
import { Inspector } from "./components/Inspector";
import type { MappingEditor, Selection, Tab } from "./components/types";
import { ViewMappingEditor } from "./components/ViewMappingEditor";
import {
  choiceStack,
  cubeForView,
  forTarget,
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
  const [physicalCandidates, setPhysicalCandidates] = useState<{ schema: string; name: string }[]>([]);
  const [tab, setTab] = useState<Tab>("tables");
  const [tableName, setTableName] = useState("");
  const [picked, setPicked] = useState<string[]>([]);
  const [tableCombine, setTableCombine] = useState<TableCombine>("join");
  const [draft, setDraft] = useState<Doc>();
  const [editing, setEditing] = useState<Doc>();
  const [cubeDraft, setCubeDraft] = useState<Doc>();
  const [metricDrafts, setMetricDrafts] = useState<Doc[]>([]);
  const [segmentDrafts, setSegmentDrafts] = useState<Doc[]>([]);
  const [checked, setChecked] = useState<string[]>([]);
  const [viewName, setViewName] = useState("");
  const [selection, setSelection] = useState<Selection>(null);
  const [selectedJoinIndex, setSelectedJoinIndex] = useState<number | undefined>();
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [navOpen, setNavOpen] = useState(true);
  const [inspectorOpen, setInspectorOpen] = useState(true);
  const [mappingEditor, setMappingEditor] = useState<MappingEditor>(null);
  const [knownViews, setKnownViews] = useState<Set<string>>(new Set());

  async function loadPhysicalCandidates(database: string) {
    try {
      const items = await client.listPhysicalTables(database);
      const registered = new Set(
        forTarget(docs, "table", database).map((t) => String(t.table || t.name)),
      );
      setPhysicalCandidates(items.filter((i) => !registered.has(i.name)));
    } catch (e) {
      setPhysicalCandidates([]);
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function reload(nextTarget = target) {
    const [dbs, all] = await Promise.all([client.listDatabases(), client.listDocs()]);
    setTargets(dbs);
    const chosen = nextTarget || dbs[0]?.name || "";
    if (!nextTarget && chosen) setTarget(chosen);
    setDocs(all);
    setKnownViews(new Set(all.filter((d) => d.kind === "view").map((d) => d.name)));
    if (chosen) {
      const registered = new Set(
        all.filter((d) => d.kind === "table" && d.target === chosen).map((t) => String(t.table || t.name)),
      );
      try {
        const items = await client.listPhysicalTables(chosen);
        setPhysicalCandidates(items.filter((i) => !registered.has(i.name)));
      } catch {
        setPhysicalCandidates([]);
      }
    }
  }

  useEffect(() => {
    reload("").catch((e: Error) => setError(e.message));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const tables = forTarget(docs, "table", target);
  const views = forTarget(docs, "view", target);
  const cubes = forTarget(docs, "cube", target);
  const selectedTable = tables.find((t) => t.name === tableName) ?? tables[0];
  const selectedView = views.find((v) => v.name === viewName);
  const liveView = draft || editing;
  const cube = selectedView ? cubeForView(cubes, selectedView.name) : cubeDraft;
  const liveCube =
    cubeDraft && cube && cubeDraft.name === cube.name
      ? cubeDraft
      : cubeDraft && !cube
        ? cubeDraft
        : cube
          ? cubeDraft && cubeDraft.name === cube.name
            ? cubeDraft
            : cube
          : cubeDraft;
  const engine = targets.find((t) => t.name === target)?.engine;

  const selectedViewKey = selectedView ? `${selectedView.name}:${JSON.stringify(selectedView)}` : "";
  const selectedCube = selectedView ? cubeForView(cubes, selectedView.name) : undefined;
  const selectedCubeKey = selectedCube ? `${selectedCube.name}:${JSON.stringify(selectedCube)}` : "";

  useEffect(() => {
    if (draft) return;
    if (!viewName || !selectedViewKey) {
      setEditing(undefined);
      return;
    }
    const snap = JSON.parse(selectedViewKey.slice(selectedViewKey.indexOf(":") + 1)) as Doc;
    setEditing((cur) => {
      if (cur && cur.name === snap.name && JSON.stringify(cur) !== JSON.stringify(snap)) return cur;
      return snap;
    });
  }, [viewName, selectedViewKey, draft]);

  useEffect(() => {
    if (!selectedCubeKey) return;
    const base = JSON.parse(selectedCubeKey.slice(selectedCubeKey.indexOf(":") + 1)) as Doc;
    setCubeDraft((cur) => {
      if (cur && cur.name === base.name && JSON.stringify(cur) !== JSON.stringify(base)) return cur;
      return base;
    });
    setMetricDrafts(metricsForCube(docs, base.name).map((d) => JSON.parse(JSON.stringify(d)) as Doc));
    setSegmentDrafts(segmentsForCube(docs, base.name).map((d) => JSON.parse(JSON.stringify(d)) as Doc));
    // docs identity changes on reload only via selectedCubeKey / viewName
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedCubeKey]);

  function chooseTarget(value: string) {
    setTarget(value);
    setTableName("");
    setViewName("");
    setChecked([]);
    setPicked([]);
    setSelection(null);
    setDraft(undefined);
    setEditing(undefined);
    void loadPhysicalCandidates(value);
  }

  async function saveDoc(doc: Doc) {
    setBusy(true);
    setError("");
    try {
      if (doc.kind === "table") throw new Error("table documents are register/delete only");
      const exists =
        doc.kind === "view"
          ? knownViews.has(doc.name) || views.some((v) => v.name === doc.name)
          : docs.some((d) => d.kind === doc.kind && d.name === doc.name);
      if (exists) await client.update(doc.kind, doc.name, doc);
      else await client.create(doc);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function saveOpenView() {
    const doc = draft || editing;
    if (!doc) return;
    await saveDoc(doc);
    setDraft(undefined);
    setViewName(doc.name);
  }

  async function saveOpenCube() {
    if (!liveCube) return;
    setBusy(true);
    setError("");
    try {
      const exists = cubes.some((c) => c.name === liveCube.name);
      if (exists) await client.update("cube", liveCube.name, liveCube);
      else await client.create(liveCube);
      for (const m of metricDrafts) {
        const ex = docs.some((d) => d.kind === "metric" && d.name === m.name);
        if (ex) await client.update("metric", m.name, m);
        else await client.create(m);
      }
      for (const s of segmentDrafts) {
        const ex = docs.some((d) => d.kind === "segment" && d.name === s.name);
        if (ex) await client.update("segment", s.name, s);
        else await client.create(s);
      }
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function registerTable(physical: { schema: string; name: string }) {
    setBusy(true);
    setError("");
    try {
      const desc = await client.describePhysicalTable(target, physical.name, physical.schema);
      const doc = physicalTableDoc(target, physical, desc.columns || []);
      doc.name = uniqueName(doc.name, tables);
      await client.create(doc);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function deleteTable(name: string) {
    const usedViews = viewsReferencingTable(views, name);
    if (usedViews.length) {
      setError(`${name} is used by view ${usedViews.map((v) => v.name).join(", ")}`);
      return;
    }
    const usedCubes = cubesReferencingTable(cubes, views, name);
    if (usedCubes.length) {
      setError(`${name} is used by cube ${usedCubes.map((c) => c.name).join(", ")}`);
      return;
    }
    setBusy(true);
    setError("");
    try {
      await client.remove("table", name);
      if (tableName === name) setTableName("");
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function deleteOpenView() {
    if (!editing) return;
    const name = editing.name;
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
      setEditing(undefined);
      setSelection(null);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function deleteOpenCube() {
    if (!liveCube) return;
    setBusy(true);
    setError("");
    try {
      for (const m of metricsForCube(docs, liveCube.name)) await client.remove("metric", m.name);
      for (const s of segmentsForCube(docs, liveCube.name)) await client.remove("segment", s.name);
      await client.remove("cube", liveCube.name);
      setCubeDraft(undefined);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  function newView(combine: TableCombine) {
    setTableCombine(combine);
    const name = uniqueName(`${combine}_view`, views);
    setDraft({
      kind: "view",
      name,
      target,
      combine,
      description: "",
      sources: [],
      columns: [],
      joins: combine === "join" ? [] : undefined,
    });
    setEditing(undefined);
    setViewName("");
    setPicked([]);
    setSelection({ kind: "view", name });
  }

  function openView(name: string) {
    setDraft(undefined);
    setViewName(name);
    setSelection({ kind: "view", name });
    const v = views.find((x) => x.name === name);
    if (v?.combine === "join" || v?.combine === "union" || v?.combine === "minus") {
      setTableCombine(v.combine);
    }
  }

  function renameView(name: string) {
    const doc = draft || editing;
    if (!doc) return;
    const next = { ...doc, name };
    if (draft) setDraft(next);
    else setEditing(next);
  }

  function changeView(view: Doc) {
    if (draft && draft.name === view.name) setDraft(view);
    else setEditing(view);
  }

  function dropTable(kind: TableCombine, name: string) {
    setTableCombine(kind);
    setPicked((cur) => {
      const next = cur.includes(name) ? cur : [...cur, name];
      const pickedTables = next
        .map((n) => tables.find((t) => t.name === n))
        .filter((t): t is Doc => Boolean(t));
      const base = draft || editing;
      if (base) {
        const rebuilt = draftView(kind, pickedTables, target);
        if (rebuilt) {
          const merged = { ...rebuilt, name: base.name, description: base.description || "" };
          if (draft) setDraft(merged);
          else setEditing(merged);
        }
      } else {
        const rebuilt = draftView(kind, pickedTables, target);
        if (rebuilt) {
          rebuilt.name = uniqueName(rebuilt.name, views);
          setDraft(rebuilt);
        }
      }
      return next;
    });
  }

  function combineIntoCube() {
    if (!checked.length) return;
    const canonical = checked[checked.length - 1];
    const existing = cubeForView(cubes, canonical);
    const cubeDoc = withChoiceStack(
      {
        kind: "cube",
        name: existing?.name ?? uniqueName(`cube_${canonical}`, cubes),
        target,
        view: canonical,
        description: existing?.description ?? "",
        dimensions: existing?.dimensions ?? [],
        facts: existing?.facts ?? [],
      },
      checked,
    );
    setCubeDraft(cubeDoc);
    setViewName(canonical);
    setSelection({ kind: "view", name: canonical });
  }

  function newCube() {
    const name = uniqueName("cube", cubes);
    setCubeDraft({
      kind: "cube",
      name,
      target,
      view: checked[checked.length - 1] || views[0]?.name || "",
      description: "",
      dimensions: [],
      facts: [],
    });
    setMetricDrafts([]);
    setSegmentDrafts([]);
  }

  const cubeDirty = Boolean(
    liveCube &&
      cube &&
      (JSON.stringify(liveCube) !== JSON.stringify(cube) ||
        JSON.stringify(metricDrafts) !== JSON.stringify(metricsForCube(docs, cube.name)) ||
        JSON.stringify(segmentDrafts) !== JSON.stringify(segmentsForCube(docs, cube.name))),
  ) || Boolean(liveCube && !cube);

  const stack = choiceStack(liveCube);

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
                    <Select.Item key={t.name} value={t.name}>{t.name}</Select.Item>
                  ))}
                </Select.Content>
              </Select.Root>
            ) : (
              <Text size="2" color="gray">{error ? "No target" : "Loading targets"}</Text>
            )}
            {engine && <Badge variant="soft" color="gray">{engine}</Badge>}
          </Flex>
          <Flex align="center" gap="3">
            <Text size="1" color="gray" className="topbar-note">CRUD over semantic JSON · one target</Text>
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
        <div className={["workspace", navOpen ? "" : "nav-collapsed", inspectorOpen ? "" : "inspector-collapsed"].filter(Boolean).join(" ")}>
          <Catalog
            open={navOpen}
            onToggle={() => setNavOpen((o) => !o)}
            tab={tab}
            onTab={setTab}
            tables={tables}
            selectedTable={selectedTable}
            onSelectTable={(name) => {
              setTableName(name);
              setSelection({ kind: "table", name });
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
              openView(name);
            }}
            busy={busy}
            onCombine={combineIntoCube}
            physicalCandidates={physicalCandidates}
            onRegister={registerTable}
            onDeleteTable={deleteTable}
          />
          <Canvas
            tab={tab}
            selectedView={selectedView}
            stackNames={stack}
            checked={checked}
            onDropStack={(from, to) => {
              if (!liveCube) return;
              setCubeDraft(withChoiceStack(liveCube, reorder(choiceStack(liveCube), from, to)));
            }}
            combine={tableCombine}
            onCombine={setTableCombine}
            picked={picked}
            draft={draft}
            editing={editing}
            busy={busy}
            views={views}
            cube={liveCube}
            cubeDirty={cubeDirty}
            onSaveView={() => void saveOpenView()}
            onDeleteView={() => void deleteOpenView()}
            onNewView={newView}
            onOpenView={openView}
            onRenameView={renameView}
            onChangeView={changeView}
            onDropTable={dropTable}
            onSelectTable={(name) => {
              setTableName(name);
              setSelection({ kind: "table", name });
            }}
            onSelectJoin={(index) => {
              setSelectedJoinIndex(index);
              setSelection({ kind: "join", index });
            }}
            selectedJoinIndex={selectedJoinIndex}
            onNewCube={newCube}
            onSaveCube={() => void saveOpenCube()}
            onDeleteCube={() => void deleteOpenCube()}
            onRenameCube={(name) => liveCube && setCubeDraft({ ...liveCube, name })}
          />
          <Inspector
            open={inspectorOpen}
            onToggle={() => setInspectorOpen((o) => !o)}
            tab={tab}
            view={liveView}
            tables={tables}
            cube={liveCube}
            metrics={liveCube ? metricDrafts.length ? metricDrafts : metricsForCube(docs, liveCube.name) : []}
            segments={liveCube ? segmentDrafts.length ? segmentDrafts : segmentsForCube(docs, liveCube.name) : []}
            selected={selection}
            onSelect={setSelection}
            onOpenViewMapping={() => setMappingEditor({ mode: "view" })}
            onOpenCubeMapping={() => setMappingEditor({ mode: "cube" })}
          />
        </div>
        <section className="editor" aria-label="Detail">
          <Detail
            selection={selection}
            table={selectedTable}
            view={liveView}
            tables={tables}
            docs={docs}
            cube={liveCube}
            busy={busy}
            onSave={(doc) => {
              if (doc.kind === "cube") {
                setCubeDraft(doc);
                void saveDoc(doc);
                return;
              }
              if (doc.kind === "metric") {
                setMetricDrafts((cur) => {
                  const next = cur.some((m) => m.name === doc.name)
                    ? cur.map((m) => (m.name === doc.name ? doc : m))
                    : [...cur, doc];
                  return next;
                });
              }
              if (doc.kind === "segment") {
                setSegmentDrafts((cur) => {
                  const next = cur.some((m) => m.name === doc.name)
                    ? cur.map((m) => (m.name === doc.name ? doc : m))
                    : [...cur, doc];
                  return next;
                });
              }
              void saveDoc(doc);
            }}
            onChangeView={changeView}
          />
        </section>
        {mappingEditor?.mode === "view" && liveView && (
          <ViewMappingEditor
            open
            onOpenChange={(o) => !o && setMappingEditor(null)}
            view={liveView}
            tables={tables}
            onApply={changeView}
          />
        )}
        {mappingEditor?.mode === "cube" && liveCube && (
          <CubeMappingEditor
            open
            onOpenChange={(o) => !o && setMappingEditor(null)}
            cube={liveCube}
            views={views}
            onApply={setCubeDraft}
          />
        )}
      </div>
    </Theme>
  );
}
