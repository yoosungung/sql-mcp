#!/usr/bin/env python3
"""eval.json EX FAIL → eval_sql.json (agent_sql + execute_sql dry_run 물리 SQL).

예:
  python write_eval_sql.py --url http://127.0.0.1:8090/mcp
  python write_eval_sql.py --url http://127.0.0.1:8090/mcp --remap-cubes
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
from pathlib import Path
from typing import Any

from bird_eval import (
    build_eval_sql_doc,
    default_eval_dir,
    parse_dry_run_result,
    write_eval_sql_json,
)

_TARGET_DB = Path(__file__).resolve().parents[1] / "target_db"
if str(_TARGET_DB) not in sys.path:
    sys.path.insert(0, str(_TARGET_DB))
from hub_grain import remap_agent_sql  # noqa: E402


async def wait_for_mcp(url: str, *, timeout_s: float = 60.0) -> None:
    from agent_bird import wait_for_mcp as _real

    await _real(url, timeout_s=timeout_s)


def _tool_named(tools: list[Any], name: str) -> Any:
    for t in tools:
        if getattr(t, "name", None) == name:
            return t
    raise SystemExit(f"mcp tool not found: {name}")


async def run(args: argparse.Namespace) -> int:
    src = Path(args.input) if args.input else default_eval_dir() / "eval.json"
    out = Path(args.output) if args.output else default_eval_dir() / "eval_sql.json"
    if not src.is_file():
        raise SystemExit(f"eval.json missing: {src}")
    eval_doc = json.loads(src.read_text(encoding="utf-8"))
    url = (args.url or os.environ.get("SQL_MCP_URL") or "http://127.0.0.1:8080/mcp").strip()
    await wait_for_mcp(url)

    from langchain.mcp import MCPAdapter

    preview = build_eval_sql_doc(eval_doc)
    compiled: dict[tuple[str, str], tuple[str | None, str | None]] = {}
    async with MCPAdapter(url) as adapter:
        execute_sql = _tool_named(await adapter.list_tools(), "execute_sql")
        todo = [
            (row.get("db_id"), row.get("agent_sql"), row.get("gold_sql"))
            for row in preview["items"]
            if isinstance(row.get("db_id"), str)
            and isinstance(row.get("agent_sql"), str)
            and row.get("agent_sql")
        ]
        for i, (db_id, agent_sql, gold_sql) in enumerate(todo, start=1):
            assert isinstance(db_id, str) and isinstance(agent_sql, str)
            sql = agent_sql
            if args.remap_cubes:
                sql = remap_agent_sql(
                    agent_sql,
                    db_id,
                    gold_sql if isinstance(gold_sql, str) else "",
                )
            try:
                result = await execute_sql.ainvoke(
                    {"database": db_id, "sql": sql, "dry_run": True}
                )
                compiled[(db_id, agent_sql)] = parse_dry_run_result(result)
            except Exception as e:  # noqa: BLE001
                compiled[(db_id, agent_sql)] = (None, str(e))
            if i % 20 == 0 or i == len(todo):
                print(f"dry_run {i}/{len(todo)}", file=sys.stderr)

    def lookup(database: str, sql: str) -> tuple[str | None, str | None]:
        return compiled.get((database, sql), (None, "dry_run missing"))

    doc = build_eval_sql_doc(eval_doc, dry_run=lookup)
    write_eval_sql_json(doc, out)
    ok = sum(1 for it in doc["items"] if it.get("agent_db_sql"))
    err = sum(1 for it in doc["items"] if it.get("agent_db_sql_error"))
    print(f"wrote {out} fail={doc['meta']['item_count']} compiled={ok} error={err}")
    return 0


def main() -> int:
    p = argparse.ArgumentParser(description="eval.json EX FAIL → eval_sql.json")
    p.add_argument("--input", help="eval.json path (default client/eval/eval.json)")
    p.add_argument("--output", help="eval_sql.json path (default client/eval/eval_sql.json)")
    p.add_argument("--url", help="sql-mcp Streamable HTTP URL")
    p.add_argument(
        "--remap-cubes",
        action="store_true",
        help="dry_run with FROM {db}/{db}_all remapped to the yaml hub cube matching gold tables",
    )
    args = p.parse_args()
    return asyncio.run(run(args))


if __name__ == "__main__":
    raise SystemExit(main())
