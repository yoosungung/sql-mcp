"""BIRD eval.json: question + evidence + gold SQL/rows + DW agent 결과."""

from __future__ import annotations

import json
import re
import sqlite3
from collections import Counter
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Literal

from bird_qa import BirdQuestion, repo_root

FailureBucket = Literal["dw_agent", "sql_mcp_code", "semantic_layer"]


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


def unwrap_tool_result(result: Any) -> dict[str, Any] | None:
    """MCP ToolMessage / eval.json execute_sql result → JSON object."""
    if result is None:
        return None
    text: str | None = None
    if isinstance(result, list) and result and isinstance(result[0], dict) and "text" in result[0]:
        text = str(result[0]["text"])
    elif isinstance(result, str):
        try:
            outer = json.loads(result)
        except json.JSONDecodeError:
            return {"raw": result}
        if isinstance(outer, list) and outer and isinstance(outer[0], dict) and "text" in outer[0]:
            text = str(outer[0]["text"])
        elif isinstance(outer, dict):
            return outer
        else:
            return {"raw": result}
    elif isinstance(result, dict):
        return result
    else:
        return {"raw": result}
    if text is None:
        return None
    try:
        body = json.loads(text)
    except json.JSONDecodeError:
        return {"raw": text}
    return body if isinstance(body, dict) else {"raw": body}


def last_successful_rows(agent: dict[str, Any] | AgentRun | None) -> list[list[Any]] | None:
    """마지막 성공 execute_sql의 rows (validation/error 제외)."""
    if agent is None:
        return None
    if isinstance(agent, AgentRun):
        tool_calls = agent.tool_calls
    else:
        tool_calls = agent.get("tool_calls") or []
    last: list[list[Any]] | None = None
    for tc in tool_calls:
        if (tc.get("name") if isinstance(tc, dict) else None) != "execute_sql":
            continue
        body = unwrap_tool_result(tc.get("result") if isinstance(tc, dict) else None)
        if isinstance(body, dict) and "rows" in body and "code" not in body:
            rows = body.get("rows")
            if isinstance(rows, list):
                last = rows  # type: ignore[assignment]
    return last


def collect_execute_errors(agent: dict[str, Any] | AgentRun | None) -> list[str]:
    if agent is None:
        return []
    tool_calls = agent.tool_calls if isinstance(agent, AgentRun) else (agent.get("tool_calls") or [])
    out: list[str] = []
    for tc in tool_calls:
        if not isinstance(tc, dict) or tc.get("name") != "execute_sql":
            continue
        body = unwrap_tool_result(tc.get("result"))
        if not isinstance(body, dict):
            continue
        if body.get("code") or "message" in body:
            msg = str(body.get("message") or body.get("code") or body)
            out.append(msg)
        elif isinstance(body.get("raw"), str) and "error" in body["raw"].lower():
            out.append(str(body["raw"]))
    return out


def last_execute_sql(agent: dict[str, Any] | AgentRun | None) -> str | None:
    if agent is None:
        return None
    tool_calls = agent.tool_calls if isinstance(agent, AgentRun) else (agent.get("tool_calls") or [])
    last: str | None = None
    for tc in tool_calls:
        if isinstance(tc, dict) and tc.get("name") == "execute_sql":
            args = tc.get("args") or {}
            if isinstance(args, dict) and args.get("sql"):
                last = str(args["sql"])
    return last


def rows_match_ex(pred: list[list[Any]] | None, gold: list[list[Any]] | None) -> bool:
    """BIRD-style EX: unordered set equality of row tuples."""
    if pred is None or gold is None:
        return False

    def _tup(rows: list[list[Any]]) -> set[Any]:
        out: list[tuple[Any, ...]] = []
        for r in rows:
            out.append(tuple(r))
        try:
            return set(out)
        except TypeError:
            return set(map(repr, out))

    return _tup(pred) == _tup(gold)


def _bucket_from_error(msg: str) -> tuple[FailureBucket, str] | None:
    m = msg.lower()
    # Semantic registration gaps (contractual surface needs better meta)
    if "dimension" in m and "not in metric group_by" in m:
        return "semantic_layer", "metric.group_by에 필요한 dimension 미등록/누락"
    if "unknown" in m and ("metric" in m or "dimension" in m or "column" in m or "segment" in m):
        return "semantic_layer", "시맨틱 식별자 미등록(unknown metric/dimension/column)"
    if "joined cube" in m and "must project" in m:
        return "semantic_layer", "JOIN 큐브에 쓸 named metric 미비"
    if "must project at least one metric" in m or "must project at least one ad-hoc" in m:
        return "semantic_layer", "JOIN 측에 쓸 metric/ad-hoc 프로젝션 부족(메타·질의 표면)"
    if "subquery not allowed" in m:
        return "semantic_layer", "서브쿼리 불가 — 사전 집계 metric/view 등록 필요"
    if "unsupported metric expression form: extract" in m:
        return "semantic_layer", "EXTRACT 미지원 — date_trunc용 date dimension/metric 등록 필요"

    # sql-mcp code — only clear server-side failures (not agent-authored junk SQL)
    if "internal" in m or "panic" in m:
        return "sql_mcp_code", "서버 내부 오류"
    # sql parse errors are usually agent SQL; CASE WHEN disambiguated in classify_failure
    if "sql parse error" in m or "sql parser error" in m:
        return None

    # DW agent (wrong use of documented semantic SQL)
    if "order by must reference" in m:
        return "dw_agent", "ORDER BY에 집계식 직접 사용(alias 필요)"
    if "having is not allowed" in m or "group by is not allowed" in m:
        return "dw_agent", "D23 모드 혼동(ungrouped에 GROUP BY/HAVING)"
    if "ungrouped cube join without select distinct" in m:
        return "dw_agent", "ungrouped Cube JOIN에 SELECT DISTINCT 또는 GROUP BY 누락"
    if "function not allowed" in m and any(
        x in m for x in ("divide", "subtract", "multiply", "year", "month", "strftime")
    ):
        return "dw_agent", "물리 함수(DIVIDE/YEAR/strftime) 사용 — `/`·date_trunc 등으로 대체 가능"
    if "qualifier" in m and "does not match" in m:
        return "dw_agent", "물리 테이블 alias(T1 등) 사용 — FROM cube name만 허용"
    if "select *" in m:
        return "dw_agent", "SELECT * 금지 — dimension/metric 명시"
    if "unterminated string" in m:
        return "dw_agent", "문자열 리터럴 이스케이프 오류"
    if "aggregate not allowed here" in m:
        return "dw_agent", "모드/절 위치에 맞지 않는 집계"
    if "ad-hoc projection must be an aggregate" in m:
        return "dw_agent", "ad-hoc(B) 모드인데 비집계 프로젝션"
    return None


def _rank_errors(
    errors: list[str], sql_u: str
) -> list[tuple[FailureBucket, str]]:
    ranked: list[tuple[FailureBucket, str]] = []
    for err in reversed(errors):
        hit = _bucket_from_error(err)
        if hit:
            ranked.append(hit)
        elif "sql parse error" in err.lower() or "sql parser error" in err.lower():
            if "found: when" in err.lower():
                if re.search(r"\b(DIVIDE|SUBTRACT|MULTIPLY|YEAR|STRFTIME)\b", sql_u):
                    ranked.append(
                        (
                            "dw_agent",
                            "비허용 함수+CASE 혼합으로 파서 실패 — `/`·date_trunc·alias 사용 필요",
                        )
                    )
                elif re.search(r"\bCASE\b", sql_u) and re.search(r"\bEND\b", sql_u):
                    ranked.append(
                        (
                            "sql_mcp_code",
                            "유효해 보이는 CASE WHEN이 파서에서 거절됨",
                        )
                    )
                else:
                    ranked.append(("dw_agent", "CASE WHEN 구문 오류"))
            else:
                ranked.append(("dw_agent", "시맨틱 SQL 구문/괄호 오류"))
    return ranked


def classify_failure(
    *,
    pred_rows: list[list[Any]] | None,
    gold_rows: list[list[Any]] | None,
    errors: list[str],
    last_sql: str | None,
    gold_sql: str | None = None,
) -> tuple[FailureBucket, str]:
    """EX 실패 1건 → (bucket, reason)."""
    sql = last_sql or ""
    sql_u = sql.upper()
    ranked = _rank_errors(errors, sql_u)
    sem_n = sum(1 for b, _ in ranked if b == "semantic_layer")

    if pred_rows is not None and gold_rows is not None and not rows_match_ex(pred_rows, gold_rows):
        # 시맨틱 제약에 막힌 뒤 틀린 근사 질의로 "성공"한 경우 → 등록 이슈를 우선
        if sem_n > 0:
            return (
                "semantic_layer",
                "시맨틱 제약(JOIN/metric/subquery 등)에 막힌 뒤 근사 질의로 오답",
            )
        gsql = (gold_sql or "").lower()
        if " join " in gsql and " join " not in sql.lower():
            return (
                "semantic_layer",
                "gold는 JOIN인데 에이전트는 단일 큐브만 질의해 오답 — named metric/관계 메타 점검",
            )
        return "dw_agent", "execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)"

    # CASE WHEN without END → agent syntax
    if re.search(r"\bCASE\b", sql_u) and not re.search(r"\bEND\b", sql_u):
        return "dw_agent", "CASE 식에 END 누락(파서 WHEN 오류로 이어짐)"

    if ranked:
        recent = ranked[:5]
        counts = Counter(b for b, _ in recent)
        best = counts.most_common(1)[0][0]
        reason = next(r for b, r in recent if b == best)
        return best, reason

    if pred_rows is None:
        return "dw_agent", "성공한 execute_sql 없음(도구/재시도 실패)"
    return "dw_agent", "미분류 EX 실패"


def score_eval_item(item: dict[str, Any]) -> dict[str, Any]:
    agent = item.get("agent")
    gold = item.get("gold_rows")
    pred = last_successful_rows(agent if isinstance(agent, dict) else None)
    errors = collect_execute_errors(agent if isinstance(agent, dict) else None)
    sql = last_execute_sql(agent if isinstance(agent, dict) else None)
    ex = bool(gold is not None and rows_match_ex(pred, gold))
    bucket: FailureBucket | None = None
    reason: str | None = None
    if not ex:
        bucket, reason = classify_failure(
            pred_rows=pred,
            gold_rows=gold,
            errors=errors,
            last_sql=sql,
            gold_sql=item.get("gold_sql") if isinstance(item.get("gold_sql"), str) else None,
        )
    return {
        "question_id": item.get("question_id"),
        "db_id": item.get("db_id"),
        "difficulty": item.get("difficulty"),
        "ex": ex,
        "pred_rows": pred,
        "gold_row_count": len(gold) if isinstance(gold, list) else None,
        "pred_row_count": len(pred) if isinstance(pred, list) else None,
        "error_count": len(errors),
        "last_sql": sql,
        "last_error": errors[-1] if errors else None,
        "failure_bucket": bucket,
        "failure_reason": reason,
    }


def score_eval_doc(doc: dict[str, Any]) -> dict[str, Any]:
    items = doc.get("items") or []
    scored = [score_eval_item(it) for it in items if isinstance(it, dict)]
    n = len(scored)
    ex_n = sum(1 for s in scored if s["ex"])
    buckets: Counter[str] = Counter()
    reasons: Counter[str] = Counter()
    by_db: dict[str, dict[str, int]] = {}
    by_diff: dict[str, dict[str, int]] = {}
    for s in scored:
        db = str(s.get("db_id") or "?")
        diff = str(s.get("difficulty") or "?")
        by_db.setdefault(db, {"n": 0, "ex": 0, "dw_agent": 0, "sql_mcp_code": 0, "semantic_layer": 0})
        by_diff.setdefault(diff, {"n": 0, "ex": 0, "dw_agent": 0, "sql_mcp_code": 0, "semantic_layer": 0})
        by_db[db]["n"] += 1
        by_diff[diff]["n"] += 1
        if s["ex"]:
            by_db[db]["ex"] += 1
            by_diff[diff]["ex"] += 1
        else:
            b = str(s.get("failure_bucket") or "dw_agent")
            buckets[b] += 1
            reasons[str(s.get("failure_reason") or "?")] += 1
            by_db[db][b] = by_db[db].get(b, 0) + 1
            by_diff[diff][b] = by_diff[diff].get(b, 0) + 1
    return {
        "meta": {
            **(doc.get("meta") or {}),
            "scored_at": datetime.now(timezone.utc).isoformat(),
            "ex_metric": "unordered_set_equality_last_successful_execute_sql",
            "item_count": n,
            "ex_correct": ex_n,
            "ex_accuracy": round(100.0 * ex_n / n, 2) if n else 0.0,
            "failure_count": n - ex_n,
        },
        "failure_buckets": dict(buckets),
        "failure_reasons": reasons.most_common(),
        "by_db": by_db,
        "by_difficulty": by_diff,
        "items": scored,
    }
