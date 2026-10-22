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


def apply_member_descriptions(cube: dict, descriptions: dict[str, str]) -> bool:
    """Fill empty dim/fact descriptions. Never overwrite non-empty."""
    changed = False
    for member in (cube.get("dimensions") or []) + (cube.get("facts") or []):
        name = member.get("name")
        text = descriptions.get(name)
        if not text:
            continue
        if (member.get("description") or "").strip():
            continue
        member["description"] = text
        changed = True
    return changed


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


def apply_knowledge(db_id: str) -> int:
    """Patch cube description / dim-fact text / expression dims. Does not overwrite."""
    spec = DOMAIN_SPECS.get(db_id) or {}
    cubes = list_cubes(db_id)
    if not cubes:
        return 0
    desc_by_cube: dict[str, dict[str, str]] = spec.get("descriptions") or {}
    shared = desc_by_cube.get("_all") or {}
    cube_descs: dict[str, str] = spec.get("cube_descriptions") or {}
    expr_dims: list[dict] = spec.get("expression_dims") or []
    n = 0
    for cname, cube in cubes.items():
        changed = False
        merged = dict(shared)
        merged.update(desc_by_cube.get(cname) or {})
        if apply_member_descriptions(cube, merged):
            changed = True
        cdesc = cube_descs.get(cname)
        if cdesc and not (cube.get("description") or "").strip():
            cube["description"] = cdesc
            changed = True
        for dim in expr_dims:
            if dim.get("cube") not in (None, cname):
                continue
            if dim.get("cube") is None and len(cubes) > 1:
                continue
            if add_expression_dim(cube, dim):
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
                "One row per superhero. Eye colour is `colour` (not `color`); "
                "hair/skin are `hair_colour` / `skin_colour`. "
                "Attributes and powers are other cubes."
            ),
            "hero_attribute": (
                "One row per hero-attribute. Dumbest/strongest = MIN/MAX(attribute_value) "
                "filtered by attribute_name (Intelligence, Strength, Durability, Speed; match case)."
            ),
            "hero_power": (
                "One row per hero-power. Filter power_name (e.g. Agility); "
                "do not join hero_attribute in this cube."
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
                "colour": "Eye colour (join on eye_colour_id). Not hair or skin. Never a column named color.",
                "hair_colour": "Hair colour (join on hair_colour_id). Not eye/skin.",
                "skin_colour": "Skin colour (join on skin_colour_id). Not eye/hair.",
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
            "tox_molecule": "One row per molecule. Use for label ('+' carcinogenic / '-' non-carcinogenic) only.",
            "tox_atom": "One row per atom. Element codes are lowercase ('c','o','h','f','cl'). Molecule label is available; do not join bond.",
            "tox_bond": "One row per bond. bond_type '-' single, '=' double, '#' triple. Molecule label available; do not join atom/connected.",
            "tox_connected": (
                "One row per atom-bond connection. Use atom_id + atom_id2 + bond_id to bridge atoms and bonds. "
                "Not a molecule-grain cube."
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
                "One row per patient. Birthday identifier is Birthday (not BIRTHDAY). "
                "Age = DATEDIFF or EXTRACT(YEAR FROM …) - birth_year. Laboratory and examination are other cubes."
            ),
            "throm_laboratory": (
                "One lab draw per patient-date. Do not join Examination. "
                "Abnormal platelets: PLT<=100 OR PLT>=400; abnormal IGM: IGM<=40 OR IGM>=400; high GPT: GPT>=60."
            ),
            "throm_examination": (
                "One examination per patient. Thrombosis: 2=severe, 1=most serious. "
                "Diagnosis on this cube is examination diagnosis; patient diagnosis is Patient_Diagnosis."
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
                "Diagnosis": "Patient-level diagnosis (e.g. SLE, SJS).",
            },
            "throm_laboratory": {
                "Diagnosis": "Patient diagnosis on the lab cube.",
                "Date": "Laboratory date. Filter year with EXTRACT(YEAR FROM Date) or lab_year.",
                "PLT": "Platelets. Abnormal: PLT<=100 OR PLT>=400. Normal: 100-400. Low: PLT<100. High: PLT>=400.",
                "IGM": "IgM. Abnormal: IGM<=40 OR IGM>=400.",
                "GPT": "ALT/GPT. Beyond normal range: GPT>=60. GPT>30 is a looser threshold.",
                "LDH": "LDH. Beyond normal: LDH>500.",
            },
            "throm_examination": {
                "Thrombosis": "Thrombosis code: 2=severe, 1=most serious, 0=none.",
                "Diagnosis": "Examination diagnosis (not Patient_Diagnosis).",
                "Patient_Diagnosis": "Patient-level diagnosis copied from Patient.",
                "Examination_Date": "Examination date. Identifier uses underscore (not 'Examination Date').",
                "aCL_IgM": "Anti-Cardiolipin antibody IgM.",
                "Symptoms": "Symptoms text; observed means Symptoms IS NOT NULL.",
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
                "cube": "throm_laboratory",
                "name": "birth_year",
                "type": "date",
                "expression": "EXTRACT(YEAR FROM Birthday)",
                "description": "Year of Birthday. Age ≈ EXTRACT(YEAR FROM CURRENT_DATE) - birth_year.",
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
                "description": "Average anti-Cardiolipin antibody IgM (AVG(aCL_IgM)).",
                "expression": "AVG(aCL_IgM)",
                "cube": "throm_examination",
            },
        ],
    },
    "california_schools": {
        "cube_descriptions": {
            "schools_frpm": (
                "One FRPM row joined to schools. Meal/enrollment rates are metrics "
                "(eligible_free_rate_k12, eligible_frpm_rate_k12) — not columns. "
                "SAT scores are the schools_sat cube; join cubes for FRPM+SAT."
            ),
            "schools_sat": (
                "One SAT score row joined to schools. Excellence rate = NumGE1500/NumTstTakr (metric). "
                "Address is Street, City, State, Zip (four columns). FRPM is schools_frpm."
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
                "EdOpsCode": "Educational option code (with EdOpsName).",
                "Virtual": "Virtual status; exclusively virtual = 'F'.",
                "OpenDate": "School open date. Open year is open_year (EXTRACT).",
                "County": "County name from schools (not County_Name from FRPM).",
                "School": "School name from schools (not School_Name from FRPM).",
            },
            "schools_frpm": {
                "Enrollment_K_12": "K-12 enrollment. Do not guess rate columns; use metrics.",
                "Free_Meal_Count_K_12": "Free meal count K-12. Eligible free rate is the metric eligible_free_rate_k12.",
                "FRPM_Count_K_12": "FRPM count K-12. Eligible FRPM rate is the metric eligible_frpm_rate_k12.",
                "NSLP_Provision_Status": "NSLP provision; Multiple Provision Types is this literal.",
            },
            "schools_sat": {
                "NumGE1500": "Count of testers with SAT >= 1500. Excellence rate metric uses this / NumTstTakr.",
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
        ],
        "metrics": [
            {
                "name": "excellence_rate",
                "description": "Excellence rate = NumGE1500 / NumTstTakr. Not a table column.",
                "expression": "(SUM(NumGE1500)) / (SUM(NumTstTakr))",
                "cube": "schools_sat",
            },
            {
                "name": "eligible_free_rate_k12",
                "description": (
                    "Eligible free rate for K-12 = "
                    "Free_Meal_Count_K_12 / Enrollment_K_12. Not a column."
                ),
                "expression": (
                    "(SUM(Free_Meal_Count_K_12)) / (SUM(Enrollment_K_12))"
                ),
                "cube": "schools_frpm",
            },
            {
                "name": "eligible_frpm_rate_k12",
                "description": (
                    "Eligible free or reduced price meal rate for K-12 = "
                    "FRPM_Count_K_12 / Enrollment_K_12. Not a column."
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
                "One row per driver race result. Finishing `time` IS NOT NULL means classified finish. "
                "Not lap times, qualifying, or standings. Full driver name = forename AND surname (two filters)."
            ),
            "f1_qualifying": (
                "One qualifying row. q1/q2/q3 are session times; Q3 like 0:0M:SS means q3 LIKE 'M:SS%'. "
                "circuits_name vs circuitRef vs location vs (lat,lng) are different fields."
            ),
            "f1_lap_times": (
                "One row per lap. `time` is lap clock; milliseconds is duration. "
                "LIKE 'M:SS%' matches lap clock strings. Not race results."
            ),
            "f1_pit_stops": "One row per pit stop. duration/milliseconds are stop length; `time` is clock.",
            "f1_driver_standings": (
                "Driver standing after a race (race grain, cumulative). "
                "Season final = filter to that season's last raceId. Not constructorResults."
            ),
            "f1_constructor_standings": (
                "Constructor standing after a race (race grain, cumulative). "
                "Season final = last raceId of the year. Not constructorResults."
            ),
            "f1_constructor_results": (
                "Constructor points in a single race (not cumulative standings)."
            ),
        },
        "descriptions": {
            "_all": {
                "forename": "Driver first name. Full name is forename AND surname (two filters, do not concat).",
                "surname": "Driver last name. Full name is forename AND surname (two filters, do not concat).",
                "circuitRef": "Circuit code/slug. Distinct from circuits_name, location, and (lat,lng).",
                "location": "Circuit locality. Distinct from circuits_name, circuitRef, lat/lng.",
                "lat": "Circuit latitude. Pair with lng; not the same as location.",
                "lng": "Circuit longitude. Pair with lat; not the same as location.",
                "name": "Race name (races.name). Circuit display name is circuits_name.",
                "year": "Race season year (races.year). Standings are per race; season final = last race of year.",
                "nationality": "On results/lap cubes this is the driver nationality; constructor nationality is constructors_nationality.",
            },
            "f1_race_results": {
                "time": "Result classified time. Finished/completed means time IS NOT NULL. Race clock is races_time.",
                "circuits_name": "Circuit display name. Distinct from circuitRef, location, lat/lng.",
                "constructors_name": "Constructor display name. Distinct from constructorRef.",
                "status": "Result status text from status table (not constructorResults.status).",
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
                "points": "Constructor points in this race (not standings).",
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
                "description": "Average race-result points (AVG(points)).",
                "expression": "AVG(points)",
                "cube": "f1_race_results",
            },
        ],
    },
    "student_club": {
        "cube_descriptions": {
            "club_event": (
                "One event. `type` is event type (Meeting, etc.). Not income/expense. "
                "Attendance, budget, and money are other cubes."
            ),
            "club_attendance": "One attendance link (event × member). Full name = first_name AND last_name.",
            "club_budget": (
                "One budget line for an event. `category` is budget category (Food); "
                "`event_status` is budget/event status (Closed); `type` is the event type."
            ),
            "club_expense": (
                "One expense. `approved` True/False. Full name = first_name AND last_name. "
                "Do not pull income into expense queries."
            ),
            "club_income": (
                "One income row linked to a member (not an event). "
                "source includes 'Dues' and 'Fundraising'. Full name = first_name AND last_name."
            ),
            "club_member": (
                "One member with zip_code and major. Full name = first_name AND last_name "
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
                "description": "Total expense cost (SUM(cost)).",
                "expression": "SUM(cost)",
                "cube": "club_expense",
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
        docs = build_docs(db_id)
        n = write_docs(db_id, docs)
        seg_n = sum(1 for d in docs if d["kind"] == "segment")
        met_n = sum(1 for d in docs if d["kind"] == "metric")
        print(
            f"{db_id}: wrote {seg_n} segments, {met_n} metrics; "
            f"patched {patched} cubes"
        )
        total += n
    print(f"done: {total} documents")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
