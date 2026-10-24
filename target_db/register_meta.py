#!/usr/bin/env python3
"""target_db/*/meta/*.json → sql-mcp metadata_create/update (HTTP).

필요: SQL_MCP_MODE=author 인 sql-mcp HTTP.
view update 시 cube가 view를 FK로 참조하므로, 의존 metric/cube를 잠시 지웠다가
view 교체 후 다시 넣는다.

예:
  SQL_MCP_URL=http://127.0.0.1:8090/mcp python3 target_db/register_meta.py
  python3 target_db/register_meta.py --db superhero --prune
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parent
KIND_ORDER = {"table": 0, "view": 1, "cube": 2, "metric": 3, "segment": 4}
PRUNE_KINDS = ("segment", "metric", "cube", "view")


def _item_target(item: dict, catalog: list[dict]) -> str | None:
    target = item.get("target")
    if isinstance(target, str) and target:
        return target
    cube_name = item.get("cube")
    if not isinstance(cube_name, str) or not cube_name:
        return None
    for other in catalog:
        if other.get("kind") == "cube" and other.get("name") == cube_name:
            t = other.get("target")
            return t if isinstance(t, str) else None
    return None


def stale_docs_to_prune(catalog: list[dict], local: list[dict]) -> list[dict]:
    """Catalog docs on local targets that are not in the new meta set.

    Views still referenced by a kept cube are skipped until that cube is updated.
    Metrics/segments whose catalog cube is itself stale are detached even if the
    same name exists locally (it will be recreated on the new cube).
    """
    local_keys = {
        (d.get("kind"), d.get("name"))
        for d in local
        if d.get("kind") and d.get("name")
    }
    targets = {d.get("target") for d in local if isinstance(d.get("target"), str)}
    stale_cube_names = {
        c.get("name")
        for c in catalog
        if c.get("kind") == "cube"
        and c.get("name")
        and ("cube", c.get("name")) not in local_keys
        and _item_target(c, catalog) in targets
    }
    held_views = {
        c.get("view")
        for c in catalog
        if c.get("kind") == "cube"
        and ("cube", c.get("name")) in local_keys
        and isinstance(c.get("view"), str)
    }
    stale: list[dict] = []
    for item in catalog:
        kind = item.get("kind")
        name = item.get("name")
        if kind not in PRUNE_KINDS or not name:
            continue
        if kind in ("metric", "segment") and item.get("cube") in stale_cube_names:
            stale.append(item)
            continue
        if (kind, name) in local_keys:
            continue
        if _item_target(item, catalog) not in targets:
            continue
        if kind == "view" and name in held_views:
            continue
        stale.append(item)
    stale.sort(
        key=lambda d: (PRUNE_KINDS.index(d["kind"]), d.get("name") or "")
    )
    return stale


def mcp_call(url: str, name: str, arguments: dict) -> dict:
    payload = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": name, "arguments": arguments},
    }
    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode(),
        headers={
            "Content-Type": "application/json",
            "Accept": "application/json, text/event-stream",
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=180) as resp:
        raw = resp.read().decode()
    data = None
    for line in raw.splitlines():
        line = line.strip()
        if not line:
            continue
        if line.startswith("data:"):
            line = line[5:].strip()
        try:
            data = json.loads(line)
            break
        except json.JSONDecodeError:
            continue
    if data is None:
        data = json.loads(raw)
    if "error" in data:
        raise RuntimeError(data["error"])
    result = data.get("result") or {}
    if result.get("isError"):
        text = ""
        for c in result.get("content") or []:
            if isinstance(c, dict) and c.get("type") == "text":
                text = c.get("text") or text
        raise RuntimeError(text or str(result))
    text = ""
    for c in result.get("content") or []:
        if isinstance(c, dict) and c.get("type") == "text":
            text = c.get("text") or ""
    return json.loads(text) if text else {}


def load_docs(db_filter: str | None) -> list[dict]:
    docs: list[dict] = []
    for meta_dir in sorted(ROOT.glob("*/meta")):
        db_id = meta_dir.parent.name
        if db_filter and db_id != db_filter:
            continue
        for path in sorted(meta_dir.glob("*.json")):
            docs.append(json.loads(path.read_text(encoding="utf-8")))
    docs.sort(key=lambda d: (KIND_ORDER.get(d.get("kind", ""), 99), d.get("name", "")))
    return docs


def list_all(url: str) -> list[dict]:
    return list(mcp_call(url, "metadata_list", {}).get("items") or [])


def upsert(url: str, doc: dict, keys: set[tuple[str, str]]) -> str:
    kind, name = doc["kind"], doc["name"]
    if (kind, name) in keys:
        mcp_call(url, "metadata_update", {"kind": kind, "name": name, "body": doc})
        return "update"
    mcp_call(url, "metadata_create", {"body": doc})
    keys.add((kind, name))
    return "create"


def delete_doc(url: str, kind: str, name: str, keys: set[tuple[str, str]]) -> None:
    if (kind, name) not in keys:
        return
    mcp_call(url, "metadata_delete", {"kind": kind, "name": name})
    keys.discard((kind, name))
    print(f"delete {kind}/{name}")


def prepare_cube_replace(
    url: str, cube_name: str, all_items: list[dict], keys: set[tuple[str, str]]
) -> None:
    """Detach metrics/segments so metadata_update can replace the cube row."""
    metrics = [
        d
        for d in all_items
        if d.get("kind") == "metric" and d.get("cube") == cube_name
    ]
    segments = [
        d
        for d in all_items
        if d.get("kind") == "segment" and d.get("cube") == cube_name
    ]
    for d in segments + metrics:
        delete_doc(url, d["kind"], d["name"], keys)


def prepare_view_replace(
    url: str, view_name: str, all_items: list[dict], keys: set[tuple[str, str]]
) -> tuple[list[dict], list[dict], list[dict]]:
    """Remove cubes/metrics/segments that reference view so meta_views row can be replaced."""
    cubes = [
        d
        for d in all_items
        if d.get("kind") == "cube" and d.get("view") == view_name
    ]
    cube_names = {c["name"] for c in cubes}
    metrics = [
        d
        for d in all_items
        if d.get("kind") == "metric" and d.get("cube") in cube_names
    ]
    segments = [
        d
        for d in all_items
        if d.get("kind") == "segment" and d.get("cube") in cube_names
    ]
    for d in segments + metrics + cubes:
        delete_doc(url, d["kind"], d["name"], keys)
    return cubes, metrics, segments


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--url", default=None)
    p.add_argument("--db", help="only this db_id folder under target_db/")
    p.add_argument(
        "--prune",
        action="store_true",
        help="delete catalog segment/metric/cube/view on the same target that are not in local meta",
    )
    args = p.parse_args()
    url = args.url or os.environ.get("SQL_MCP_URL") or "http://127.0.0.1:8090/mcp"

    docs = load_docs(args.db)
    if not docs:
        print("no meta JSON found", file=sys.stderr)
        return 2

    try:
        items = list_all(url)
    except Exception as e:
        print(f"cannot reach {url}: {e}", file=sys.stderr)
        print("hint: start sql-mcp with SQL_MCP_MODE=author", file=sys.stderr)
        return 2

    keys = {(i["kind"], i["name"]) for i in items if i.get("kind") and i.get("name")}
    by_kind_name = {(d["kind"], d["name"]): d for d in docs}
    created = updated = 0
    deferred_after_view: list[dict] = []

    # Pass 1: tables (create with fk=[] first if any referenced table is not yet in keys, then update with full fk)
    deferred_table_fks: list[dict] = []
    for doc in docs:
        if doc["kind"] != "table":
            continue
        try:
            ref_tables = [
                fk.get("references", {}).get("table")
                for fk in (doc.get("fk") or [])
            ]
            if any(rt and ("table", rt) not in keys for rt in ref_tables):
                stub = dict(doc, fk=[])
                op = upsert(url, stub, keys)
                deferred_table_fks.append(doc)
            else:
                op = upsert(url, doc, keys)
            created += op == "create"
            updated += op == "update"
            print(f"{op} table/{doc['name']}")
        except Exception as e:
            print(f"FAIL table/{doc['name']}: {e}", file=sys.stderr)
            return 1
    for doc in deferred_table_fks:
        try:
            upsert(url, doc, keys)
        except Exception as e:
            print(f"FAIL table/{doc['name']} (fk update): {e}", file=sys.stderr)
            return 1

    def run_prune() -> int:
        if not args.prune:
            return 0
        items_now = list_all(url)
        n = 0
        for stale in stale_docs_to_prune(items_now, docs):
            try:
                delete_doc(url, stale["kind"], stale["name"], keys)
                n += 1
            except Exception as e:
                print(
                    f"FAIL prune {stale['kind']}/{stale['name']}: {e}",
                    file=sys.stderr,
                )
                return -1
        return n

    if run_prune() < 0:
        return 1

    # Pass 2: views (detach dependent cubes first)
    items = list_all(url)
    keys = {(i["kind"], i["name"]) for i in items if i.get("kind") and i.get("name")}
    for doc in docs:
        if doc["kind"] != "view":
            continue
        name = doc["name"]
        try:
            if (doc["kind"], name) in keys:
                cubes, metrics, segments = prepare_view_replace(url, name, items, keys)
                # Prefer local meta JSON for recreate when present
                recreate: list[dict] = []
                for c in cubes:
                    recreate.append(by_kind_name.get(("cube", c["name"]), c))
                for m in metrics:
                    recreate.append(by_kind_name.get(("metric", m["name"]), m))
                for s in segments:
                    recreate.append(by_kind_name.get(("segment", s["name"]), s))
                deferred_after_view.extend(recreate)
                mcp_call(
                    url,
                    "metadata_update",
                    {"kind": "view", "name": name, "body": doc},
                )
                updated += 1
                print(f"update view/{name}")
            else:
                mcp_call(url, "metadata_create", {"body": doc})
                keys.add(("view", name))
                created += 1
                print(f"create view/{name}")
        except Exception as e:
            print(f"FAIL view/{name}: {e}", file=sys.stderr)
            return 1

    # Pass 3: cubes/metrics/segments from files + any deferred recreates
    rest = [d for d in docs if d["kind"] in ("cube", "metric", "segment")]
    # deferred may duplicate file docs — prefer file order, unique by kind/name
    seen: set[tuple[str, str]] = set()
    ordered: list[dict] = []
    for d in rest + deferred_after_view:
        key = (d["kind"], d["name"])
        if key in seen:
            continue
        seen.add(key)
        ordered.append(d)
    ordered.sort(key=lambda d: (KIND_ORDER.get(d["kind"], 99), d["name"]))

    items = list_all(url)
    keys = {(i["kind"], i["name"]) for i in items if i.get("kind") and i.get("name")}
    for doc in ordered:
        try:
            if doc["kind"] == "cube" and ("cube", doc["name"]) in keys:
                prepare_cube_replace(url, doc["name"], items, keys)
            op = upsert(url, doc, keys)
            created += op == "create"
            updated += op == "update"
            print(f"{op} {doc['kind']}/{doc['name']}")
        except Exception as e:
            print(f"FAIL {doc['kind']}/{doc['name']}: {e}", file=sys.stderr)
            return 1

    items = list_all(url)
    keys = {(i["kind"], i["name"]) for i in items if i.get("kind") and i.get("name")}
    if run_prune() < 0:
        return 1

    print(f"done: created={created} updated={updated} url={url}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
