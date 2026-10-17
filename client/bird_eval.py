"""BIRD eval.json: question + evidence + gold SQL/rows + DW agent 결과."""

from __future__ import annotations

import json
import sqlite3
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from bird_qa import BirdQuestion, repo_root


@dataclass
class AgentRun:
    final: str = ""
    tool_calls: list[dict[str, Any]] = field(default_factory=list)
    error: str | None = None
    elapsed_ms: int | None = None
    model: str | None = None


@dataclass
class EvalItem:
    question_id: int
    db_id: str
    question: str
    evidence: str
    difficulty: str
    gold_sql: str
    gold_rows: list[list[Any]] | None = None
    gold_error: str | None = None
    agent: AgentRun | None = None


def default_eval_dir() -> Path:
    return Path(__file__).resolve().parent / "eval"


def sqlite_path_for(db_id: str) -> Path:
    return repo_root() / "target_db" / db_id / f"{db_id}.sqlite"


def run_gold_sql(db_id: str, sql: str, *, row_limit: int = 500) -> tuple[list[list[Any]] | None, str | None]:
    path = sqlite_path_for(db_id)
    if not path.is_file():
        return None, f"sqlite missing: {path}"
    try:
        con = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
        try:
            cur = con.execute(sql)
            rows = cur.fetchmany(row_limit + 1)
        finally:
            con.close()
    except Exception as e:  # noqa: BLE001
        return None, str(e)
    clipped = False
    if len(rows) > row_limit:
        rows = rows[:row_limit]
        clipped = True
    out = [list(r) for r in rows]
    if clipped:
        return out, f"truncated to {row_limit} rows"
    return out, None


def item_from_question(
    q: BirdQuestion,
    *,
    gold_rows: list[list[Any]] | None = None,
    gold_error: str | None = None,
    agent: AgentRun | None = None,
) -> EvalItem:
    return EvalItem(
        question_id=q.question_id,
        db_id=q.db_id,
        question=q.question,
        evidence=q.evidence,
        difficulty=q.difficulty,
        gold_sql=q.sql,
        gold_rows=gold_rows,
        gold_error=gold_error,
        agent=agent,
    )


def serialize_messages(messages: list[Any], *, log_max: int = 4000) -> list[dict[str, Any]]:
    """LangChain message 궤적 → JSON-friendly tool_calls + final용 요약."""
    steps: list[dict[str, Any]] = []
    for msg in messages:
        role = getattr(msg, "type", None) or type(msg).__name__
        entry: dict[str, Any] = {"role": role}
        name = getattr(msg, "name", None)
        if name:
            entry["name"] = name
        tool_calls = getattr(msg, "tool_calls", None) or []
        if tool_calls:
            tcs = []
            for tc in tool_calls:
                if isinstance(tc, dict):
                    tcs.append(
                        {
                            "name": tc.get("name"),
                            "args": tc.get("args"),
                            "id": tc.get("id"),
                        }
                    )
                else:
                    tcs.append(
                        {
                            "name": getattr(tc, "name", None),
                            "args": getattr(tc, "args", None),
                            "id": getattr(tc, "id", None),
                        }
                    )
            entry["tool_calls"] = tcs
        content = getattr(msg, "content", None)
        if content is not None:
            text = content if isinstance(content, str) else json.dumps(content, ensure_ascii=False, default=str)
            if log_max > 0 and len(text) > log_max:
                text = text[:log_max] + f"…(+{len(text) - log_max})"
            entry["content"] = text
        steps.append(entry)
    return steps


def tool_calls_from_messages(messages: list[Any], *, log_max: int = 4000) -> list[dict[str, Any]]:
    """AI tool_call + 이어지는 ToolMessage 결과를 한 리스트로."""
    out: list[dict[str, Any]] = []
    pending: dict[str, dict[str, Any]] = {}
    for msg in messages:
        for tc in getattr(msg, "tool_calls", None) or []:
            if isinstance(tc, dict):
                tid, name, args = tc.get("id"), tc.get("name"), tc.get("args")
            else:
                tid = getattr(tc, "id", None)
                name = getattr(tc, "name", None)
                args = getattr(tc, "args", None)
            rec = {"name": name, "args": args, "id": tid, "result": None}
            out.append(rec)
            if tid:
                pending[str(tid)] = rec
        if getattr(msg, "type", None) == "tool" or type(msg).__name__ == "ToolMessage":
            content = getattr(msg, "content", "")
            text = content if isinstance(content, str) else json.dumps(content, default=str)
            if log_max > 0 and len(text) > log_max:
                text = text[:log_max] + f"…(+{len(text) - log_max})"
            tid = getattr(msg, "tool_call_id", None)
            if tid and str(tid) in pending:
                pending[str(tid)]["result"] = text
            else:
                out.append(
                    {
                        "name": getattr(msg, "name", None),
                        "args": None,
                        "id": tid,
                        "result": text,
                    }
                )
    return out


def build_eval_doc(
    items: list[EvalItem],
    *,
    model: str | None = None,
    mcp_url: str | None = None,
    qa_path: str | None = None,
    extra_meta: dict[str, Any] | None = None,
) -> dict[str, Any]:
    meta: dict[str, Any] = {
        "created_at": datetime.now(timezone.utc).isoformat(),
        "model": model,
        "mcp_url": mcp_url,
        "qa_path": qa_path,
        "item_count": len(items),
    }
    if extra_meta:
        meta.update(extra_meta)
    return {
        "meta": meta,
        "items": [
            {
                **{k: v for k, v in asdict(it).items() if k != "agent"},
                "agent": asdict(it.agent) if it.agent else None,
            }
            for it in items
        ],
    }


def write_eval_json(doc: dict[str, Any], path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
