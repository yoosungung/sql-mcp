"""eval.json 스키마·gold SQLite·agent 직렬화 단위 테스트 (TDD)."""

from __future__ import annotations

import json
import sqlite3
from pathlib import Path
from types import SimpleNamespace

import pytest

from bird_eval import (
    AgentRun,
    build_eval_doc,
    classify_failure,
    item_from_question,
    last_successful_rows,
    rows_match_ex,
    score_eval_item,
    serialize_messages,
    tool_calls_from_messages,
    unwrap_tool_result,
    write_eval_json,
    run_gold_sql,
)
from bird_qa import BirdQuestion


def _q(**kwargs) -> BirdQuestion:
    base = dict(
        question_id=10,
        db_id="superhero",
        question="How many?",
        evidence="count",
        sql="SELECT 1",
        difficulty="simple",
    )
    base.update(kwargs)
    return BirdQuestion(**base)


def test_item_from_question_fields() -> None:
    it = item_from_question(_q(), gold_rows=[[1]], agent=AgentRun(final="1"))
    assert it.question_id == 10
    assert it.evidence == "count"
    assert it.gold_sql == "SELECT 1"
    assert it.gold_rows == [[1]]
    assert it.agent and it.agent.final == "1"


def test_build_eval_doc_roundtrip(tmp_path: Path) -> None:
    it = item_from_question(
        _q(),
        gold_rows=[["Agility"]],
        agent=AgentRun(
            final="Agility",
            tool_calls=[{"name": "execute_sql", "args": {"sql": "SELECT x"}, "result": "ok"}],
            model="ollama:gemma4:e2b",
            elapsed_ms=12,
        ),
    )
    doc = build_eval_doc([it], model="ollama:gemma4:e2b", mcp_url="http://127.0.0.1:8090/mcp")
    assert doc["meta"]["item_count"] == 1
    assert doc["meta"]["model"] == "ollama:gemma4:e2b"
    row = doc["items"][0]
    assert row["question"] == "How many?"
    assert row["evidence"] == "count"
    assert row["gold_sql"] == "SELECT 1"
    assert row["gold_rows"] == [["Agility"]]
    assert row["agent"]["final"] == "Agility"
    assert row["agent"]["tool_calls"][0]["name"] == "execute_sql"

    out = tmp_path / "eval.json"
    write_eval_json(doc, out)
    loaded = json.loads(out.read_text(encoding="utf-8"))
    assert loaded["items"][0]["question_id"] == 10


def test_run_gold_sql_on_temp_db(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    db_dir = tmp_path / "tiny"
    db_dir.mkdir()
    db_path = db_dir / "tiny.sqlite"
    con = sqlite3.connect(db_path)
    con.execute("CREATE TABLE t (n INTEGER)")
    con.execute("INSERT INTO t VALUES (42)")
    con.commit()
    con.close()

    import bird_eval

    monkeypatch.setattr(bird_eval, "sqlite_path_for", lambda db_id: db_dir / f"{db_id}.sqlite")
    rows, err = run_gold_sql("tiny", "SELECT n FROM t")
    assert err is None
    assert rows == [[42]]


def test_run_gold_sql_missing_db(monkeypatch: pytest.MonkeyPatch) -> None:
    import bird_eval

    monkeypatch.setattr(bird_eval, "sqlite_path_for", lambda db_id: Path("/no/such/x.sqlite"))
    rows, err = run_gold_sql("x", "SELECT 1")
    assert rows is None
    assert err and "missing" in err


def test_tool_calls_from_messages_pairs_result() -> None:
    ai = SimpleNamespace(
        type="ai",
        name=None,
        content="",
        tool_calls=[{"id": "c1", "name": "execute_sql", "args": {"sql": "SELECT 1"}}],
    )
    tool = SimpleNamespace(
        type="tool",
        name="execute_sql",
        content='{"rows":[[1]]}',
        tool_call_id="c1",
        tool_calls=None,
    )
    tcs = tool_calls_from_messages([ai, tool])
    assert len(tcs) == 1
    assert tcs[0]["name"] == "execute_sql"
    assert tcs[0]["args"] == {"sql": "SELECT 1"}
    body = unwrap_tool_result(tcs[0]["result"])
    assert body is not None
    assert body["rows"] == [[1]]


def test_tool_calls_from_messages_execute_sql_not_char_clipped() -> None:
    """execute_sql 결과는 log_max 문자 절단 없이 rows를 보존 (채점용)."""
    rows = [[i, f"name-{i}"] for i in range(80)]
    payload = json.dumps({"columns": ["id", "n"], "rows": rows, "row_count": len(rows)})
    assert len(payload) > 400  # would be clipped by a small log_max
    ai = SimpleNamespace(
        type="ai",
        name=None,
        content="",
        tool_calls=[{"id": "c1", "name": "execute_sql", "args": {"sql": "SELECT id, n"}}],
    )
    tool = SimpleNamespace(
        type="tool",
        name="execute_sql",
        content=[{"type": "text", "text": payload}],
        tool_call_id="c1",
        tool_calls=None,
    )
    tcs = tool_calls_from_messages([ai, tool], log_max=100)
    body = unwrap_tool_result(tcs[0]["result"])
    assert body is not None
    assert "rows" in body
    assert len(body["rows"]) == 80
    assert body["rows"][-1] == [79, "name-79"]
    # scoring path must see these rows
    assert last_successful_rows({"tool_calls": tcs}) == rows


def test_tool_calls_from_messages_execute_sql_row_limit() -> None:
    rows = [[i] for i in range(600)]
    payload = json.dumps({"columns": ["n"], "rows": rows, "row_count": 600})
    ai = SimpleNamespace(
        type="ai",
        name=None,
        content="",
        tool_calls=[{"id": "c1", "name": "execute_sql", "args": {"sql": "SELECT n"}}],
    )
    tool = SimpleNamespace(
        type="tool",
        name="execute_sql",
        content=payload,
        tool_call_id="c1",
        tool_calls=None,
    )
    tcs = tool_calls_from_messages([ai, tool], log_max=50, row_limit=500)
    body = unwrap_tool_result(tcs[0]["result"])
    assert body is not None
    assert len(body["rows"]) == 500
    assert body.get("truncated") is True


def test_serialize_messages_clips() -> None:
    long = "x" * 100
    msg = SimpleNamespace(type="ai", name=None, content=long, tool_calls=None)
    ser = serialize_messages([msg], log_max=10)
    assert ser[0]["content"].startswith("xxxxxxxxxx")
    assert "…" in ser[0]["content"]


def test_unwrap_tool_result_mcp_text_envelope() -> None:
    raw = [
        {
            "type": "text",
            "text": '{"columns":["n"],"rows":[[1]],"row_count":1}',
        }
    ]
    body = unwrap_tool_result(raw)
    assert body is not None
    assert body["rows"] == [[1]]


def test_rows_match_ex_unordered() -> None:
    assert rows_match_ex([[2], [1]], [[1], [2]])
    assert not rows_match_ex([[1]], [[2]])


def test_last_successful_rows_picks_last_ok() -> None:
    agent = {
        "tool_calls": [
            {
                "name": "execute_sql",
                "result": [
                    {
                        "type": "text",
                        "text": '{"code":"validation","message":"bad"}',
                    }
                ],
            },
            {
                "name": "execute_sql",
                "result": [
                    {
                        "type": "text",
                        "text": '{"columns":["c"],"rows":[["Business"]]}',
                    }
                ],
            },
        ]
    }
    assert last_successful_rows(agent) == [["Business"]]


def test_score_eval_item_ex_pass() -> None:
    item = {
        "question_id": 1,
        "db_id": "student_club",
        "difficulty": "simple",
        "gold_rows": [["Business"]],
        "agent": {
            "tool_calls": [
                {
                    "name": "execute_sql",
                    "args": {"sql": "SELECT major_name FROM student_club"},
                    "result": [
                        {
                            "type": "text",
                            "text": '{"columns":["major_name"],"rows":[["Business"]]}',
                        }
                    ],
                }
            ]
        },
    }
    scored = score_eval_item(item)
    assert scored["ex"] is True
    assert scored["failure_bucket"] is None


def test_classify_failure_wrong_rows_is_agent() -> None:
    bucket, reason = classify_failure(
        pred_rows=[[0]],
        gold_rows=[[7]],
        errors=[],
        last_sql="SELECT COUNT(*) FROM student_club",
        gold_sql="SELECT COUNT(*) FROM t",
    )
    assert bucket == "dw_agent"
    assert "불일치" in reason


def test_classify_failure_gold_join_without_agent_join_is_not_auto_semantic() -> None:
    """단일 큐브(OBV) 질의는 gold JOIN만으로 semantic_layer로 묶지 않는다."""
    bucket, reason = classify_failure(
        pred_rows=[[0]],
        gold_rows=[[7]],
        errors=[],
        last_sql="SELECT COUNT(city) FROM student_club WHERE county = 'Orange County'",
        gold_sql=(
            "SELECT COUNT(city) FROM zip_code JOIN member ON member.zip = zip_code.zip "
            "WHERE county = 'Orange County'"
        ),
    )
    assert bucket == "dw_agent"
    assert "단일 큐브" not in reason


def test_classify_failure_compiled_extra_join_is_semantic() -> None:
    bucket, reason = classify_failure(
        pred_rows=[[0]],
        gold_rows=[[7]],
        errors=[],
        last_sql="SELECT COUNT(city) FROM student_club WHERE county = 'Orange County'",
        gold_sql="SELECT COUNT(city) FROM zip_code WHERE county = 'Orange County'",
        compiled_sql=(
            "SELECT COUNT(zip_code.city) FROM member "
            "INNER JOIN zip_code ON member.zip = zip_code.zip "
            "WHERE zip_code.county = 'Orange County'"
        ),
    )
    assert bucket == "semantic_layer"
    assert "JOIN" in reason or "조인" in reason


def test_classify_failure_type_coerce_match_is_type() -> None:
    bucket, reason = classify_failure(
        pred_rows=[["7"]],
        gold_rows=[[7]],
        errors=[],
        last_sql="SELECT COUNT(*) FROM c",
        gold_sql="SELECT COUNT(*) FROM t",
    )
    assert bucket == "dw_agent"
    assert "타입" in reason


def test_classify_failure_wrong_rows_after_join_block_is_semantic() -> None:
    bucket, reason = classify_failure(
        pred_rows=[[0]],
        gold_rows=[[7]],
        errors=[
            "validation failed: joined cube sales_b (alias sales_b) must project at least one metric",
        ],
        last_sql="SELECT COUNT(*) FROM student_club",
        gold_sql="SELECT COUNT(*) FROM a JOIN b ON a.id=b.id",
    )
    assert bucket == "semantic_layer"
    assert "근사" in reason or "JOIN" in reason


def test_classify_failure_missing_metric_is_semantic() -> None:
    bucket, reason = classify_failure(
        pred_rows=None,
        gold_rows=[[1]],
        errors=[
            "validation failed: dimension year not in metric group_by",
        ],
        last_sql="SELECT m FROM formula_1 GROUP BY year",
    )
    assert bucket == "semantic_layer"


def test_classify_failure_order_by_agg_is_agent() -> None:
    bucket, _reason = classify_failure(
        pred_rows=None,
        gold_rows=[[1]],
        errors=[
            "validation failed: ORDER BY must reference a semantic column or alias, got: COUNT(*)",
        ],
        last_sql="SELECT COUNT(*) FROM c ORDER BY COUNT(*)",
    )
    assert bucket == "dw_agent"


def test_classify_failure_divide_case_parse_is_agent() -> None:
    bucket, _reason = classify_failure(
        pred_rows=None,
        gold_rows=[[1]],
        errors=["sql parse error: Expected: ), found: WHEN"],
        last_sql=(
            "SELECT DIVIDE(COUNT(CASE WHEN x=1 THEN 1 END), "
            "COUNT(CASE WHEN x=1 THEN 1 END)) FROM c"
        ),
    )
    assert bucket == "dw_agent"
