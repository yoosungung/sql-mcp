#!/usr/bin/env python3
"""target_db/{db}/*.sqlite → meta/{name}.json (table|view|cube|metric 골격).

물리 카탈로그는 sqlite에서 직접 읽고, MCP는 쓰지 않는다.
`semantic_plan.yaml`이 있으면 허브(팩트)마다 view+cube 하나(`{db}_all` 없음).
cube/metric은 view당 휴리스틱 초안(코딩 에이전트가 다듬을 수 있음).
"""

from __future__ import annotations

import json
import re
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parent

NAME_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
# physical_ident: non-empty, no quote/backtick/control/semicolon (document.schema.json)
PHYSICAL_IDENT_RE = re.compile(r'^[^"`\x00-\x1F;]+$')
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


def is_physical_ident(name: str) -> bool:
    return bool(name) and len(name) <= 512 and bool(PHYSICAL_IDENT_RE.match(name))


def sanitize_ident(name: str) -> str:
    """Map a physical column name to a semantic `name` pattern for view/cube members."""
    if is_ident(name):
        return name
    s = re.sub(r"[^A-Za-z0-9_]+", "_", name)
    s = re.sub(r"_+", "_", s).strip("_")
    if not s:
        s = "col"
    if s[0].isdigit():
        s = f"c_{s}"
    if not is_ident(s):
        s = "col"
    return s


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
    """Return ('dimension'|'fact', member dict) or None if skipped.

    description은 메타 사용용이라 자동 생성하지 않는다(빈 문자열).
    """
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
                "description": "",
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
                "description": "",
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
            "description": "",
            "type": dim_type,
            "column": view_col,
            "encryption": False,
            "role": "all",
        },
    )


def alloc_name(base: str, taken: set[str], *suffixes: str) -> str:
    """Pick base if free+ident, else base_suffix, else base_suffixN.

    Conflicts are case-insensitive (`player` blocks `Player`) for macOS/Windows FS.
    """
    taken_lower = {t.lower() for t in taken}

    def free(cand: str) -> bool:
        return is_ident(cand) and cand.lower() not in taken_lower

    if free(base):
        return base
    for suf in suffixes:
        cand = f"{base}_{suf}" if suf else base
        if free(cand):
            return cand
        n = 2
        while True:
            cand_n = f"{cand}_{n}"
            if free(cand_n):
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
            if src.get("table") == old:
                src["table"] = new
            if src.get("alias") == old:
                src["alias"] = new
            if src.get("name") == old:  # legacy
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
    """cube/dim/fact/metric/segment keep natural names; table/view/view-col yield.

    Name conflicts are case-insensitive so `player` cube does not overwrite `Player.json`
    on case-insensitive filesystems.
    """
    reserved = priority_names(cubes, metrics)
    taken = set(reserved)
    taken_lower = {n.lower() for n in taken}

    def _take(name: str) -> None:
        taken.add(name)
        taken_lower.add(name.lower())

    # tables yield
    for old in list(tables):
        if old.lower() not in taken_lower:
            _take(old)
            continue
        new = alloc_name(old, taken, "table")
        # alloc_name only checks `taken` exact; ensure case-insensitive uniqueness
        while new.lower() in taken_lower:
            new = alloc_name(new, taken, "table")
        rename_table(tables, views, old, new)
        _take(new)

    # views yield
    for v in list(views):
        old = v["name"]
        if old.lower() not in taken_lower:
            _take(old)
            continue
        new = alloc_name(old, taken, "view")
        while new.lower() in taken_lower:
            new = alloc_name(new, taken, "view")
        rename_view(views, cubes, old, new)
        _take(new)

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


def _view_alias_to_catalog(view: dict) -> dict[str, str]:
    """mapping.source / join alias → catalog table name."""
    out: dict[str, str] = {}
    for s in view.get("sources") or []:
        catalog = s.get("table") or s.get("name")
        if not catalog:
            continue
        alias = s.get("alias") or catalog
        out[alias] = catalog
    return out


def _is_row_identifier(name: str) -> bool:
    n = name.lower()
    return n in {"id", "uuid", "guid", "code"} or n.endswith("id") or n.endswith("_uuid")


def _date_rank(name: str) -> tuple[int, str]:
    n = name.lower()
    if n in {"date", "order_date"} or "year" in n:
        return (0, n)
    if any(x in n for x in ("update", "close", "access", "birth", "creat", "last")):
        return (3, n)
    if n.endswith("date"):
        return (1, n)
    return (2, n)


def default_group_by(dimensions: list[dict]) -> list[str]:
    """D24: default grain only, not an allowlist of every dimension.

    One event date (if any) plus non-identifier ``code`` dimensions.
    When there is no code dimension, add one short categorical label.
    """
    dated: list[dict] = []
    codes: list[str] = []
    labels: list[str] = []
    skip_label = (
        "description",
        "text",
        "name",
        "url",
        "email",
        "phone",
        "body",
        "note",
        "comment",
        "street",
        "address",
        "about",
        "title",
    )
    for dim in dimensions:
        col = dim.get("column")
        if not isinstance(col, str) or not col:
            continue
        label = str(dim.get("name") or col)
        if _is_row_identifier(label) or _is_row_identifier(col):
            continue
        typ = dim.get("type")
        if typ in {"date", "datetime", "time"}:
            dated.append(dim)
        elif typ == "code":
            codes.append(col)
        elif not any(tok in label.lower() for tok in skip_label):
            labels.append(col)

    out: list[str] = []
    if dated:
        best = min(dated, key=lambda d: _date_rank(str(d.get("name") or d["column"])))
        out.append(best["column"])
    out.extend(codes)
    if not codes and labels:
        out.append(labels[0])
    seen: set[str] = set()
    uniq: list[str] = []
    for col in out:
        if col not in seen:
            seen.add(col)
            uniq.append(col)
    if uniq:
        return uniq[:4]
    for dim in dimensions:
        col = dim.get("column")
        if isinstance(col, str) and col:
            return [col]
    return []


def rewrite_divide(expr: str) -> str:
    """Prefer ``(a) / (b)`` over the ``DIVIDE(a, b)`` alias."""
    key = "DIVIDE("
    out: list[str] = []
    i = 0
    while True:
        j = expr.find(key, i)
        if j < 0:
            out.append(expr[i:])
            break
        out.append(expr[i:j])
        args, end = _split_call_args(expr, j + len(key))
        if len(args) == 2:
            left = rewrite_divide(args[0])
            right = rewrite_divide(args[1])
            out.append(f"({left}) / ({right})")
        else:
            out.append(expr[j:end])
        i = end
    return "".join(out)


def _split_call_args(expr: str, i: int) -> tuple[list[str], int]:
    args: list[str] = []
    buf: list[str] = []
    depth = 0
    while i < len(expr):
        ch = expr[i]
        if ch == "(":
            depth += 1
            buf.append(ch)
        elif ch == ")":
            if depth == 0:
                args.append("".join(buf).strip())
                return args, i + 1
            depth -= 1
            buf.append(ch)
        elif ch == "," and depth == 0:
            args.append("".join(buf).strip())
            buf = []
        else:
            buf.append(ch)
        i += 1
    args.append("".join(buf).strip())
    return args, i


def cube_and_metric_from_view(
    view: dict, tables: dict[str, dict], priority_taken: set[str], target: str
) -> tuple[dict, dict | None]:
    """Choose natural cube/metric/member names; do not yield to table/view names."""
    dimensions: list[dict] = []
    facts: list[dict] = []
    alias_to_catalog = _view_alias_to_catalog(view)
    for col in view["columns"]:
        vname = col["name"]
        mappings = col["mappings"]
        mapping = mappings[0]
        catalog = alias_to_catalog.get(mapping["source"], mapping["source"])
        ptype = physical_type(tables, catalog, mapping["column"])
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
                "description": "",
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
                "description": "",
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
        "target": target,
        "view": view["name"],
        "description": "",
        "dimensions": dimensions,
        "facts": facts,
    }

    member_names = {m["name"] for m in dimensions} | {m["name"] for m in facts}
    metric: dict | None = None
    metric_name = alloc_name(
        f"{cube_name}_count", priority_taken | member_names, "metric"
    )
    group_cols = default_group_by(dimensions)
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
            "description": "",
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
    col_order: dict[str, int] = {}
    for cid, name, typ, notnull, _dflt, pk_ord in con.execute(
        f"PRAGMA table_info({quote_ident(table)})"
    ):
        col_order[name] = cid
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
    # SQLite PRAGMA foreign_key_list numbers fid in reverse declaration order;
    # sort by column declaration order in table_info first, then -fid.
    sorted_fids = sorted(
        by_id,
        key=lambda fid: (
            col_order.get(sorted(by_id[fid], key=lambda x: x[0])[0][2], 9999),
            -fid,
        ),
    )
    for fid in sorted_fids:
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
        "target": db_id,
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


def _edge_sort_key(
    tables: dict[str, dict],
    child: str,
    parent: str,
    on: list[tuple[str, str]],
    idx: int,
) -> tuple[int, int, int, int]:
    parent_pks = {c.lower() for c in (tables.get(parent, {}).get("pk") or [])}
    targets_pk = 0 if (parent_pks and all(rc.lower() in parent_pks for _, rc in on)) else 1
    exact_match = 0 if all(lc.lower() == rc.lower() for lc, rc in on) else 1
    name_len = sum(len(lc) + len(rc) for lc, rc in on) if exact_match == 0 else 0
    return (targets_pk, exact_match, name_len, idx)


def _table_id_base(col: str) -> str | None:
    """`customer_id` / `PostId` → `customer` / `Post`. 그 외는 None."""
    if len(col) > 3 and col.lower().endswith("_id"):
        return col[:-3]
    if len(col) > 2 and col.endswith("Id"):
        return col[:-2]
    return None


def _resolve_table_id_parent(
    tables: dict[str, dict], base: str, child_name: str
) -> tuple[str, str] | None:
    """`{base}` 또는 단순 복수 `{base}s` 테이블에서 `id`/`Id` 컬럼을 찾는다."""
    by_lower = {n.lower(): n for n in tables}
    candidates = [base]
    if not base.lower().endswith("s"):
        candidates.append(base + "s")
    for cand in candidates:
        parent = by_lower.get(cand.lower())
        if not parent or parent == child_name:
            continue
        parent_cols = {c["name"].lower(): c["name"] for c in tables[parent]["columns"]}
        if "id" in parent_cols:
            return parent, parent_cols["id"]
    return None


def infer_table_id_fks(tables: dict[str, dict]) -> None:
    """선언 FK가 없어도 `{table}_id`/`{Table}Id` → `{table}.id` FK를 table.fk에 추가."""
    for child in tables.values():
        child_pk = set(child.get("pk") or [])
        existing_cols = {
            tuple(fk.get("columns") or []) for fk in (child.get("fk") or [])
        }
        for col in [c["name"] for c in child["columns"]]:
            if col in child_pk or (col,) in existing_cols:
                continue
            base = _table_id_base(col)
            if not base:
                continue
            resolved = _resolve_table_id_parent(tables, base, child["name"])
            if not resolved:
                continue
            parent, to_col = resolved
            if any(
                fk.get("columns") == [col]
                and (fk.get("references") or {}).get("table", "").lower()
                == parent.lower()
                for fk in child["fk"]
            ):
                continue
            child.setdefault("fk", []).append(
                {
                    "columns": [col],
                    "references": {"table": parent, "columns": [to_col]},
                }
            )
            existing_cols.add((col,))


def collect_join_edges(
    tables: dict[str, dict],
    role_nodes: dict[str, dict] | None = None,
) -> list[tuple[str, str, list[tuple[str, str]]]]:
    """(child, parent, on[(child_col, parent_col), ...]) — self 제외, 중복 제거.

    `role_nodes` 키는 view alias(가상 조인 정점). FK에 `_view_alias`가 있으면 그쪽을 parent로 쓴다.
    동일 부모에 대한 FK가 4개 초과(라인업 슬롯 등)면 조인 간선에서 제외한다.
    """
    role_nodes = role_nodes or {}
    edges: list[tuple[str, str, list[tuple[str, str]]]] = []
    seen: set[tuple] = set()

    def resolve_vertex(name: str) -> str | None:
        if name in role_nodes:
            return name
        return resolve_table_name(tables, name)

    def phys_of(vertex: str) -> str:
        if vertex in role_nodes:
            return role_nodes[vertex].get("table") or role_nodes[vertex].get("catalog") or vertex
        return (tables.get(vertex) or {}).get("table") or vertex

    def add(child: str, parent: str, on: list[tuple[str, str]]) -> None:
        if child == parent or not on:
            return
        parent_r = resolve_vertex(parent)
        if parent_r is None:
            return
        parent = parent_r
        # Same physical table under different aliases must not join each other.
        if phys_of(child) == phys_of(parent) and child != parent:
            return
        key = (child, parent, tuple(on))
        if key in seen:
            return
        seen.add(key)
        edges.append((child, parent, on))

    # Skip high-cardinality role FKs (e.g. Match home/away_player_1..11 → Player)
    skip_fk_ids: set[int] = set()
    for child in tables.values():
        by_target: dict[tuple[str, str], list[dict]] = {}
        for fk in child.get("fk", []):
            cols = fk.get("columns") or []
            ref_t = resolve_table_name(tables, fk["references"]["table"])
            ref_cols = fk["references"].get("columns") or []
            if len(cols) == 1 and ref_t and len(ref_cols) == 1:
                by_target.setdefault((ref_t, ref_cols[0]), []).append(fk)
        for fks in by_target.values():
            if len(fks) > 3:
                for fk in fks:
                    skip_fk_ids.add(id(fk))

    # 1) declared FK
    for child in tables.values():
        for fk in child["fk"]:
            if id(fk) in skip_fk_ids:
                continue
            parent = resolve_table_name(tables, fk["references"]["table"])
            if parent is None:
                continue
            to_cols = fk["references"]["columns"]
            if any(c is None for c in to_cols):
                continue
            vertex = fk.get("_view_alias") or parent
            add(child["name"], vertex, list(zip(fk["columns"], to_cols)))

    # 2) `{table}_id` / `{Table}Id` → `{table}.id` (부모에 id 컬럼이 있을 때)
    for child in tables.values():
        child_pk = set(child.get("pk") or [])
        for col in [c["name"] for c in child["columns"]]:
            if col in child_pk:
                continue
            base = _table_id_base(col)
            if not base:
                continue
            resolved = _resolve_table_id_parent(tables, base, child["name"])
            if not resolved:
                continue
            parent, to_col = resolved
            add(child["name"], parent, [(col, to_col)])

    # 3) 컬럼명 == 다른 테이블 단일 PK → 추론
    #    단, child 자신의 PK 컬럼으로는 추론하지 않는다.
    #    (예: superhero.id ↔ superpower.id 를 붙이면 M:N이 깨짐)
    pk_index: dict[str, list[str]] = {}
    for name, doc in tables.items():
        if len(doc["pk"]) == 1:
            pk_index.setdefault(doc["pk"][0], []).append(name)

    for child in tables.values():
        child_pk = set(child.get("pk") or [])
        child_cols = [c["name"] for c in child["columns"]]
        for col in child_cols:
            if col in child_pk:
                continue
            for parent in pk_index.get(col, []):
                if parent == child["name"]:
                    continue
                add(child["name"], parent, [(col, col)])

    # 4) 이미 알려진 (col → parent.to) 패턴을 같은 col 가진 다른 테이블에 전파
    #    (예: set_translations.setCode→sets.code 이면 cards.setCode도 동일 조인)
    #    PK→PK 패턴은 전파하지 않는다(위와 동일 이유).
    patterns: list[tuple[str, str, str]] = []  # (from_col, parent, to_col)
    parent_pk: dict[str, set[str]] = {
        name: set(doc.get("pk") or []) for name, doc in tables.items()
    }
    for child, parent, on in list(edges):
        if len(on) != 1:
            continue
        from_col, to_col = on[0]
        if from_col in set(tables[child].get("pk") or []) and to_col in parent_pk.get(
            parent, set()
        ):
            continue
        patterns.append((from_col, parent, to_col))
    for from_col, parent, to_col in patterns:
        for child in tables.values():
            if child["name"] == parent:
                continue
            if from_col in set(child.get("pk") or []):
                continue
            if any(c["name"] == from_col for c in child["columns"]):
                add(child["name"], parent, [(from_col, to_col)])

    # Sort edges so that for any (child, parent) pair, the best edge (PK target, exact name match, shorter key) comes first.
    indexed = list(enumerate(edges))
    indexed.sort(
        key=lambda item: _edge_sort_key(
            tables, item[1][0], item[1][1], item[1][2], item[0]
        )
    )
    return [e for _, e in indexed]


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
    root: str | None = None,
) -> tuple[str, list[tuple[str, str, list[tuple[str, str]]]]]:
    """root + join steps (left, right, on[(left_col, right_col)]) including cycle-closing FK edges.

    Default root is max degree. Pass ``root`` to pin a fact hub.
    """
    comp_set = set(comp)
    adj: dict[str, list[tuple[str, str, str, list[tuple[str, str]]]]] = {
        n: [] for n in comp
    }
    pair_edges: list[tuple[str, str, list[tuple[str, str]]]] = []
    pair_used: set[tuple[str, str]] = set()
    for child, parent, on in edges:
        if child not in comp_set or parent not in comp_set:
            continue
        pair = tuple(sorted((child, parent)))
        if pair in pair_used:
            continue  # 동일 테이블 쌍은 최우선 간선 하나만
        pair_used.add(pair)
        pair_edges.append((child, parent, on))
        adj[child].append((parent, child, parent, on))
        adj[parent].append((child, child, parent, on))

    # hub = max degree unless a fact hub is pinned
    if root not in adj:
        root = max(comp, key=lambda n: (len(adj[n]), n))
    visited = {root}
    discovery_order = [root]
    tree_steps: list[tuple[str, str, list[tuple[str, str]]]] = []
    tree_pairs: set[tuple[str, str]] = set()
    queue = [root]
    while queue:
        u = queue.pop(0)
        for neighbor, child, parent, on in adj[u]:
            if neighbor in visited:
                continue
            visited.add(neighbor)
            discovery_order.append(neighbor)
            queue.append(neighbor)
            tree_pairs.add(tuple(sorted((u, neighbor))))
            if u == child and neighbor == parent:
                tree_steps.append((u, neighbor, list(on)))
            elif u == parent and neighbor == child:
                tree_steps.append((u, neighbor, [(p, c) for c, p in on]))
            else:
                tree_steps.append((u, neighbor, list(on)))

    rank = {n: i for i, n in enumerate(discovery_order)}
    extra_steps: list[tuple[str, str, list[tuple[str, str]]]] = []
    for child, parent, on in pair_edges:
        pair = tuple(sorted((child, parent)))
        if pair in tree_pairs:
            continue
        if rank.get(child, 0) <= rank.get(parent, 0):
            extra_steps.append((child, parent, list(on)))
        else:
            extra_steps.append((parent, child, [(p, c) for c, p in on]))

    return root, tree_steps + extra_steps


def build_component_view(
    tables: dict[str, dict],
    root: str,
    join_steps: list[tuple[str, str, list[tuple[str, str]]]],
    view_name: str,
    role_nodes: dict[str, dict] | None = None,
) -> dict:
    role_nodes = role_nodes or {}
    source_order = [root]
    seen_sources = {root}
    for left, right, _on in join_steps:
        for s in (left, right):
            if s not in seen_sources:
                seen_sources.add(s)
                source_order.append(s)

    def source_entry(s: str) -> dict:
        if s in role_nodes:
            return {
                "kind": "table",
                "table": role_nodes[s]["catalog"],
                "alias": s,
            }
        return {"kind": "table", "table": s}

    sources = [source_entry(s) for s in source_order]

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

    # Build join key equivalence classes over (table, col) pairs linked by join_steps
    uf_parent: dict[tuple[str, str], tuple[str, str]] = {}

    def find(x: tuple[str, str]) -> tuple[str, str]:
        while uf_parent.get(x, x) != x:
            uf_parent[x] = uf_parent.get(uf_parent[x], uf_parent[x])
            x = uf_parent[x]
        return x

    def union(a: tuple[str, str], b: tuple[str, str]) -> None:
        ra, rb = find(a), find(b)
        if ra != rb:
            uf_parent[rb] = ra

    for left, right, on in join_steps:
        for lc, rc in on:
            uf_parent.setdefault((left, lc), (left, lc))
            uf_parent.setdefault((right, rc), (right, rc))
            union((left, lc), (right, rc))

    equiv_groups: dict[tuple[str, str], list[tuple[str, str]]] = {}
    for node in uf_parent:
        equiv_groups.setdefault(find(node), []).append(node)

    source_rank = {s: i for i, s in enumerate(source_order)}

    def _member_sort_key(tc: tuple[str, str]) -> tuple[int, int]:
        t, c = tc
        is_pk = 0 if c in (tables.get(t, {}).get("pk") or []) else 1
        return (is_pk, source_rank.get(t, 9999))

    for rep, members in equiv_groups.items():
        members.sort(key=_member_sort_key)

    # (table, physical_col) → view column name
    loc: dict[tuple[str, str], str] = {}
    columns: list[dict] = []
    used: set[str] = set()

    def ensure_view_col(
        table: str,
        physical: str,
        preferred: str,
        extra_mappings: list[tuple[str, str]] | None = None,
    ) -> str:
        key = (table, physical)
        if key in loc:
            return loc[key]
        if not is_physical_ident(physical):
            raise ValueError(f"invalid physical column {table}.{physical!r}")
        name = preferred if is_ident(preferred) else sanitize_ident(preferred)
        if name in used:
            name = sanitize_ident(f"{table}_{physical}")
        base = name
        n = 2
        while name in used:
            name = f"{base}_{n}"
            n += 1
            if not is_ident(name):
                name = sanitize_ident(name)
        used.add(name)
        loc[key] = name
        mappings = [{"source": table, "column": physical}]
        for et, ec in extra_mappings or []:
            m = {"source": et, "column": ec}
            if m not in mappings:
                mappings.append(m)
        columns.append(
            {
                "name": name,
                "mappings": mappings,
            }
        )
        return name

    # For each join equivalence class, group members by case-insensitive physical column name.
    # Generic non-root `id` columns merge into the specific FK column name when one exists in the class.
    for _rep, members in equiv_groups.items():
        has_non_id = any(c.lower() != "id" or t == root for t, c in members)
        by_cname: dict[str, list[tuple[str, str]]] = {}
        for t, c in members:
            if c.lower() == "id" and t != root and has_non_id:
                continue
            by_cname.setdefault(c.lower(), []).append((t, c))

        for _cname_lower, same_name_members in by_cname.items():
            same_name_members.sort(key=_member_sort_key)
            primary_t, primary_c = same_name_members[0]
            other_members = same_name_members[1:] + [
                tc for tc in members if tc not in same_name_members
            ]
            pref = primary_c if is_ident(primary_c) else sanitize_ident(primary_c)
            vname = ensure_view_col(primary_t, primary_c, pref, other_members)
            for t, c in same_name_members[1:]:
                loc[(t, c)] = vname

        for t, c in members:
            if (t, c) not in loc:
                # Generic non-root id absorbed into the first view col of this class
                first_tc = next(tc for tc in members if tc in loc)
                loc[(t, c)] = loc[first_tc]

    # Now register all remaining columns in source_order
    for src in source_order:
        role_prefix = tables[src].get("_role_prefix")
        non_loc_cols = [
            col["name"]
            for col in tables[src]["columns"]
            if (src, col["name"]) not in loc
        ]
        for col in tables[src]["columns"]:
            phys = col["name"]
            if (src, phys) in loc:
                continue
            if role_prefix and len(non_loc_cols) == 1:
                pref = sanitize_ident(role_prefix)
            elif role_prefix:
                pref = sanitize_ident(f"{role_prefix}_{phys}")
            else:
                pref = phys if is_ident(phys) else sanitize_ident(phys)
            ensure_view_col(src, phys, pref)

    return {
        "kind": "view",
        "name": view_name,
        "target": tables[root]["target"],
        "combine": "join",
        "sources": sources,
        "joins": joins,
        "columns": columns,
    }


def _role_playing_prefix(from_col: str, parent_name: str) -> str:
    """home_team_api_id → home_team; hair_colour_id → hair_colour."""
    role_prefix = re.sub(r"(_api_id|_id|Id)$", "", from_col)
    if not role_prefix or role_prefix.lower() == parent_name.lower():
        return from_col
    return role_prefix


def expand_role_playing_lookups(tables: dict[str, dict]) -> dict[str, dict]:
    """When a child has 2..3 single-column FKs to the same lookup column
    (e.g. eye/hair/skin_colour_id → colour.id, home/away_team_api_id → Team.team_api_id),
    keep FK.references.table on the real parent and annotate secondary FKs with `_view_alias`.

    Returns virtual role nodes (alias → catalog/columns/_role_prefix) used only in the join graph;
    they are not written as table JSON.
    """
    role_nodes: dict[str, dict] = {}
    occupied = set(tables)
    for child in list(tables.values()):
        by_target: dict[tuple[str, str], list[dict]] = {}
        for fk in child.get("fk", []):
            cols = fk.get("columns") or []
            ref_t = resolve_table_name(tables, fk["references"]["table"])
            ref_cols = fk["references"].get("columns") or []
            if len(cols) == 1 and ref_t and len(ref_cols) == 1:
                by_target.setdefault((ref_t, ref_cols[0]), []).append(fk)

        for (parent_name, _to_col), fks in by_target.items():
            if not (2 <= len(fks) <= 3):
                continue
            parent_doc = tables.get(parent_name)
            if not parent_doc:
                continue
            if parent_doc.get("fk"):
                continue
            n_cols = len(parent_doc.get("columns") or [])
            # 2 roles: allow larger dim tables (Team). 3 roles: keep small-lookup only.
            if len(fks) >= 3 and n_cols > 4:
                continue
            # Keep fks[0] on original parent_name; alias the rest in the view.
            for fk in fks[1:]:
                from_col = fk["columns"][0]
                role_prefix = _role_playing_prefix(from_col, parent_name)
                alias = alloc_name(sanitize_ident(role_prefix), occupied, "role")
                occupied.add(alias)
                fk["_view_alias"] = alias
                role_nodes[alias] = {
                    "name": alias,
                    "catalog": parent_name,
                    "table": parent_doc.get("table") or parent_name,
                    "target": parent_doc["target"],
                    "columns": parent_doc["columns"],
                    "pk": parent_doc.get("pk") or [],
                    "fk": [],
                    "_role_prefix": role_prefix,
                }
    return role_nodes


def parse_semantic_plan(text: str) -> dict:
    """Minimal YAML: ``hubs`` list with ``table`` / ``cube`` / ``dimensions``."""
    hubs: list[dict] = []
    current: dict | None = None
    in_dims = False
    for raw in text.splitlines():
        if "#" in raw:
            raw = raw[: raw.index("#")]
        if not raw.strip():
            continue
        indent = len(raw) - len(raw.lstrip(" "))
        s = raw.strip()
        if indent == 0 and s.rstrip(":") == "hubs":
            continue
        if s.startswith("- ") and indent <= 2:
            if current:
                hubs.append(current)
            current = {"table": "", "cube": "", "dimensions": []}
            in_dims = False
            rest = s[2:].strip()
            if rest.rstrip(":") == "dimensions":
                in_dims = True
            elif ":" in rest:
                k, v = rest.split(":", 1)
                key = k.strip()
                if key == "dimensions":
                    inline = v.strip()
                    if inline.startswith("["):
                        current["dimensions"] = [
                            x.strip().strip("\"'")
                            for x in inline.strip("[]").split(",")
                            if x.strip()
                        ]
                    else:
                        in_dims = True
                else:
                    current[key] = v.strip().strip("\"'")
            continue
        if current is None:
            continue
        if s.rstrip(":").strip() == "dimensions" or s.startswith("dimensions:"):
            in_dims = True
            inline = s.split(":", 1)[1].strip() if ":" in s else ""
            if inline.startswith("["):
                current["dimensions"] = [
                    x.strip().strip("\"'")
                    for x in inline.strip("[]").split(",")
                    if x.strip()
                ]
                in_dims = False
            continue
        if in_dims and s.startswith("- "):
            current["dimensions"].append(s[2:].strip().strip("\"'"))
            continue
        if ":" in s and not s.startswith("- "):
            in_dims = False
            k, v = s.split(":", 1)
            current[k.strip()] = v.strip().strip("\"'")
    if current:
        hubs.append(current)
    return {"hubs": hubs}


def load_semantic_plan(db_dir: Path) -> dict | None:
    path = db_dir / "semantic_plan.yaml"
    if not path.is_file():
        return None
    plan = parse_semantic_plan(path.read_text(encoding="utf-8"))
    hubs = [
        h
        for h in plan.get("hubs") or []
        if h.get("table") and h.get("cube")
    ]
    return {"hubs": hubs} if hubs else None


def views_from_hubs(
    tables: dict[str, dict],
    db_id: str,
    role_nodes: dict[str, dict],
    hubs: list[dict],
) -> list[dict]:
    """One view+cube per planned fact hub. Sibling hubs are not auto-joined."""
    edges = collect_join_edges(tables, role_nodes)
    graph = {**tables, **role_nodes}
    graph_names = list(tables) + list(role_nodes)
    views: list[dict] = []
    used_names: set[str] = set()

    def catalog_of(vertex: str) -> str:
        if vertex in role_nodes:
            return role_nodes[vertex].get("catalog") or vertex
        return vertex

    for spec in hubs:
        hub = resolve_table_name(tables, spec["table"])
        if hub is None:
            continue
        allowed_tables = {hub}
        for dim in spec.get("dimensions") or []:
            resolved = resolve_table_name(tables, dim)
            if resolved:
                allowed_tables.add(resolved)
        allowed_vertices = {
            n
            for n in graph_names
            if n in allowed_tables or catalog_of(n) in allowed_tables
        }
        if hub not in allowed_vertices:
            allowed_vertices.add(hub)
        hub_edges = [
            e
            for e in edges
            if e[0] in allowed_vertices and e[1] in allowed_vertices
        ]
        comp = [n for n in graph_names if n in allowed_vertices]
        if hub not in comp:
            continue
        _root, steps = spanning_joins(comp, hub_edges, root=hub)
        view_name = spec["cube"]
        if view_name in used_names:
            view_name = f"{view_name}_{hub}"
        base = view_name
        n = 2
        while view_name in used_names:
            view_name = f"{base}_{n}"
            n += 1
        used_names.add(view_name)
        views.append(
            build_component_view(graph, hub, steps, view_name, role_nodes)
        )
    return views


def views_from_tables(
    tables: dict[str, dict],
    db_id: str,
    role_nodes: dict[str, dict] | None = None,
    plan: dict | None = None,
) -> list[dict]:
    role_nodes = role_nodes or {}
    hubs = (plan or {}).get("hubs") if plan else None
    if hubs:
        return views_from_hubs(tables, db_id, role_nodes, hubs)
    edges = collect_join_edges(tables, role_nodes)
    graph_names = list(tables) + list(role_nodes)
    comps = connected_components(graph_names, edges)
    table_names = set(tables)
    views: list[dict] = []
    used_names: set[str] = set()
    graph = {**tables, **role_nodes}

    multi = [c for c in comps if len(c) >= 2]
    for comp in multi:
        root, steps = spanning_joins(comp, edges)
        if not steps:
            continue
        real_comp = [n for n in comp if n not in role_nodes]
        real_lower = {n.lower() for n in real_comp}
        if len(multi) == 1 and len(real_comp) == len(tables):
            view_name = f"{db_id}_all"
        elif real_lower <= {"player", "player_attributes"} or root.lower() == "player":
            view_name = "player"
        elif "match" in real_lower or root.lower() == "match":
            # Primary match star stays `{db}_all` even when Player is a separate component.
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
        views.append(
            build_component_view(graph, root, steps, view_name, role_nodes)
        )
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

        infer_table_id_fks(tables)
        role_nodes = expand_role_playing_lookups(tables)
        plan = load_semantic_plan(sqlite_path.parent)
        views = views_from_tables(tables, db_id, role_nodes, plan=plan)
        priority_taken: set[str] = set()
        cubes: list[dict] = []
        metrics: list[dict] = []
        for view in views:
            cube, metric = cube_and_metric_from_view(
                view, tables, priority_taken, db_id
            )
            cubes.append(cube)
            if metric:
                metrics.append(metric)

        apply_name_priority(tables, views, cubes, metrics)

        for doc in tables.values():
            doc.pop("_role_prefix", None)
            for fk in doc.get("fk") or []:
                fk.pop("_view_alias", None)
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
