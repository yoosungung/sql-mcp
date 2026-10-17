"""run_eval 병렬(--concurrency) 단위 테스트 (TDD)."""

from __future__ import annotations

import asyncio
import sqlite3
from pathlib import Path

import pytest

from bird_qa import BirdQuestion
from run_eval import eval_one, run_parallel


def _q(qid: int) -> BirdQuestion:
    return BirdQuestion(
        question_id=qid,
        db_id="tiny",
        question=f"q{qid}",
        evidence="",
        sql="SELECT 1",
        difficulty="simple",
    )


def test_run_parallel_respects_concurrency_and_order(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    import bird_eval
    import run_eval

    db = tmp_path / "tiny.sqlite"
    con = sqlite3.connect(db)
    con.execute("CREATE TABLE t(x)")
    con.commit()
    con.close()
    monkeypatch.setattr(bird_eval, "sqlite_path_for", lambda db_id: db)

    active = 0
    peak = 0
    lock = asyncio.Lock()

    async def fake_invoke(q, *, model, url, wait_timeout_s=60.0):
        nonlocal active, peak
        async with lock:
            active += 1
            peak = max(peak, active)
        await asyncio.sleep(0.05)
        async with lock:
            active -= 1
        return f"ans-{q.question_id}", []

    async def fake_wait(url, *, timeout_s=60.0):
        return None

    monkeypatch.setattr(run_eval, "invoke_bird_agent", fake_invoke)
    monkeypatch.setattr(run_eval, "wait_for_mcp", fake_wait)

    async def go() -> None:
        nonlocal peak
        questions = [_q(i) for i in range(1, 9)]
        out = tmp_path / "eval.json"
        items = await run_parallel(
            questions,
            concurrency=3,
            model="m",
            url="http://x/mcp",
            skip_agent=False,
            row_limit=10,
            log_max=100,
            out=out,
            qa_path=str(tmp_path / "qa.json"),
        )
        assert [it.question_id for it in items] == list(range(1, 9))
        assert all(it.agent and it.agent.final == f"ans-{it.question_id}" for it in items)
        assert peak <= 3
        assert peak >= 2
        assert out.is_file()

    asyncio.run(go())


def test_eval_one_skip_agent(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    import bird_eval

    db = tmp_path / "tiny.sqlite"
    con = sqlite3.connect(db)
    con.execute("CREATE TABLE t(x)")
    con.execute("INSERT INTO t VALUES (1)")
    con.commit()
    con.close()
    monkeypatch.setattr(bird_eval, "sqlite_path_for", lambda db_id: db)

    it = asyncio.run(
        eval_one(
            _q(1),
            model="m",
            url="http://x",
            skip_agent=True,
            row_limit=10,
            log_max=100,
        )
    )
    assert it.agent is None
    assert it.gold_rows == [[1]]
    assert it.gold_error is None
