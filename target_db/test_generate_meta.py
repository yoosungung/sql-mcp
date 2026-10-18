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


def test_preserves_cyclic_fk_edges_and_distinct_pk_fk_names() -> None:
    """student_club 스타일: member-attendance-event-budget 삼각/사각 사이클에서
    budget -> event 간선이 유지되고, event_id 와 link_to_event 가 모두 뷰 컬럼으로 존재해야 함.
    """
    tables = {
        "member": _table(
            "member",
            ["member_id", "first_name", "zip"],
            ["member_id"],
            fk=[
                {
                    "columns": ["zip"],
                    "references": {"table": "zip_code", "columns": ["zip_code"]},
                }
            ],
        ),
        "zip_code": _table("zip_code", ["zip_code", "state"], ["zip_code"]),
        "attendance": _table(
            "attendance",
            ["link_to_member", "link_to_event"],
            ["link_to_member", "link_to_event"],
            fk=[
                {
                    "columns": ["link_to_member"],
                    "references": {"table": "member", "columns": ["member_id"]},
                },
                {
                    "columns": ["link_to_event"],
                    "references": {"table": "event", "columns": ["event_id"]},
                },
            ],
        ),
        "event": _table("event", ["event_id", "event_name"], ["event_id"]),
        "budget": _table(
            "budget",
            ["budget_id", "spent", "link_to_event"],
            ["budget_id"],
            fk=[
                {
                    "columns": ["link_to_event"],
                    "references": {"table": "event", "columns": ["event_id"]},
                }
            ],
        ),
        "expense": _table(
            "expense",
            ["expense_id", "cost", "link_to_member", "link_to_budget"],
            ["expense_id"],
            fk=[
                {
                    "columns": ["link_to_member"],
                    "references": {"table": "member", "columns": ["member_id"]},
                },
                {
                    "columns": ["link_to_budget"],
                    "references": {"table": "budget", "columns": ["budget_id"]},
                },
            ],
        ),
    }
    views = views_from_tables(tables, "student_club")
    assert len(views) == 1
    view = views[0]
    join_pairs = {tuple(sorted((j["left"], j["right"]))) for j in view["joins"]}
    # Cycle-closing edge between budget and event must be preserved!
    assert ("budget", "event") in join_pairs
    assert ("attendance", "event") in join_pairs
    assert ("budget", "expense") in join_pairs

    col_map = {c["name"]: c["mappings"] for c in view["columns"]}
    # Both event_id and link_to_event must be present and map to event/budget/attendance
    assert "event_id" in col_map
    assert "link_to_event" in col_map
    assert col_map["event_id"][0] == {"source": "event", "column": "event_id"}
    assert {"source": "budget", "column": "link_to_event"} in col_map["event_id"]

    # Both zip_code and zip must be present
    assert "zip_code" in col_map
    assert "zip" in col_map
    assert col_map["zip_code"][0] == {"source": "zip_code", "column": "zip_code"}
    assert {"source": "member", "column": "zip"} in col_map["zip_code"]


def test_prefers_exact_name_fk_and_short_api_id() -> None:
    """동일 테이블 쌍 다중 FK에서 컬럼명 일치 및 짧은 키(player_api_id vs player_fifa_api_id, atom_id vs atom_id2) 우선."""
    tables = {
        "Player": _table("Player", ["id", "player_api_id", "player_fifa_api_id"], ["id"]),
        "Player_Attributes": _table(
            "Player_Attributes",
            ["id", "player_fifa_api_id", "player_api_id", "overall_rating"],
            ["id"],
            fk=[
                {
                    "columns": ["player_fifa_api_id"],
                    "references": {"table": "Player", "columns": ["player_fifa_api_id"]},
                },
                {
                    "columns": ["player_api_id"],
                    "references": {"table": "Player", "columns": ["player_api_id"]},
                },
            ],
        ),
    }
    views = views_from_tables(tables, "football")
    assert len(views) == 1
    on = views[0]["joins"][0]["on"][0]
    assert on["left_column"] == "player_api_id"
    assert on["right_column"] == "player_api_id"


def test_role_playing_lookup_expansion_for_superhero_colours() -> None:
    """superhero의 eye_colour_id, hair_colour_id, skin_colour_id -> colour(id, colour) 다중 FK를
    역할별 lookup 테이블로 분리하여 colour, hair_colour, skin_colour 컬럼을 모두 노출하는지 검증."""
    from generate_meta import expand_role_playing_lookups

    tables = {
        "superhero": _table(
            "superhero",
            ["id", "superhero_name", "eye_colour_id", "hair_colour_id", "skin_colour_id"],
            ["id"],
            fk=[
                {"columns": ["eye_colour_id"], "references": {"table": "colour", "columns": ["id"]}},
                {"columns": ["hair_colour_id"], "references": {"table": "colour", "columns": ["id"]}},
                {"columns": ["skin_colour_id"], "references": {"table": "colour", "columns": ["id"]}},
            ],
        ),
        "colour": _table("colour", ["id", "colour"], ["id"]),
    }
    expand_role_playing_lookups(tables)
    views = views_from_tables(tables, "superhero")
    assert len(views) == 1
    col_names = {c["name"] for c in views[0]["columns"]}
    assert "colour" in col_names
    assert "hair_colour" in col_names
    assert "skin_colour" in col_names


if __name__ == "__main__":
    test_no_pk_to_pk_inference_for_superhero_style()
    test_preserves_cyclic_fk_edges_and_distinct_pk_fk_names()
    test_prefers_exact_name_fk_and_short_api_id()
    test_role_playing_lookup_expansion_for_superhero_colours()
    print("ok")
