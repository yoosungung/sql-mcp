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
from langchain.agents.middleware import AgentMiddleware, hook_config
from langchain.mcp import MCPAdapter
from langchain_core.messages import AIMessage

from bird_qa import SMALL_DBS, default_qa_path, load_questions, pick_question

# 동일 execute_sql 검증/실행 실패 한도: 최초 1 + 재시도 2 (Q1457 18회 방지).
MAX_IDENTICAL_SQL_FAILURES = 3

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
1. Call list_databases, pick the database that matches the question, then list_tables, then describe_table / search_columns to inspect tables and columns.
2. Before any WHERE equality/IN filter on a code/status/type/name-like column, call sample_column on that column and copy a value that actually appears (exact spelling and case).
3. Run execute_sql with a SQL SELECT string, read the `rows` in the result, then return the final answer.

WHERE filter values (mandatory):
- Do not invent filter literals from the question wording alone when a column may store coded values.
- Verify literals with sample_column on the same column you filter — match casing and exact tokens from samples (e.g. 'Game' vs 'game'). Optional `like` is a substring from the question; code tables match the value side, not the key.
- Use the column name from describe_table/search_columns (e.g. `type`), not a guessed rename (e.g. `event_type`) unless that column exists.
- If execute_sql returns 0 rows, do NOT conclude the answer is empty until you have sampled every WHERE filter column and confirmed the literals exist; then relax one filter at a time or fix the column/literal and re-run.

Final answer (mandatory):
- DO NOT return SQL queries in your final response.
- You MUST execute the SQL using `execute_sql`, read the `rows` in the result, and answer the user's question using the retrieved data values.

Query language (important):
- Output must be SQL SELECT text only — never MDX, DAX, LookML, GraphQL, or JSON query objects.
- Table and column names from list_tables/describe_table are SQL identifiers. describe_table.examples (metric/segment) are SQL snippets to copy — not column names.
- Example shape: SELECT SUM(amount) AS revenue, region FROM table_name WHERE status_code = 'PAID' GROUP BY region

SQL surface rules:
- Every SELECT statement must explicitly specify the FROM clause with the table name (e.g. `FROM student_club`).
- FROM/JOIN use table names from list_tables. `table.physical` and `AS alias` are accepted (first identifier is the table). WITH/subquery over tables is allowed. Qualify joined columns as `table.column`.
- Never use `SELECT *` (these tables have no all-columns surface).
- `SELECT 1` / `SELECT true` (literal-only) is rewritten by the server to a key column; prefer projecting real columns when answering questions.
- Grouped: `SUM`/`COUNT`/`AVG`/`MIN`/`MAX` (and expressions), with `GROUP BY` / `HAVING` as needed.
- Ungrouped: SELECT columns without `GROUP BY`/`HAVING` (`SELECT DISTINCT` when joining tables without aggregation).
- Allowed: read-only SELECT including subquery, WITH/CTE, window functions, UNION/EXCEPT/INTERSECT; `+ - * /`, `date_trunc('year'|'month'|'day'|'hour', col)`, `YEAR(` / `MONTH(` / `DAY(`, `CURRENT_DATE`/`CURRENT_TIMESTAMP`/`NOW()`, `DATEDIFF(end, start)` (days), `EXTRACT(YEAR|MONTH|DAY|HOUR FROM col)`. For ratios prefer `x * 1.0 / y` (emit makes `/` 0-safe; `DIVIDE` is a compatible alias).
- Forbidden: INSERT/UPDATE/DELETE/DDL; Postgres-only extras (`::`, `ILIKE`, `FILTER (WHERE …)`).
- Unqualified column names in GROUP BY (do not write `table.col` after GROUP BY — `CUBE` is a SQL keyword).
- Escape single quotes in string literals as `''`.
- On validation/parse error, fix the SQL instead of retrying the same query. The runtime stops the query if the same failing SQL is submitted 3 times.
"""


def sql_fingerprint(sql: str) -> str:
    """공백을 접어 동일 SQL 여부를 비교한다."""
    return " ".join(str(sql).split()).strip()


def _result_dict(result: Any) -> dict[str, Any] | None:
    if result is None:
        return None
    if isinstance(result, dict):
        return result
    text: str | None = None
    if isinstance(result, list) and result and isinstance(result[0], dict) and "text" in result[0]:
        text = str(result[0]["text"])
    elif isinstance(result, str):
        text = result
    else:
        return None
    try:
        parsed = json.loads(text)
    except json.JSONDecodeError:
        return {"raw": text}
    if isinstance(parsed, dict):
        return parsed
    if isinstance(parsed, list) and parsed and isinstance(parsed[0], dict) and "text" in parsed[0]:
        return _result_dict(str(parsed[0]["text"]))
    return {"raw": text}


def is_execute_sql_failure(result: Any, *, status: str | None = None) -> bool:
    """execute_sql 검증/실행 실패. 0 rows는 성공 실행이며 재시도 한도에 넣지 않는다."""
    if status == "error":
        return True
    body = _result_dict(result)
    if not isinstance(body, dict):
        text = str(result).lower() if result is not None else ""
        return any(tok in text for tok in ("error", "validation", "parse"))
    if body.get("code"):
        return True
    if "rows" in body:
        return False
    if body.get("dry_run") is True:
        return False
    raw = body.get("raw")
    if isinstance(raw, str) and any(tok in raw.lower() for tok in ("error", "validation", "parse")):
        return True
    return "message" in body and "rows" not in body


def execute_sql_attempts_from_messages(
    messages: list[Any],
) -> list[tuple[str, Any, str | None]]:
    """완료된 execute_sql (sql, result, status) 목록."""
    pending: dict[str, str] = {}
    out: list[tuple[str, Any, str | None]] = []
    for msg in messages:
        for tc in getattr(msg, "tool_calls", None) or []:
            if isinstance(tc, dict):
                tid, name, args = tc.get("id"), tc.get("name"), tc.get("args") or {}
            else:
                tid = getattr(tc, "id", None)
                name = getattr(tc, "name", None)
                args = getattr(tc, "args", None) or {}
            if name != "execute_sql" or not tid:
                continue
            sql = args.get("sql") if isinstance(args, dict) else None
            if sql:
                pending[str(tid)] = str(sql)
        name = getattr(msg, "name", None)
        tid = getattr(msg, "tool_call_id", None)
        if name == "execute_sql" and tid and str(tid) in pending:
            sql = pending.pop(str(tid))
            status = getattr(msg, "status", None)
            out.append((sql, getattr(msg, "content", None), status if isinstance(status, str) else None))
    return out


def should_stop_for_identical_sql_failures(
    attempts: list[tuple[str, Any, str | None]],
    *,
    max_failures: int = MAX_IDENTICAL_SQL_FAILURES,
) -> str | None:
    """한도에 도달한 SQL fingerprint를 반환. 아니면 None."""
    counts: dict[str, int] = {}
    for sql, result, status in attempts:
        if not is_execute_sql_failure(result, status=status):
            continue
        fp = sql_fingerprint(sql)
        if not fp:
            continue
        counts[fp] = counts.get(fp, 0) + 1
        if counts[fp] >= max_failures:
            return fp
    return None


def identical_sql_failure_stop_message(*, max_failures: int) -> str:
    return (
        f"Stopped: execute_sql validation/execution failed {max_failures} times "
        "with the same SQL. Aborting instead of retrying."
    )


class IdenticalSqlFailureMiddleware(AgentMiddleware):
    """동일 SQL 검증 실패가 max_failures회면 에이전트 루프를 즉시 종료한다."""

    def __init__(self, *, max_failures: int = MAX_IDENTICAL_SQL_FAILURES) -> None:
        super().__init__()
        if max_failures < 1:
            raise ValueError("max_failures must be >= 1")
        self.max_failures = max_failures

    def _stop_update(self, state: dict[str, Any]) -> dict[str, Any] | None:
        attempts = execute_sql_attempts_from_messages(state.get("messages") or [])
        if should_stop_for_identical_sql_failures(attempts, max_failures=self.max_failures) is None:
            return None
        return {
            "jump_to": "end",
            "messages": [
                AIMessage(content=identical_sql_failure_stop_message(max_failures=self.max_failures))
            ],
        }

    @hook_config(can_jump_to=["end"])
    def before_model(self, state: dict[str, Any], runtime: Any) -> dict[str, Any] | None:  # noqa: ARG002
        return self._stop_update(state)

    @hook_config(can_jump_to=["end"])
    async def abefore_model(self, state: dict[str, Any], runtime: Any) -> dict[str, Any] | None:
        return self.before_model(state, runtime)


def bird_agent_middleware(
    *, max_failures: int = MAX_IDENTICAL_SQL_FAILURES
) -> list[IdenticalSqlFailureMiddleware]:
    return [IdenticalSqlFailureMiddleware(max_failures=max_failures)]


def create_bird_agent(model: str, tools: list[Any], *, max_failures: int = MAX_IDENTICAL_SQL_FAILURES):
    return create_agent(
        model,
        tools,
        system_prompt=SYSTEM,
        middleware=bird_agent_middleware(max_failures=max_failures),
    )


def user_prompt(q) -> str:
    return q.question


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
        agent = create_bird_agent(model, tools)
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
