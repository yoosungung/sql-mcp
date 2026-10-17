#!/usr/bin/env python3
"""target_db/*/meta/*.json → sql-mcp metadata_create/update (HTTP).

필요: SQL_MCP_MODE=author 인 sql-mcp HTTP.
view update 시 cube가 view를 FK로 참조하므로, 의존 metric/cube를 잠시 지웠다가
view 교체 후 다시 넣는다.

예:
  SQL_MCP_URL=http://127.0.0.1:8090/mcp python3 target_db/register_meta.py
  python3 target_db/register_meta.py --db superhero
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

    # Pass 1: tables
    for doc in docs:
        if doc["kind"] != "table":
            continue
        try:
            op = upsert(url, doc, keys)
            created += op == "create"
            updated += op == "update"
            print(f"{op} table/{doc['name']}")
        except Exception as e:
            print(f"FAIL table/{doc['name']}: {e}", file=sys.stderr)
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
            op = upsert(url, doc, keys)
            created += op == "create"
            updated += op == "update"
            print(f"{op} {doc['kind']}/{doc['name']}")
        except Exception as e:
            print(f"FAIL {doc['kind']}/{doc['name']}: {e}", file=sys.stderr)
            return 1

    print(f"done: created={created} updated={updated} url={url}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
