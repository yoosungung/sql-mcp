"""DW agent 시스템 프롬프트·유저 프롬프트·동일 SQL 실패 루프 중단 단위 테스트 (TDD)."""

from __future__ import annotations

import json
from types import SimpleNamespace

from bird_qa import BirdQuestion
from agent_bird import (
    MAX_IDENTICAL_SQL_FAILURES,
    SYSTEM,
    IdenticalSqlFailureMiddleware,
    bird_agent_middleware,
    is_execute_sql_failure,
    should_stop_for_identical_sql_failures,
    sql_fingerprint,
    user_prompt,
)


def test_system_prompt_bans_physical_sql_habits() -> None:
    s = SYSTEM.lower()
    assert "divide" in s
    assert "strftime" in s or "year(" in s
    assert "date_trunc" in s
    assert "alias" in s or "t1" in s


def test_system_prompt_requires_sql_not_olap_dsl() -> None:
    s = SYSTEM.lower()
    assert "sql select" in s or "select text" in s
    assert "mdx" in s and "dax" in s
    assert "table" in s and "column" in s
    assert "examples" in s
    assert "dimension" not in s
    assert "member" not in s
    # "CUBE"는 GROUP BY 키워드 경고만. 스키마 용어로 cube를 쓰지 않는다.
    assert "cube name" not in s
    assert "cubes" not in s


def test_system_prompt_covers_d23_d24_and_join() -> None:
    s = SYSTEM
    assert "GROUP BY" in s and "HAVING" in s
    assert "JOIN" in s
    assert "SELECT DISTINCT" in s
    assert "SELECT *" in s
    assert "subquery" in s.lower() or "WITH" in s
    assert "CTE" in s or "WITH" in s


def test_system_prompt_retry_and_escaping() -> None:
    s = SYSTEM.lower()
    assert "validation" in s or "error" in s
    assert "''" in SYSTEM or "escape" in s
    assert "same" in s or "identical" in s
    assert str(MAX_IDENTICAL_SQL_FAILURES) in SYSTEM


def test_system_prompt_requires_execute_and_answer_from_rows() -> None:
    """최종 답은 SQL이 아니라 execute_sql rows 값으로 답해야 한다."""
    s = SYSTEM.lower()
    assert "do not return sql" in s or "must not return sql" in s
    assert "execute_sql" in s
    assert "rows" in s


def test_system_prompt_requires_explicit_from() -> None:
    """모든 SELECT는 FROM에 테이블 이름을 명시해야 한다."""
    s = SYSTEM.lower()
    assert "from" in s
    assert "must" in s or "explicit" in s or "every select" in s


def test_system_prompt_bans_star_allows_literal_rewrite() -> None:
    s = SYSTEM.lower()
    assert "select *" in s
    assert "select 1" in s or "literal" in s


def test_system_prompt_requires_sample_column_for_where_literals() -> None:
    """WHERE 필터 리터럴은 sample_column으로 실제 값을 확인해야 한다."""
    s = SYSTEM.lower()
    assert "sample_column" in s
    assert "where" in s
    assert "substring" in s
    assert "value side" in s
    assert "empty" in s or "row_count" in s or "0 row" in s
    assert "never omit" in s or "do not omit" in s
    assert '"table"' in SYSTEM or "`table`" in SYSTEM
    assert "same args" in s or "same arguments" in s
    assert "count" in s
    assert "superhero" not in s


def test_user_prompt_is_question_only() -> None:
    q = BirdQuestion(
        question_id=1,
        db_id="student_club",
        question="What's Angela's major?",
        evidence="Angela Sanders; major_name",
        sql="SELECT 1",
        difficulty="simple",
    )
    text = user_prompt(q)
    assert text == "What's Angela's major?"
    assert "student_club" not in text
    assert "Angela Sanders" not in text
    assert "Evidence" not in text
    assert "Difficulty" not in text


def test_system_prompt_discovers_database_without_evidence() -> None:
    assert "list_databases" in SYSTEM
    assert "Evidence" not in SYSTEM


_ERR = {"code": "validation", "message": "subqueries are not allowed in semantic expressions"}
_OK = {"columns": ["x"], "rows": [], "row_count": 0}
_Q1457_SQL = (
    "SELECT AVG(r.points) FROM results r WHERE r.raceId IN "
    "(SELECT raceId FROM races WHERE year = 2017)"
)


def test_sql_fingerprint_collapses_whitespace() -> None:
    assert sql_fingerprint("SELECT  1\nFROM t") == sql_fingerprint("SELECT 1 FROM t")


def test_zero_row_success_is_not_verification_failure() -> None:
    assert is_execute_sql_failure(_OK) is False
    assert is_execute_sql_failure(_ERR) is True
    assert is_execute_sql_failure("parse error", status="error") is True


def test_same_sql_failure_stops_at_max_retries() -> None:
    """동일 SQL 검증 실패 3회(재시도 2회)면 중단 — Q1457 18회 방지."""
    attempts = [(_Q1457_SQL, _ERR, None)] * 2
    assert should_stop_for_identical_sql_failures(attempts) is None
    attempts.append((_Q1457_SQL, _ERR, None))
    assert should_stop_for_identical_sql_failures(attempts) == sql_fingerprint(_Q1457_SQL)


def test_whitespace_variants_count_as_same_sql() -> None:
    attempts = [
        ("SELECT AVG(x) FROM t", _ERR, None),
        ("SELECT  AVG(x)  FROM  t", _ERR, None),
        ("SELECT AVG(x) FROM t\n", _ERR, None),
    ]
    assert should_stop_for_identical_sql_failures(attempts) is not None


def test_different_sql_failures_do_not_stop() -> None:
    attempts = [
        ("SELECT a FROM t", _ERR, None),
        ("SELECT b FROM t", _ERR, None),
        ("SELECT c FROM t", _ERR, None),
    ]
    assert should_stop_for_identical_sql_failures(attempts) is None


def test_empty_rows_do_not_count_toward_retry_limit() -> None:
    attempts = [(_Q1457_SQL, _OK, None)] * 5
    assert should_stop_for_identical_sql_failures(attempts) is None


def _ai(tid: str, sql: str) -> SimpleNamespace:
    return SimpleNamespace(
        type="ai",
        tool_calls=[{"id": tid, "name": "execute_sql", "args": {"sql": sql}}],
        content="",
    )


def _tool(tid: str, result: dict, *, status: str | None = None) -> SimpleNamespace:
    return SimpleNamespace(
        type="tool",
        name="execute_sql",
        tool_call_id=tid,
        content=json.dumps(result),
        status=status,
        tool_calls=None,
    )


def test_middleware_jumps_to_end_after_three_identical_failures() -> None:
    mw = IdenticalSqlFailureMiddleware(max_failures=3)
    msgs = []
    for i in range(3):
        msgs.append(_ai(f"c{i}", _Q1457_SQL))
        msgs.append(_tool(f"c{i}", _ERR))
    out = mw.before_model({"messages": msgs}, None)
    assert out is not None
    assert out["jump_to"] == "end"
    assert "3" in out["messages"][0].content

    two = mw.before_model({"messages": msgs[:4]}, None)
    assert two is None


def test_bird_middleware_includes_identical_sql_limiter() -> None:
    mw = bird_agent_middleware()
    assert len(mw) == 1
    assert isinstance(mw[0], IdenticalSqlFailureMiddleware)
    assert mw[0].max_failures == MAX_IDENTICAL_SQL_FAILURES
    assert MAX_IDENTICAL_SQL_FAILURES == 3
