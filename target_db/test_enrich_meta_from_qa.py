"""enrich_meta_from_qa 단위 테스트 (TDD)."""

from __future__ import annotations

import json
import re
from pathlib import Path

import pytest

from enrich_meta_from_qa import (
    DOMAIN_SPECS,
    QA_PATH,
    add_expression_dim,
    apply_member_descriptions,
    build_docs,
    evidence_mentions,
    group_by_columns,
    list_cubes,
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


def _cube_for_item(db_id: str, item: dict, cubes: dict[str, dict]) -> dict:
    name = item.get("cube")
    if name:
        assert name in cubes, f"{db_id}: missing cube {name} for {item['name']}"
        return cubes[name]
    if db_id in cubes:
        return cubes[db_id]
    if len(cubes) == 1:
        return next(iter(cubes.values()))
    raise AssertionError(f"{db_id}: spec item {item['name']} needs cube")


def test_apply_member_descriptions_fills_empty_only() -> None:
    cube = {
        "name": "c",
        "description": "",
        "dimensions": [{"name": "a", "description": ""}],
        "facts": [{"name": "b", "description": "keep"}],
    }
    assert apply_member_descriptions(cube, {"a": "A meaning", "b": "B new"})
    assert cube["dimensions"][0]["description"] == "A meaning"
    assert cube["facts"][0]["description"] == "keep"
    assert not apply_member_descriptions(cube, {"a": "other"})


def test_add_expression_dim_skips_existing() -> None:
    cube = {
        "dimensions": [{"name": "id", "column": "id"}],
        "facts": [],
    }
    spec = {
        "name": "birth_year",
        "expression": "EXTRACT(YEAR FROM Birthday)",
        "type": "date",
        "description": "y",
    }
    assert add_expression_dim(cube, spec)
    assert cube["dimensions"][-1]["expression"] == "EXTRACT(YEAR FROM Birthday)"
    assert "column" not in cube["dimensions"][-1]
    assert not add_expression_dim(cube, spec)


def test_names_and_shapes_valid():
    for db_id, spec in DOMAIN_SPECS.items():
        cubes = list_cubes(db_id)
        if not cubes:
            pytest.skip(f"meta missing for {db_id}")
        seen: set[str] = set()
        for seg in spec.get("segments") or []:
            cube = _cube_for_item(db_id, seg, cubes)
            members = {m["name"] for m in cube["dimensions"] + cube["facts"]}
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
            cube = _cube_for_item(db_id, met, cubes)
            members = {m["name"] for m in cube["dimensions"] + cube["facts"]}
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
            col_dims = [d["column"] for d in cube["dimensions"] if d.get("column")]
            assert doc["group_by"] == group_by_columns(cube)
            assert set(doc["group_by"]).issubset(set(col_dims))
            assert 1 <= len(doc["group_by"]) <= 4
            if len(col_dims) > 4:
                assert len(doc["group_by"]) < len(col_dims)
            assert "DIVIDE(" not in doc["expression"]


def test_hub_specs_name_their_cube():
    """Split DBs must pin every metric/segment to a planned cube."""
    split = {
        "formula_1",
        "toxicology",
        "california_schools",
        "student_club",
        "thrombosis_prediction",
        "superhero",
    }
    for db_id in split:
        spec = DOMAIN_SPECS[db_id]
        cubes = list_cubes(db_id)
        for item in (spec.get("segments") or []) + (spec.get("metrics") or []):
            assert item.get("cube"), f"{db_id}.{item['name']} missing cube"
            if cubes:
                assert item["cube"] in cubes, f"{db_id}.{item['name']} -> {item['cube']}"
            else:
                assert item["cube"]  # still required without generated meta
        for dim in spec.get("expression_dims") or []:
            assert dim.get("cube")
            if cubes:
                assert dim["cube"] in cubes


def test_build_docs_writes_only_metric_segment():
    for db_id in DOMAIN_SPECS:
        if not list_cubes(db_id):
            continue
        docs = build_docs(db_id)
        if not docs:
            spec = DOMAIN_SPECS[db_id]
            names = [s["name"] for s in spec.get("segments") or []] + [
                m["name"] for m in spec.get("metrics") or []
            ]
            meta = ROOT / db_id / "meta"
            docs = [
                json.loads((meta / f"{name}.json").read_text())
                for name in names
                if (meta / f"{name}.json").is_file()
            ]
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
