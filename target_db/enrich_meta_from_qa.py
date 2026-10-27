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
import re
from pathlib import Path

from generate_meta import default_group_by, rewrite_divide

ROOT = Path(__file__).resolve().parent
QA_PATH = ROOT / "bird_qa" / "mini_dev_sqlite.json"


def list_cubes(db_id: str) -> dict[str, dict]:
    meta_dir = ROOT / db_id / "meta"
    out: dict[str, dict] = {}
    if not meta_dir.is_dir():
        return out
    for path in meta_dir.glob("*.json"):
        doc = json.loads(path.read_text(encoding="utf-8"))
        if doc.get("kind") == "cube":
            out[doc["name"]] = doc
    return out


def load_cube(db_id: str, cube_name: str | None = None) -> dict:
    cubes = list_cubes(db_id)
    if cube_name:
        if cube_name not in cubes:
            raise FileNotFoundError(f"missing cube {db_id}/{cube_name}")
        return cubes[cube_name]
    if db_id in cubes:
        return cubes[db_id]
    if len(cubes) == 1:
        return next(iter(cubes.values()))
    raise FileNotFoundError(
        f"missing default cube {db_id} (have {sorted(cubes)})"
    )


def write_cube(db_id: str, cube: dict) -> None:
    path = ROOT / db_id / "meta" / f"{cube['name']}.json"
    path.write_text(
        json.dumps(cube, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )


def apply_member_descriptions(
    cube: dict,
    descriptions: dict[str, str],
    overwrite_names: set[str] | None = None,
) -> bool:
    """Fill empty dim/fact descriptions. `overwrite_names`는 비어 있지 않아도 덮어쓴다."""
    force = overwrite_names or set()
    changed = False
    for member in (cube.get("dimensions") or []) + (cube.get("facts") or []):
        name = member.get("name")
        text = descriptions.get(name)
        if not text:
            continue
        current = (member.get("description") or "").strip()
        if current and name not in force:
            continue
        if current == text:
            continue
        member["description"] = text
        changed = True
    return changed


def apply_cube_description(cube: dict, text: str | None) -> bool:
    """큐브 description은 spec 정본 — 비어 있지 않아도 덮어쓴다(R3 JOIN 철회)."""
    if not text:
        return False
    if (cube.get("description") or "") == text:
        return False
    cube["description"] = text
    return True


def add_expression_dim(cube: dict, spec: dict) -> bool:
    """Append an expression dimension if the name is free."""
    name = spec["name"]
    existing = {m["name"] for m in (cube.get("dimensions") or [])}
    existing |= {m["name"] for m in (cube.get("facts") or [])}
    if name in existing:
        return False
    cube.setdefault("dimensions", []).append(
        {
            "name": name,
            "description": spec.get("description") or "",
            "type": spec.get("type") or "date",
            "expression": spec["expression"],
            "encryption": False,
            "role": "all",
        }
    )
    return True


def add_expression_fact(cube: dict, spec: dict) -> bool:
    """Append a scalar expression fact if the name is free (D28)."""
    name = spec["name"]
    existing = {m["name"] for m in (cube.get("dimensions") or [])}
    existing |= {m["name"] for m in (cube.get("facts") or [])}
    if name in existing:
        return False
    fact = {
        "name": name,
        "description": spec.get("description") or "",
        "type": spec.get("type") or "quantity",
        "expression": spec["expression"],
        "encryption": False,
        "role": "all",
    }
    if spec.get("additive") is False:
        fact["additive"] = False
    cube.setdefault("facts", []).append(fact)
    return True


_HOMONYM_FORCE = {"position", "points", "wins"}
_CONFUSION_FORCE = {
    "GPT",
    "GLU",
    "CPK",
    "colour",
    "skin_colour",
    "circuitRef",
    "EdOpsCode",
    "Diagnosis",
}


def apply_knowledge(db_id: str) -> int:
    """Patch cube description (overwrite) / dim-fact text / expression dims and facts."""
    spec = DOMAIN_SPECS.get(db_id) or {}
    cubes = list_cubes(db_id)
    if not cubes:
        return 0
    desc_by_cube: dict[str, dict[str, str]] = spec.get("descriptions") or {}
    shared = desc_by_cube.get("_all") or {}
    cube_descs: dict[str, str] = spec.get("cube_descriptions") or {}
    expr_dims: list[dict] = spec.get("expression_dims") or []
    expr_facts: list[dict] = spec.get("expression_facts") or []
    n = 0
    for cname, cube in cubes.items():
        changed = False
        merged = dict(shared)
        merged.update(desc_by_cube.get(cname) or {})
        if apply_member_descriptions(cube, merged):
            changed = True
        force = _HOMONYM_FORCE | _CONFUSION_FORCE
        if apply_member_descriptions(
            cube,
            {k: merged[k] for k in force if k in merged},
            overwrite_names=force,
        ):
            changed = True
        if apply_cube_description(cube, cube_descs.get(cname)):
            changed = True
        for dim in expr_dims:
            if dim.get("cube") not in (None, cname):
                continue
            if dim.get("cube") is None and len(cubes) > 1:
                continue
            if add_expression_dim(cube, dim):
                changed = True
        for fact in expr_facts:
            if fact.get("cube") not in (None, cname):
                continue
            if fact.get("cube") is None and len(cubes) > 1:
                continue
            if add_expression_fact(cube, fact):
                changed = True
        if changed:
            write_cube(db_id, cube)
            n += 1
    return n


def group_by_columns(cube: dict) -> list[str]:
    """D24 default grain — not every dimension column."""
    return default_group_by(cube.get("dimensions") or [])


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
        "description": rewrite_divide(description),
        "cube": cube["name"],
        "expression": rewrite_divide(expression),
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
                    "(SUM(IIF(Currency = 'EUR', 1, 0))) / ("
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
                    "((COUNT male) / (COUNT clients) * 100)."
                ),
                "expression": (
                    "(SUM(IIF(gender = 'M', 1, 0)) * 100) / (COUNT(client_id))"
                ),
            },
            {
                "name": "loan_finished_ok_amount_share",
                "description": (
                    "Share of loan amount with status A "
                    "(finished, no problems) over total loan amount."
                ),
                "expression": (
                    "(SUM(IIF(status = 'A', amount, 0)) * 100) / (SUM(amount))"
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
        "cube_descriptions": {
            "superhero": (
                "One row per superhero. Copy: SELECT superhero_name, colour, gender FROM superhero. "
                "Never FROM superhero for Intelligence/Strength scores (use hero_attribute) or power lists (hero_power). "
                "Eye colour is `colour` (not `color`); hair/skin are `hair_colour` / `skin_colour`."
            ),
            "hero_attribute": (
                "One row per hero-attribute. Copy: SELECT superhero_name, attribute_name, attribute_value FROM hero_attribute. "
                "Dumbest/strongest = MIN/MAX(attribute_value) "
                "filtered by attribute_name (Intelligence, Strength, Durability, Speed; match case)."
            ),
            "hero_power": (
                "One row per hero-power. Copy: SELECT superhero_name, power_name FROM hero_power. "
                "Filter power_name (e.g. Agility); do not join hero_attribute in this cube."
            ),
        },
        "descriptions": {
            "_all": {
                "superhero_name": "Hero display name (not full_name).",
                "full_name": "Legal/full name when present; prefer superhero_name for 'which superhero'.",
                "publisher_name": "Publisher literal (Marvel Comics, DC Comics, Dark Horse Comics).",
                "alignment": "Alignment label. Bad = self-interest / villainous.",
                "gender": "Gender label (Female / Male).",
                "race": "Race label; human is lowercase 'human'.",
                "colour": (
                    "Eye colour (join on eye_colour_id). Blue eyes = colour = 'Blue'. "
                    "Never a column named color. Blue skin is skin_colour, not this."
                ),
                "hair_colour": "Hair colour (join on hair_colour_id). Not eye/skin.",
                "skin_colour": (
                    "Skin colour (join on skin_colour_id). Blue skin = skin_colour = 'Blue'. "
                    "Not eye colour (`colour`) and not hair_colour."
                ),
                "height_cm": "Height in centimetres.",
                "weight_kg": "Weight in kilograms.",
            },
            "hero_attribute": {
                "attribute_name": (
                    "Attribute label: Intelligence, Strength, Durability, Speed. "
                    "Match the evidence spelling/case (Intelligence vs intelligence)."
                ),
                "attribute_value": (
                    "Numeric attribute score. Dumbest = MIN where attribute_name = Intelligence; "
                    "strongest = MAX where attribute_name = Strength."
                ),
            },
            "hero_power": {
                "power_name": "Power label (e.g. Agility). One hero may have many powers.",
            },
        },
        "segments": [
            {
                "name": "marvel_comics",
                "description": "Heroes published by Marvel Comics.",
                "filter": "publisher_name = 'Marvel Comics'",
                "cube": "superhero",
            },
            {
                "name": "dc_comics",
                "description": "Heroes published by DC Comics.",
                "filter": "publisher_name = 'DC Comics'",
                "cube": "superhero",
            },
            {
                "name": "female_heroes",
                "description": "Female superheroes (gender = 'Female').",
                "filter": "gender = 'Female'",
                "cube": "superhero",
            },
            {
                "name": "male_heroes",
                "description": "Male superheroes (gender = 'Male').",
                "filter": "gender = 'Male'",
                "cube": "superhero",
            },
            {
                "name": "bad_alignment",
                "description": "Bad alignment heroes (alignment = 'Bad' = self-interest).",
                "filter": "alignment = 'Bad'",
                "cube": "superhero",
            },
            {
                "name": "blue_eye_colour",
                "description": "Blue eye colour (`colour` = 'Blue', not hair_colour).",
                "filter": "colour = 'Blue'",
                "cube": "superhero",
            },
            {
                "name": "attr_intelligence",
                "description": "Intelligence attribute rows (attribute_name = 'Intelligence').",
                "filter": "attribute_name = 'Intelligence'",
                "cube": "hero_attribute",
            },
            {
                "name": "attr_strength",
                "description": "Strength attribute rows (attribute_name = 'Strength').",
                "filter": "attribute_name = 'Strength'",
                "cube": "hero_attribute",
            },
        ],
        "metrics": [
            {
                "name": "avg_weight_kg",
                "description": "Average hero weight in kg (AVG(weight_kg)).",
                "expression": "AVG(weight_kg)",
                "cube": "superhero",
            },
            {
                "name": "avg_height_cm",
                "description": "Average hero height in cm (AVG(height_cm)).",
                "expression": "AVG(height_cm)",
                "cube": "superhero",
            },
            {
                "name": "marvel_female_pct",
                "description": (
                    "Percent female among Marvel Comics heroes "
                    "(evidence percentage formula)."
                ),
                "expression": (
                    "(SUM(IIF(publisher_name = 'Marvel Comics' "
                    "AND gender = 'Female', 1, 0)) * 100) / ("
                    "SUM(IIF(publisher_name = 'Marvel Comics', 1, 0)))"
                ),
                "cube": "superhero",
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
                "cube": "superhero",
            },
        ],
    },
    "toxicology": {
        "cube_descriptions": {
            "tox_molecule": (
                "One row per molecule. Copy: SELECT molecule_id, label FROM tox_molecule. "
                "Use for label ('+' carcinogenic / '-' non-carcinogenic) only. "
                "Never JOIN tox_atom or tox_bond for label-only. Atom counts are tox_atom; bonds are tox_bond."
            ),
            "tox_atom": (
                "One row per atom. Copy: SELECT atom_id, element FROM tox_atom. "
                "Atom counts and element lists: this cube only. Never JOIN tox_connected or tox_bond. "
                "Element codes are lowercase ('c','o','h','f','cl'). "
                "Molecule label is on this cube; do not join tox_molecule for a single-molecule filter."
            ),
            "tox_bond": (
                "One row per bond. Copy: SELECT bond_id, bond_type FROM tox_bond. "
                "Bond-type counts and percentages: this cube only (molecule_id is already here). "
                "Never JOIN tox_molecule, tox_atom, or tox_connected. "
                "bond_type '-' single, '=' double, '#' triple."
            ),
            "tox_connected": (
                "One row per atom-bond connection. Copy: SELECT atom_id, atom_id2, bond_id FROM tox_connected. "
                "Use only when both atom_id and atom_id2 are needed (bridge). "
                "Never FROM tox_connected for atom-only or bond-only (use tox_atom / tox_bond). "
                "Not a molecule-grain cube. Ungrouped cube JOIN is rejected."
            ),
        },
        "descriptions": {
            "_all": {
                "label": "Molecule carcinogenicity: '+' = carcinogenic, '-' = non-carcinogenic.",
                "element": "Atom element code, lowercase: 'c' carbon, 'o' oxygen, 'h' hydrogen, 'f' fluorine, 'cl' chlorine, 'i' iodine, 's' sulfur.",
                "bond_type": "Bond type: '-' single, '=' double, '#' triple.",
                "atom_id": "Atom identifier (e.g. TR004_8). The 4th character pattern is in the id string, not a separate column.",
                "atom_id2": "Second atom on a connected row. Pair with atom_id; order may be reversed.",
                "bond_id": "Bond identifier.",
                "molecule_id": "Molecule identifier.",
            },
        },
        "expression_dims": [
            {
                "cube": "tox_molecule",
                "name": "is_carcinogenic",
                "type": "code",
                "expression": "IIF(label = '+', 1, 0)",
                "description": (
                    "1 if label = '+' (carcinogenic), else 0. "
                    "Use WHERE is_carcinogenic = 1; segment carcinogenic is examples only."
                ),
            },
        ],
        "segments": [
            {
                "name": "carcinogenic",
                "description": "Carcinogenic molecules (label = '+').",
                "filter": "label = '+'",
                "cube": "tox_molecule",
            },
            {
                "name": "non_carcinogenic",
                "description": "Non-carcinogenic molecules (label = '-').",
                "filter": "label = '-'",
                "cube": "tox_molecule",
            },
            {
                "name": "single_bond",
                "description": "Single-bonded (bond_type = '-').",
                "filter": "bond_type = '-'",
                "cube": "tox_bond",
            },
            {
                "name": "double_bond",
                "description": "Double-bonded (bond_type = '=').",
                "filter": "bond_type = '='",
                "cube": "tox_bond",
            },
            {
                "name": "triple_bond",
                "description": "Triple-bonded (bond_type = '#').",
                "filter": "bond_type = '#'",
                "cube": "tox_bond",
            },
            {
                "name": "carbon_atom",
                "description": "Carbon atoms (element = 'c').",
                "filter": "element = 'c'",
                "cube": "tox_atom",
            },
            {
                "name": "oxygen_atom",
                "description": "Oxygen atoms (element = 'o').",
                "filter": "element = 'o'",
                "cube": "tox_atom",
            },
        ],
        "metrics": [
            {
                "name": "pct_carcinogenic",
                "description": "Percentage of carcinogenic molecules (label = '+').",
                "expression": (
                    "(SUM(IIF(label = '+', 1, 0)) * 100) / (COUNT(molecule_id))"
                ),
                "cube": "tox_molecule",
            },
            {
                "name": "pct_double_bond",
                "description": "Share of double bonds among bonds (bond_type = '=').",
                "expression": (
                    "(SUM(IIF(bond_type = '=', 1, 0))) / (COUNT(bond_id))"
                ),
                "cube": "tox_bond",
            },
            {
                "name": "single_bond_share",
                "description": "Share of single bonds among bonds (bond_type = '-').",
                "expression": (
                    "(SUM(IIF(bond_type = '-', 1, 0))) / (COUNT(bond_id))"
                ),
                "cube": "tox_bond",
            },
            {
                "name": "carbon_atom_pct",
                "description": "Percentage of carbon atoms (element = 'c').",
                "expression": (
                    "(SUM(IIF(element = 'c', 1, 0)) * 100) / (COUNT(atom_id))"
                ),
                "cube": "tox_atom",
            },
        ],
    },
    "thrombosis_prediction": {
        "cube_descriptions": {
            "throm_patient": (
                "One row per patient. Copy: SELECT ID, Birthday, SEX FROM throm_patient. "
                "Identifier is Birthday (not birthday / BIRTHDAY). "
                "Age = DATEDIFF or EXTRACT(YEAR FROM …) - birth_year. "
                "Birthday/sex/Admission: this cube only. Never JOIN throm_laboratory or throm_examination. "
                "Laboratory (T_CHO, GLU, PLT) is throm_laboratory; KCT/Symptoms/Thrombosis is throm_examination."
            ),
            "throm_laboratory": (
                "One lab draw per patient-date. Copy: SELECT ID, T_CHO, GLU, PLT FROM throm_laboratory. "
                "Blood markers (T_CHO, GLU, PLT, GPT, CPK, RNP) and lab Date: this cube only. "
                "Never JOIN throm_examination or throm_patient. "
                "KCT/Symptoms/Thrombosis are throm_examination. Ungrouped cube JOIN is rejected. "
                "Abnormal platelets: PLT<=100 OR PLT>=400; abnormal IGM: IGM<=40 OR IGM>=400; high GPT: GPT>=60."
            ),
            "throm_examination": (
                "One examination per patient. Copy: SELECT ID, KCT, Symptoms, Thrombosis FROM throm_examination. "
                "KCT/Symptoms/Thrombosis: this cube only. Never JOIN throm_laboratory or throm_patient. "
                "KCT/RVVT/LAC live here (not laboratory). Thrombosis: 2=severe, 1=most serious. "
                "Diagnosis on this cube is examination diagnosis; patient diagnosis is Patient_Diagnosis. "
                "Ungrouped cube JOIN is rejected."
            ),
        },
        "descriptions": {
            "_all": {
                "Birthday": (
                    "Patient date of birth. Identifier is Birthday (not BIRTHDAY). "
                    "Larger Birthday = younger. Birth year is birth_year (EXTRACT)."
                ),
                "SEX": "Patient sex: 'M' male, 'F' female.",
                "Admission": "Admission: '+' in-patient, '-' outpatient.",
                "Description": "Patient description / first-visit text; year(Description) is visit year.",
                "First_Date": "First hospital date. Age at first visit uses EXTRACT(YEAR FROM First_Date) - birth_year.",
                "ID": "Patient identifier (joins Laboratory/Examination).",
            },
            "throm_patient": {
                "Diagnosis": (
                    "Patient-level diagnosis (SLE, SJS, APS). "
                    "APS diagnosis is Diagnosis on throm_patient, not throm_examination."
                ),
            },
            "throm_laboratory": {
                "Diagnosis": (
                    "Patient diagnosis on the lab cube. APS is throm_patient.Diagnosis, "
                    "not a laboratory marker."
                ),
                "Date": "Laboratory date. Filter year with EXTRACT(YEAR FROM Date) or lab_year.",
                "PLT": "Platelets. Abnormal: PLT<=100 OR PLT>=400. Normal: 100-400. Low: PLT<100. High: PLT>=400.",
                "IGM": "IgM. Abnormal: IGM<=40 OR IGM>=400.",
                "GPT": (
                    "ALT glutamic pyruvic transaminase (GPT). Beyond normal: GPT >= 60. "
                    "Never GLU (glucose)."
                ),
                "LDH": "LDH. Beyond normal: LDH>500.",
                "T_CHO": (
                    "Total cholesterol (physical T-CHO). Identifier is T_CHO, never CHO. "
                    "High cholesterol: T_CHO >= 250; within normal: T_CHO < 250."
                ),
                "GLU": "Blood glucose (GLU). Not GPT/ALT. Beyond normal: GLU >= 180.",
                "CPK": "Creatinine phosphokinase. Beyond normal: CPK >= 250.",
                "SM": (
                    "Anti-SM. Normal: SM IN ('negative','0') meaning '-' and '+-'. "
                    "SM='1' means '+'. Not ANA_Pattern."
                ),
                "RNP": (
                    "Anti-ribonuclear protein. Normal: RNP IN ('negative','0'). "
                    "Not ANA_Pattern."
                ),
                "SC170": (
                    "Anti-Scl-70. Normal: SC170 IN ('negative','0'). Not ANA_Pattern."
                ),
                "CRP": "C-reactive protein. Abnormal: CRP = '+'.",
                "IGG": "IgG. Higher than normal: IGG >= 2000.",
                "IGA": "IgA immunoglobulin.",
                "UA": "Uric acid. Male high: UA > 8; female high: UA > 6.5.",
                "TG": "Triglycerides. High: TG > 200.",
                "T_BIL": "Total bilirubin.",
                "CENTROMEA": "Centromere antibody A. Distinct from ANA_Pattern.",
                "GOT": "AST/GOT. Distinct from GPT (ALT).",
            },
            "throm_examination": {
                "Thrombosis": "Thrombosis code: 2=severe, 1=most serious, 0=none.",
                "Diagnosis": (
                    "Examination diagnosis (not Patient_Diagnosis). "
                    "APS is Patient.Diagnosis on throm_patient."
                ),
                "ANA_Pattern": (
                    "ANA pattern (e.g. 'S'). Distinct from anti-SM / RNP / SC170 laboratory markers."
                ),
                "ANA": "ANA marker. Distinct from ANA_Pattern.",
                "aCL_IgG": "Anti-Cardiolipin IgG. IgM is aCL_IgM.",
                "Patient_Diagnosis": "Patient-level diagnosis copied from Patient.",
                "Examination_Date": "Examination date. Identifier uses underscore (not 'Examination Date').",
                "aCL_IgM": "Anti-Cardiolipin antibody IgM.",
                "Symptoms": "Symptoms text; observed means Symptoms IS NOT NULL. Not a laboratory column.",
                "KCT": "KCT coagulation marker. Positive = '+'. Lives on throm_examination, not throm_laboratory.",
                "RVVT": "RVVT coagulation marker. Positive = '+'. Pair with KCT/LAC.",
                "LAC": "LAC coagulation marker. Positive = '+'. Pair with KCT/RVVT.",
            },
        },
        "expression_dims": [
            {
                "cube": "throm_patient",
                "name": "birth_year",
                "type": "date",
                "expression": "EXTRACT(YEAR FROM Birthday)",
                "description": "Year of Birthday. Use for YEAR(Birthday) filters (not BIRTHDAY).",
            },
            {
                "cube": "throm_patient",
                "name": "age_years",
                "type": "code",
                "expression": "EXTRACT(YEAR FROM CURRENT_DATE) - EXTRACT(YEAR FROM Birthday)",
                "description": (
                    "Patient age in years. Use age_years instead of YEAR(Birthday) nested EXTRACT. "
                    "Age at first visit: EXTRACT(YEAR FROM First_Date) - birth_year."
                ),
            },
            {
                "cube": "throm_laboratory",
                "name": "birth_year",
                "type": "date",
                "expression": "EXTRACT(YEAR FROM Birthday)",
                "description": "Year of Birthday. Age ≈ EXTRACT(YEAR FROM CURRENT_DATE) - birth_year.",
            },
            {
                "cube": "throm_laboratory",
                "name": "age_years",
                "type": "code",
                "expression": "EXTRACT(YEAR FROM CURRENT_DATE) - EXTRACT(YEAR FROM Birthday)",
                "description": "Patient age in years (CURRENT_DATE year minus birth year).",
            },
            {
                "cube": "throm_laboratory",
                "name": "lab_year",
                "type": "date",
                "expression": "EXTRACT(YEAR FROM Date)",
                "description": "Year of laboratory Date.",
            },
            {
                "cube": "throm_examination",
                "name": "birth_year",
                "type": "date",
                "expression": "EXTRACT(YEAR FROM Birthday)",
                "description": "Year of Birthday.",
            },
            {
                "cube": "throm_examination",
                "name": "age_years",
                "type": "code",
                "expression": "EXTRACT(YEAR FROM CURRENT_DATE) - EXTRACT(YEAR FROM Birthday)",
                "description": "Patient age in years (CURRENT_DATE year minus birth year).",
            },
            {
                "cube": "throm_examination",
                "name": "exam_year",
                "type": "date",
                "expression": "EXTRACT(YEAR FROM Examination_Date)",
                "description": "Year of Examination_Date.",
            },
        ],
        "segments": [
            {
                "name": "male_patients",
                "description": "Male patients (SEX = 'M').",
                "filter": "SEX = 'M'",
                "cube": "throm_patient",
            },
            {
                "name": "female_patients",
                "description": "Female patients (SEX = 'F').",
                "filter": "SEX = 'F'",
                "cube": "throm_patient",
            },
            {
                "name": "inpatient",
                "description": "In-patients (Admission = '+').",
                "filter": "Admission = '+'",
                "cube": "throm_patient",
            },
            {
                "name": "outpatient",
                "description": "Outpatients (Admission = '-').",
                "filter": "Admission = '-'",
                "cube": "throm_patient",
            },
            {
                "name": "diagnosis_sle",
                "description": "SLE diagnosed patients (Diagnosis = 'SLE').",
                "filter": "Diagnosis = 'SLE'",
                "cube": "throm_patient",
            },
            {
                "name": "diagnosis_aps",
                "description": "APS diagnosis (Diagnosis = 'APS') on the patient.",
                "filter": "Diagnosis = 'APS'",
                "cube": "throm_patient",
            },
            {
                "name": "severe_thrombosis",
                "description": "Severe thrombosis (Thrombosis = 2). Code 1 is most serious.",
                "filter": "Thrombosis = 2",
                "cube": "throm_examination",
            },
            {
                "name": "plt_abnormal",
                "description": "Abnormal platelet level (PLT<=100 OR PLT>=400).",
                "filter": "PLT <= 100 OR PLT >= 400",
                "cube": "throm_laboratory",
            },
            {
                "name": "igm_abnormal",
                "description": "Abnormal IgM (IGM<=40 OR IGM>=400).",
                "filter": "IGM <= 40 OR IGM >= 400",
                "cube": "throm_laboratory",
            },
            {
                "name": "gpt_high",
                "description": "GPT/ALT beyond normal range (GPT>=60).",
                "filter": "GPT >= 60",
                "cube": "throm_laboratory",
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
                    "(SUM(IIF(SEX = 'M' AND Admission = '+', 1, 0)) * 100) / ("
                    "COUNT(ID))"
                ),
                "cube": "throm_patient",
            },
            {
                "name": "avg_acl_igm",
                "description": (
                    "Average anti-Cardiolipin antibody IgM (AVG(aCL_IgM)). "
                    "Lab+KCT grouped: SELECT throm_laboratory.ID FROM throm_laboratory "
                    "JOIN throm_examination ON throm_laboratory.ID = throm_examination.ID "
                    "GROUP BY throm_laboratory.ID."
                ),
                "expression": "AVG(aCL_IgM)",
                "cube": "throm_examination",
            },
        ],
    },
    "california_schools": {
        "cube_descriptions": {
            "schools_frpm": (
                "One FRPM row with school attributes. Copy: SELECT CDSCode, Enrollment_K_12 FROM schools_frpm. "
                "FRPM eligibility and enrollment: this cube only. Never JOIN schools_sat. "
                "SAT scores and test takers are schools_sat. Ungrouped cube JOIN is rejected. "
                "Row rates are facts eligible_frpm_rate_k12 / eligible_free_rate_k12 (not additive)."
            ),
            "schools_sat": (
                "One SAT score row with school attributes. Copy: SELECT cds, NumTstTakr, NumGE1500 FROM schools_sat. "
                "SAT scores, test takers, and school address: this cube only. Never JOIN schools_frpm. "
                "FRPM eligibility/enrollment is schools_frpm. Ungrouped cube JOIN is rejected. "
                "Row excellence_rate is a non-additive fact. Address is Street, City, State, Zip (four columns)."
            ),
        },
        "descriptions": {
            "_all": {
                "Street": "Street address. Complete address = Street, City, State, Zip (four columns, not one).",
                "City": "City. Part of complete address with Street, State, Zip.",
                "State": "State. Part of complete address with Street, City, Zip.",
                "Zip": "ZIP code. Part of complete address with Street, City, State.",
                "DOC": "District ownership code: 31=State Special Schools, 52=Elementary School District, 54=Unified School District.",
                "SOC": "School ownership code; 62=Intermediate/Middle Schools.",
                "Magnet": "Magnet program: 1=offers magnet (K-8 questions use Magnet=1). Not a meal column.",
                "EdOpsCode": (
                    "Educational option code (with EdOpsName). "
                    "State Special School = EdOpsCode = 'SSS'."
                ),
                "Phone": "Communication number / phone (schools.Phone).",
                "FundingType": (
                    "School funding type. Locally funded = FundingType = 'Locally funded'. "
                    "Distinct from Charter_Funding_Type."
                ),
                "StatusType": "School status: 'Merged', 'Active', 'Closed'.",
                "ClosedDate": (
                    "School close date. Closed before year Y: EXTRACT(YEAR FROM ClosedDate) < Y."
                ),
                "AdmFName1": "Administrator first name (AdmFName1).",
                "Virtual": "Virtual status; exclusively virtual = 'F'.",
                "OpenDate": "School open date. Open year is open_year (EXTRACT).",
                "County": "County name from schools (not County_Name from FRPM).",
                "School": "School name from schools (not School_Name from FRPM).",
            },
            "schools_frpm": {
                "Academic_Year": (
                    "FRPM academic year string (e.g. 2014-2015). "
                    "Filter the stored range; do not YEAR(Academic_Year)."
                ),
                "Charter_Funding_Type": (
                    "Charter funding type. Non-charter 'locally funded' uses FundingType, not this."
                ),
                "School_Name": "FRPM school name. Prefer School from schools when listing school names.",
                "Enrollment_Ages_5_17": (
                    "Enrollment ages 5-17. K-12 vs 15-17 difference is "
                    "Enrollment_K_12 - Enrollment_Ages_5_17."
                ),
                "Percent_Eligible_Free_K_12": (
                    "Stored K-12 free eligible percent. Prefer fact eligible_free_rate_k12 * 100 "
                    "(not additive)."
                ),
                "Percent_Eligible_FRPM_K_12": (
                    "Stored K-12 FRPM eligible percent. Prefer fact eligible_frpm_rate_k12 * 100 "
                    "(not additive)."
                ),
                "Enrollment_K_12": (
                    "K-12 enrollment. Row rates are facts eligible_frpm_rate_k12 / "
                    "eligible_free_rate_k12 (not additive). Grouped share is metric "
                    "k12_frpm_eligible_share / k12_free_eligible_share."
                ),
                "Free_Meal_Count_K_12": (
                    "Free meal count K-12. Row eligible free rate is fact eligible_free_rate_k12 "
                    "(not additive; do not SUM)."
                ),
                "FRPM_Count_K_12": (
                    "FRPM count K-12. Row eligible FRPM rate is fact eligible_frpm_rate_k12 "
                    "(not additive; do not SUM)."
                ),
                "NSLP_Provision_Status": "NSLP provision; Multiple Provision Types is this literal.",
            },
            "schools_sat": {
                "AvgScrWrite": (
                    "Average SAT writing score. SAT writing questions FROM schools_sat "
                    "(not schools_frpm Enrollment)."
                ),
                "AvgScrRead": "Average SAT reading score. SAT-only: FROM schools_sat.",
                "NumGE1500": (
                    "Count of testers with SAT >= 1500. Row excellence_rate = NumGE1500 / NumTstTakr "
                    "(not additive). Grouped share is metric sat_excellence_share."
                ),
                "NumTstTakr": "Number of SAT test takers. Excellence rate denominator.",
                "AvgScrMath": "Average SAT math score.",
                "sname": "SAT file school name; prefer School from schools when joining.",
            },
        },
        "expression_dims": [
            {
                "cube": "schools_frpm",
                "name": "open_year",
                "type": "date",
                "expression": "EXTRACT(YEAR FROM OpenDate)",
                "description": "Year the school opened (from OpenDate).",
            },
            {
                "cube": "schools_sat",
                "name": "open_year",
                "type": "date",
                "expression": "EXTRACT(YEAR FROM OpenDate)",
                "description": "Year the school opened (from OpenDate).",
            },
            {
                "cube": "schools_sat",
                "name": "is_exclusively_virtual",
                "type": "code",
                "expression": "IIF(Virtual = 'F', 1, 0)",
                "description": "1 if exclusively virtual (Virtual = 'F'), else 0. Use WHERE is_exclusively_virtual = 1.",
            },
        ],
        "expression_facts": [
            {
                "cube": "schools_frpm",
                "name": "eligible_frpm_rate_k12",
                "type": "quantity",
                "additive": False,
                "expression": "(FRPM_Count_K_12) / (Enrollment_K_12)",
                "description": (
                    "Row K-12 FRPM eligible rate. Not additive — do not SUM/AVG. "
                    "ORDER BY / SELECT / MAX / MIN ok. Grouped share is metric k12_frpm_eligible_share."
                ),
            },
            {
                "cube": "schools_frpm",
                "name": "eligible_free_rate_k12",
                "type": "quantity",
                "additive": False,
                "expression": "(Free_Meal_Count_K_12) / (Enrollment_K_12)",
                "description": (
                    "Row K-12 free-meal eligible rate. Not additive — do not SUM/AVG. "
                    "Grouped share is metric k12_free_eligible_share."
                ),
            },
            {
                "cube": "schools_sat",
                "name": "excellence_rate",
                "type": "quantity",
                "additive": False,
                "expression": "(NumGE1500) / (NumTstTakr)",
                "description": (
                    "Row SAT excellence rate (NumGE1500 / NumTstTakr). Not additive. "
                    "Grouped share is metric sat_excellence_share."
                ),
            },
        ],
        "segments": [
            {
                "name": "exclusively_virtual",
                "description": "Exclusively virtual schools (Virtual = 'F').",
                "filter": "Virtual = 'F'",
                "cube": "schools_sat",
            },
            {
                "name": "magnet_k8",
                "description": "Offers a magnet program (Magnet = 1). K-8 is grade span, not a column.",
                "filter": "Magnet = 1",
                "cube": "schools_frpm",
            },
            {
                "name": "merged_schools",
                "description": "Merged schools (StatusType = 'Merged').",
                "filter": "StatusType = 'Merged'",
                "cube": "schools_frpm",
            },
            {
                "name": "locally_funded",
                "description": "Locally funded schools (FundingType = 'Locally funded').",
                "filter": "FundingType = 'Locally funded'",
                "cube": "schools_frpm",
            },
        ],
        "metrics": [
            {
                "name": "sat_excellence_share",
                "description": (
                    "Grouped SAT excellence share = SUM(NumGE1500)/SUM(NumTstTakr). "
                    "Row rate is fact excellence_rate (not additive). "
                    "FRPM+SAT grouped: SELECT schools_frpm.CDSCode FROM schools_frpm "
                    "JOIN schools_sat ON schools_frpm.CDSCode = schools_sat.cds "
                    "GROUP BY schools_frpm.CDSCode."
                ),
                "expression": "(SUM(NumGE1500)) / (SUM(NumTstTakr))",
                "cube": "schools_sat",
            },
            {
                "name": "k12_free_eligible_share",
                "description": (
                    "Grouped K-12 free-meal share = SUM(Free_Meal_Count_K_12)/SUM(Enrollment_K_12). "
                    "Row rate is fact eligible_free_rate_k12 (not additive)."
                ),
                "expression": (
                    "(SUM(Free_Meal_Count_K_12)) / (SUM(Enrollment_K_12))"
                ),
                "cube": "schools_frpm",
            },
            {
                "name": "k12_frpm_eligible_share",
                "description": (
                    "Grouped K-12 FRPM share = SUM(FRPM_Count_K_12)/SUM(Enrollment_K_12). "
                    "Row rate is fact eligible_frpm_rate_k12 (not additive)."
                ),
                "expression": "(SUM(FRPM_Count_K_12)) / (SUM(Enrollment_K_12))",
                "cube": "schools_frpm",
            },
            {
                "name": "avg_math_score",
                "description": "Average of average math scores (AVG(AvgScrMath)).",
                "expression": "AVG(AvgScrMath)",
                "cube": "schools_sat",
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
                    "((count borderless) / (count id) * 100)."
                ),
                "expression": (
                    "(SUM(IIF(borderColor = 'borderless', 1, 0)) * 100) / ("
                    "COUNT(id))"
                ),
            },
            {
                "name": "chinese_simplified_pct",
                "description": (
                    "Percentage of Chinese Simplified printings among cards."
                ),
                "expression": (
                    "(SUM(IIF(language = 'Chinese Simplified', 1, 0)) * 100) / ("
                    "COUNT(id))"
                ),
            },
            {
                "name": "promo_pct",
                "description": "Percentage of promotional cards (isPromo = 1).",
                "expression": (
                    "(SUM(IIF(isPromo = 1, 1, 0)) * 100) / (COUNT(id))"
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
                    "(SUM(IIF(Age > 65, 1, 0)) * 100) / (COUNT(Id))"
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
                "cube": "player",
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
                "cube": "player",
            },
        ],
        "metrics": [
            {
                "name": "left_foot_pct",
                "description": "Percentage of players who prefer left foot.",
                "expression": (
                    "(SUM(IIF(preferred_foot = 'left', 1, 0)) * 100) / ("
                    "COUNT(player_api_id))"
                ),
                "cube": "player",
            },
            {
                "name": "avg_long_shots",
                "description": "Average long shots (AVG(long_shots)).",
                "expression": "AVG(long_shots)",
                "cube": "player",
            },
            {
                "name": "avg_heading_accuracy",
                "description": "Average heading accuracy (AVG(heading_accuracy)).",
                "expression": "AVG(heading_accuracy)",
                "cube": "player",
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
                "cube": "player",
            },
        ],
    },
    "formula_1": {
        "cube_descriptions": {
            "f1_race_results": (
                "One row per driver race result. Copy: SELECT resultId, forename, surname, position FROM f1_race_results. "
                "Never FROM this cube for qualifying (f1_qualifying), lap times (f1_lap_times), "
                "or f1_driver_standings / f1_constructor_standings. Finished means time IS NOT NULL. "
                "Full driver name = forename AND surname (two filters). Ungrouped cube JOIN is rejected."
            ),
            "f1_qualifying": (
                "One qualifying row. Copy: SELECT forename, surname, q3 FROM f1_qualifying. "
                "Never FROM f1_race_results. q1/q2/q3 are session times; Q3 like 0:0M:SS means q3 LIKE 'M:SS%'. "
                "circuits_name vs circuitRef vs location vs (lat,lng) are different fields."
            ),
            "f1_lap_times": (
                "One row per lap. Copy: SELECT lap, milliseconds FROM f1_lap_times. Never FROM f1_race_results. "
                "`time` is lap clock; milliseconds is duration. LIKE 'M:SS%' matches lap clock strings."
            ),
            "f1_pit_stops": (
                "One row per pit stop. Copy: SELECT stop, milliseconds FROM f1_pit_stops. "
                "duration/milliseconds are stop length; `time` is clock."
            ),
            "f1_driver_standings": (
                "Driver championship standings after a race (cumulative rank). "
                "Copy: SELECT forename, surname, points, position FROM f1_driver_standings. Never FROM f1_race_results. "
                "Season final = filter to that season's last raceId. Search: championship, standings, cumulative, Hamilton not first."
            ),
            "f1_constructor_standings": (
                "Constructor championship standings after a race (cumulative rank). "
                "Copy: SELECT constructors_name, points, position FROM f1_constructor_standings. "
                "Never FROM f1_constructor_results. Season final = last raceId of the year. "
                "Search: championship, standings, cumulative."
            ),
            "f1_constructor_results": (
                "Constructor points in a single race. "
                "Copy: SELECT constructors_name, points FROM f1_constructor_results. "
                "Cumulative rank is f1_constructor_standings."
            ),
        },
        "descriptions": {
            "_all": {
                "forename": "Driver first name. Full name is forename AND surname (two filters, do not concat).",
                "surname": "Driver last name. Full name is forename AND surname (two filters, do not concat).",
                "circuitRef": (
                    "Circuit reference/slug (circuitRef). The reference name of a circuit is this "
                    "column, never circuits_name."
                ),
                "url": "Race wikipedia page (races.url). Season page is seasons_url.",
                "seasons_url": "Season wikipedia page (seasons.url). Race page is url.",
                "dob": (
                    "Driver date of birth. Youngest = ORDER BY dob DESC (larger dob is younger). "
                    "Oldest = ORDER BY dob ASC."
                ),
                "location": "Circuit locality. Distinct from circuits_name, circuitRef, lat/lng.",
                "lat": "Circuit latitude. Pair with lng; not the same as location.",
                "lng": "Circuit longitude. Pair with lat; not the same as location.",
                "name": "Race name (races.name). Circuit display name is circuits_name.",
                "year": "Race season year (races.year). Standings are per race; season final = last race of year.",
                "nationality": "On results/lap cubes this is the driver nationality; constructor nationality is constructors_nationality.",
            },
            "f1_race_results": {
                "fastestLapSpeed": (
                    "Fastest lap SPEED this race. Fastest lap duration is f1_lap_times.milliseconds, "
                    "not this column."
                ),
                "rank": (
                    "Fastest-lap rank this race (1 = fastest lap). Not finishing position. "
                    "Highest rank = rank = 1."
                ),
                "statusId": (
                    "Result status id. 2 = disqualified. A finisher still requires time IS NOT NULL."
                ),
                "time": "Result classified time. Finished/completed means time IS NOT NULL. Race clock is races_time.",
                "circuits_name": "Circuit display name. Distinct from circuitRef, location, lat/lng.",
                "constructors_name": "Constructor display name. Distinct from constructorRef.",
                "status": "Result status text from status table (not constructorResults.status).",
                "position": (
                    "Finishing position this race (1 = winner of this race). "
                    "Not f1_driver_standings.position."
                ),
                "points": (
                    "Points scored in this race only. Not f1_driver_standings.points."
                ),
            },
            "f1_driver_standings": {
                "position": (
                    "Cumulative championship rank after that race (driver standings). "
                    "Not finishing position of a single race. "
                    "Search: championship, standings, cumulative, Hamilton not first."
                ),
                "points": (
                    "Cumulative driver championship standings points after that race. "
                    "Not race-result points from f1_race_results."
                ),
                "wins": (
                    "Cumulative championship race wins after that race (driver standings). "
                    "Not wins in a single grand prix."
                ),
            },
            "f1_constructor_standings": {
                "position": (
                    "Cumulative constructor championship rank after that race (standings). "
                    "Not a single-race constructor result."
                ),
                "points": (
                    "Cumulative constructor championship standings points after that race. "
                    "Not f1_constructor_results.points for one race."
                ),
                "wins": (
                    "Cumulative constructor championship wins after that race (standings)."
                ),
            },
            "f1_qualifying": {
                "q3": "Q3 time string. Pattern 0:0M:SS refers to q3 LIKE 'M:SS%'.",
                "q1": "Q1 time string.",
                "q2": "Q2 time string.",
                "circuits_name": "Circuit display name. Distinct from circuitRef and location.",
                "constructors_nationality": "Constructor nationality (driver nationality is nationality).",
            },
            "f1_lap_times": {
                "time": "Lap clock string. Pattern 0:0M:SS refers to time LIKE 'M:SS%'. Duration is milliseconds.",
                "milliseconds": "Lap duration in milliseconds (use for fastest/average lap).",
            },
            "f1_constructor_results": {
                "nationality": "Constructor nationality (this cube has no drivers).",
                "points": (
                    "Constructor points in this race only. "
                    "Not f1_constructor_standings.points."
                ),
                "status": "Constructor result status (not race status table).",
            },
        },
        "expression_dims": [
            {
                "cube": "f1_race_results",
                "name": "driver_birth_year",
                "type": "date",
                "expression": "EXTRACT(YEAR FROM dob)",
                "description": "Year of driver dob.",
            },
            {
                "cube": "f1_qualifying",
                "name": "driver_birth_year",
                "type": "date",
                "expression": "EXTRACT(YEAR FROM dob)",
                "description": "Year of driver dob.",
            },
            {
                "cube": "f1_lap_times",
                "name": "driver_birth_year",
                "type": "date",
                "expression": "EXTRACT(YEAR FROM dob)",
                "description": "Year of driver dob.",
            },
            {
                "cube": "f1_pit_stops",
                "name": "driver_birth_year",
                "type": "date",
                "expression": "EXTRACT(YEAR FROM dob)",
                "description": "Year of driver dob.",
            },
            {
                "cube": "f1_driver_standings",
                "name": "driver_birth_year",
                "type": "date",
                "expression": "EXTRACT(YEAR FROM dob)",
                "description": "Year of driver dob.",
            },
        ],
        "segments": [
            {
                "name": "japanese_constructors",
                "description": "Japanese constructors (nationality = 'Japanese').",
                "filter": "nationality = 'Japanese'",
                "cube": "f1_constructor_results",
            },
            {
                "name": "since_2010",
                "description": "Races/results since 2010 (year >= 2010).",
                "filter": "year >= 2010",
                "cube": "f1_race_results",
            },
            {
                "name": "race_completed",
                "description": "Classified race finish (time IS NOT NULL on the result).",
                "filter": "time IS NOT NULL",
                "cube": "f1_race_results",
            },
        ],
        "metrics": [
            {
                "name": "avg_lap_time_ms",
                "description": "Average lap time in milliseconds (AVG(milliseconds)).",
                "expression": "AVG(milliseconds)",
                "cube": "f1_lap_times",
            },
            {
                "name": "race_completion_pct",
                "description": (
                    "Percentage of classified finishes "
                    "(time IS NOT NULL) among results."
                ),
                "expression": (
                    "(SUM(IIF(time IS NOT NULL, 1, 0)) * 100) / ("
                    "COUNT(resultId))"
                ),
                "cube": "f1_race_results",
            },
            {
                "name": "avg_points",
                "description": (
                    "Average race-result points (AVG(points)). "
                    "Copy: SELECT AVG(points) FROM f1_race_results. Never championship standings."
                ),
                "expression": "AVG(points)",
                "cube": "f1_race_results",
            },
            {
                "name": "driver_standing_points",
                "description": (
                    "Cumulative driver championship points after a race. "
                    "Copy: SELECT forename, surname, points FROM f1_driver_standings. Never FROM f1_race_results."
                ),
                "expression": "SUM(points)",
                "cube": "f1_driver_standings",
            },
            {
                "name": "constructor_standing_points",
                "description": (
                    "Cumulative constructor championship points after a race. "
                    "Copy: SELECT constructors_name, points FROM f1_constructor_standings. "
                    "Never FROM f1_constructor_results."
                ),
                "expression": "SUM(points)",
                "cube": "f1_constructor_standings",
            },
        ],
    },
    "student_club": {
        "cube_descriptions": {
            "club_event": (
                "One event. Copy: SELECT event_id, event_name, type FROM club_event. "
                "cost is on club_expense; amount is on club_income or club_budget — never FROM club_event for money. "
                "Never JOIN club_expense for type/location. Attendance counts: club_attendance. "
                "`type` is event type (Meeting, etc.). Ungrouped cube JOIN is rejected."
            ),
            "club_attendance": (
                "One attendance link (event × member). Copy: SELECT event_id, member_id FROM club_attendance. "
                "Headcount/who attended: this cube only. Never JOIN club_expense (cost is club_expense). "
                "Full name = first_name AND last_name. Ungrouped cube JOIN is rejected."
            ),
            "club_budget": (
                "One budget line for an event. Copy: SELECT event_name, category, amount, spent FROM club_budget. "
                "Spend-to-budget is this cube only (spent/amount). Never JOIN club_event. "
                "`category` is budget category (Food); `event_status` is budget/event status (Closed); "
                "`type` is the event type. Ungrouped cube JOIN is rejected."
            ),
            "club_expense": (
                "One expense. Copy: SELECT expense_description, cost FROM club_expense. "
                "Total/event cost: this cube only. Never JOIN club_event or club_attendance. "
                "event_name is already on this cube. `approved` True/False. "
                "Full name = first_name AND last_name. Do not pull income into expense queries. "
                "Ungrouped cube JOIN is rejected."
            ),
            "club_income": (
                "One income row linked to a member (not an event). "
                "Copy: SELECT amount, source, notes FROM club_income. "
                "source includes 'Dues' and 'Fundraising'. Full name = first_name AND last_name."
            ),
            "club_member": (
                "One member with zip_code and major. Copy: SELECT member_id, first_name, last_name FROM club_member. "
                "amount is on club_income (not this cube). Full name = first_name AND last_name "
                "(two filters, do not concat). `type` here is zip type, not event type."
            ),
        },
        "descriptions": {
            "_all": {
                "first_name": "Member first name. Full name is first_name AND last_name (two filters, not concat).",
                "last_name": "Member last name. Full name is first_name AND last_name (two filters, not concat).",
                "event_name": "Event name (e.g. October Meeting, Yearly Kickoff). Distinct from type.",
            },
            "club_event": {
                "type": "Event type (Meeting, etc.). Not budget category and not zip type.",
                "status": "Event status on the event row (distinct from budget event_status).",
            },
            "club_budget": {
                "category": "Budget category (Food). Not event type.",
                "event_status": "Budget/event status (Closed). Not event.type.",
                "type": "Event type copied from event. Budget category is category.",
                "spent": "Amount spent against this budget line.",
                "amount": "Budgeted amount. Spend-to-budget ratio is the metric spend_to_budget_ratio.",
            },
            "club_expense": {
                "category": "Budget category copied onto the expense (Food). Not event type.",
                "approved": "Expense approval: True approved, False not approved.",
                "expense_description": "Expense item text (Pizza).",
                "cost": "Expense cost. Total cost is the metric total_expense_cost.",
                "type": "Event type (from event). Not expense category.",
            },
            "club_income": {
                "source": "Income source: 'Dues', 'Fundraising'. This cube is member-linked, not event-linked.",
                "notes": "Income notes. Querying notes stays on club_income (no event/attendance).",
                "date_received": "Date the income (including Dues) was received.",
            },
            "club_member": {
                "type": "ZIP type from zip_code (not event type).",
                "major_name": "Major name (join from major).",
            },
        },
        "segments": [
            {
                "name": "meeting_events",
                "description": "Meeting events (type = 'Meeting').",
                "filter": "type = 'Meeting'",
                "cube": "club_event",
            },
            {
                "name": "fundraising_income",
                "description": "Fundraising income (source = 'Fundraising').",
                "filter": "source = 'Fundraising'",
                "cube": "club_income",
            },
            {
                "name": "dues_income",
                "description": "Dues income (source = 'Dues').",
                "filter": "source = 'Dues'",
                "cube": "club_income",
            },
            {
                "name": "food_budget",
                "description": "Food budget category (category = 'Food').",
                "filter": "category = 'Food'",
                "cube": "club_budget",
            },
            {
                "name": "closed_events",
                "description": "Closed events (event_status = 'Closed').",
                "filter": "event_status = 'Closed'",
                "cube": "club_budget",
            },
            {
                "name": "pizza_expense",
                "description": "Pizza expenses (expense_description = 'Pizza').",
                "filter": "expense_description = 'Pizza'",
                "cube": "club_expense",
            },
        ],
        "metrics": [
            {
                "name": "pizza_expense_total",
                "description": "Total pizza expense cost (SUM(cost) for Pizza).",
                "expression": (
                    "SUM(IIF(expense_description = 'Pizza', cost, 0))"
                ),
                "cube": "club_expense",
            },
            {
                "name": "spend_to_budget_ratio",
                "description": "Spend-to-budget ratio ((SUM(spent)) / (SUM(amount))).",
                "expression": "(SUM(spent)) / (SUM(amount))",
                "cube": "club_budget",
            },
            {
                "name": "food_spent_total",
                "description": "Total spent on Food category (SUM spent where Food).",
                "expression": "SUM(IIF(category = 'Food', spent, 0))",
                "cube": "club_budget",
            },
            {
                "name": "total_expense_cost",
                "description": (
                    "Total expense cost (SUM(cost)). "
                    "Copy: SELECT SUM(cost) FROM club_expense. Never FROM club_event."
                ),
                "expression": "SUM(cost)",
                "cube": "club_expense",
            },
        ],
    },
}


_FAIL_PLAN_CUBE_EXTRA: dict[str, dict[str, str]] = {
    "formula_1": {
        "f1_qualifying": "q1 is the first qualifying session. raceId is the race number.",
        "f1_lap_times": "A race number filters raceId on this cube.",
        "f1_driver_standings": "Championship position and points are this cube.",
        "f1_constructor_standings": "Championship points of 0 are points = 0 on this cube.",
        "f1_race_results": "fastestLapSpeed is this race, not a lap-time row.",
    },
    "student_club": {
        "club_member": "City and county use this cube's zip columns.",
        "club_expense": "Event cost uses event_name and cost on this cube.",
        "club_event": "Money and attendance are not this cube.",
    },
    "california_schools": {
        "schools_sat": "SAT scores and school address columns are this cube.",
        "schools_frpm": "Free meal eligibility is this cube.",
    },
    "thrombosis_prediction": {
        "throm_patient": "Birthday and SEX are this cube.",
        "throm_laboratory": "ALB, UN, CRE, and GPT are lab columns on this cube.",
        "throm_examination": "Symptoms and Thrombosis are this cube.",
    },
    "toxicology": {
        "tox_atom": "element counts use this cube. atom_id is here.",
        "tox_bond": "bond_type counts use this cube.",
        "tox_connected": "atom_id and atom_id2 together use this cube.",
    },
    "superhero": {
        "hero_attribute": "Intelligence and Strength scores are attribute_name plus attribute_value.",
    },
}

_FAIL_PLAN_METRIC_EXTRA: dict[str, dict[str, str]] = {
    "toxicology": {
        "carbon_atom_pct": (
            "Cross atom and bond only as SELECT element FROM tox_atom "
            "JOIN tox_bond ON tox_atom.molecule_id = tox_bond.molecule_id "
            "GROUP BY element. atom_id is not on tox_bond. "
            "Alias is the cube name, not T1."
        ),
        "pct_double_bond": (
            "Order by an alias: SELECT bond_type AS b, COUNT(bond_id) AS n "
            "FROM tox_bond GROUP BY bond_type ORDER BY n."
        ),
    },
    "student_club": {
        "total_expense_cost": "Use event_name and cost. Alias is club_expense, not T1.",
    },
    "thrombosis_prediction": {
        "avg_acl_igm": "Alias is the cube name, not T1.",
    },
    "formula_1": {
        "driver_standing_points": "Alias is f1_driver_standings, not T1.",
        "avg_lap_time_ms": "Alias is f1_lap_times, not T1.",
    },
    "california_schools": {
        "sat_excellence_share": "Alias is the cube name, not T1.",
    },
}

_ALIAS_NOTE = "Alias is the cube name, not T1."
_REFERS_RE = re.compile(r"([^;.]{3,90}?)\s+refers to\s+([^;.]{2,140})", re.I)
_PERSON_RE = re.compile(r"\b[A-Z][a-z]+\s+[A-Z][a-z]+\b")


def apply_fail_plan_copies() -> None:
    """fail_plan.md: Copy/alias는 큐브 description, GROUP BY JOIN은 metric description."""
    for db_id, spec in DOMAIN_SPECS.items():
        descs = spec.get("cube_descriptions") or {}
        extras = _FAIL_PLAN_CUBE_EXTRA.get(db_id) or {}
        for cube, text in list(descs.items()):
            extra = extras.get(cube)
            parts = [text.rstrip()]
            if extra and extra not in text:
                parts.append(extra)
            if _ALIAS_NOTE not in text:
                parts.append(_ALIAS_NOTE)
            descs[cube] = " ".join(parts)
        for met in spec.get("metrics") or []:
            extra = (_FAIL_PLAN_METRIC_EXTRA.get(db_id) or {}).get(met["name"])
            if extra and extra not in (met.get("description") or ""):
                met["description"] = (met.get("description") or "").rstrip() + " " + extra


def clauses_for_members(evidence: str, names: set[str]) -> dict[str, str]:
    """evidence의 'refers to'를 멤버 이름에만 붙인다. 사람 이름·짧은 토큰은 제외."""
    usable = {n for n in names if len(n) >= 4}
    out: dict[str, str] = {}
    for match in _REFERS_RE.finditer(evidence or ""):
        left, right = match.group(1).strip(), match.group(2).strip()
        if _PERSON_RE.search(left):
            continue
        if "DIVIDE" in right.upper() or re.search(r"\bT\d\b", right):
            continue
        hits = [n for n in usable if re.search(rf"\b{re.escape(n)}\b", right)]
        if not hits:
            continue
        clause = f"{left} refers to {right}."
        for name in hits:
            if any(name != other and name in other for other in hits):
                continue
            out.setdefault(name, clause)
    return out


def apply_evidence_clauses(cube: dict, evidence: str) -> bool:
    """빈 dim/fact description만 evidence 문장으로 채운다."""
    names = {
        m.get("name")
        for m in (cube.get("dimensions") or []) + (cube.get("facts") or [])
        if m.get("name")
    }
    return apply_member_descriptions(cube, clauses_for_members(evidence, names))


def fill_empty_from_evidence(db_id: str) -> int:
    blob = " ".join(evidence_mentions(db_id))
    if not blob:
        return 0
    n = 0
    for cube in list_cubes(db_id).values():
        if apply_evidence_clauses(cube, blob):
            write_cube(db_id, cube)
            n += 1
    return n


apply_fail_plan_copies()


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
    cubes: dict[str, dict] = list_cubes(db_id)
    if cube:
        cubes.setdefault(cube["name"], cube)
    default_cube = cube
    if default_cube is None:
        if db_id in cubes:
            default_cube = cubes[db_id]
        elif len(cubes) == 1:
            default_cube = next(iter(cubes.values()))
    reserved: set[str] = set()
    for c in cubes.values():
        reserved |= member_names(c)
    meta_dir = ROOT / db_id / "meta"
    if meta_dir.is_dir():
        for path in meta_dir.glob("*.json"):
            doc = json.loads(path.read_text(encoding="utf-8"))
            if doc.get("kind") in ("metric", "segment", "table", "view", "cube"):
                reserved.add(doc["name"])
                if doc.get("kind") == "cube":
                    cubes.setdefault(doc["name"], doc)

    def resolve_cube(item: dict) -> dict:
        cname = item.get("cube")
        if not cname:
            if default_cube is None:
                raise FileNotFoundError(
                    f"{db_id}: spec item {item.get('name')} needs cube"
                )
            return default_cube
        if cname not in cubes:
            cubes[cname] = load_cube(db_id, cname)
            reserved.update(member_names(cubes[cname]))
        return cubes[cname]

    docs: list[dict] = []
    for seg in spec.get("segments") or []:
        name = seg["name"]
        if name in reserved:
            continue  # already present (re-run after generate_meta+enrich)
        docs.append(
            make_segment(
                name=name,
                description=seg["description"],
                cube=resolve_cube(seg),
                filter_expr=seg["filter"],
            )
        )
        reserved.add(name)
    for met in spec.get("metrics") or []:
        name = met["name"]
        if name in reserved:
            continue
        docs.append(
            make_metric(
                name=name,
                description=met["description"],
                cube=resolve_cube(met),
                expression=met["expression"],
                filter_expr=met.get("filter"),
            )
        )
        reserved.add(name)
    return docs


def prune_stale_metric_segment(db_id: str) -> int:
    """Drop metric/segment JSON whose names left DOMAIN_SPECS (e.g. D28 rename)."""
    spec = DOMAIN_SPECS.get(db_id) or {}
    keep = {s["name"] for s in spec.get("segments") or []}
    keep |= {m["name"] for m in spec.get("metrics") or []}
    meta_dir = ROOT / db_id / "meta"
    if not meta_dir.is_dir():
        return 0
    n = 0
    for path in meta_dir.glob("*.json"):
        doc = json.loads(path.read_text(encoding="utf-8"))
        kind = doc.get("kind")
        name = doc.get("name")
        if kind == "segment" and name not in keep:
            path.unlink()
            n += 1
        elif (
            kind == "metric"
            and name not in keep
            and not str(name).endswith("_count")
        ):
            path.unlink()
            n += 1
    return n


def patch_metric_descriptions(db_id: str) -> int:
    """Refresh metric.description from DOMAIN_SPECS (existing files skip build_docs)."""
    spec = DOMAIN_SPECS.get(db_id) or {}
    by_name = {m["name"]: m for m in spec.get("metrics") or []}
    meta_dir = ROOT / db_id / "meta"
    if not meta_dir.is_dir() or not by_name:
        return 0
    n = 0
    for path in meta_dir.glob("*.json"):
        doc = json.loads(path.read_text(encoding="utf-8"))
        if doc.get("kind") != "metric":
            continue
        spec_m = by_name.get(doc.get("name"))
        if not spec_m:
            continue
        new = spec_m.get("description") or ""
        if not new or doc.get("description") == new:
            continue
        doc["description"] = new
        path.write_text(
            json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
        n += 1
    return n


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
        patched = apply_knowledge(db_id)
        patched += fill_empty_from_evidence(db_id)
        docs = build_docs(db_id)
        n = write_docs(db_id, docs)
        stale = prune_stale_metric_segment(db_id)
        met_desc = patch_metric_descriptions(db_id)
        seg_n = sum(1 for d in docs if d["kind"] == "segment")
        met_n = sum(1 for d in docs if d["kind"] == "metric")
        print(
            f"{db_id}: wrote {seg_n} segments, {met_n} metrics; "
            f"patched {patched} cubes; pruned {stale} stale; "
            f"metric desc {met_desc}"
        )
        total += n
    print(f"done: {total} documents")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
