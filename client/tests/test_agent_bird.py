"""DW agent 시스템 프롬프트·유저 프롬프트 단위 테스트 (TDD)."""

from __future__ import annotations

from bird_qa import BirdQuestion
from agent_bird import SYSTEM, user_prompt


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
    assert "cube" in s and "dimension" in s
    assert "examples" in s


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


def test_system_prompt_requires_execute_and_answer_from_rows() -> None:
    """최종 답은 SQL이 아니라 execute_sql rows 값으로 답해야 한다."""
    s = SYSTEM.lower()
    assert "do not return sql" in s or "must not return sql" in s
    assert "execute_sql" in s
    assert "rows" in s


def test_system_prompt_requires_explicit_from() -> None:
    """모든 SELECT는 FROM에 큐브 이름을 명시해야 한다."""
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
    assert "empty" in s or "row_count" in s or "0 row" in s


def test_user_prompt_includes_evidence_and_db() -> None:
    q = BirdQuestion(
        question_id=1,
        db_id="student_club",
        question="What's Angela's major?",
        evidence="Angela Sanders; major_name",
        sql="SELECT 1",
        difficulty="simple",
    )
    text = user_prompt(q)
    assert "student_club" in text
    assert "Angela Sanders" in text
    assert "Evidence" in text
