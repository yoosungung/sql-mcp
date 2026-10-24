"""Hub grain helpers: yaml allowed tables, agent SQL FROM remap."""

from __future__ import annotations

import json
import re

from generate_meta import ROOT, load_semantic_plan

_FROM_TABLE_RE = re.compile(
    r"\b((?:from|join)\s+(?:(?:\"[^\"]+\"|`[^`]+`|[A-Za-z_][\w$]*)\s*\.\s*)?)"
    r"(?:\"([^\"]+)\"|`([^`]+)`|([A-Za-z_][\w$]*))",
    re.I,
)

HUB_DBS = (
    "formula_1",
    "toxicology",
    "california_schools",
    "student_club",
    "thrombosis_prediction",
    "superhero",
)


def hubs_for_db(db_id: str) -> list[dict]:
    plan = load_semantic_plan(ROOT / db_id)
    return list((plan or {}).get("hubs") or [])


def allowed_tables(hub: dict) -> set[str]:
    names = {str(hub.get("table") or "").lower()}
    names.update(str(d).lower() for d in (hub.get("dimensions") or []))
    names.discard("")
    return names


def _logical_table_name(name: str) -> str:
    """Catalog names may take `_table` when cube/dim claimed the short name."""
    n = name.lower()
    if n.endswith("_table"):
        return n[: -len("_table")]
    return n


def view_source_tables(view: dict) -> set[str]:
    out: set[str] = set()
    for src in view.get("sources") or []:
        table = src.get("table")
        if isinstance(table, str) and table:
            out.add(_logical_table_name(table))
    return out


def cube_for_gold(db_id: str, gold_sql: str) -> str | None:
    """Pick the hub cube whose fact table appears in gold FROM/JOIN."""
    gold = {
        (m.group(2) or m.group(3) or m.group(4) or "").strip().strip('"').strip("`").lower()
        for m in _FROM_TABLE_RE.finditer(gold_sql or "")
    }
    gold.discard("")
    gold.discard("main")
    matches = [h for h in hubs_for_db(db_id) if str(h.get("table") or "").lower() in gold]
    if not matches:
        return None
    return matches[0].get("cube")


def old_cube_names(db_id: str) -> set[str]:
    return {db_id.lower(), f"{db_id}_all".lower()}


def remap_agent_sql(sql: str, db_id: str, gold_sql: str) -> str:
    """Replace FROM/JOIN `{db}` / `{db}_all` with the hub cube matching gold tables."""
    new_cube = cube_for_gold(db_id, gold_sql)
    if not new_cube or not sql:
        return sql
    old = old_cube_names(db_id)

    def repl(m: re.Match[str]) -> str:
        prefix, q1, q2, bare = m.group(1), m.group(2), m.group(3), m.group(4)
        name = q1 or q2 or bare or ""
        if name.lower() not in old:
            return m.group(0)
        if q1 is not None:
            return f'{prefix}"{new_cube}"'
        if q2 is not None:
            return f"{prefix}`{new_cube}`"
        return f"{prefix}{new_cube}"

    return _FROM_TABLE_RE.sub(repl, sql)


def compiled_tables(sql: str) -> set[str]:
    names: set[str] = set()
    for m in _FROM_TABLE_RE.finditer(sql or ""):
        name = (m.group(2) or m.group(3) or m.group(4) or "").strip()
        key = name.strip('"').strip("`").lower()
        if key and key not in {"select", "main"}:
            names.add(_logical_table_name(key))
    return names


def extra_compiled_tables(compiled_sql: str, hub: dict) -> set[str]:
    return compiled_tables(compiled_sql) - allowed_tables(hub)


def local_view_grain_failures() -> list[str]:
    """View JSON sources must stay inside yaml hub + dimensions."""
    failures: list[str] = []
    for db_id in HUB_DBS:
        hubs = {h["cube"]: h for h in hubs_for_db(db_id) if h.get("cube")}
        meta = ROOT / db_id / "meta"
        if not meta.is_dir():
            failures.append(f"{db_id}: missing meta/")
            continue
        for cube_name, hub in hubs.items():
            path = meta / f"{cube_name}_view.json"
            if not path.is_file():
                failures.append(f"{db_id}/{cube_name}: missing {path.name}")
                continue
            view = json.loads(path.read_text(encoding="utf-8"))
            extra = view_source_tables(view) - allowed_tables(hub)
            if extra:
                failures.append(
                    f"{db_id}/{cube_name}: extra sources {sorted(extra)}"
                )
    return failures


def mcp_grain_failures(url: str) -> list[str]:
    """dry_run SELECT 1 FROM each hub cube; compiled tables must ⊆ hub+dims."""
    from register_meta import mcp_call

    failures: list[str] = []
    for db_id in HUB_DBS:
        for hub in hubs_for_db(db_id):
            cube = hub.get("cube")
            if not cube:
                continue
            try:
                result = mcp_call(
                    url,
                    "execute_sql",
                    {"database": db_id, "sql": f"SELECT 1 FROM {cube}", "dry_run": True},
                )
            except Exception as e:  # noqa: BLE001
                failures.append(f"{db_id}/{cube}: dry_run error {e}")
                continue
            compiled = result.get("sql") if isinstance(result, dict) else None
            if not isinstance(compiled, str) or not compiled:
                failures.append(
                    f"{db_id}/{cube}: no compiled sql ({result})"
                )
                continue
            extra = extra_compiled_tables(compiled, hub)
            if extra:
                failures.append(
                    f"{db_id}/{cube}: extra compiled {sorted(extra)} sql={compiled}"
                )
    return failures


def main() -> int:
    import argparse
    import sys

    p = argparse.ArgumentParser(description="Hub grain gate (local JSON + optional MCP dry_run)")
    p.add_argument("--url", help="sql-mcp HTTP URL for execute_sql dry_run")
    args = p.parse_args()
    local = local_view_grain_failures()
    for line in local:
        print(line, file=sys.stderr)
    remote: list[str] = []
    if args.url:
        remote = mcp_grain_failures(args.url)
        for line in remote:
            print(line, file=sys.stderr)
    if local or remote:
        print(f"FAIL local={len(local)} mcp={len(remote)}")
        return 1
    print("ok" + (" mcp" if args.url else " local"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
