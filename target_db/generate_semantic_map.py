#!/usr/bin/env python3
"""meta/*.json → meta/semantic-map.md (diagram-first Semantic Map).

View join은 Mermaid(테이블명+간선만). PK/FK는 Column mappings 셀에
`(PK)`/`(FK)`로 표기. Cube는 Dim/Fact/Metric/Segment Markdown 표. SoT는 JSON.
"""

from __future__ import annotations

import argparse
import json
import re
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent

FORMAT = "sql-mcp.semantic-map/v1"

KINDS = ("table", "view", "cube", "metric", "segment")


def load_meta_docs(meta_dir: Path) -> dict[str, list[dict]]:
    """Load kind-grouped docs from a meta dir or any tree of *.json."""
    by_kind: dict[str, list[dict]] = {k: [] for k in KINDS}
    paths = sorted(meta_dir.rglob("*.json"))
    for path in paths:
        if path.name == "semantic-map.md":
            continue
        try:
            doc = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        kind = doc.get("kind")
        if kind in by_kind and isinstance(doc.get("name"), str):
            by_kind[kind].append(doc)
    for kind in KINDS:
        by_kind[kind].sort(key=lambda d: d["name"])
    return by_kind


def _mmd_escape(text: str) -> str:
    return (
        text.replace("\\", "/")
        .replace('"', "'")
        .replace("`", "'")
        .replace("<", "(")
        .replace(">", ")")
        .replace("\n", " ")
    )


def _table_key_sets(table: dict | None) -> tuple[set[str], set[str]]:
    if not table:
        return set(), set()
    pk = set(table.get("pk") or [])
    fk_cols: set[str] = set()
    for fk in table.get("fk") or []:
        fk_cols.update(fk.get("columns") or [])
    return pk, fk_cols


def _phys_with_keys(col: str, pk: set[str], fk_cols: set[str]) -> str:
    """`col` (PK) / `col` (FK) / `col` (PK, FK)."""
    cell = f"`{_md_cell(col)}`"
    marks: list[str] = []
    if col in pk:
        marks.append("PK")
    if col in fk_cols:
        marks.append("FK")
    if not marks:
        return cell
    return f"{cell} ({', '.join(marks)})"


def _flow_node_id(name: str) -> str:
    s = re.sub(r"[^A-Za-z0-9_]", "_", name or "t")
    if not s or s[0].isdigit():
        s = f"t_{s}"
    return s


def _join_on_label(join: dict) -> str:
    parts = []
    for on in join.get("on") or []:
        lc = on.get("left_column") or "?"
        rc = on.get("right_column") or "?"
        if lc == rc:
            parts.append(str(lc))
        else:
            parts.append(f"{lc}={rc}")
    jtype = join.get("type") or "inner"
    on_s = ",".join(parts) if parts else "?"
    return _mmd_escape(f"{jtype} · {on_s}")


def _md_cell(text: str) -> str:
    return str(text).replace("|", "\\|").replace("\n", " ")


def render_view_column_table(
    view: dict,
    tables_by_name: dict[str, dict] | None = None,
) -> str:
    """view column × source table 매트릭스. 물리 컬럼에 (PK)/(FK) 표기."""
    tables_by_name = tables_by_name or {}
    def _source_label(s: dict) -> str | None:
        alias = s.get("alias")
        catalog = s.get("table") or s.get("name")
        if not catalog:
            return None
        if alias and alias != catalog:
            return f"{alias} ({catalog})"
        return alias or catalog

    def _source_catalog(s: dict) -> str | None:
        return s.get("table") or s.get("name")

    source_labels = []
    source_catalogs = []
    for s in view.get("sources") or []:
        label = _source_label(s)
        catalog = _source_catalog(s)
        if label and catalog:
            source_labels.append(label)
            source_catalogs.append(catalog)
    # mapping.source uses alias (or catalog when no alias)
    source_aliases = [
        (s.get("alias") or s.get("table") or s.get("name"))
        for s in (view.get("sources") or [])
        if (s.get("alias") or s.get("table") or s.get("name"))
    ]
    cols = view.get("columns") or []
    if not source_aliases and not cols:
        return "_No column mappings._\n"

    key_sets = {
        alias: _table_key_sets(tables_by_name.get(cat))
        for alias, cat in zip(source_aliases, source_catalogs)
    }

    header = ["view column", *source_labels]
    sep = ["---"] * len(header)
    rows = [
        "| " + " | ".join(header) + " |",
        "| " + " | ".join(sep) + " |",
    ]

    mapped: dict[str, set[str]] = {s: set() for s in source_aliases}
    for col in cols:
        name = _md_cell(col.get("name") or "?")
        by_src: dict[str, list[str]] = {s: [] for s in source_aliases}
        for m in col.get("mappings") or []:
            src = m.get("source")
            phys = m.get("column")
            if not src or phys is None:
                continue
            if src in by_src:
                by_src[src].append(str(phys))
            if src in mapped:
                mapped[src].add(str(phys))
        cells = [f"`{name}`"]
        for s in source_aliases:
            pk, fk = key_sets[s]
            if by_src[s]:
                cells.append(
                    ", ".join(_phys_with_keys(p, pk, fk) for p in by_src[s])
                )
            else:
                cells.append("")
        rows.append("| " + " | ".join(cells) + " |")

    # 미매핑: source table 물리 컬럼 중 어떤 view mapping에도 안 나온 것
    unmapped_cells = ["_(unmapped)_"]
    any_unmapped = False
    for alias, cat in zip(source_aliases, source_catalogs):
        tdoc = tables_by_name.get(cat)
        if not tdoc:
            unmapped_cells.append("")
            continue
        pk, fk = key_sets[alias]
        phys_cols = [
            str(c.get("name"))
            for c in (tdoc.get("columns") or [])
            if c.get("name") is not None
        ]
        um = [c for c in phys_cols if c not in mapped[alias]]
        if um:
            any_unmapped = True
            unmapped_cells.append(
                ", ".join(_phys_with_keys(c, pk, fk) for c in um)
            )
        else:
            unmapped_cells.append("")
    if any_unmapped:
        rows.append("| " + " | ".join(unmapped_cells) + " |")

    return "\n".join(rows) + "\n"


def render_view_graph(
    views: list[dict],
    tables: list[dict] | None = None,
) -> str:
    """View = Mermaid(테이블명+join) + Column mappings((PK)/(FK))."""
    if not views:
        return "_No views._\n"
    tables_by_name = {t["name"]: t for t in (tables or []) if t.get("name")}
    chunks: list[str] = []
    for view in views:
        combine = view.get("combine") or "join"
        sources = view.get("sources") or []
        heading = f"#### `{view['name']}` · combine=`{combine}`"
        lines = ["```mermaid", "flowchart LR"]
        def _src_alias(src: dict) -> str:
            return src.get("alias") or src.get("table") or src.get("name") or "src"

        def _src_label(src: dict) -> str:
            alias = _src_alias(src)
            catalog = src.get("table") or src.get("name") or alias
            if alias != catalog:
                return f"{alias} ({catalog})"
            return alias

        source_ids: dict[str, str] = {}
        for src in sources:
            sname = _src_alias(src)
            sid = _flow_node_id(sname)
            source_ids[sname] = sid
            lines.append(f'  {sid}["{_mmd_escape(_src_label(src))}"]')
        if combine == "join":
            joins = view.get("joins") or []
            if joins:
                for join in joins:
                    left = source_ids.get(
                        join.get("left") or "",
                        _flow_node_id(join.get("left") or "L"),
                    )
                    right = source_ids.get(
                        join.get("right") or "",
                        _flow_node_id(join.get("right") or "R"),
                    )
                    label = _join_on_label(join)
                    lines.append(f'  {left} -- "{label}" --> {right}')
            elif len(sources) >= 2:
                for i in range(len(sources) - 1):
                    a = source_ids[_src_alias(sources[i])]
                    b = source_ids[_src_alias(sources[i + 1])]
                    lines.append(f"  {a} --- {b}")
        elif combine in ("union", "minus"):
            names = [_src_alias(s) for s in sources]
            for i in range(len(names) - 1):
                a = source_ids[names[i]]
                b = source_ids[names[i + 1]]
                lines.append(f'  {a} -. "{combine}" .- {b}')
        lines.append("```")
        block = (
            heading
            + "\n\n"
            + "\n".join(lines)
            + "\n\n"
            + "**Column mappings**\n\n"
            + render_view_column_table(view, tables_by_name)
        )
        chunks.append(block.rstrip())
    return "\n\n".join(chunks) + "\n"


def _view_mapping_cell(member: dict) -> str:
    """dim/fact → view column 또는 expression."""
    if member.get("column"):
        return f"`{_md_cell(member['column'])}`"
    if member.get("expression"):
        return f"`{_md_cell(member['expression'])}`"
    return "—"


def _as_code_note(member: dict) -> str:
    ac = member.get("as_code")
    if not isinstance(ac, dict) or not ac.get("table"):
        return ""
    key = ac.get("key_column") or "?"
    val = ac.get("value_column") or "?"
    return f"as_code `{_md_cell(ac['table'])}`({_md_cell(key)}→{_md_cell(val)})"


def _member_table(title: str, members: list[dict]) -> str:
    """Dimensions/Facts: name · description · view column."""
    if not members:
        return f"**{title}**\n\n_(none)_\n"
    rows = [
        f"**{title}**",
        "",
        "| name | type | description | view column |",
        "|---|---|---|---|",
    ]
    for m in members:
        name = _md_cell(m.get("name") or "?")
        mtype = _md_cell(m.get("type") or "")
        raw_desc = (m.get("description") or "").strip()
        desc = _md_cell(raw_desc) if raw_desc else "—"
        mapping = _view_mapping_cell(m)
        note = _as_code_note(m)
        if note:
            desc = f"{desc} · {note}" if raw_desc else note
        rows.append(f"| `{name}` | {mtype} | {desc} | {mapping} |")
    return "\n".join(rows) + "\n"


def _metrics_table(metrics: list[dict]) -> str:
    if not metrics:
        return "**Metrics**\n\n_(none)_\n"
    rows = [
        "**Metrics**",
        "",
        "| name | description | expression | aggregation | group_by | filter |",
        "|---|---|---|---|---|---|",
    ]
    for m in sorted(metrics, key=lambda x: x.get("name") or ""):
        name = _md_cell(m.get("name") or "?")
        raw_desc = (m.get("description") or "").strip()
        desc = _md_cell(raw_desc) if raw_desc else "—"
        expr = f"`{_md_cell(m.get('expression') or '')}`"
        agg = _md_cell(m.get("aggregation") or "")
        gb = ", ".join(f"`{_md_cell(g)}`" for g in (m.get("group_by") or []))
        filt = m.get("filter")
        if isinstance(filt, dict) and filt.get("expression"):
            fcell = f"`{_md_cell(filt['expression'])}`"
        elif filt is None:
            fcell = "—"
        else:
            fcell = _md_cell(str(filt))
        rows.append(
            f"| `{name}` | {desc} | {expr} | {agg} | {gb or '—'} | {fcell} |"
        )
    return "\n".join(rows) + "\n"


def _segments_table(segments: list[dict]) -> str:
    if not segments:
        return "**Segments**\n\n_(none)_\n"
    rows = [
        "**Segments**",
        "",
        "| name | description | filter |",
        "|---|---|---|",
    ]
    for s in sorted(segments, key=lambda x: x.get("name") or ""):
        name = _md_cell(s.get("name") or "?")
        raw_desc = (s.get("description") or "").strip()
        desc = _md_cell(raw_desc) if raw_desc else "—"
        filt = s.get("filter") or {}
        expr = ""
        if isinstance(filt, dict):
            expr = str(filt.get("expression") or "")
        fcell = f"`{_md_cell(expr)}`" if expr else "—"
        rows.append(f"| `{name}` | {desc} | {fcell} |")
    return "\n".join(rows) + "\n"


def render_cube_canvas(
    cubes: list[dict],
    metrics: list[dict],
    segments: list[dict],
) -> str:
    """Cube = 표만. Dim/Fact(name·description·view column) + metric/segment."""
    if not cubes:
        return "_No cubes._\n"
    metrics_by_cube: dict[str, list[dict]] = defaultdict(list)
    for m in metrics:
        metrics_by_cube[m.get("cube") or ""].append(m)
    segs_by_cube: dict[str, list[dict]] = defaultdict(list)
    for s in segments:
        segs_by_cube[s.get("cube") or ""].append(s)

    chunks: list[str] = []
    for cube in cubes:
        view = cube.get("view") or "?"
        heading = f"#### `{cube['name']}` · view=`{view}`"
        desc = cube.get("description") or ""
        parts = [heading, ""]
        if desc:
            parts.append(desc)
            parts.append("")
        dims = list(cube.get("dimensions") or [])
        facts = list(cube.get("facts") or [])
        parts.append(_member_table("Dimensions", dims).rstrip())
        parts.append("")
        parts.append(_member_table("Facts", facts).rstrip())
        parts.append("")
        parts.append(
            _metrics_table(metrics_by_cube.get(cube["name"], [])).rstrip()
        )
        parts.append("")
        parts.append(
            _segments_table(segs_by_cube.get(cube["name"], [])).rstrip()
        )
        chunks.append("\n".join(parts).rstrip())
    return "\n\n".join(chunks) + "\n"


def _infer_target(docs: dict[str, list[dict]], fallback: str | None) -> str:
    if fallback:
        return fallback
    for kind in ("cube", "view", "table"):
        for d in docs[kind]:
            if d.get("target"):
                return str(d["target"])
    return "unknown"


def build_semantic_map(
    docs: dict[str, list[dict]],
    target: str | None = None,
    generated_at: str | None = None,
) -> str:
    tgt = _infer_target(docs, target)
    ts = generated_at or datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    counts = {k: len(docs[k]) for k in KINDS}
    overview = (
        f"{counts['table']} tables · {counts['view']} views · "
        f"{counts['cube']} cubes · {counts['metric']} metrics · "
        f"{counts['segment']} segments"
    )

    parts = [
        "---",
        f"format: {FORMAT}",
        f"target: {tgt}",
        "mode: diagram-first",
        "generated_from: meta/*.json",
        f"generated_at: {ts}",
        "---",
        "",
        f"# Semantic Map — `{tgt}`",
        "",
        "> SoT: `/api/meta` / meta JSON. 이 파일은 읽기 전용 다이어그램 투영이다.",
        f"> {overview}",
        "",
        "## A. Views",
        "",
        "각 view: Mermaid=테이블명+join, Column mappings에 `(PK)`/`(FK)`.",
        "",
        render_view_graph(docs["view"], docs["table"]),
        "## B. Cubes",
        "",
        "Dim/Fact: name · description · view column 매핑. "
        "Metric/Segment도 표.",
        "",
        render_cube_canvas(docs["cube"], docs["metric"], docs["segment"]),
    ]
    return "\n".join(parts).rstrip() + "\n"


def write_semantic_map(meta_dir: Path, target: str | None = None) -> Path:
    docs = load_meta_docs(meta_dir)
    if not any(docs[k] for k in KINDS):
        raise FileNotFoundError(f"no meta JSON in {meta_dir}")
    md = build_semantic_map(docs, target=target or meta_dir.parent.name)
    out = meta_dir / "semantic-map.md"
    out.write_text(md, encoding="utf-8")
    return out


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "db_id",
        nargs="*",
        help="target_db/<db_id> only (default: all dirs with meta/)",
    )
    args = parser.parse_args(argv)
    if args.db_id:
        metas = [ROOT / db / "meta" for db in args.db_id]
    else:
        metas = sorted(p for p in ROOT.glob("*/meta") if p.is_dir())
    n = 0
    for meta_dir in metas:
        if not meta_dir.is_dir():
            print(f"skip missing: {meta_dir}")
            continue
        try:
            out = write_semantic_map(meta_dir)
        except FileNotFoundError as e:
            print(f"skip: {e}")
            continue
        n += 1
        print(f"wrote {out.relative_to(ROOT.parent)}")
    print(f"done: {n} semantic-map.md")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
