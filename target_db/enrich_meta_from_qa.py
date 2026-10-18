#!/usr/bin/env python3
"""BIRD Q-A evidence → domain metric/segment JSON (semantic-from-qa 큐레이션).

generate_meta.py가 만든 table/view/cube/*_count 골격 위에, mini_dev_sqlite.json
evidence에 이름이 붙은 공식·코호트만 승격한다(질문 1건 = 문서 1개 금지).

예:
  python3 target_db/enrich_meta_from_qa.py
  python3 target_db/enrich_meta_from_qa.py --db financial
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
QA_PATH = ROOT / "bird_qa" / "mini_dev_sqlite.json"


def load_cube(db_id: str) -> dict:
    path = ROOT / db_id / "meta" / f"{db_id}.json"
    if not path.is_file():
        raise FileNotFoundError(f"missing cube {path}")
    cube = json.loads(path.read_text(encoding="utf-8"))
    if cube.get("kind") != "cube":
        raise ValueError(f"{path} is not a cube")
    return cube


def group_by_columns(cube: dict) -> list[str]:
    return [d["column"] for d in cube.get("dimensions") or [] if d.get("column")]


def member_names(cube: dict) -> set[str]:
    names = {cube["name"]}
    for m in (cube.get("dimensions") or []) + (cube.get("facts") or []):
        names.add(m["name"])
    return names


def make_metric(
    *,
    name: str,
    description: str,
    cube: dict,
    expression: str,
    filter_expr: str | None = None,
) -> dict:
    gb = group_by_columns(cube)
    if not gb:
        raise ValueError(f"cube {cube['name']} has no column dimensions for group_by")
    return {
        "kind": "metric",
        "name": name,
        "description": description,
        "cube": cube["name"],
        "expression": expression,
        "aggregation": "group",
        "group_by": gb,
        "filter": {"expression": filter_expr} if filter_expr else None,
        "role": "all",
    }


def make_segment(
    *, name: str, description: str, cube: dict, filter_expr: str
) -> dict:
    return {
        "kind": "segment",
        "name": name,
        "description": description,
        "cube": cube["name"],
        "filter": {"expression": filter_expr},
        "role": "all",
    }


# evidence에 반복·이름이 붙은 공식/코호트만. SQL gold 복붙 금지.
DOMAIN_SPECS: dict[str, dict[str, list[dict]]] = {
    "debit_card_specializing": {
        "segments": [
            {
                "name": "pay_eur",
                "description": "Customers paying in EUR (evidence: Currency = 'EUR').",
                "filter": "Currency = 'EUR'",
            },
            {
                "name": "pay_czk",
                "description": "Customers paying in CZK (evidence: Currency = 'CZK').",
                "filter": "Currency = 'CZK'",
            },
            {
                "name": "gasstation_cze",
                "description": "Gas stations in Czech Republic (Country = 'CZE').",
                "filter": "Country = 'CZE'",
            },
            {
                "name": "gasstation_svk",
                "description": "Gas stations in Slovakia (Country = 'SVK').",
                "filter": "Country = 'SVK'",
            },
            {
                "name": "segment_lam",
                "description": "LAM customer segment (Segment = 'LAM').",
                "filter": "Segment = 'LAM'",
            },
        ],
        "metrics": [
            {
                "name": "eur_to_czk_customer_ratio",
                "description": (
                    "Ratio of EUR-paying to CZK-paying customers "
                    "(count Currency='EUR' / count Currency='CZK')."
                ),
                "expression": (
                    "DIVIDE(SUM(IIF(Currency = 'EUR', 1, 0)), "
                    "SUM(IIF(Currency = 'CZK', 1, 0)))"
                ),
            },
            {
                "name": "avg_consumption",
                "description": "Average consumption (AVG(Consumption)).",
                "expression": "AVG(Consumption)",
            },
            {
                "name": "avg_price_per_item",
                "description": "Average price per single item (AVG(Price)).",
                "expression": "AVG(Price)",
            },
        ],
    },
    "financial": {
        "segments": [
            {
                "name": "male_clients",
                "description": "Male clients (gender = 'M').",
                "filter": "gender = 'M'",
            },
            {
                "name": "female_clients",
                "description": "Female clients (gender = 'F').",
                "filter": "gender = 'F'",
            },
            {
                "name": "loan_finished_ok",
                "description": "Loan contract finished with no problems (status = 'A').",
                "filter": "status = 'A'",
            },
            {
                "name": "loan_running_ok",
                "description": "Running loan contract ok so far (status = 'C').",
                "filter": "status = 'C'",
            },
            {
                "name": "account_owner",
                "description": "Disposition type OWNER (eligible for loans).",
                "filter": "type = 'OWNER'",
            },
            {
                "name": "credit_card_withdrawal",
                "description": "Credit card withdrawal (operation = 'VYBER KARTOU').",
                "filter": "operation = 'VYBER KARTOU'",
            },
            {
                "name": "non_credit_withdraw",
                "description": "Non-credit-card withdraw (trans_type = 'VYDAJ').",
                "filter": "trans_type = 'VYDAJ'",
            },
        ],
        "metrics": [
            {
                "name": "male_client_pct",
                "description": (
                    "Percentage of male clients "
                    "(DIVIDE(COUNT male, COUNT clients) * 100)."
                ),
                "expression": (
                    "DIVIDE(SUM(IIF(gender = 'M', 1, 0)) * 100, COUNT(client_id))"
                ),
            },
            {
                "name": "loan_finished_ok_amount_share",
                "description": (
                    "Share of loan amount with status A "
                    "(finished, no problems) over total loan amount."
                ),
                "expression": (
                    "DIVIDE(SUM(IIF(status = 'A', amount, 0)) * 100, SUM(amount))"
                ),
            },
            {
                "name": "avg_salary",
                "description": "Average salary (A11).",
                "expression": "AVG(A11)",
            },
        ],
    },
    "superhero": {
        "segments": [
            {
                "name": "marvel_comics",
                "description": "Heroes published by Marvel Comics.",
                "filter": "publisher_name = 'Marvel Comics'",
            },
            {
                "name": "dc_comics",
                "description": "Heroes published by DC Comics.",
                "filter": "publisher_name = 'DC Comics'",
            },
            {
                "name": "female_heroes",
                "description": "Female superheroes (gender = 'Female').",
                "filter": "gender = 'Female'",
            },
            {
                "name": "male_heroes",
                "description": "Male superheroes (gender = 'Male').",
                "filter": "gender = 'Male'",
            },
            {
                "name": "bad_alignment",
                "description": "Bad alignment heroes (alignment = 'Bad').",
                "filter": "alignment = 'Bad'",
            },
            {
                "name": "blue_eye_colour",
                "description": "Blue eye colour (colour = 'Blue').",
                "filter": "colour = 'Blue'",
            },
        ],
        "metrics": [
            {
                "name": "avg_weight_kg",
                "description": "Average hero weight in kg (AVG(weight_kg)).",
                "expression": "AVG(weight_kg)",
            },
            {
                "name": "avg_height_cm",
                "description": "Average hero height in cm (AVG(height_cm)).",
                "expression": "AVG(height_cm)",
            },
            {
                "name": "marvel_female_pct",
                "description": (
                    "Percent female among Marvel Comics heroes "
                    "(evidence percentage formula)."
                ),
                "expression": (
                    "DIVIDE(SUM(IIF(publisher_name = 'Marvel Comics' "
                    "AND gender = 'Female', 1, 0)) * 100, "
                    "SUM(IIF(publisher_name = 'Marvel Comics', 1, 0)))"
                ),
            },
            {
                "name": "marvel_minus_dc_count",
                "description": (
                    "Marvel Comics hero count minus DC Comics hero count."
                ),
                "expression": (
                    "SUM(IIF(publisher_name = 'Marvel Comics', 1, 0)) - "
                    "SUM(IIF(publisher_name = 'DC Comics', 1, 0))"
                ),
            },
        ],
    },
    "toxicology": {
        "segments": [
            {
                "name": "carcinogenic",
                "description": "Carcinogenic molecules (label = '+').",
                "filter": "label = '+'",
            },
            {
                "name": "non_carcinogenic",
                "description": "Non-carcinogenic molecules (label = '-').",
                "filter": "label = '-'",
            },
            {
                "name": "single_bond",
                "description": "Single-bonded (bond_type = '-').",
                "filter": "bond_type = '-'",
            },
            {
                "name": "double_bond",
                "description": "Double-bonded (bond_type = '=').",
                "filter": "bond_type = '='",
            },
            {
                "name": "triple_bond",
                "description": "Triple-bonded (bond_type = '#').",
                "filter": "bond_type = '#'",
            },
            {
                "name": "carbon_atom",
                "description": "Carbon atoms (element = 'c').",
                "filter": "element = 'c'",
            },
            {
                "name": "oxygen_atom",
                "description": "Oxygen atoms (element = 'o').",
                "filter": "element = 'o'",
            },
        ],
        "metrics": [
            {
                "name": "carcinogenic_molecule_pct",
                "description": "Percentage of carcinogenic molecules (label = '+').",
                "expression": (
                    "DIVIDE(SUM(IIF(label = '+', 1, 0)) * 100, COUNT(molecule_id))"
                ),
            },
            {
                "name": "single_bond_share",
                "description": "Share of single bonds among bonds (bond_type = '-').",
                "expression": (
                    "DIVIDE(SUM(IIF(bond_type = '-', 1, 0)), COUNT(bond_id))"
                ),
            },
            {
                "name": "carbon_atom_pct",
                "description": "Percentage of carbon atoms (element = 'c').",
                "expression": (
                    "DIVIDE(SUM(IIF(element = 'c', 1, 0)) * 100, COUNT(atom_id))"
                ),
            },
        ],
    },
    "thrombosis_prediction": {
        "segments": [
            {
                "name": "male_patients",
                "description": "Male patients (SEX = 'M').",
                "filter": "SEX = 'M'",
            },
            {
                "name": "female_patients",
                "description": "Female patients (SEX = 'F').",
                "filter": "SEX = 'F'",
            },
            {
                "name": "inpatient",
                "description": "In-patients (Admission = '+').",
                "filter": "Admission = '+'",
            },
            {
                "name": "outpatient",
                "description": "Outpatients (Admission = '-').",
                "filter": "Admission = '-'",
            },
            {
                "name": "diagnosis_sle",
                "description": "SLE diagnosed patients (Diagnosis = 'SLE').",
                "filter": "Diagnosis = 'SLE'",
            },
            {
                "name": "thrombosis_level_2",
                "description": "Thrombosis level 2 (Thrombosis = 2).",
                "filter": "Thrombosis = 2",
            },
        ],
        "metrics": [
            {
                "name": "male_inpatient_pct",
                "description": (
                    "Percentage of male in-patients among patients "
                    "(SEX='M' and Admission='+')."
                ),
                "expression": (
                    "DIVIDE(SUM(IIF(SEX = 'M' AND Admission = '+', 1, 0)) * 100, "
                    "COUNT(ID))"
                ),
            },
            {
                "name": "avg_acl_igm",
                "description": "Average anti-Cardiolipin antibody IgM (AVG(aCL_IgM)).",
                "expression": "AVG(aCL_IgM)",
            },
        ],
    },
    "california_schools": {
        "segments": [
            {
                "name": "exclusively_virtual",
                "description": "Exclusively virtual schools (Virtual = 'F').",
                "filter": "Virtual = 'F'",
            },
        ],
        "metrics": [
            {
                "name": "excellence_rate",
                "description": "Excellence rate = NumGE1500 / NumTstTakr.",
                "expression": "DIVIDE(SUM(NumGE1500), SUM(NumTstTakr))",
            },
            {
                "name": "eligible_free_rate_k12",
                "description": (
                    "Eligible free rate for K-12 = "
                    "Free_Meal_Count_K_12 / Enrollment_K_12."
                ),
                "expression": (
                    "DIVIDE(SUM(Free_Meal_Count_K_12), SUM(Enrollment_K_12))"
                ),
            },
            {
                "name": "eligible_frpm_rate_k12",
                "description": (
                    "Eligible free or reduced price meal rate for K-12 = "
                    "FRPM_Count_K_12 / Enrollment_K_12."
                ),
                "expression": "DIVIDE(SUM(FRPM_Count_K_12), SUM(Enrollment_K_12))",
            },
            {
                "name": "avg_math_score",
                "description": "Average of average math scores (AVG(AvgScrMath)).",
                "expression": "AVG(AvgScrMath)",
            },
        ],
    },
    "card_games": {
        "segments": [
            {
                "name": "borderless_cards",
                "description": "Borderless cards (borderColor = 'borderless').",
                "filter": "borderColor = 'borderless'",
            },
            {
                "name": "promo_cards",
                "description": "Promotional printing (isPromo = 1).",
                "filter": "isPromo = 1",
            },
            {
                "name": "story_spotlight",
                "description": "Story Spotlight cards (isStorySpotlight = 1).",
                "filter": "isStorySpotlight = 1",
            },
            {
                "name": "textless_cards",
                "description": "Cards without a text box (isTextless = 1).",
                "filter": "isTextless = 1",
            },
            {
                "name": "no_content_warning",
                "description": "Cards without content warning (hasContentWarning = 0).",
                "filter": "hasContentWarning = 0",
            },
            {
                "name": "commander_legal",
                "description": "Commander format legality (format = 'commander').",
                "filter": "format = 'commander'",
            },
            {
                "name": "french_language",
                "description": "French language printings (language = 'French').",
                "filter": "language = 'French'",
            },
            {
                "name": "chinese_simplified",
                "description": (
                    "Chinese Simplified language (language = 'Chinese Simplified')."
                ),
                "filter": "language = 'Chinese Simplified'",
            },
        ],
        "metrics": [
            {
                "name": "borderless_pct",
                "description": (
                    "Percentage of borderless cards "
                    "(DIVIDE(count borderless, count id) * 100)."
                ),
                "expression": (
                    "DIVIDE(SUM(IIF(borderColor = 'borderless', 1, 0)) * 100, "
                    "COUNT(id))"
                ),
            },
            {
                "name": "chinese_simplified_pct",
                "description": (
                    "Percentage of Chinese Simplified printings among cards."
                ),
                "expression": (
                    "DIVIDE(SUM(IIF(language = 'Chinese Simplified', 1, 0)) * 100, "
                    "COUNT(id))"
                ),
            },
            {
                "name": "promo_pct",
                "description": "Percentage of promotional cards (isPromo = 1).",
                "expression": (
                    "DIVIDE(SUM(IIF(isPromo = 1, 1, 0)) * 100, COUNT(id))"
                ),
            },
        ],
    },
    "codebase_community": {
        "segments": [
            {
                "name": "elder_users",
                "description": "Elder users (Age > 65).",
                "filter": "Age > 65",
            },
            {
                "name": "high_score_posts",
                "description": "Posts with score over 5 (Score > 5).",
                "filter": "Score > 5",
            },
            {
                "name": "closed_posts",
                "description": "Closed / not well-finished posts (ClosedDate IS NOT NULL).",
                "filter": "ClosedDate IS NOT NULL",
            },
            {
                "name": "student_badges",
                "description": "Student badges (Name = 'Student' on badges).",
                "filter": "Name = 'Student'",
            },
        ],
        "metrics": [
            {
                "name": "elder_user_pct",
                "description": "Percentage of elder users (Age > 65).",
                "expression": (
                    "DIVIDE(SUM(IIF(Age > 65, 1, 0)) * 100, COUNT(Id))"
                ),
            },
            {
                "name": "avg_view_count",
                "description": "Average post view count (AVG(ViewCount)).",
                "expression": "AVG(ViewCount)",
            },
            {
                "name": "avg_upvotes",
                "description": "Average up votes (AVG(UpVotes)).",
                "expression": "AVG(UpVotes)",
            },
            {
                "name": "avg_user_age",
                "description": "Average user age (AVG(Age)).",
                "expression": "AVG(Age)",
            },
        ],
    },
    "european_football_2": {
        "segments": [
            {
                "name": "left_foot_players",
                "description": "Players who prefer left foot (preferred_foot = 'left').",
                "filter": "preferred_foot = 'left'",
            },
            {
                "name": "season_2015_2016",
                "description": "2016 season matches (season = '2015/2016').",
                "filter": "season = '2015/2016'",
            },
            {
                "name": "tall_players",
                "description": "Players taller than 180 cm (height > 180).",
                "filter": "height > 180",
            },
        ],
        "metrics": [
            {
                "name": "left_foot_pct",
                "description": "Percentage of players who prefer left foot.",
                "expression": (
                    "DIVIDE(SUM(IIF(preferred_foot = 'left', 1, 0)) * 100, "
                    "COUNT(player_api_id))"
                ),
            },
            {
                "name": "avg_long_shots",
                "description": "Average long shots (AVG(long_shots)).",
                "expression": "AVG(long_shots)",
            },
            {
                "name": "avg_heading_accuracy",
                "description": "Average heading accuracy (AVG(heading_accuracy)).",
                "expression": "AVG(heading_accuracy)",
            },
            {
                "name": "avg_home_team_goal",
                "description": "Average home team goals (AVG(home_team_goal)).",
                "expression": "AVG(home_team_goal)",
            },
            {
                "name": "total_match_goals",
                "description": "Total goals = SUM(home_team_goal + away_team_goal).",
                "expression": "SUM(home_team_goal + away_team_goal)",
            },
            {
                "name": "avg_overall_rating",
                "description": "Average overall rating (AVG(overall_rating)).",
                "expression": "AVG(overall_rating)",
            },
        ],
    },
    "formula_1": {
        "segments": [
            {
                "name": "japanese_constructors",
                "description": "Japanese constructors (nationality = 'Japanese').",
                "filter": "nationality = 'Japanese'",
            },
            {
                "name": "since_2010",
                "description": "Races/results since 2010 (year >= 2010).",
                "filter": "year >= 2010",
            },
            {
                "name": "race_completed",
                "description": "Race completion (results_time IS NOT NULL).",
                "filter": "results_time IS NOT NULL",
            },
        ],
        "metrics": [
            {
                "name": "avg_lap_time_ms",
                "description": "Average lap time in milliseconds (AVG(milliseconds)).",
                "expression": "AVG(milliseconds)",
            },
            {
                "name": "race_completion_pct",
                "description": (
                    "Percentage of race completions "
                    "(results_time IS NOT NULL) among results."
                ),
                "expression": (
                    "DIVIDE(SUM(IIF(results_time IS NOT NULL, 1, 0)) * 100, "
                    "COUNT(resultId))"
                ),
            },
            {
                "name": "avg_points",
                "description": "Average constructor/result points (AVG(points)).",
                "expression": "AVG(points)",
            },
        ],
    },
    "student_club": {
        "segments": [
            {
                "name": "meeting_events",
                "description": "Meeting events (type = 'Meeting').",
                "filter": "type = 'Meeting'",
            },
            {
                "name": "fundraising_income",
                "description": "Fundraising income (source = 'Fundraising').",
                "filter": "source = 'Fundraising'",
            },
            {
                "name": "food_budget",
                "description": "Food budget category (category = 'Food').",
                "filter": "category = 'Food'",
            },
            {
                "name": "closed_events",
                "description": "Closed events (event_status = 'Closed').",
                "filter": "event_status = 'Closed'",
            },
            {
                "name": "pizza_expense",
                "description": "Pizza expenses (expense_description = 'Pizza').",
                "filter": "expense_description = 'Pizza'",
            },
        ],
        "metrics": [
            {
                "name": "pizza_expense_total",
                "description": "Total pizza expense cost (SUM(cost) for Pizza).",
                "expression": (
                    "SUM(IIF(expense_description = 'Pizza', cost, 0))"
                ),
            },
            {
                "name": "spend_to_budget_ratio",
                "description": "Spend-to-budget ratio (DIVIDE(SUM(spent), SUM(amount))).",
                "expression": "DIVIDE(SUM(spent), SUM(amount))",
            },
            {
                "name": "food_spent_total",
                "description": "Total spent on Food category (SUM spent where Food).",
                "expression": "SUM(IIF(category = 'Food', spent, 0))",
            },
            {
                "name": "total_expense_cost",
                "description": "Total expense cost (SUM(cost)).",
                "expression": "SUM(cost)",
            },
        ],
    },
}


def evidence_mentions(db_id: str) -> list[str]:
    """Load evidence strings for a db (for tests / audit)."""
    if not QA_PATH.is_file():
        return []
    data = json.loads(QA_PATH.read_text(encoding="utf-8"))
    return [
        (row.get("evidence") or "")
        for row in data
        if row.get("db_id") == db_id and row.get("evidence")
    ]


def build_docs(db_id: str, cube: dict | None = None) -> list[dict]:
    spec = DOMAIN_SPECS.get(db_id)
    if not spec:
        return []
    cube = cube or load_cube(db_id)
    reserved = member_names(cube)
    # also reserve existing meta metric/segment names
    meta_dir = ROOT / db_id / "meta"
    for path in meta_dir.glob("*.json"):
        doc = json.loads(path.read_text(encoding="utf-8"))
        if doc.get("kind") in ("metric", "segment", "table", "view", "cube"):
            reserved.add(doc["name"])

    docs: list[dict] = []
    for seg in spec.get("segments") or []:
        name = seg["name"]
        if name in reserved:
            raise ValueError(f"{db_id}: segment name conflicts: {name}")
        docs.append(
            make_segment(
                name=name,
                description=seg["description"],
                cube=cube,
                filter_expr=seg["filter"],
            )
        )
        reserved.add(name)
    for met in spec.get("metrics") or []:
        name = met["name"]
        if name in reserved:
            raise ValueError(f"{db_id}: metric name conflicts: {name}")
        docs.append(
            make_metric(
                name=name,
                description=met["description"],
                cube=cube,
                expression=met["expression"],
                filter_expr=met.get("filter"),
            )
        )
        reserved.add(name)
    return docs


def write_docs(db_id: str, docs: list[dict]) -> int:
    meta_dir = ROOT / db_id / "meta"
    meta_dir.mkdir(exist_ok=True)
    for doc in docs:
        path = meta_dir / f"{doc['name']}.json"
        path.write_text(
            json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
    return len(docs)


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--db", help="only this db_id under target_db/")
    args = p.parse_args()
    dbs = [args.db] if args.db else sorted(DOMAIN_SPECS)
    total = 0
    for db_id in dbs:
        if db_id not in DOMAIN_SPECS:
            print(f"skip unknown db {db_id}")
            continue
        docs = build_docs(db_id)
        n = write_docs(db_id, docs)
        seg_n = sum(1 for d in docs if d["kind"] == "segment")
        met_n = sum(1 for d in docs if d["kind"] == "metric")
        print(f"{db_id}: wrote {seg_n} segments, {met_n} metrics")
        total += n
    print(f"done: {total} documents")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
