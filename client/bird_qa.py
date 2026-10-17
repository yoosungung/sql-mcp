"""BIRD Mini-Dev 질문-SQL 로더.

출처: `target_db/fetch_bird.sh`가 zip에서 추출한
`target_db/bird_qa/mini_dev_sqlite.json`.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

import json


@dataclass(frozen=True)
class BirdQuestion:
    question_id: int
    db_id: str
    question: str
    evidence: str
    sql: str
    difficulty: str


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def default_qa_path() -> Path:
    return repo_root() / "target_db" / "bird_qa" / "mini_dev_sqlite.json"


# config.bird.toml --small 타깃과 맞춤
SMALL_DBS = frozenset(
    {
        "superhero",
        "student_club",
        "toxicology",
        "thrombosis_prediction",
        "california_schools",
        "formula_1",
    }
)


def load_questions(
    path: Path | None = None,
    *,
    db_ids: Iterable[str] | None = None,
) -> list[BirdQuestion]:
    qa_path = path or default_qa_path()
    if not qa_path.is_file():
        raise FileNotFoundError(
            f"BIRD Q-SQL not found: {qa_path}\n"
            "Run: ./target_db/fetch_bird.sh --small"
        )
    raw = json.loads(qa_path.read_text(encoding="utf-8"))
    if not isinstance(raw, list):
        raise ValueError(f"expected JSON array in {qa_path}")
    allow = set(db_ids) if db_ids is not None else None
    out: list[BirdQuestion] = []
    for row in raw:
        db_id = str(row["db_id"])
        if allow is not None and db_id not in allow:
            continue
        out.append(
            BirdQuestion(
                question_id=int(row["question_id"]),
                db_id=db_id,
                question=str(row["question"]),
                evidence=str(row.get("evidence") or ""),
                sql=str(row["SQL"]),
                difficulty=str(row.get("difficulty") or ""),
            )
        )
    return out


def pick_question(
    questions: list[BirdQuestion],
    *,
    question_id: int | None = None,
    db_id: str | None = None,
    index: int = 0,
) -> BirdQuestion:
    rows = questions
    if question_id is not None:
        for q in rows:
            if q.question_id == question_id:
                return q
        raise LookupError(f"question_id={question_id} not found")
    if db_id is not None:
        rows = [q for q in rows if q.db_id == db_id]
        if not rows:
            raise LookupError(f"no questions for db_id={db_id}")
    if not rows:
        raise LookupError("question list is empty")
    if index < 0 or index >= len(rows):
        raise IndexError(f"index={index} out of range (len={len(rows)})")
    return rows[index]
