import { useEffect, useMemo, useState } from "react";
import { Badge, Callout, Flex, IconButton, Select, Separator, Text, Theme } from "@radix-ui/themes";
import { AuthorApi, bannerError, type BannerError } from "./api";
import { Canvas } from "./components/Canvas";
import { Catalog } from "./components/Catalog";
import { CubeMappingEditor } from "./components/CubeMappingEditor";
import { Detail } from "./components/Detail";
import {
  cubesReferencingTable,
  autoMatchColumnsForSources,
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
  dropViewFromCube,
  forTarget,
  metricsForCube,
  reorder,
  segmentsForCube,
  sourceLabel,
  uniqueSourceAlias,
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
  const [viewName, setViewName] = useState("");
  const [cubeName, setCubeName] = useState("");
  const [selection, setSelection] = useState<Selection>(null);
  const [selectedJoinIndex, setSelectedJoinIndex] = useState<number | undefined>();
  const [error, setError] = useState<BannerError | null>(null);
  const [busy, setBusy] = useState(false);
  const [navOpen, setNavOpen] = useState(true);
  const [inspectorOpen, setInspectorOpen] = useState(true);
  const [detailSize, setDetailSize] = useState<"collapsed" | "standard" | "expanded">("standard");
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
      setError(bannerError(e));
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
      try {
        setPhysicalCandidates(await client.listPhysicalTables(chosen));
      } catch {
        setPhysicalCandidates([]);
      }
    }
  }

  useEffect(() => {
    reload("").catch((e: Error) => setError(bannerError(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const tables = forTarget(docs, "table", target);
  const views = forTarget(docs, "view", target);
  const cubes = forTarget(docs, "cube", target);
  const selectedTable = tables.find((t) => t.name === tableName) ?? tables[0];
  const selectedView = views.find((v) => v.name === viewName);
  const liveView = draft || editing;
  const cube = cubes.find((c) => c.name === cubeName);
  const liveCube = cubeDraft ?? cube;
  const engine = targets.find((t) => t.name === target)?.engine;

  const selectedViewKey = selectedView ? `${selectedView.name}:${JSON.stringify(selectedView)}` : "";
  const selectedCubeKey = cube ? `${cube.name}:${JSON.stringify(cube)}` : "";

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
    setCubeName("");
    setCubeDraft(undefined);
    setPicked([]);
    setSelection(null);
    setDraft(undefined);
    setEditing(undefined);
    void loadPhysicalCandidates(value);
  }

  async function saveDoc(doc: Doc, previousName?: string) {
    setBusy(true);
    setError(null);
    try {
      const renamed = Boolean(previousName && previousName !== doc.name);
      if (renamed && previousName) {
        await client.create(doc);
        if (doc.kind === "view") {
          for (const cubeDoc of cubes) {
            const stack = choiceStack(cubeDoc);
            if (!stack.includes(previousName)) continue;
            await client.update(
              "cube",
              cubeDoc.name,
              withChoiceStack(
                cubeDoc,
                stack.map((n) => (n === previousName ? doc.name : n)),
              ),
            );
          }
        }
        if (docs.some((d) => d.kind === doc.kind && d.name === previousName)) {
          await client.remove(doc.kind, previousName);
        }
      } else {
        const exists =
          doc.kind === "view"
            ? knownViews.has(doc.name) || views.some((v) => v.name === doc.name)
            : docs.some((d) => d.kind === doc.kind && d.name === doc.name);
        if (exists) await client.update(doc.kind, doc.name, doc);
        else await client.create(doc);
      }
      await reload();
    } catch (e) {
      setError(bannerError(e));
    } finally {
      setBusy(false);
    }
  }

  async function saveOpenView() {
    const doc = draft || editing;
    if (!doc) return;
    const previous = !draft && viewName && viewName !== doc.name ? viewName : undefined;
    await saveDoc(doc, previous);
    setDraft(undefined);
    setViewName(doc.name);
  }

  async function saveOpenCube() {
    if (!liveCube) return;
    setBusy(true);
    setError(null);
    try {
      const previous = cube && cube.name !== liveCube.name ? cube.name : undefined;
      if (previous) {
        await client.create(liveCube);
        for (const m of metricDrafts) {
          const body = { ...m, cube: liveCube.name };
          const ex = docs.some((d) => d.kind === "metric" && d.name === m.name);
          if (ex) await client.update("metric", m.name, body);
          else await client.create(body);
        }
        for (const s of segmentDrafts) {
          const body = { ...s, cube: liveCube.name };
          const ex = docs.some((d) => d.kind === "segment" && d.name === s.name);
          if (ex) await client.update("segment", s.name, body);
          else await client.create(body);
        }
        await client.remove("cube", previous);
      } else {
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
      }
      setCubeName(liveCube.name);
      await reload();
    } catch (e) {
      setError(bannerError(e));
    } finally {
      setBusy(false);
    }
  }

  async function registerTables(physicals: { schema: string; name: string }[]) {
    setBusy(true);
    setError(null);
    try {
      for (const physical of physicals) {
        const desc = await client.describePhysicalTable(target, physical.name, physical.schema);
        const doc = physicalTableDoc(target, physical, desc.columns || []);
        const existing = tables.find((t) => t.name === doc.name);
        if (existing) {
          await client.update("table", doc.name, {
            ...existing,
            ...doc,
            pk: existing.pk,
            fk: existing.fk,
            description: existing.description || doc.description,
            scope_filter: existing.scope_filter,
            default_filter: existing.default_filter,
            permission_filters: existing.permission_filters,
          });
        } else {
          await client.create(doc);
        }
      }
      await reload();
    } catch (e) {
      setError(bannerError(e));
    } finally {
      setBusy(false);
    }
  }

  async function deleteTable(name: string) {
    const usedViews = viewsReferencingTable(views, name);
    if (usedViews.length) {
      setError({ message: `${name} is used by view ${usedViews.map((v) => v.name).join(", ")}` });
      return;
    }
    const usedCubes = cubesReferencingTable(cubes, views, name);
    if (usedCubes.length) {
      setError({ message: `${name} is used by cube ${usedCubes.map((c) => c.name).join(", ")}` });
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await client.remove("table", name);
      if (tableName === name) setTableName("");
      await reload();
    } catch (e) {
      setError(bannerError(e));
    } finally {
      setBusy(false);
    }
  }

  async function deleteOpenView() {
    if (!editing) return;
    const name = editing.name;
    const affected = cubes.filter(
      (c) => c.view === name || (c.choice || []).some((item) => item.view === name),
    );
    setBusy(true);
    setError(null);
    try {
      for (const cubeDoc of affected) {
        const next = dropViewFromCube(cubeDoc, name);
        if (!next.view) continue;
        if (liveCube?.name === cubeDoc.name) setCubeDraft(next);
        await client.update("cube", cubeDoc.name, next);
      }
      await client.remove("view", name);
      setViewName((cur) => (cur === name ? "" : cur));
      setDraft((d) => (d?.name === name ? undefined : d));
      setEditing(undefined);
      setSelection(null);
      await reload();
    } catch (e) {
      setError(bannerError(e));
    } finally {
      setBusy(false);
    }
  }

  async function deleteOpenCube() {
    if (!liveCube) return;
    setBusy(true);
    setError(null);
    try {
      for (const m of metricsForCube(docs, liveCube.name)) await client.remove("metric", m.name);
      for (const s of segmentsForCube(docs, liveCube.name)) await client.remove("segment", s.name);
      await client.remove("cube", liveCube.name);
      setCubeDraft(undefined);
      setCubeName("");
      await reload();
    } catch (e) {
      setError(bannerError(e));
    } finally {
      setBusy(false);
    }
  }

  function newView(combine: TableCombine, name: string) {
    setTableCombine(combine);
    const chosen = uniqueName(name.trim() || `${combine}_view`, views);
    setDraft({
      kind: "view",
      name: chosen,
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
    setSelection({ kind: "view", name: chosen });
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

  function openCube(name: string) {
    const found = cubes.find((c) => c.name === name);
    if (!found) return;
    setCubeName(name);
    setCubeDraft(found);
    setMetricDrafts(metricsForCube(docs, found.name).map((d) => JSON.parse(JSON.stringify(d)) as Doc));
    setSegmentDrafts(segmentsForCube(docs, found.name).map((d) => JSON.parse(JSON.stringify(d)) as Doc));
    setSelection({ kind: "cube", name });
  }

  function renameView(name: string) {
    const doc = draft || editing;
    if (!doc) return;
    const next = { ...doc, name };
    if (draft) setDraft(next);
    else setEditing(next);
  }

  function changeView(view: Doc) {
    const prev = draft || editing;
    if (selection?.kind === "table") {
      const oldLabels = (prev?.sources || []).map(sourceLabel);
      const newLabels = (view.sources || []).map(sourceLabel);
      if (!newLabels.includes(selection.name)) {
        const idx = oldLabels.indexOf(selection.name);
        if (idx >= 0 && newLabels[idx]) setSelection({ kind: "table", name: newLabels[idx] });
      }
    }
    if (selection?.kind === "column") {
      const oldNames = (prev?.columns || []).map((c) => c.name);
      const newNames = (view.columns || []).map((c) => c.name);
      if (!newNames.includes(selection.name)) {
        const idx = oldNames.indexOf(selection.name);
        if (idx >= 0 && oldNames.length === newNames.length && newNames[idx] !== undefined) {
          setSelection({ kind: "column", name: newNames[idx] });
        } else {
          setSelection({ kind: "view", name: view.name });
        }
      }
    }
    if (draft && draft.name === view.name) setDraft(view);
    else setEditing(view);
  }

  function dropTable(kind: TableCombine, name: string) {
    setTableCombine(kind);
    const base = draft || editing;
    if (kind === "join" && base) {
      const sources = base.sources || [];
      const alias = uniqueSourceAlias(sources, name);
      const entry = alias === name ? { kind: "table", table: name } : { kind: "table", table: name, alias };
      const nextSources = [...sources, entry];
      const next = {
        ...base,
        sources: nextSources,
        columns: autoMatchColumnsForSources(nextSources, tables),
        joins: base.joins || [],
      };
      if (draft) setDraft(next);
      else setEditing(next);
      setPicked(nextSources.map((s) => s.table));
      return;
    }
    if ((kind === "union" || kind === "minus") && base) {
      const sources = base.sources || [];
      if (sources.some((s) => s.table === name)) return;
      const nextSources = [...sources, { kind: "table", table: name }];
      const pickedTables = nextSources
        .map((s) => tables.find((t) => t.name === s.table))
        .filter((t): t is Doc => Boolean(t));
      const rebuilt = draftView(kind, pickedTables, target);
      const next = rebuilt
        ? { ...rebuilt, name: base.name, description: base.description || "" }
        : { ...base, sources: nextSources };
      if (draft) setDraft(next);
      else setEditing(next);
      setPicked(nextSources.map((s) => s.table));
      return;
    }
    setPicked((cur) => {
      const next = cur.includes(name) ? cur : [...cur, name];
      const pickedTables = next
        .map((n) => tables.find((t) => t.name === n))
        .filter((t): t is Doc => Boolean(t));
      const rebuilt = draftView(kind, pickedTables, target);
      if (rebuilt) {
        rebuilt.name = uniqueName(rebuilt.name, views);
        setDraft(rebuilt);
      }
      return next;
    });
  }

  function dropViewOntoStack(name: string) {
    if (!views.some((v) => v.name === name)) return;
    if (liveCube) {
      const stack = choiceStack(liveCube);
      if (stack.includes(name)) return;
      setCubeDraft(withChoiceStack(liveCube, [...stack, name]));
      return;
    }
    const cubeDoc = withChoiceStack(
      {
        kind: "cube",
        name: uniqueName(`cube_${name}`, cubes),
        target,
        view: name,
        description: "",
        dimensions: [],
        facts: [],
      },
      [name],
    );
    setCubeDraft(cubeDoc);
    setCubeName(cubeDoc.name);
    setMetricDrafts([]);
    setSegmentDrafts([]);
    setSelection({ kind: "cube", name: cubeDoc.name });
  }

  function newCube() {
    const name = uniqueName("cube", cubes);
    setCubeName(name);
    setCubeDraft({
      kind: "cube",
      name,
      target,
      description: "",
      dimensions: [],
      facts: [],
    });
    setMetricDrafts([]);
    setSegmentDrafts([]);
    setSelection({ kind: "cube", name });
  }

  function addMetric() {
    if (!liveCube) return;
    const name = uniqueName("metric", [
      ...metricDrafts,
      ...docs.filter((d) => d.kind === "metric"),
    ]);
    const viewDoc = views.find((v) => v.name === liveCube.view) || liveView;
    const first = viewDoc?.columns?.[0]?.name;
    const doc: Doc = {
      kind: "metric",
      name,
      description: "",
      cube: liveCube.name,
      expression: "",
      aggregation: "group",
      filter: null,
    };
    if (first) doc.group_by = [first];
    setMetricDrafts((cur) => [...cur, doc]);
    setSelection({ kind: "metric", name });
  }

  function addSegment() {
    if (!liveCube) return;
    const name = uniqueName("segment", [
      ...segmentDrafts,
      ...docs.filter((d) => d.kind === "segment"),
    ]);
    setSegmentDrafts((cur) => [
      ...cur,
      {
        kind: "segment",
        name,
        description: "",
        cube: liveCube.name,
        filter: { expression: "" },
      },
    ]);
    setSelection({ kind: "segment", name });
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
      <div className={[error ? "shell has-error" : "shell", `detail-${detailSize}`].join(" ")}>
        <header className="topbar">
          <Flex align="center" gap="3">
            <Text size="3" weight="bold">Semantic</Text>
            <Separator orientation="vertical" />
            <Text size="2" color="gray">Database</Text>
            {target ? (
              <Select.Root size="2" value={target} onValueChange={chooseTarget}>
                <Select.Trigger aria-label="Database" />
                <Select.Content position="popper">
                  {targets.map((t) => (
                    <Select.Item key={t.name} value={t.name}>{t.name}</Select.Item>
                  ))}
                </Select.Content>
              </Select.Root>
            ) : (
              <Text size="2" color="gray">{error ? "No database" : "Loading databases"}</Text>
            )}
            {engine && <Badge variant="soft" color="gray">{engine}</Badge>}
          </Flex>
          <Flex align="center" gap="3">
            <Text size="1" color="gray" className="topbar-note">CRUD over semantic JSON · one target</Text>
            {busy && <Badge variant="soft">Saving</Badge>}
          </Flex>
        </header>
        {error && (
          <div className="alert" role="alert">
            <Callout.Root color={error.code === "forbidden" ? "amber" : "red"} size="1" variant="soft">
              <Flex align="start" justify="between" gap="3">
                <div>
                  {error.code && (
                    <Text size="1" weight="medium" className="error-code">{error.code}</Text>
                  )}
                  <Callout.Text>{error.message}</Callout.Text>
                </div>
                <IconButton size="1" variant="ghost" color="gray" aria-label="Dismiss error" onClick={() => setError(null)}>
                  ×
                </IconButton>
              </Flex>
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
            onSelectView={(name) => {
              openView(name);
            }}
            busy={busy}
            physicalCandidates={physicalCandidates}
            onRegister={registerTables}
            onDeleteTable={deleteTable}
          />
          <Canvas
            tab={tab}
            selectedView={selectedView}
            stackNames={stack}
            onDropStack={(from, to) => {
              if (!liveCube) return;
              setCubeDraft(withChoiceStack(liveCube, reorder(choiceStack(liveCube), from, to)));
            }}
            onRemoveFromStack={(name) => {
              if (liveCube) setCubeDraft(dropViewFromCube(liveCube, name));
            }}
            onViewDrop={dropViewOntoStack}
            combine={tableCombine}
            onCombine={setTableCombine}
            picked={picked}
            draft={draft}
            editing={editing}
            busy={busy}
            views={views}
            cube={liveCube}
            cubes={cubes}
            cubeDirty={cubeDirty}
            onSaveView={() => void saveOpenView()}
            onDeleteView={() => void deleteOpenView()}
            onNewView={newView}
            onOpenView={openView}
            onRenameView={renameView}
            onChangeView={changeView}
            onDropTable={dropTable}
            onSelectTable={(name) => {
              const src = liveView?.sources?.find((s) => (s.alias || s.table) === name);
              setTableName(src?.table || name);
              setSelection({ kind: "table", name });
            }}
            onSelectJoin={(index) => {
              setSelectedJoinIndex(index);
              setSelection({ kind: "join", index });
            }}
            selectedJoinIndex={selectedJoinIndex}
            onNewCube={newCube}
            onOpenCube={openCube}
            onSaveCube={() => void saveOpenCube()}
            onDeleteCube={() => void deleteOpenCube()}
            onRenameCube={(name) => liveCube && setCubeDraft({ ...liveCube, name })}
          />
          <Inspector
            open={inspectorOpen}
            onToggle={() => setInspectorOpen((o) => !o)}
            tab={tab}
            view={liveView}
            views={views}
            tables={tables}
            cube={liveCube}
            metrics={liveCube ? metricDrafts.length ? metricDrafts : metricsForCube(docs, liveCube.name) : []}
            segments={liveCube ? segmentDrafts.length ? segmentDrafts : segmentsForCube(docs, liveCube.name) : []}
            selected={selection}
            onSelect={setSelection}
            onOpenViewMapping={() => setMappingEditor({ mode: "view" })}
            onOpenCubeMapping={() => setMappingEditor({ mode: "cube" })}
            onAddMetric={addMetric}
            onAddSegment={addSegment}
            onChangeView={changeView}
          />
        </div>
        <section className={detailSize === "collapsed" ? "editor is-collapsed" : "editor"} aria-label="Detail">
          <div className="pane-bar" role="toolbar" aria-label="Detail header">
            <Text size="2" weight="medium">Detail</Text>
            <Flex gap="1">
              <IconButton
                size="1"
                variant="ghost"
                color="gray"
                aria-pressed={detailSize === "collapsed"}
                aria-label="Collapse detail"
                onClick={() => setDetailSize("collapsed")}
              >
                <DetailSizeGlyph kind="collapse" />
              </IconButton>
              <IconButton
                size="1"
                variant="ghost"
                color="gray"
                aria-pressed={detailSize === "standard"}
                aria-label="Standard detail"
                onClick={() => setDetailSize("standard")}
              >
                <DetailSizeGlyph kind="standard" />
              </IconButton>
              <IconButton
                size="1"
                variant="ghost"
                color="gray"
                aria-pressed={detailSize === "expanded"}
                aria-label="Expand detail"
                onClick={() => setDetailSize("expanded")}
              >
                <DetailSizeGlyph kind="expand" />
              </IconButton>
            </Flex>
          </div>
          {detailSize !== "collapsed" && (
            <div className="detail-body">
              <Detail
                selection={selection}
                tab={tab}
                table={selectedTable}
                view={liveView}
                tables={tables}
                docs={docs}
                cube={liveCube}
                metrics={metricDrafts}
                segments={segmentDrafts}
                busy={busy}
                onSave={(doc, previousName) => {
                  if (doc.kind === "cube") {
                    setCubeDraft(doc);
                    return;
                  }
                  if (doc.kind === "metric") {
                    setMetricDrafts((cur) => {
                      const key = previousName || doc.name;
                      if (cur.some((m) => m.name === key)) {
                        return cur.map((m) => (m.name === key ? doc : m));
                      }
                      return [...cur, doc];
                    });
                  }
                  if (doc.kind === "segment") {
                    setSegmentDrafts((cur) => {
                      const key = previousName || doc.name;
                      if (cur.some((m) => m.name === key)) {
                        return cur.map((m) => (m.name === key ? doc : m));
                      }
                      return [...cur, doc];
                    });
                  }
                  void saveDoc(doc, previousName);
                }}
                onSelect={setSelection}
                onChangeView={changeView}
              />
            </div>
          )}
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
            tables={tables}
            onApply={setCubeDraft}
          />
        )}
      </div>
    </Theme>
  );
}

function DetailSizeGlyph({ kind }: { kind: "collapse" | "standard" | "expand" }) {
  return (
    <svg className="detail-size-glyph" viewBox="0 0 16 16" width="18" height="18" aria-hidden="true" fill="currentColor">
      {kind === "collapse" ? <path d="M2.5 5 L8 12.5 L13.5 5 Z" /> : null}
      {kind === "standard" ? <rect x="2" y="6.75" width="12" height="2.5" rx="0.6" /> : null}
      {kind === "expand" ? <path d="M2.5 11 L8 3.5 L13.5 11 Z" /> : null}
    </svg>
  );
}
