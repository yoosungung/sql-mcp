#!/usr/bin/env python3
"""BIRD 질문 1건을 LangChain create_agent + sql-mcp(HTTP)로 질의하는 최소 예제.

사전:
  1. ./target_db/fetch_bird.sh --small   # SQLite + bird_qa JSON
  2. sql-mcp HTTP 기동 (client는 서버를 띄우지 않음)
  3. Ollama 기동 + 모델 (기본 `gemma4:e2b`). OpenAI/Anthropic도 `--model`로 가능.

예:
  python agent_bird.py --db superhero
  python agent_bird.py --model ollama:gemma4:e2b
  python agent_bird.py --model openai:gpt-4o-mini
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
from pathlib import Path
from typing import Any
from urllib.parse import urlparse, urlunparse

import httpx
from langchain.agents import create_agent
from langchain.mcp import MCPAdapter

from bird_qa import SMALL_DBS, default_qa_path, load_questions, pick_question

def _clip(text: str, limit: int) -> str:
    if limit <= 0 or len(text) <= limit:
        return text
    return f"{text[:limit]}\n… ({len(text) - limit} more chars)"


def _content_text(content: Any) -> str:
    if content is None:
        return ""
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts: list[str] = []
        for block in content:
            if isinstance(block, dict) and "text" in block:
                parts.append(str(block["text"]))
            else:
                parts.append(str(block))
        return "\n".join(parts)
    return str(content)


def log_agent_trace(
    messages: list[Any],
    *,
    log_max: int = 2000,
    file=sys.stderr,
) -> None:
    """create_agent 메시지 궤적(모델 → tool_call → ToolMessage …)을 stderr에 출력."""
    print("--- agent trace ---", file=file)
    for i, msg in enumerate(messages):
        role = getattr(msg, "type", None) or type(msg).__name__
        name = getattr(msg, "name", None)
        header = f"[{i}] {role}" + (f" name={name}" if name else "")
        print(header, file=file)

        tool_calls = getattr(msg, "tool_calls", None) or []
        for tc in tool_calls:
            if isinstance(tc, dict):
                tn, args, tid = tc.get("name"), tc.get("args"), tc.get("id")
            else:
                tn = getattr(tc, "name", None)
                args = getattr(tc, "args", None)
                tid = getattr(tc, "id", None)
            args_s = json.dumps(args, ensure_ascii=False, default=str)
            print(f"  → tool_call {tn} id={tid}", file=file)
            print(f"    args: {_clip(args_s, log_max)}", file=file)

        text = _content_text(getattr(msg, "content", None)).strip()
        if text:
            print(f"  content: {_clip(text, log_max)}", file=file)
    print("--- end trace ---", file=file)

SYSTEM = """You are a data analyst. Answer questions using only sql-mcp tools:
list_databases, list_tables, describe_table, search_columns, sample_column, execute_sql.

Workflow:
1. Call list_tables(database=db_id), then describe_table / search_columns to inspect cubes and members.
2. Run execute_sql with semantic SQL, then return the final answer.

Semantic SQL rules:
- FROM/JOIN must use cube names (never physical table names or T1/T2 aliases). Qualify joined members as `cube.member`.
- Never use `SELECT *`.
- Grouped queries: use named metrics, `SUM`/`COUNT`/`AVG`/`MIN`/`MAX`, `GROUP BY`, `HAVING`.
- Ungrouped queries: `SELECT` dimensions/facts without `GROUP BY` or `HAVING` (use `SELECT DISTINCT` when joining cubes without aggregation).
- Expressions: use `+ - * /` and `date_trunc('year'|'month'|'day', col)` — never `DIVIDE()`, `YEAR()`, `STRFTIME()`, `EXTRACT()`.
- Follow Evidence filter literals exactly (use `sample_column` if unsure; escape single quotes as `''`).
- On validation/parse error, fix the SQL instead of retrying the same query.
"""


def user_prompt(q) -> str:
    evidence = q.evidence.strip() or "(none)"
    return (
        f"Database (db_id / MCP database name): {q.db_id}\n"
        f"Difficulty: {q.difficulty}\n"
        f"Evidence (use these literals exactly in filters): {evidence}\n\n"
        f"Question: {q.question}\n\n"
        "Use sql-mcp tools only. Discover cubes/members via tools; "
        "write semantic SQL (D23). Match Evidence strings exactly."
    )


def _health_url(mcp_url: str) -> str:
    parsed = urlparse(mcp_url)
    return urlunparse((parsed.scheme, parsed.netloc, "/health", "", "", ""))


async def wait_for_mcp(mcp_url: str, *, timeout_s: float = 60.0) -> None:
    """Compound 디버그 시 sql-mcp가 listen 하기 전에 agent가 뜨는 레이스를 흡수."""
    health = _health_url(mcp_url)
    deadline = asyncio.get_running_loop().time() + timeout_s
    last: Exception | None = None
    async with httpx.AsyncClient(timeout=2.0) as client:
        while asyncio.get_running_loop().time() < deadline:
            try:
                r = await client.get(health)
                if r.status_code == 200:
                    return
                last = RuntimeError(f"{health} → HTTP {r.status_code}")
            except Exception as e:  # noqa: BLE001 — 재시도용
                last = e
            await asyncio.sleep(0.5)
    raise SystemExit(
        f"sql-mcp not ready at {health} within {timeout_s:.0f}s "
        f"(start Debug sql-mcp (BIRD) or compound). last={last!r}"
    )


async def invoke_bird_agent(
    q,
    *,
    model: str,
    url: str,
    wait_timeout_s: float = 60.0,
) -> tuple[str, list[Any]]:
    """한 질문에 대해 agent를 돌리고 (final_text, messages)를 반환."""
    await wait_for_mcp(url, timeout_s=wait_timeout_s)
    async with MCPAdapter(url) as adapter:
        tools = await adapter.list_tools()
        print(
            f"mcp tools ({len(tools)}): "
            + ", ".join(getattr(t, "name", str(t)) for t in tools),
            file=sys.stderr,
        )
        agent = create_agent(model, tools, system_prompt=SYSTEM)
        result = await agent.ainvoke(
            {"messages": [{"role": "user", "content": user_prompt(q)}]}
        )
    messages = result.get("messages") or []
    final = ""
    if messages:
        last = messages[-1]
        final = _content_text(getattr(last, "content", last)) or str(last)
    else:
        final = json.dumps(result, default=str)
    return final, messages


async def run(args: argparse.Namespace) -> int:
    questions = load_questions(
        Path(args.qa) if args.qa else None,
        db_ids=None if args.all_dbs else SMALL_DBS,
    )
    q = pick_question(
        questions,
        question_id=args.question_id,
        db_id=args.db,
        index=args.index,
    )

    model = (
        args.model
        or os.environ.get("SQL_MCP_AGENT_MODEL")
        or "ollama:gemma4:e2b"
    ).strip()
    if not model:
        raise SystemExit(
            "model is empty; pass --model or set SQL_MCP_AGENT_MODEL "
            "(e.g. ollama:gemma4:e2b)"
        )
    url = (args.url or os.environ.get("SQL_MCP_URL") or "http://127.0.0.1:8080/mcp").strip()

    print(f"qa: question_id={q.question_id} db_id={q.db_id} difficulty={q.difficulty}")
    print(f"model: {model}")
    print(f"mcp: {url}")
    if args.show_gold:
        print("--- gold SQL (reference; not executable via execute_sql) ---")
        print(q.sql)
        print("---")

    final, messages = await invoke_bird_agent(q, model=model, url=url)
    if not args.quiet_trace:
        log_agent_trace(messages, log_max=args.log_max)
    print("--- final ---")
    print(final)
    return 0


def main() -> None:
    p = argparse.ArgumentParser(description="BIRD question → LangChain agent → sql-mcp")
    p.add_argument("--qa", help=f"path to mini_dev_sqlite.json (default: {default_qa_path()})")
    p.add_argument("--db", help="filter / pick first question for this db_id")
    p.add_argument("--question-id", type=int, help="exact BIRD question_id")
    p.add_argument("--index", type=int, default=0, help="index within filtered list")
    p.add_argument("--all-dbs", action="store_true", help="do not restrict to --small DBs")
    p.add_argument("--url", help="sql-mcp Streamable HTTP URL (default SQL_MCP_URL or :8080/mcp)")
    p.add_argument("--model", help="LangChain model id (default ollama:gemma4:e2b)")
    p.add_argument("--show-gold", action="store_true", help="print BIRD gold SQL")
    p.add_argument(
        "--quiet-trace",
        action="store_true",
        help="hide agent message/tool trace (stderr)",
    )
    p.add_argument(
        "--log-max",
        type=int,
        default=2000,
        help="max chars per logged content/args (0=unlimited)",
    )
    args = p.parse_args()
    try:
        raise SystemExit(asyncio.run(run(args)))
    except FileNotFoundError as e:
        print(e, file=sys.stderr)
        raise SystemExit(2) from e


if __name__ == "__main__":
    main()
