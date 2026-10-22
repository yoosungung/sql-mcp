"""eval.json 스키마·gold SQLite·agent 직렬화 단위 테스트 (TDD)."""

from __future__ import annotations

import json
import sqlite3
from pathlib import Path
from types import SimpleNamespace

import pytest

from bird_eval import (
    AgentRun,
    annotate_eval_sql_causes,
    build_eval_doc,
    build_eval_sql_doc,
    classify_eval_sql_item,
    classify_failure,
    item_from_question,
    last_successful_rows,
    parse_dry_run_result,
    render_eval_md,
    rows_match_ex,
    score_eval_item,
    serialize_messages,
    tool_calls_from_messages,
    unwrap_tool_result,
    write_eval_json,
    write_eval_md,
    write_eval_sql_json,
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


def _eval_doc_two_items() -> dict:
    fail = item_from_question(
        _q(
            question_id=1312,
            db_id="student_club",
            question="What's Angela Sanders's major?",
            evidence="major refers to major_name.",
            sql="SELECT major_name FROM member WHERE first_name = 'Angela'",
            difficulty="simple",
        ),
        gold_rows=[["Business"]],
        agent=AgentRun(
            final="not found",
            tool_calls=[
                {
                    "name": "execute_sql",
                    "args": {
                        "sql": (
                            "SELECT major_name FROM student_club "
                            "WHERE first_name = 'Angela Sanders'"
                        )
                    },
                    "result": {"columns": ["major_name"], "rows": [], "row_count": 0},
                }
            ],
            elapsed_ms=1200,
            model="ollama:gemma4:e2b",
        ),
    )
    ok = item_from_question(
        _q(
            question_id=1317,
            db_id="student_club",
            question="How many medium shirts?",
            evidence="count",
            sql="SELECT COUNT(*) FROM event WHERE t_shirt_size = 'Medium'",
            difficulty="moderate",
        ),
        gold_rows=[[7]],
        agent=AgentRun(
            final="7",
            tool_calls=[
                {
                    "name": "execute_sql",
                    "args": {"sql": "SELECT count(*) FROM student_club WHERE t_shirt_size = 'Medium'"},
                    "result": {"columns": ["count(*)"], "rows": [[7]], "row_count": 1},
                }
            ],
            elapsed_ms=800,
        ),
    )
    return build_eval_doc([fail, ok], model="ollama:gemma4:e2b")


def test_render_eval_md_index_and_question_sections() -> None:
    md = render_eval_md(_eval_doc_two_items())
    assert "# BIRD Mini-Dev Eval (문항별)" in md
    assert "| [1312](#q1312) |" in md and "FAIL" in md
    assert "| [1317](#q1317) |" in md and "PASS" in md
    assert "### Q1312" in md
    assert "What's Angela Sanders's major?" in md
    assert "major refers to major_name." in md
    assert "#### Gold SQL" in md
    assert "#### Agent SQL" in md
    assert "SELECT major_name FROM member WHERE first_name = 'Angela'" in md
    assert "SELECT major_name FROM student_club WHERE first_name = 'Angela Sanders'" in md
    assert "#### Gold rows" in md
    assert "#### Agent rows" in md
    assert "| Business |" in md
    assert "_empty_" in md
    assert "only_gold=1" in md
    assert "| difficulty |" not in md
    assert "| bucket |" not in md
    assert "**difficulty**" not in md
    assert "**bucket**" not in md
    assert "**elapsed**" not in md
    assert "dw_agent" not in md
    assert "필터/조인으로 결과 공집합" in md
    assert "### Q1317" in md
    assert "How many medium shirts?" in md
    assert "[eval_report.md](eval_report.md)" in md


def test_render_eval_md_truncates_long_row_lists() -> None:
    rows = [[i] for i in range(45)]
    it = item_from_question(
        _q(question_id=99, sql="SELECT n FROM t"),
        gold_rows=rows,
        agent=AgentRun(
            final="many",
            tool_calls=[
                {
                    "name": "execute_sql",
                    "args": {"sql": "SELECT n FROM superhero"},
                    "result": {"columns": ["n"], "rows": rows, "row_count": 45},
                }
            ],
        ),
    )
    md = render_eval_md(build_eval_doc([it], model="x"))
    assert "#### Gold rows (45)" in md
    assert "#### Agent rows (45)" in md
    assert "| n |" in md
    assert "| 0 |" in md
    assert "| 39 |" in md
    assert "… +5 rows" in md
    assert "| 44 |" not in md


def test_write_eval_md_writes_utf8(tmp_path: Path) -> None:
    out = tmp_path / "eval.md"
    write_eval_md(_eval_doc_two_items(), out)
    text = out.read_text(encoding="utf-8")
    assert text.startswith("# BIRD Mini-Dev Eval (문항별)")
    assert "Q1312" in text


def test_parse_dry_run_result_sql_and_error() -> None:
    sql, err = parse_dry_run_result({"sql": "SELECT 1 FROM member", "dry_run": True})
    assert sql == "SELECT 1 FROM member"
    assert err is None
    sql, err = parse_dry_run_result({"code": "invalid_sql", "message": "unknown dimension"})
    assert sql is None
    assert err and "unknown dimension" in err


def test_build_eval_sql_doc_fail_only_with_dry_run() -> None:
    calls: list[tuple[str, str]] = []

    def dry_run(database: str, sql: str) -> tuple[str | None, str | None]:
        calls.append((database, sql))
        return "SELECT T2.major_name FROM member AS T1", None

    doc = build_eval_sql_doc(_eval_doc_two_items(), dry_run=dry_run)
    assert doc["meta"]["item_count"] == 1
    assert doc["meta"]["source"] == "ex_fail"
    assert len(doc["items"]) == 1
    row = doc["items"][0]
    assert row["question_id"] == 1312
    assert row["db_id"] == "student_club"
    assert row["question"] == "What's Angela Sanders's major?"
    assert row["evidence"] == "major refers to major_name."
    assert row["gold_sql"] == "SELECT major_name FROM member WHERE first_name = 'Angela'"
    assert "Angela Sanders" in row["agent_sql"]
    assert row["agent_db_sql"] == "SELECT T2.major_name FROM member AS T1"
    assert "agent_db_sql_error" not in row
    assert row["cause"] == "agent가 실패"
    assert calls == [
        (
            "student_club",
            "SELECT major_name FROM student_club WHERE first_name = 'Angela Sanders'",
        )
    ]


def test_build_eval_sql_doc_records_dry_run_error() -> None:
    def dry_run(database: str, sql: str) -> tuple[str | None, str | None]:
        return None, "unknown dimension first_name"

    row = build_eval_sql_doc(_eval_doc_two_items(), dry_run=dry_run)["items"][0]
    assert row["agent_db_sql"] is None
    assert row["agent_db_sql_error"] == "unknown dimension first_name"
    assert row["cause"] == "잘못된 시멘틱 정의"


def test_write_eval_sql_json_roundtrip(tmp_path: Path) -> None:
    out = tmp_path / "eval_sql.json"
    write_eval_sql_json(build_eval_sql_doc(_eval_doc_two_items()), out)
    loaded = json.loads(out.read_text(encoding="utf-8"))
    assert loaded["items"][0]["question_id"] == 1312
    assert loaded["items"][0]["agent_db_sql"] is None
    assert loaded["items"][0]["cause"] == "agent가 실패"


def test_classify_eval_sql_same_tables_is_agent() -> None:
    cause, reason = classify_eval_sql_item(
        {
            "gold_sql": (
                "SELECT T2.major_name FROM member AS T1 "
                "INNER JOIN major AS T2 ON T1.link_to_major = T2.major_id "
                "WHERE T1.first_name = 'Angela'"
            ),
            "agent_sql": (
                "SELECT major_name FROM student_club "
                "WHERE first_name = 'Angela Sanders'"
            ),
            "agent_db_sql": (
                'SELECT major.major_name FROM member '
                "INNER JOIN major ON member.link_to_major = major.major_id "
                "WHERE member.first_name = 'Angela Sanders'"
            ),
        }
    )
    assert cause == "agent가 실패"
    assert "로직" in reason or "필터" in reason


def test_classify_eval_sql_extra_compiled_tables_is_semantic() -> None:
    cause, reason = classify_eval_sql_item(
        {
            "gold_sql": (
                "SELECT T1.event_name FROM event AS T1 "
                "INNER JOIN attendance AS T2 ON T1.event_id = T2.link_to_event "
                "GROUP BY T1.event_id HAVING COUNT(T2.link_to_event) > 20"
            ),
            "agent_sql": (
                "SELECT event_name FROM student_club "
                "GROUP BY event_name HAVING COUNT(event_id) > 20 AND source != 'Fundraising'"
            ),
            "agent_db_sql": (
                "SELECT event.event_name FROM member "
                "INNER JOIN attendance ON member.member_id = attendance.link_to_member "
                "INNER JOIN income ON member.member_id = income.link_to_member "
                "INNER JOIN event ON attendance.link_to_event = event.event_id"
            ),
        }
    )
    assert cause == "잘못된 시멘틱 정의"
    assert "JOIN" in reason or "조인" in reason or "테이블" in reason


def test_classify_eval_sql_error_paths() -> None:
    cause, _ = classify_eval_sql_item(
        {"gold_sql": "SELECT 1", "agent_sql": None, "agent_db_sql_error": "no agent_sql"}
    )
    assert cause == "agent가 실패"
    cause, _ = classify_eval_sql_item(
        {
            "gold_sql": "SELECT Date FROM Laboratory",
            "agent_sql": "SELECT SUBTRACT(year(First_Date), year(Birthday)) FROM c",
            "agent_db_sql_error": "validation failed: function not allowed in metric expr: SUBTRACT",
        }
    )
    assert cause == "agent가 실패"
    cause, reason = classify_eval_sql_item(
        {
            "gold_sql": "SELECT CASE WHEN x=1 THEN 1 ELSE 0 END FROM t",
            "agent_sql": "SELECT CASE WHEN x=1 THEN 1 ELSE 0 END FROM c",
            "agent_db_sql_error": "sql parse error: sql parser error: Expected: ), found: WHEN",
        }
    )
    assert cause == "sql-mcp 버그"
    assert "CASE" in reason
    cause, _ = classify_eval_sql_item(
        {
            "gold_sql": "SELECT Birthday FROM Patient",
            "agent_sql": "SELECT YEAR(BIRTHDAY) FROM c",
            "agent_db_sql_error": "validation failed: unknown identifier in expression: BIRTHDAY",
        }
    )
    assert cause == "잘못된 시멘틱 정의"
    cause, _ = classify_eval_sql_item(
        {
            "gold_sql": "SELECT name FROM colour",
            "agent_sql": "SELECT color FROM superhero",
            "agent_db_sql_error": "validation failed: unknown identifier in expression: color",
        }
    )
    assert cause == "agent가 실패"
    cause, _ = classify_eval_sql_item(
        {
            "gold_sql": "SELECT 1",
            "agent_sql": "SELECT 1 FROM c",
            "agent_db_sql_error": "boom panic in compiler",
        }
    )
    assert cause == "sql-mcp 버그"


def test_annotate_eval_sql_causes_counts_and_field_order() -> None:
    doc = annotate_eval_sql_causes(
        {
            "meta": {"item_count": 1},
            "items": [
                {
                    "question_id": 1,
                    "db_id": "student_club",
                    "question": "q",
                    "gold_sql": "SELECT n FROM t",
                    "agent_sql": None,
                    "agent_db_sql": None,
                    "agent_db_sql_error": "no agent_sql",
                }
            ],
        }
    )
    row = doc["items"][0]
    keys = list(row)
    assert keys[keys.index("db_id") + 1] == "cause"
    assert row["cause"] == "agent가 실패"
    assert doc["meta"]["cause_counts"] == {
        "agent가 실패": 1,
        "잘못된 시멘틱 정의": 0,
        "sql-mcp 버그": 0,
        "기타": 0,
    }
