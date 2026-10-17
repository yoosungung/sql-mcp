"""BIRD Mini-Dev 질문-SQL 로더 단위 테스트 (TDD)."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from bird_qa import BirdQuestion, load_questions, pick_question, repo_root

FIXTURE = [
    {
        "question_id": 1,
        "db_id": "superhero",
        "question": "How many superheroes?",
        "evidence": "count rows",
        "SQL": "SELECT COUNT(*) FROM superhero",
        "difficulty": "simple",
    },
    {
        "question_id": 2,
        "db_id": "formula_1",
        "question": "How many races?",
        "evidence": "",
        "SQL": "SELECT COUNT(*) FROM races",
        "difficulty": "moderate",
    },
]


def test_load_questions_from_path(tmp_path: Path) -> None:
    path = tmp_path / "mini_dev_sqlite.json"
    path.write_text(json.dumps(FIXTURE), encoding="utf-8")
    rows = load_questions(path)
    assert len(rows) == 2
    assert rows[0] == BirdQuestion(
        question_id=1,
        db_id="superhero",
        question="How many superheroes?",
        evidence="count rows",
        sql="SELECT COUNT(*) FROM superhero",
        difficulty="simple",
    )


def test_load_questions_filters_db_ids(tmp_path: Path) -> None:
    path = tmp_path / "mini_dev_sqlite.json"
    path.write_text(json.dumps(FIXTURE), encoding="utf-8")
    rows = load_questions(path, db_ids={"superhero"})
    assert [r.question_id for r in rows] == [1]


def test_pick_question_by_id(tmp_path: Path) -> None:
    path = tmp_path / "mini_dev_sqlite.json"
    path.write_text(json.dumps(FIXTURE), encoding="utf-8")
    q = pick_question(load_questions(path), question_id=2)
    assert q.db_id == "formula_1"


def test_pick_question_missing_raises(tmp_path: Path) -> None:
    path = tmp_path / "mini_dev_sqlite.json"
    path.write_text(json.dumps(FIXTURE), encoding="utf-8")
    with pytest.raises(LookupError):
        pick_question(load_questions(path), question_id=999)


def test_default_qa_path_under_repo() -> None:
    from bird_qa import default_qa_path

    root = repo_root()
    assert (root / "target_db").is_dir()
    assert default_qa_path() == root / "target_db" / "bird_qa" / "mini_dev_sqlite.json"


def test_load_extracted_bird_qa_if_present() -> None:
    from bird_qa import default_qa_path

    path = default_qa_path()
    if not path.is_file():
        pytest.skip("run ./target_db/fetch_bird.sh first")
    rows = load_questions(path, db_ids={"superhero"})
    assert len(rows) >= 1
    assert all(r.db_id == "superhero" and r.sql for r in rows)
