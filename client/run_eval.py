#!/usr/bin/env python3
"""BIRD [question, gold SQL, evidence] + DW agent 결과를 eval.json으로 저장.

예:
  # gold SQLite 행만 (agent 없음, MCP 불필요)
  python run_eval.py --db superhero --limit 3 --skip-agent

  # agent까지 (기본 병렬 8)
  python run_eval.py --url http://127.0.0.1:8090/mcp --concurrency 8
"""

from __future__ import annotations

import argparse
import asyncio
import os
import sys
import time
from pathlib import Path
from typing import Any

from bird_eval import (
    AgentRun,
    EvalItem,
    build_eval_doc,
    default_eval_dir,
    item_from_question,
    run_gold_sql,
    tool_calls_from_messages,
    write_eval_json,
)
from bird_qa import SMALL_DBS, BirdQuestion, default_qa_path, load_questions


async def invoke_bird_agent(*args: Any, **kwargs: Any):
    """테스트에서 monkeypatch 가능하도록 래핑. 실제 구현은 agent_bird."""
    from agent_bird import invoke_bird_agent as _real

    return await _real(*args, **kwargs)


async def wait_for_mcp(url: str, *, timeout_s: float = 60.0) -> None:
    from agent_bird import wait_for_mcp as _real

    await _real(url, timeout_s=timeout_s)


def _resolve_model(args: argparse.Namespace) -> str:
    model = (
        args.model
        or os.environ.get("SQL_MCP_AGENT_MODEL")
        or "ollama:gemma4:e2b"
    ).strip()
    if not model and not args.skip_agent:
        raise SystemExit(
            "model is empty; pass --model or set SQL_MCP_AGENT_MODEL "
            "(e.g. ollama:gemma4:e2b)"
        )
    return model


async def eval_one(
    q: BirdQuestion,
    *,
    model: str,
    url: str,
    skip_agent: bool,
    row_limit: int,
    log_max: int,
    wait_timeout_s: float = 60.0,
) -> EvalItem:
    gold_rows, gold_error = await asyncio.to_thread(
        run_gold_sql, q.db_id, q.sql, row_limit=row_limit
    )
    agent: AgentRun | None = None
    if not skip_agent:
        t0 = time.perf_counter()
        try:
            final, messages = await invoke_bird_agent(
                q, model=model, url=url, wait_timeout_s=wait_timeout_s
            )
            agent = AgentRun(
                final=final,
                tool_calls=tool_calls_from_messages(messages, log_max=log_max),
                elapsed_ms=int((time.perf_counter() - t0) * 1000),
                model=model,
            )
        except Exception as e:  # noqa: BLE001
            agent = AgentRun(
                final="",
                error=str(e),
                elapsed_ms=int((time.perf_counter() - t0) * 1000),
                model=model,
            )
            print(f"  agent error qid={q.question_id}: {e}", file=sys.stderr)
    return item_from_question(
        q, gold_rows=gold_rows, gold_error=gold_error, agent=agent
    )


async def run_parallel(
    questions: list[BirdQuestion],
    *,
    concurrency: int,
    model: str,
    url: str,
    skip_agent: bool,
    row_limit: int,
    log_max: int,
    out: Path,
    qa_path: str,
) -> list[EvalItem]:
    n = len(questions)
    conc = max(1, concurrency)
    sem = asyncio.Semaphore(conc)
    write_lock = asyncio.Lock()
    slots: list[EvalItem | None] = [None] * n
    done_count = 0

    meta_kw = dict(
        model=None if skip_agent else model,
        mcp_url=None if skip_agent else url,
        qa_path=qa_path,
        extra_meta={"skip_agent": bool(skip_agent), "concurrency": conc},
    )

    async def flush() -> None:
        items = [it for it in slots if it is not None]
        write_eval_json(build_eval_doc(items, **meta_kw), out)

    async def worker(i: int, q: BirdQuestion) -> None:
        nonlocal done_count
        async with sem:
            print(
                f"[{i + 1}/{n}] start qid={q.question_id} db={q.db_id}",
                file=sys.stderr,
            )
            item = await eval_one(
                q,
                model=model,
                url=url,
                skip_agent=skip_agent,
                row_limit=row_limit,
                log_max=log_max,
            )
        async with write_lock:
            slots[i] = item
            done_count += 1
            await flush()
            print(
                f"[{i + 1}/{n}] done qid={q.question_id} "
                f"({done_count}/{n} written)",
                file=sys.stderr,
            )

    if not skip_agent:
        await wait_for_mcp(url)

    await asyncio.gather(*(worker(i, q) for i, q in enumerate(questions)))
    items = [it for it in slots if it is not None]
    assert len(items) == n
    return items


async def run(args: argparse.Namespace) -> int:
    qa_path = Path(args.qa) if args.qa else default_qa_path()
    db_ids = None if args.all_dbs else SMALL_DBS
    questions = load_questions(qa_path, db_ids=db_ids)
    if args.db:
        questions = [q for q in questions if q.db_id == args.db]
    if args.question_id is not None:
        questions = [q for q in questions if q.question_id == args.question_id]
    if args.limit is not None:
        questions = questions[: max(0, args.limit)]
    if not questions:
        raise SystemExit("no questions matched filters")

    model = _resolve_model(args)
    url = (args.url or os.environ.get("SQL_MCP_URL") or "http://127.0.0.1:8080/mcp").strip()
    out = Path(args.out) if args.out else default_eval_dir() / "eval.json"
    conc = max(1, int(args.concurrency))

    print(
        f"eval: {len(questions)} question(s) → {out}"
        + (
            " (skip-agent)"
            if args.skip_agent
            else f" model={model} mcp={url} concurrency={conc}"
        ),
        file=sys.stderr,
    )

    items = await run_parallel(
        questions,
        concurrency=conc,
        model=model,
        url=url,
        skip_agent=args.skip_agent,
        row_limit=args.row_limit,
        log_max=args.log_max,
        out=out,
        qa_path=str(qa_path),
    )
    print(f"done {out} ({len(items)} items)", file=sys.stderr)
    return 0


def main() -> None:
    p = argparse.ArgumentParser(description="BIRD + DW agent → eval.json")
    p.add_argument("--qa", help=f"path to mini_dev_sqlite.json (default: {default_qa_path()})")
    p.add_argument("--db", help="filter db_id")
    p.add_argument("--question-id", type=int, help="exact BIRD question_id")
    p.add_argument("--limit", type=int, help="max questions after filters")
    p.add_argument("--all-dbs", action="store_true", help="do not restrict to --small DBs")
    p.add_argument("--out", help=f"output path (default: {default_eval_dir() / 'eval.json'})")
    p.add_argument("--skip-agent", action="store_true", help="gold SQLite only (no MCP)")
    p.add_argument("--url", help="sql-mcp Streamable HTTP URL")
    p.add_argument("--model", help="LangChain model id")
    p.add_argument(
        "--concurrency",
        type=int,
        default=8,
        help="parallel agent workers (default: 8)",
    )
    p.add_argument("--row-limit", type=int, default=500, help="max gold rows per question")
    p.add_argument("--log-max", type=int, default=4000, help="max chars per tool result")
    args = p.parse_args()
    try:
        raise SystemExit(asyncio.run(run(args)))
    except FileNotFoundError as e:
        print(e, file=sys.stderr)
        raise SystemExit(2) from e


if __name__ == "__main__":
    main()
