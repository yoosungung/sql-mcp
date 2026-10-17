"""DW agent 시스템 프롬프트·유저 프롬프트 단위 테스트 (TDD)."""

from __future__ import annotations

from bird_qa import BirdQuestion
from agent_bird import SYSTEM, user_prompt


def test_system_prompt_bans_physical_sql_habits() -> None:
    s = SYSTEM.lower()
    assert "divide" in s
    assert "strftime" in s or "year(" in s
    assert "date_trunc" in s
    assert "alias" in s


def test_system_prompt_covers_d23_and_join() -> None:
    s = SYSTEM
    assert "GROUP BY" in s and "HAVING" in s
    assert "JOIN" in s
    assert "SELECT DISTINCT" in s
    assert "SELECT *" in s


def test_system_prompt_retry_and_escaping() -> None:
    s = SYSTEM.lower()
    assert "validation" in s or "error" in s
    assert "''" in SYSTEM or "escape" in s


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
