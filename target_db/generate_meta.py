#!/usr/bin/env python3
"""target_db/{db}/*.sqlite → meta/{name}.json (table|view|cube|metric 골격).

물리 카탈로그는 sqlite에서 직접 읽고, MCP는 쓰지 않는다.
cube/metric은 view당 휴리스틱 초안(코딩 에이전트가 다듬을 수 있음).
"""

from __future__ import annotations

import json
import re
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parent

NAME_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
AMOUNT_HINT = re.compile(
    r"(amount|balance|price|cost|salary|revenue|sum|total)",
    re.I,
)
FX_HINT = re.compile(r"(^|_)(fx|rate|exchange)(_|$)|fx_rate|exchange_rate", re.I)
QTY_HINT = re.compile(
    r"(qty|quantity|count|score|points?|wins?|loss|num|number|volume)",
    re.I,
)
CODE_HINT = re.compile(
    r"(^|_)(code|status|type|freq|gender|class|level|flag|align)(_|$)",
    re.I,
)
DATE_HINT = re.compile(r"(date|time|year|month|day|dob|birth)", re.I)
ID_HINT = re.compile(r"(^|_)(id|key)(_|$)", re.I)
NUMERIC_TYPES = re.compile(
    r"^(INT|INTEGER|REAL|FLOAT|DOUBLE|NUMERIC|DECIMAL|NUMBER)", re.I
)


def quote_ident(name: str) -> str:
    return '"' + name.replace('"', '""') + '"'


def resolve_table_name(tables: dict[str, dict], ref: str) -> str | None:
    if ref in tables:
        return ref
    lower = {k.lower(): k for k in tables}
    return lower.get(ref.lower())


def is_ident(name: str) -> bool:
    return bool(NAME_RE.match(name))


def mark_as_code(col_name: str, col_type: str) -> bool:
    if CODE_HINT.search(col_name):
        return True
    if col_type and re.search(r"CHAR|TEXT|CLOB", col_type, re.I) and CODE_HINT.search(
        col_name
    ):
        return True
    return False


def physical_type(tables: dict[str, dict], source: str, column: str) -> str:
    t = tables.get(source)
    if not t:
        return "NULL"
    for c in t["columns"]:
        if c["name"] == column:
            return c.get("type") or "NULL"
    return "NULL"


def classify_view_column(
    view_col: str, phys_type: str
) -> tuple[str, dict] | None:
    """Return ('dimension'|'fact', member dict) or None if skipped."""
    if not is_ident(view_col):
        return None
    typ = phys_type or ""
    lower = view_col.lower()
    if DATE_HINT.search(lower) and not NUMERIC_TYPES.match(typ):
        dtype = "datetime" if "time" in lower else "date"
        return (
            "dimension",
            {
                "name": view_col,
                "description": f"Dimension {view_col}.",
                "type": dtype,
                "column": view_col,
                "encryption": False,
                "role": "all",
            },
        )
    if NUMERIC_TYPES.match(typ) and not ID_HINT.search(lower):
        if FX_HINT.search(lower) and not AMOUNT_HINT.search(lower):
            ftype = "fx"
        elif AMOUNT_HINT.search(lower):
            ftype = "amount"
        elif QTY_HINT.search(lower):
            ftype = "quantity"
        else:
            ftype = "quantity"
        return (
            "fact",
            {
                "name": view_col,
                "description": f"Fact {view_col} ({ftype}).",
                "type": ftype,
                "column": view_col,
                "encryption": False,
                "role": "all",
            },
        )
    # ids / text / other → dimension
    dim_type = "code" if CODE_HINT.search(lower) or mark_as_code(view_col, typ) else "string"
    return (
        "dimension",
        {
            "name": view_col,
            "description": f"Dimension {view_col}.",
            "type": dim_type,
            "column": view_col,
            "encryption": False,
            "role": "all",
        },
    )


def alloc_name(base: str, taken: set[str], *suffixes: str) -> str:
    """Pick base if free+ident, else base_suffix, else base_suffixN."""
    if is_ident(base) and base not in taken:
        return base
    for suf in suffixes:
        cand = f"{base}_{suf}" if suf else base
        if is_ident(cand) and cand not in taken:
            return cand
        n = 2
        while True:
            cand_n = f"{cand}_{n}"
            if is_ident(cand_n) and cand_n not in taken:
                return cand_n
            n += 1
    # unreachable if suffixes non-empty
    raise ValueError(f"cannot alloc name from {base!r}")


def priority_names(
    cubes: list[dict], metrics: list[dict], segments: list[dict] | None = None
) -> set[str]:
    out: set[str] = set()
    for c in cubes:
        out.add(c["name"])
        for d in c.get("dimensions", []):
            out.add(d["name"])
        for f in c.get("facts", []):
            out.add(f["name"])
    for m in metrics:
        out.add(m["name"])
    for s in segments or []:
        out.add(s["name"])
    return out


def rename_table(
    tables: dict[str, dict], views: list[dict], old: str, new: str
) -> None:
    doc = tables.pop(old)
    doc["name"] = new
    tables[new] = doc
    for t in tables.values():
        for fk in t.get("fk", []):
            if fk["references"]["table"] == old:
                fk["references"]["table"] = new
    for v in views:
        for src in v.get("sources", []):
            if src.get("name") == old:
                src["name"] = new
        for j in v.get("joins", []):
            if j.get("left") == old:
                j["left"] = new
            if j.get("right") == old:
                j["right"] = new
        for col in v.get("columns", []):
            for m in col.get("mappings", []):
                if m.get("source") == old:
                    m["source"] = new


def rename_view(views: list[dict], cubes: list[dict], old: str, new: str) -> None:
    for v in views:
        if v["name"] == old:
            v["name"] = new
    for c in cubes:
        if c.get("view") == old:
            c["view"] = new


def rename_view_column(
    view: dict,
    cubes: list[dict],
    metrics: list[dict],
    old: str,
    new: str,
) -> None:
    for col in view["columns"]:
        if col["name"] == old:
            col["name"] = new
    vname = view["name"]
    cube_names = {c["name"] for c in cubes if c.get("view") == vname}
    for c in cubes:
        if c.get("view") != vname:
            continue
        for member in c.get("dimensions", []) + c.get("facts", []):
            if member.get("column") == old:
                member["column"] = new
            expr = member.get("expression")
            if isinstance(expr, str):
                member["expression"] = re.sub(
                    rf"\b{re.escape(old)}\b", new, expr
                )
    for m in metrics:
        if m.get("cube") not in cube_names:
            continue
        m["expression"] = re.sub(
            rf"\b{re.escape(old)}\b", new, m.get("expression", "")
        )
        m["group_by"] = [new if g == old else g for g in m.get("group_by") or []]


def apply_name_priority(
    tables: dict[str, dict],
    views: list[dict],
    cubes: list[dict],
    metrics: list[dict],
) -> None:
    """cube/dim/fact/metric/segment keep natural names; table/view/view-col yield."""
    reserved = priority_names(cubes, metrics)
    taken = set(reserved)

    # tables yield
    for old in list(tables):
        if old not in reserved:
            taken.add(old)
            continue
        new = alloc_name(old, taken, "table")
        rename_table(tables, views, old, new)
        taken.add(new)

    # views yield
    for v in list(views):
        old = v["name"]
        if old not in reserved:
            taken.add(old)
            continue
        new = alloc_name(old, taken, "view")
        rename_view(views, cubes, old, new)
        taken.add(new)

    # view columns yield unless they are the bound column for a priority member
    # with the same natural name on a cube over this view
    for v in views:
        bound_same = set()
        for c in cubes:
            if c.get("view") != v["name"]:
                continue
            for member in c.get("dimensions", []) + c.get("facts", []):
                col = member.get("column")
                if col and col == member.get("name"):
                    bound_same.add(col)
        for col in list(v["columns"]):
            old = col["name"]
            if old not in reserved or old in bound_same:
                taken.add(old)
                continue
            new = alloc_name(old, taken, "col")
            rename_view_column(v, cubes, metrics, old, new)
            taken.add(new)


def cube_and_metric_from_view(
    view: dict, tables: dict[str, dict], priority_taken: set[str]
) -> tuple[dict, dict | None]:
    """Choose natural cube/metric/member names; do not yield to table/view names."""
    dimensions: list[dict] = []
    facts: list[dict] = []
    for col in view["columns"]:
        vname = col["name"]
        mapping = col["mappings"][0]
        ptype = physical_type(tables, mapping["source"], mapping["column"])
        classified = classify_view_column(vname, ptype)
        if classified is None:
            continue
        kind, member = classified
        if kind == "dimension":
            dimensions.append(member)
        else:
            facts.append(member)

    if not dimensions and facts:
        f0 = facts[0]
        dimensions.append(
            {
                "name": f0["name"],
                "description": f"Dimension {f0['name']}.",
                "type": "string",
                "column": f0["column"],
                "encryption": False,
                "role": "all",
            }
        )
        facts = facts[1:]

    if not dimensions:
        dimensions.append(
            {
                "name": "row_key",
                "description": "Synthetic row key.",
                "type": "string",
                "expression": "1",
                "encryption": False,
                "role": "all",
            }
        )

    preferred = view["name"]
    if preferred.endswith("_all"):
        preferred = preferred[: -len("_all")] or view["name"]
    cube_name = alloc_name(preferred, priority_taken, "cube")
    priority_taken.add(cube_name)
    for m in dimensions + facts:
        priority_taken.add(m["name"])

    cube = {
        "kind": "cube",
        "name": cube_name,
        "view": view["name"],
        "description": f"Auto cube over view {view['name']}.",
        "dimensions": dimensions,
        "facts": facts,
    }

    member_names = {m["name"] for m in dimensions} | {m["name"] for m in facts}
    metric: dict | None = None
    metric_name = alloc_name(
        f"{cube_name}_count", priority_taken | member_names, "metric"
    )
    group_cols = [d["column"] for d in dimensions[:2] if d.get("column")]
    if metric_name not in member_names and group_cols:
        amount_facts = [
            f for f in facts if f.get("type") == "amount" and f.get("column")
        ]
        if amount_facts:
            expr = f"sum({amount_facts[0]['column']})"
        else:
            expr = f"count({group_cols[0]})"
        metric = {
            "kind": "metric",
            "name": metric_name,
            "description": f"Auto metric {metric_name} over {cube_name}.",
            "cube": cube_name,
            "expression": expr,
            "aggregation": "group",
            "group_by": group_cols,
            "filter": None,
            "role": "all",
        }
        priority_taken.add(metric_name)
    return cube, metric


def extract_table(
    con: sqlite3.Connection, db_id: str, table: str, table_pks: dict[str, list[str]]
) -> dict:
    columns = []
    pk = []
    for _cid, name, typ, notnull, _dflt, pk_ord in con.execute(
        f"PRAGMA table_info({quote_ident(table)})"
    ):
        col: dict = {
            "name": name,
            "type": typ if typ else "NULL",
            "nullable": notnull == 0,
        }
        if mark_as_code(name, typ or ""):
            col["as_code"] = True
        columns.append(col)
        if pk_ord > 0:
            pk.append((pk_ord, name))
    pk_names = [name for _, name in sorted(pk)]

    by_id: dict[int, list] = {}
    for row in con.execute(f"PRAGMA foreign_key_list({quote_ident(table)})"):
        fid, seq, ref_table, from_col, to_col, *_ = row
        by_id.setdefault(fid, []).append((seq, ref_table, from_col, to_col))

    fk = []
    seen: set[tuple] = set()
    for fid in sorted(by_id):
        parts = sorted(by_id[fid], key=lambda x: x[0])
        ref_table = parts[0][1]
        from_cols = [p[2] for p in parts]
        to_cols = [p[3] for p in parts]
        if any(c is None for c in to_cols):
            parent_pk = table_pks.get(ref_table) or table_pks.get(
                next(
                    (k for k in table_pks if k.lower() == ref_table.lower()),
                    "",
                )
            )
            if parent_pk and len(parent_pk) == len(from_cols):
                to_cols = list(parent_pk)
            else:
                continue
        key = (tuple(from_cols), ref_table.lower(), tuple(to_cols))
        if key in seen:
            continue
        seen.add(key)
        fk.append(
            {
                "columns": from_cols,
                "references": {"table": ref_table, "columns": to_cols},
            }
        )

    return {
        "kind": "table",
        "name": table,
        "db": db_id,
        "schema": "main",
        "table": table,
        "columns": columns,
        "pk": pk_names,
        "fk": fk,
        "permission_filters": [],
        "scope_filter": None,
        "default_filter": None,
        "role": "all",
    }


def collect_join_edges(
    tables: dict[str, dict],
) -> list[tuple[str, str, list[tuple[str, str]]]]:
    """(child, parent, on[(child_col, parent_col), ...]) — self 제외, 중복 제거."""
    edges: list[tuple[str, str, list[tuple[str, str]]]] = []
    seen: set[tuple] = set()

    def add(child: str, parent: str, on: list[tuple[str, str]]) -> None:
        if child == parent or not on:
            return
        if resolve_table_name(tables, parent) is None:
            return
        parent = resolve_table_name(tables, parent)  # type: ignore[assignment]
        key = (child, parent, tuple(on))
        if key in seen:
            return
        seen.add(key)
        edges.append((child, parent, on))

    # 1) declared FK
    for child in tables.values():
        for fk in child["fk"]:
            parent = resolve_table_name(tables, fk["references"]["table"])
            if parent is None:
                continue
            to_cols = fk["references"]["columns"]
            if any(c is None for c in to_cols):
                continue
            add(child["name"], parent, list(zip(fk["columns"], to_cols)))

    # 2) 컬럼명 == 다른 테이블 단일 PK → 추론
    pk_index: dict[str, list[str]] = {}
    for name, doc in tables.items():
        if len(doc["pk"]) == 1:
            pk_index.setdefault(doc["pk"][0], []).append(name)

    for child in tables.values():
        child_cols = {c["name"] for c in child["columns"]}
        for col in child_cols:
            for parent in pk_index.get(col, []):
                if parent == child["name"]:
                    continue
                add(child["name"], parent, [(col, col)])

    # 3) 이미 알려진 (col → parent.to) 패턴을 같은 col 가진 다른 테이블에 전파
    #    (예: set_translations.setCode→sets.code 이면 cards.setCode도 동일 조인)
    patterns: list[tuple[str, str, str]] = []  # (from_col, parent, to_col)
    for child, parent, on in list(edges):
        if len(on) == 1:
            patterns.append((on[0][0], parent, on[0][1]))
    for from_col, parent, to_col in patterns:
        for child in tables.values():
            if child["name"] == parent:
                continue
            if any(c["name"] == from_col for c in child["columns"]):
                add(child["name"], parent, [(from_col, to_col)])

    return edges


def connected_components(
    table_names: list[str],
    edges: list[tuple[str, str, list[tuple[str, str]]]],
) -> list[list[str]]:
    adj: dict[str, set[str]] = {n: set() for n in table_names}
    for child, parent, _on in edges:
        adj.setdefault(child, set()).add(parent)
        adj.setdefault(parent, set()).add(child)

    seen: set[str] = set()
    comps: list[list[str]] = []
    for n in table_names:
        if n in seen:
            continue
        stack = [n]
        seen.add(n)
        comp: list[str] = []
        while stack:
            u = stack.pop()
            comp.append(u)
            for v in adj.get(u, ()):
                if v not in seen:
                    seen.add(v)
                    stack.append(v)
        comps.append(sorted(comp))
    return comps


def spanning_joins(
    comp: list[str],
    edges: list[tuple[str, str, list[tuple[str, str]]]],
) -> tuple[str, list[tuple[str, str, list[tuple[str, str]]]]]:
    """root + join steps (left, right, on[(left_col, right_col)])."""
    comp_set = set(comp)
    # undirected adjacency with payload
    adj: dict[str, list[tuple[str, str, str, list[tuple[str, str]]]]] = {
        n: [] for n in comp
    }
    # store as (neighbor, child, parent, on_child_parent)
    pair_used: set[tuple[str, str]] = set()
    for child, parent, on in edges:
        if child not in comp_set or parent not in comp_set:
            continue
        pair = tuple(sorted((child, parent)))
        if pair in pair_used:
            continue  # 동일 테이블 쌍은 스패닝에 한 간선만
        pair_used.add(pair)
        adj[child].append((parent, child, parent, on))
        adj[parent].append((child, child, parent, on))

    # hub = max degree
    root = max(comp, key=lambda n: (len(adj[n]), n))
    visited = {root}
    order: list[tuple[str, str, list[tuple[str, str]]]] = []
    queue = [root]
    while queue:
        u = queue.pop(0)
        for neighbor, child, parent, on in adj[u]:
            if neighbor in visited:
                continue
            visited.add(neighbor)
            queue.append(neighbor)
            if u == child and neighbor == parent:
                order.append((u, neighbor, list(on)))
            elif u == parent and neighbor == child:
                order.append((u, neighbor, [(p, c) for c, p in on]))
            else:
                # should not happen
                order.append((u, neighbor, list(on)))
    return root, order


def build_component_view(
    tables: dict[str, dict],
    root: str,
    join_steps: list[tuple[str, str, list[tuple[str, str]]]],
    view_name: str,
) -> dict:
    sources = [{"kind": "table", "name": root}]
    for _left, right, _on in join_steps:
        sources.append({"kind": "table", "name": right})

    joins = []
    for left, right, on in join_steps:
        joins.append(
            {
                "left": left,
                "right": right,
                "type": "inner",
                "on": [
                    {"left_column": lc, "right_column": rc} for lc, rc in on
                ],
            }
        )

    # (table, physical_col) → view column name
    loc: dict[tuple[str, str], str] = {}
    columns: list[dict] = []
    used: set[str] = set()

    def ensure_view_col(table: str, physical: str, preferred: str) -> str:
        key = (table, physical)
        if key in loc:
            return loc[key]
        name = preferred
        if name in used:
            name = f"{table}_{physical}"
        base = name
        n = 2
        while name in used:
            name = f"{base}_{n}"
            n += 1
        used.add(name)
        loc[key] = name
        columns.append(
            {
                "name": name,
                "mappings": [{"source": table, "column": physical}],
            }
        )
        return name

    def add_mapping(view_col: str, table: str, physical: str) -> None:
        loc[(table, physical)] = view_col
        for col in columns:
            if col["name"] == view_col:
                mapping = {"source": table, "column": physical}
                if mapping not in col["mappings"]:
                    col["mappings"].append(mapping)
                return

    # root columns
    for col in tables[root]["columns"]:
        ensure_view_col(root, col["name"], col["name"])

    absorbed_right: set[tuple[str, str]] = set()
    for left, right, on in join_steps:
        for lc, rc in on:
            vname = ensure_view_col(left, lc, lc)
            add_mapping(vname, right, rc)
            absorbed_right.add((right, rc))
        for col in tables[right]["columns"]:
            if (right, col["name"]) in absorbed_right:
                continue
            ensure_view_col(right, col["name"], col["name"])

    return {
        "kind": "view",
        "name": view_name,
        "combine": "join",
        "sources": sources,
        "joins": joins,
        "columns": columns,
    }


def views_from_tables(tables: dict[str, dict], db_id: str) -> list[dict]:
    edges = collect_join_edges(tables)
    comps = connected_components(list(tables), edges)
    table_names = set(tables)
    views: list[dict] = []
    used_names: set[str] = set()

    multi = [c for c in comps if len(c) >= 2]
    for comp in multi:
        root, steps = spanning_joins(comp, edges)
        if not steps:
            continue
        if len(multi) == 1 and len(comp) == len(tables):
            view_name = f"{db_id}_all"
        else:
            view_name = f"{db_id}_{root}"
        if view_name in table_names or view_name in used_names:
            view_name = f"{view_name}_view"
        base = view_name
        n = 2
        while view_name in table_names or view_name in used_names:
            view_name = f"{base}_{n}"
            n += 1
        used_names.add(view_name)
        views.append(build_component_view(tables, root, steps, view_name))
    return views


def load_pks(con: sqlite3.Connection, table_names: list[str]) -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    for table in table_names:
        pk = []
        for _cid, name, _typ, _nn, _d, pk_ord in con.execute(
            f"PRAGMA table_info({quote_ident(table)})"
        ):
            if pk_ord > 0:
                pk.append((pk_ord, name))
        out[table] = [name for _, name in sorted(pk)]
    return out


def write_json(path: Path, doc: dict) -> None:
    path.write_text(
        json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )


def main() -> None:
    table_n = view_n = cube_n = metric_n = 0
    for sqlite_path in sorted(ROOT.glob("*/*.sqlite")):
        db_id = sqlite_path.parent.name
        meta_dir = sqlite_path.parent / "meta"
        meta_dir.mkdir(exist_ok=True)
        for old in meta_dir.glob("*.json"):
            old.unlink()

        con = sqlite3.connect(sqlite_path)
        table_names = [
            r[0]
            for r in con.execute(
                "SELECT name FROM sqlite_master "
                "WHERE type='table' AND name NOT LIKE 'sqlite_%' "
                "ORDER BY name"
            )
        ]
        table_pks = load_pks(con, table_names)
        # Also index PKs by lowercase ref names used in FK pragmas.
        for t, pk in list(table_pks.items()):
            table_pks.setdefault(t.lower(), pk)

        tables: dict[str, dict] = {}
        for table in table_names:
            doc = extract_table(con, db_id, table, table_pks)
            for fk in doc["fk"]:
                resolved = resolve_table_name(
                    {n: {} for n in table_names}, fk["references"]["table"]
                )
                if resolved:
                    fk["references"]["table"] = resolved
            tables[table] = doc

        views = views_from_tables(tables, db_id)
        priority_taken: set[str] = set()
        cubes: list[dict] = []
        metrics: list[dict] = []
        for view in views:
            cube, metric = cube_and_metric_from_view(view, tables, priority_taken)
            cubes.append(cube)
            if metric:
                metrics.append(metric)

        apply_name_priority(tables, views, cubes, metrics)

        for doc in tables.values():
            write_json(meta_dir / f"{doc['name']}.json", doc)
            table_n += 1
        for doc in views:
            write_json(meta_dir / f"{doc['name']}.json", doc)
            view_n += 1
        for doc in cubes:
            write_json(meta_dir / f"{doc['name']}.json", doc)
            cube_n += 1
        for doc in metrics:
            write_json(meta_dir / f"{doc['name']}.json", doc)
            metric_n += 1

        con.close()
        print(
            f"{db_id}: {len(tables)} tables, {len(views)} views, "
            f"{len(cubes)} cubes -> {meta_dir.relative_to(ROOT.parent)}"
        )
    print(
        f"wrote {table_n} tables, {view_n} views, "
        f"{cube_n} cubes, {metric_n} metrics"
    )


if __name__ == "__main__":
    main()
