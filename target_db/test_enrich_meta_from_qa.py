"""enrich_meta_from_qa 단위 테스트 (TDD)."""

from __future__ import annotations

import json
import re
from pathlib import Path

import pytest

from enrich_meta_from_qa import (
    DOMAIN_SPECS,
    QA_PATH,
    build_docs,
    evidence_mentions,
    group_by_columns,
    load_cube,
    make_metric,
    make_segment,
)

NAME_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
ROOT = Path(__file__).resolve().parent


def test_domain_specs_cover_all_bird_dbs_in_qa():
    if not QA_PATH.is_file():
        pytest.skip("bird_qa missing")
    data = json.loads(QA_PATH.read_text(encoding="utf-8"))
    qa_dbs = {row["db_id"] for row in data}
    assert qa_dbs == set(DOMAIN_SPECS), f"missing/extra: {qa_dbs ^ set(DOMAIN_SPECS)}"


def test_each_db_has_reusable_not_one_per_question():
    if not QA_PATH.is_file():
        pytest.skip("bird_qa missing")
    data = json.loads(QA_PATH.read_text(encoding="utf-8"))
    counts = {}
    for row in data:
        counts[row["db_id"]] = counts.get(row["db_id"], 0) + 1
    for db_id, spec in DOMAIN_SPECS.items():
        n_docs = len(spec.get("segments") or []) + len(spec.get("metrics") or [])
        n_q = counts.get(db_id, 0)
        assert n_docs < n_q, f"{db_id}: {n_docs} docs >= {n_q} questions (overfit)"
        assert n_docs >= 3, f"{db_id}: too few domain docs"


def test_names_and_shapes_valid():
    for db_id, spec in DOMAIN_SPECS.items():
        cube_path = ROOT / db_id / "meta" / f"{db_id}.json"
        if not cube_path.is_file():
            pytest.skip(f"meta missing for {db_id}")
        cube = load_cube(db_id)
        members = {m["name"] for m in cube["dimensions"] + cube["facts"]}
        seen: set[str] = set()
        for seg in spec.get("segments") or []:
            assert NAME_RE.match(seg["name"])
            assert seg["name"] not in members
            assert seg["name"] not in seen
            seen.add(seg["name"])
            assert seg["filter"].strip()
            doc = make_segment(
                name=seg["name"],
                description=seg["description"],
                cube=cube,
                filter_expr=seg["filter"],
            )
            assert doc["kind"] == "segment"
            assert doc["cube"] == cube["name"]
        for met in spec.get("metrics") or []:
            assert NAME_RE.match(met["name"])
            assert met["name"] not in members
            assert met["name"] not in seen
            seen.add(met["name"])
            doc = make_metric(
                name=met["name"],
                description=met["description"],
                cube=cube,
                expression=met["expression"],
            )
            assert doc["group_by"] == group_by_columns(cube)
            assert set(doc["group_by"]).issubset(
                {d["column"] for d in cube["dimensions"] if d.get("column")}
            )


def test_build_docs_writes_only_metric_segment():
    for db_id in DOMAIN_SPECS:
        cube_path = ROOT / db_id / "meta" / f"{db_id}.json"
        if not cube_path.is_file():
            continue
        docs = build_docs(db_id)
        assert docs
        assert all(d["kind"] in ("metric", "segment") for d in docs)
        assert len({(d["kind"], d["name"]) for d in docs}) == len(docs)


def test_evidence_signal_present_for_key_cohorts():
    """주요 cohort 키워드가 해당 DB evidence에 실제로 등장하는지 스모크."""
    checks = {
        "superhero": ["Marvel Comics", "Female"],
        "financial": ["gender = 'M'", "status = 'A'"],
        "toxicology": ["label = '+'", "bond_type"],
        "california_schools": ["Excellence rate", "NumGE1500"],
        "debit_card_specializing": ["Currency = 'EUR'", "CZK"],
    }
    for db_id, needles in checks.items():
        evs = "\n".join(evidence_mentions(db_id))
        if not evs:
            pytest.skip("bird_qa missing")
        for needle in needles:
            assert needle in evs, f"{db_id}: evidence missing {needle!r}"
