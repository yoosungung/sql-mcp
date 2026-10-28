"""enrich_meta_from_qa 단위 테스트 (TDD)."""

from __future__ import annotations

import json
import re
from pathlib import Path

import pytest

from enrich_meta_from_qa import (
    DOMAIN_SPECS,
    QA_PATH,
    _CONFUSION_FORCE,
    add_expression_dim,
    add_expression_fact,
    apply_cube_description,
    apply_member_descriptions,
    build_docs,
    evidence_mentions,
    group_by_columns,
    list_cubes,
    make_metric,
    make_segment,
    prune_stale_metric_segment,
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


def test_apply_cube_description_overwrites() -> None:
    cube = {"name": "c", "description": "old JOIN Copy"}
    assert apply_cube_description(cube, "Copy: SELECT id FROM c. Ungrouped cube JOIN is rejected.")
    assert "JOIN Copy" not in cube["description"]
    assert not apply_cube_description(cube, cube["description"])


def test_apply_member_descriptions_overwrite_named() -> None:
    cube = {
        "dimensions": [{"name": "position", "description": "stale"}],
        "facts": [{"name": "points", "description": ""}],
    }
    assert apply_member_descriptions(
        cube,
        {"position": "championship standing", "points": "race points"},
        overwrite_names={"position"},
    )
    assert cube["dimensions"][0]["description"] == "championship standing"
    assert cube["facts"][0]["description"] == "race points"


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


def test_add_expression_fact_non_additive_skips_existing() -> None:
    cube = {
        "dimensions": [{"name": "id", "column": "id"}],
        "facts": [],
    }
    spec = {
        "name": "eligible_frpm_rate_k12",
        "expression": "(FRPM_Count_K_12) / (Enrollment_K_12)",
        "type": "quantity",
        "additive": False,
        "description": "row rate",
    }
    assert add_expression_fact(cube, spec)
    fact = cube["facts"][-1]
    assert fact["additive"] is False
    assert "column" not in fact
    assert not add_expression_fact(cube, spec)


def test_prune_stale_metric_segment(tmp_path, monkeypatch) -> None:
    import enrich_meta_from_qa as em

    monkeypatch.setattr(em, "ROOT", tmp_path)
    db = "california_schools"
    meta = tmp_path / db / "meta"
    meta.mkdir(parents=True)
    (meta / "eligible_frpm_rate_k12.json").write_text(
        json.dumps({"kind": "metric", "name": "eligible_frpm_rate_k12"}),
        encoding="utf-8",
    )
    (meta / "k12_frpm_eligible_share.json").write_text(
        json.dumps({"kind": "metric", "name": "k12_frpm_eligible_share"}),
        encoding="utf-8",
    )
    (meta / "schools_frpm_count.json").write_text(
        json.dumps({"kind": "metric", "name": "schools_frpm_count"}),
        encoding="utf-8",
    )
    (meta / "schools_frpm.json").write_text(
        json.dumps({"kind": "cube", "name": "schools_frpm"}),
        encoding="utf-8",
    )
    n = prune_stale_metric_segment(db)
    assert n == 1
    assert not (meta / "eligible_frpm_rate_k12.json").exists()
    assert (meta / "k12_frpm_eligible_share.json").exists()
    assert (meta / "schools_frpm_count.json").exists()
    assert (meta / "schools_frpm.json").exists()


def test_d28_promotions_do_not_collide_with_metrics() -> None:
    ca = DOMAIN_SPECS["california_schools"]
    fact_names = {f["name"] for f in ca["expression_facts"]}
    metric_names = {m["name"] for m in ca["metrics"]}
    assert fact_names.isdisjoint(metric_names)
    assert "eligible_frpm_rate_k12" in fact_names
    assert "k12_frpm_eligible_share" in metric_names
    throm = DOMAIN_SPECS["thrombosis_prediction"]["expression_dims"]
    assert any(d["name"] == "age_years" for d in throm)
    tox = DOMAIN_SPECS["toxicology"]["expression_dims"]
    assert any(d["name"] == "is_carcinogenic" for d in tox)


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
        for fact in spec.get("expression_facts") or []:
            assert fact.get("cube")
            if cubes:
                assert fact["cube"] in cubes
            assert fact.get("additive") is False


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


def test_round2_cube_descriptions_copy_from_own_cube():
    """list_tables 라우팅: 각 허브 큐브 description에 Copy SELECT FROM 자신."""
    split = {
        "formula_1",
        "toxicology",
        "california_schools",
        "student_club",
        "thrombosis_prediction",
        "superhero",
    }
    for db_id in split:
        descs = DOMAIN_SPECS[db_id].get("cube_descriptions") or {}
        assert descs, db_id
        for cube, text in descs.items():
            assert f"FROM {cube}" in text, f"{db_id}.{cube} missing FROM {cube}"
            assert "Copy:" in text, f"{db_id}.{cube} missing Copy:"


def test_round2_f1_standings_not_results():
    d = DOMAIN_SPECS["formula_1"]["cube_descriptions"]
    assert "f1_driver_standings" in d["f1_race_results"]
    assert "f1_race_results" in d["f1_driver_standings"]
    assert "f1_constructor_standings" in d["f1_constructor_results"]
    assert "f1_qualifying" in d["f1_race_results"]
    mets = {m["name"]: m for m in DOMAIN_SPECS["formula_1"]["metrics"]}
    assert "FROM f1_driver_standings" in mets["driver_standing_points"]["description"]
    assert mets["driver_standing_points"]["cube"] == "f1_driver_standings"
    assert "FROM f1_constructor_standings" in mets["constructor_standing_points"]["description"]


def test_round2_ident_routing():
    lab = DOMAIN_SPECS["thrombosis_prediction"]["descriptions"]["throm_laboratory"]
    assert "T_CHO" in lab
    assert "CHO" in lab["T_CHO"]
    exam = DOMAIN_SPECS["thrombosis_prediction"]["descriptions"]["throm_examination"]
    assert "KCT" in exam
    ev = DOMAIN_SPECS["student_club"]["cube_descriptions"]["club_event"]
    assert "club_expense" in ev and "cost" in ev
    mem = DOMAIN_SPECS["student_club"]["cube_descriptions"]["club_member"]
    assert "club_income" in mem and "amount" in mem
    tox = DOMAIN_SPECS["toxicology"]["cube_descriptions"]
    assert "tox_atom" in tox["tox_connected"]
    assert "Never" in tox["tox_connected"] or "never" in tox["tox_connected"]


def test_round3_no_join_recipe_in_cube_descriptions():
    """§10 P0: cube description에 JOIN … ON / GROUP BY Copy 없음."""
    split = {
        "formula_1",
        "toxicology",
        "california_schools",
        "student_club",
        "thrombosis_prediction",
        "superhero",
    }
    join_on = re.compile(r"\bJOIN\s+\w+\s+ON\b", re.I)
    for db_id in split:
        for cube, text in (DOMAIN_SPECS[db_id].get("cube_descriptions") or {}).items():
            assert join_on.search(text) is None, f"{db_id}.{cube} JOIN ON recipe"
            assert "GROUP BY" not in text, f"{db_id}.{cube} GROUP BY recipe"
            assert "Grouped JOIN Copy" not in text, f"{db_id}.{cube}"
    ca = DOMAIN_SPECS["california_schools"]["cube_descriptions"]
    assert "schools_sat" in ca["schools_frpm"]
    assert "Ungrouped cube JOIN is rejected" in ca["schools_frpm"]
    assert "Ungrouped cube JOIN is rejected" in ca["schools_sat"]
    lab = DOMAIN_SPECS["thrombosis_prediction"]["cube_descriptions"]["throm_laboratory"]
    assert "throm_examination" in lab
    assert "Ungrouped cube JOIN is rejected" in lab


def test_round3_f1_homonym_search_text():
    """championship/standings는 standings 큐브 멤버, race results는 한 레이스."""
    d = DOMAIN_SPECS["formula_1"]["descriptions"]
    race = d["f1_race_results"]
    drv = d["f1_driver_standings"]
    con = d["f1_constructor_standings"]
    cres = d["f1_constructor_results"]
    assert "this race" in race["position"].lower() or "finishing" in race["position"].lower()
    assert "f1_driver_standings" in race["position"]
    assert "championship" not in race["position"].lower()
    assert "cumulative" not in race["position"].lower()
    assert "this race" in race["points"].lower()
    assert "championship" not in race["points"].lower()
    assert "cumulative" not in race["points"].lower()
    for standing in (drv, con):
        for name in ("position", "points", "wins"):
            text = standing[name].lower()
            assert "championship" in text or "standings" in text or "cumulative" in text, name
    assert "standings" in cres["points"].lower()
    ca_mets = {m["name"]: m["description"] for m in DOMAIN_SPECS["california_schools"]["metrics"]}
    assert "JOIN schools_sat" in ca_mets["sat_excellence_share"]
    assert "GROUP BY" in ca_mets["sat_excellence_share"]
    throm_mets = {
        m["name"]: m["description"]
        for m in DOMAIN_SPECS["thrombosis_prediction"]["metrics"]
    }
    assert "JOIN throm_examination" in throm_mets["avg_acl_igm"]
    assert "GROUP BY" in throm_mets["avg_acl_igm"]


def test_p1_single_cube_tox_club():
    """§10 P1: atom/bond/cost는 한 큐브. connected는 atom_id2 브릿지만. JOIN ON 레시피 없음."""
    tox = DOMAIN_SPECS["toxicology"]["cube_descriptions"]
    atom, bond, conn, mol = (
        tox["tox_atom"],
        tox["tox_bond"],
        tox["tox_connected"],
        tox["tox_molecule"],
    )
    assert "tox_connected" in atom and "tox_bond" in atom
    assert "never" in atom.lower()
    assert "tox_molecule" in bond and "tox_atom" in bond and "tox_connected" in bond
    assert "never" in bond.lower()
    assert "atom_id2" in conn
    assert "tox_atom" in conn and "tox_bond" in conn
    assert "tox_atom" in mol and "tox_bond" in mol
    club = DOMAIN_SPECS["student_club"]["cube_descriptions"]
    exp, ev, att, bud = (
        club["club_expense"],
        club["club_event"],
        club["club_attendance"],
        club["club_budget"],
    )
    assert "club_event" in exp and "club_attendance" in exp
    assert "never" in exp.lower()
    assert "club_expense" in ev
    assert "club_expense" in att
    assert "club_event" in bud and "never" in bud.lower()
    join_on = re.compile(r"\bJOIN\s+\w+\s+ON\b", re.I)
    for text in (atom, bond, conn, mol, exp, ev, att, bud):
        assert join_on.search(text) is None
        assert "GROUP BY" not in text


def test_p1_single_cube_ca_throm():
    """§10 P1: SAT-only는 schools_sat, 혈액은 throm_laboratory. JOIN ON 레시피 없음."""
    ca = DOMAIN_SPECS["california_schools"]["cube_descriptions"]
    frpm, sat = ca["schools_frpm"], ca["schools_sat"]
    assert "schools_sat" in frpm and "never" in frpm.lower()
    assert "schools_frpm" in sat and "never" in sat.lower()
    throm = DOMAIN_SPECS["thrombosis_prediction"]["cube_descriptions"]
    lab, exam, pat = (
        throm["throm_laboratory"],
        throm["throm_examination"],
        throm["throm_patient"],
    )
    assert "throm_examination" in lab and "never" in lab.lower()
    assert "throm_laboratory" in exam and "never" in exam.lower()
    assert "throm_laboratory" in pat and "throm_examination" in pat
    join_on = re.compile(r"\bJOIN\s+\w+\s+ON\b", re.I)
    for text in (frpm, sat, lab, exam, pat):
        assert join_on.search(text) is None
        assert "GROUP BY" not in text


def test_round4_logic_filter_confusion_text():
    """§10 P0 잔여: execute-OK 로직/필터 혼동 쌍 (빈 칸 + GPT≠GLU 등 overwrite)."""
    ca = DOMAIN_SPECS["california_schools"]["descriptions"]
    ca_all = {**(ca.get("_all") or {}), **(ca.get("schools_frpm") or {}), **(ca.get("schools_sat") or {})}
    assert "Phone" in ca_all and "phone" in ca_all["Phone"].lower()
    assert "AvgScrWrite" in ca["schools_sat"]
    assert "Locally funded" in ca_all["FundingType"]
    assert "Merged" in ca_all["StatusType"]
    assert "Academic_Year" in ca["schools_frpm"]
    assert "YEAR" in ca["schools_frpm"]["Academic_Year"]
    assert "SSS" in ca_all["EdOpsCode"]
    f1 = DOMAIN_SPECS["formula_1"]["descriptions"]
    race = {**(f1.get("_all") or {}), **(f1.get("f1_race_results") or {})}
    assert "seasons_url" in race["url"]
    assert "url" in race["seasons_url"]
    assert "fastestLapSpeed" in race
    assert "f1_lap_times" in race["fastestLapSpeed"]
    assert "youngest" in race["dob"].lower() and "DESC" in race["dob"]
    assert "circuits_name" in f1["_all"]["circuitRef"]
    assert "statusId" in race and "2" in race["statusId"]
    lab = DOMAIN_SPECS["thrombosis_prediction"]["descriptions"]["throm_laboratory"]
    exam = DOMAIN_SPECS["thrombosis_prediction"]["descriptions"]["throm_examination"]
    patient = DOMAIN_SPECS["thrombosis_prediction"]["descriptions"]["throm_patient"]
    assert "GLU" in lab["GPT"] and ">=" in lab["GPT"]
    assert "GPT" in lab["GLU"]
    assert ">= 250" in lab["CPK"]
    assert "negative" in lab["SM"] and "ANA_Pattern" in lab["SM"]
    assert "2000" in lab["IGG"]
    assert "CRP" in lab and "'+'" in lab["CRP"]
    assert "ANA_Pattern" in exam and "SM" in exam["ANA_Pattern"]
    assert "APS" in patient["Diagnosis"]
    sh = DOMAIN_SPECS["superhero"]["descriptions"]["_all"]
    assert "skin_colour" in sh["colour"]
    assert "colour" in sh["skin_colour"]
    club = DOMAIN_SPECS["student_club"]["descriptions"]["club_expense"]
    assert "category" in club and "Food" in club["category"]
    segs = {s["name"]: s for s in DOMAIN_SPECS["thrombosis_prediction"]["segments"]}
    assert segs["diagnosis_aps"]["filter"] == "Diagnosis = 'APS'"
    assert segs["diagnosis_aps"]["cube"] == "throm_patient"
    ca_segs = {s["name"]: s for s in DOMAIN_SPECS["california_schools"]["segments"]}
    assert ca_segs["merged_schools"]["filter"] == "StatusType = 'Merged'"
    assert ca_segs["locally_funded"]["filter"] == "FundingType = 'Locally funded'"


def test_round4_confusion_force_overwrites():
    assert {"GPT", "GLU", "CPK", "colour", "skin_colour", "circuitRef", "EdOpsCode", "Diagnosis"} <= _CONFUSION_FORCE
    cube = {
        "dimensions": [
            {"name": "GPT", "description": "stale GPT"},
            {"name": "colour", "description": "stale colour"},
        ],
        "facts": [],
    }
    texts = {"GPT": "ALT GPT not GLU", "colour": "eye not skin_colour"}
    assert apply_member_descriptions(cube, texts, overwrite_names=_CONFUSION_FORCE)
    assert cube["dimensions"][0]["description"] == "ALT GPT not GLU"
    assert cube["dimensions"][1]["description"] == "eye not skin_colour"


def test_fail_plan_copy_keeps_cube_descriptions_free_of_join_recipes():
    """fail_plan: alias/no-sql Copy는 큐브 description. GROUP BY JOIN은 metric만."""
    join_on = re.compile(r"\bJOIN\s+\w+\s+ON\b", re.I)
    for db_id in (
        "formula_1",
        "toxicology",
        "california_schools",
        "student_club",
        "thrombosis_prediction",
        "superhero",
    ):
        for cube, text in DOMAIN_SPECS[db_id]["cube_descriptions"].items():
            assert "not T1" in text, f"{db_id}.{cube}"
            assert join_on.search(text) is None, cube
            assert "GROUP BY" not in text, cube
    tox = {m["name"]: m["description"] for m in DOMAIN_SPECS["toxicology"]["metrics"]}
    assert "GROUP BY" in tox["carbon_atom_pct"]
    assert "tox_bond" in tox["carbon_atom_pct"]
    club = {m["name"]: m["description"] for m in DOMAIN_SPECS["student_club"]["metrics"]}
    assert "event_name" in club["total_expense_cost"]
    assert "not T1" in club["total_expense_cost"]


def test_clauses_for_members_skips_person_names_and_filled_text():
    from enrich_meta_from_qa import apply_evidence_clauses, clauses_for_members

    evidence = (
        "Bruno Senna refers to the full name of the driver; "
        "driver reference name refers to driverRef; "
        "full name refers to forename and surname"
    )
    got = clauses_for_members(evidence, {"driverRef", "forename", "surname", "id", "name"})
    assert "driverRef" in got["driverRef"]
    assert "forename" in got and "surname" in got
    assert "id" not in got
    assert "Bruno" not in " ".join(got.values())
    cube = {
        "dimensions": [
            {"name": "driverRef", "description": ""},
            {"name": "forename", "description": "kept"},
        ],
        "facts": [],
    }
    assert apply_evidence_clauses(cube, evidence)
    assert "driverRef" in cube["dimensions"][0]["description"]
    assert cube["dimensions"][1]["description"] == "kept"
    assert not apply_evidence_clauses(cube, evidence)


def test_superhero_self_interest_pct_is_metric_on_superhero_cube():
    facts = DOMAIN_SPECS["superhero"].get("expression_facts") or []
    assert all(f.get("name") != "self_interest_pct" for f in facts)
    mets = {m["name"]: m for m in DOMAIN_SPECS["superhero"]["metrics"]}
    m = mets["self_interest_pct"]
    assert m["cube"] == "superhero"
    expr = m["expression"]
    assert "SUM(" in expr and "IIF(alignment = 'Bad'" in expr
    assert "COUNT(hero_id)" in expr
    desc = m["description"]
    assert "Q743" not in desc
    assert "SUM(" not in desc and "COUNT(" not in desc
    for needle in ("self-interest", "Bad alignment", "Percentage", "moral"):
        assert needle in desc, needle


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
