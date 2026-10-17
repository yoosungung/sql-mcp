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
    item_from_question,
    run_gold_sql,
    serialize_messages,
    tool_calls_from_messages,
    write_eval_json,
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
    assert tcs[0]["result"] == '{"rows":[[1]]}'


def test_serialize_messages_clips() -> None:
    long = "x" * 100
    msg = SimpleNamespace(type="ai", name=None, content=long, tool_calls=None)
    ser = serialize_messages([msg], log_max=10)
    assert ser[0]["content"].startswith("xxxxxxxxxx")
    assert "…" in ser[0]["content"]
