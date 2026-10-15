#!/usr/bin/env python3
"""target_db/{db}/*.sqlite → meta/{table|view}.json (document.schema 골격)."""

from __future__ import annotations

import json
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def quote_ident(name: str) -> str:
    return '"' + name.replace('"', '""') + '"'


def resolve_table_name(tables: dict[str, dict], ref: str) -> str | None:
    if ref in tables:
        return ref
    lower = {k.lower(): k for k in tables}
    return lower.get(ref.lower())


def extract_table(
    con: sqlite3.Connection, db_id: str, table: str, table_pks: dict[str, list[str]]
) -> dict:
    columns = []
    pk = []
    for _cid, name, typ, notnull, _dflt, pk_ord in con.execute(
        f"PRAGMA table_info({quote_ident(table)})"
    ):
        columns.append(
            {
                "name": name,
                "type": typ if typ else "NULL",
                "nullable": notnull == 0,
            }
        )
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


def main() -> None:
    table_n = 0
    view_n = 0
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
            # Normalize FK ref table to canonical casing.
            for fk in doc["fk"]:
                resolved = resolve_table_name(
                    {n: {} for n in table_names}, fk["references"]["table"]
                )
                if resolved:
                    fk["references"]["table"] = resolved
            tables[table] = doc
            (meta_dir / f"{table}.json").write_text(
                json.dumps(doc, indent=2, ensure_ascii=False) + "\n",
                encoding="utf-8",
            )
            table_n += 1

        views = views_from_tables(tables, db_id)
        for doc in views:
            (meta_dir / f"{doc['name']}.json").write_text(
                json.dumps(doc, indent=2, ensure_ascii=False) + "\n",
                encoding="utf-8",
            )
            view_n += 1

        con.close()
        print(
            f"{db_id}: {len(tables)} tables, {len(views)} views "
            f"-> {meta_dir.relative_to(ROOT.parent)}"
        )
    print(f"wrote {table_n} tables, {view_n} views")


if __name__ == "__main__":
    main()
