"""generate_meta join 휴리스틱 단위 테스트."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from generate_meta import collect_join_edges, spanning_joins, views_from_tables


def _table(name: str, cols: list[str], pk: list[str], fk: list[dict] | None = None) -> dict:
    return {
        "kind": "table",
        "name": name,
        "target": "superhero",
        "columns": [{"name": c, "type": "INTEGER", "nullable": True} for c in cols],
        "pk": pk,
        "fk": fk or [],
    }


def test_no_pk_to_pk_inference_for_superhero_style() -> None:
    """superhero.id 와 superpower.id 를 직접 붙이지 않고 hero_power 경유."""
    tables = {
        "superhero_table": _table(
            "superhero_table",
            ["id", "superhero_name"],
            ["id"],
        ),
        "hero_power": _table(
            "hero_power",
            ["hero_id", "power_id"],
            [],
            fk=[
                {
                    "columns": ["hero_id"],
                    "references": {"table": "superhero_table", "columns": ["id"]},
                },
                {
                    "columns": ["power_id"],
                    "references": {"table": "superpower", "columns": ["id"]},
                },
            ],
        ),
        "superpower": _table("superpower", ["id", "power_name"], ["id"]),
        "attribute": _table("attribute", ["id", "attribute_name"], ["id"]),
        "hero_attribute": _table(
            "hero_attribute",
            ["hero_id", "attribute_id", "attribute_value"],
            [],
            fk=[
                {
                    "columns": ["hero_id"],
                    "references": {"table": "superhero_table", "columns": ["id"]},
                },
                {
                    "columns": ["attribute_id"],
                    "references": {"table": "attribute", "columns": ["id"]},
                },
            ],
        ),
    }
    edges = collect_join_edges(tables)
    pairs = {(c, p): on for c, p, on in edges}

    assert ("superhero_table", "superpower") not in pairs
    assert ("superpower", "superhero_table") not in pairs
    assert ("superhero_table", "attribute") not in pairs
    assert pairs[("hero_power", "superpower")] == [("power_id", "id")]
    assert pairs[("hero_power", "superhero_table")] == [("hero_id", "id")]

    views = views_from_tables(tables, "superhero")
    assert len(views) == 1
    joins = {(j["left"], j["right"], j["on"][0]["left_column"], j["on"][0]["right_column"]) for j in views[0]["joins"]}
    assert ("superhero_table", "superpower", "id", "id") not in joins
    # power는 hero_power 경유
    assert any(r == "hero_power" for _, r, _, _ in joins)
    assert any(r == "superpower" for _, r, _, _ in joins)
    root, steps = spanning_joins(list(tables), edges)
    assert root
    assert steps


if __name__ == "__main__":
    test_no_pk_to_pk_inference_for_superhero_style()
    print("ok")
