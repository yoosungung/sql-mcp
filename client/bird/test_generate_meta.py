"""generate_meta join 휴리스틱 단위 테스트."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from generate_meta import (
    ROOT,
    annotate_hub_relationships,
    resolve_catalog_table,
    apply_name_priority,
    classify_join_relationship,
    collect_join_edges,
    cube_and_metric_from_view,
    default_group_by,
    edge_is_key_relationship,
    infer_table_id_fks,
    load_semantic_plan,
    parse_semantic_plan,
    rewrite_divide,
    spanning_joins,
    views_from_tables,
)


def _table(name: str, cols: list[str], pk: list[str], fk: list[dict] | None = None) -> dict:
    return {
        "kind": "table",
        "name": name,
        "target": "superhero",
        "table": name,
        "columns": [{"name": c, "type": "INTEGER", "nullable": True} for c in cols],
        "pk": pk,
        "fk": fk or [],
    }


def test_default_group_by_is_grain_not_allowlist() -> None:
    dims = [
        {"name": "id", "type": "string", "column": "id"},
        {"name": "OpenDate", "type": "date", "column": "OpenDate"},
        {"name": "LastUpdate", "type": "date", "column": "LastUpdate"},
        {"name": "Academic_Year", "type": "date", "column": "Academic_Year"},
        {"name": "School_Code", "type": "code", "column": "School_Code"},
        {"name": "District_Type", "type": "code", "column": "District_Type"},
        {"name": "Street", "type": "string", "column": "Street"},
    ]
    gb = default_group_by(dims)
    assert gb[0] == "Academic_Year"
    assert "School_Code" in gb and "District_Type" in gb
    assert "id" not in gb and "Street" not in gb and "LastUpdate" not in gb
    assert len(gb) < len(dims)


def test_rewrite_divide_keeps_nested_commas() -> None:
    src = "DIVIDE(SUM(IIF(status = 'A', amount, 0)) * 100, SUM(amount))"
    assert rewrite_divide(src) == "(SUM(IIF(status = 'A', amount, 0)) * 100) / (SUM(amount))"


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


def test_auto_cube_descriptions_are_empty() -> None:
    """자동 생성 description은 메타 사용용이 아니므로 비운다."""
    tables = {
        "orders": _table(
            "orders",
            ["order_id", "customer_id", "sale_amount"],
            ["order_id"],
        ),
    }
    view = {
        "kind": "view",
        "name": "orders_all",
        "target": "dw",
        "combine": "join",
        "sources": [{"kind": "table", "table": "orders"}],
        "joins": [],
        "columns": [
            {
                "name": "order_id",
                "mappings": [{"source": "orders", "column": "order_id"}],
            },
            {
                "name": "sale_amount",
                "mappings": [{"source": "orders", "column": "sale_amount"}],
            },
        ],
    }
    # sale_amount needs numeric type for fact classification
    tables["orders"]["columns"][2]["type"] = "REAL"
    cube, metric = cube_and_metric_from_view(view, tables, set(), "dw")
    assert cube["description"] == ""
    assert all(d["description"] == "" for d in cube["dimensions"])
    assert all(f["description"] == "" for f in cube["facts"])
    assert metric is not None
    assert metric["description"] == ""


def test_infers_table_id_column_joins_to_table_id() -> None:
    """선언 FK가 없어도 {table}_id / {Table}Id → {table}.id 조인·fk를 추론한다."""
    tables = {
        "customer": _table("customer", ["id", "region_code"], ["id"]),
        "orders": _table(
            "orders",
            ["order_id", "customer_id", "sale_amount"],
            ["order_id"],
        ),
        "posts": _table("posts", ["Id", "Title"], ["Id"]),
        "comments": _table(
            "comments",
            ["Id", "PostId", "Text"],
            ["Id"],
        ),
    }
    infer_table_id_fks(tables)
    order_fks = {
        (tuple(fk["columns"]), fk["references"]["table"], tuple(fk["references"]["columns"]))
        for fk in tables["orders"]["fk"]
    }
    assert (("customer_id",), "customer", ("id",)) in order_fks
    comment_fks = {
        (tuple(fk["columns"]), fk["references"]["table"], tuple(fk["references"]["columns"]))
        for fk in tables["comments"]["fk"]
    }
    assert (("PostId",), "posts", ("Id",)) in comment_fks

    # infer 없이도 collect_join_edges가 동일 패턴을 잡는다
    bare = {
        "customer": _table("customer", ["id", "region_code"], ["id"]),
        "orders": _table(
            "orders",
            ["order_id", "customer_id", "sale_amount"],
            ["order_id"],
        ),
    }
    edges = collect_join_edges(bare)
    pairs = {(c, p): on for c, p, on in edges}
    assert pairs[("orders", "customer")] == [("customer_id", "id")]


def test_role_playing_home_away_team_for_match() -> None:
    """Match.home/away_team_api_id → Team: 동일 카탈로그 Team + view alias away_team."""
    from generate_meta import expand_role_playing_lookups

    tables = {
        "Team": _table(
            "Team",
            ["id", "team_api_id", "team_fifa_api_id", "team_long_name", "team_short_name"],
            ["id"],
        ),
        "Match": _table(
            "Match",
            ["id", "home_team_api_id", "away_team_api_id", "home_team_goal", "away_team_goal"],
            ["id"],
            fk=[
                {
                    "columns": ["home_team_api_id"],
                    "references": {"table": "Team", "columns": ["team_api_id"]},
                },
                {
                    "columns": ["away_team_api_id"],
                    "references": {"table": "Team", "columns": ["team_api_id"]},
                },
            ],
        ),
    }
    role_nodes = expand_role_playing_lookups(tables)
    assert tables["Match"]["fk"][0]["references"]["table"] == "Team"
    assert tables["Match"]["fk"][1]["references"]["table"] == "Team"
    away_alias = tables["Match"]["fk"][1]["_view_alias"]
    assert away_alias in role_nodes
    assert role_nodes[away_alias]["catalog"] == "Team"
    assert "away_team_table" not in tables
    assert set(tables) == {"Team", "Match"}

    views = views_from_tables(tables, "european_football_2", role_nodes)
    assert len(views) == 1
    sources = views[0]["sources"]
    assert {"kind": "table", "table": "Team"} in sources
    assert any(
        s.get("table") == "Team" and s.get("alias") == away_alias for s in sources
    )
    join_ons = {
        (
            j["left"],
            j["right"],
            j["on"][0]["left_column"],
            j["on"][0]["right_column"],
        )
        for j in views[0]["joins"]
    }
    assert ("Match", "Team", "home_team_api_id", "team_api_id") in join_ons
    assert (
        "Match",
        away_alias,
        "away_team_api_id",
        "team_api_id",
    ) in join_ons
    assert not any({left, right} == {"Team", away_alias} for left, right, _, _ in join_ons)
    col_names = {c["name"] for c in views[0]["columns"]}
    assert "team_long_name" in col_names
    assert any("away" in n.lower() and "long_name" in n.lower() for n in col_names)


def test_role_playing_lookup_expansion_for_superhero_colours() -> None:
    """eye/hair/skin_colour_id → colour: view alias로 역할 컬럼 노출(클론 table 없음)."""
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
    role_nodes = expand_role_playing_lookups(tables)
    assert set(tables) == {"superhero", "colour"}
    assert len(role_nodes) == 2
    views = views_from_tables(tables, "superhero", role_nodes)
    assert len(views) == 1
    assert all(s.get("table") == "colour" or s.get("table") == "superhero" for s in views[0]["sources"])
    col_names = {c["name"] for c in views[0]["columns"]}
    assert "colour" in col_names
    assert "hair_colour" in col_names
    assert "skin_colour" in col_names


def test_match_player_lineup_excluded_and_player_cube_split() -> None:
    """Match→Player 라인업 FK(>3)는 조인 제외; Player+Attributes는 player view."""
    from generate_meta import expand_role_playing_lookups

    player_fks = [
        {
            "columns": [f"home_player_{i}"],
            "references": {"table": "Player", "columns": ["player_api_id"]},
        }
        for i in range(1, 12)
    ] + [
        {
            "columns": [f"away_player_{i}"],
            "references": {"table": "Player", "columns": ["player_api_id"]},
        }
        for i in range(1, 12)
    ]
    tables = {
        "Match": _table(
            "Match",
            ["id", "home_team_api_id", "away_team_api_id"]
            + [f"home_player_{i}" for i in range(1, 12)]
            + [f"away_player_{i}" for i in range(1, 12)],
            ["id"],
            fk=[
                {
                    "columns": ["home_team_api_id"],
                    "references": {"table": "Team", "columns": ["team_api_id"]},
                },
                {
                    "columns": ["away_team_api_id"],
                    "references": {"table": "Team", "columns": ["team_api_id"]},
                },
                *player_fks,
            ],
        ),
        "Team": _table("Team", ["id", "team_api_id", "team_long_name"], ["id"]),
        "Player": _table(
            "Player", ["id", "player_api_id", "player_name"], ["id"]
        ),
        "Player_Attributes": _table(
            "Player_Attributes",
            ["id", "player_api_id", "overall_rating"],
            ["id"],
            fk=[
                {
                    "columns": ["player_api_id"],
                    "references": {"table": "Player", "columns": ["player_api_id"]},
                }
            ],
        ),
    }
    role_nodes = expand_role_playing_lookups(tables)
    views = views_from_tables(tables, "european_football_2", role_nodes)
    by_name = {v["name"]: v for v in views}
    assert "player" in by_name
    assert "european_football_2_all" in by_name
    player_src = {s.get("table") for s in by_name["player"]["sources"]}
    assert player_src == {"Player", "Player_Attributes"}
    match_view = by_name["european_football_2_all"]
    src_tables = {s.get("table") for s in match_view["sources"]}
    assert "Player" not in src_tables
    assert "Team" in src_tables
    # case-insensitive FS: cube `player` must not clobber table `Player`
    cubes: list[dict] = []
    metrics: list[dict] = []
    priority_taken: set[str] = set()
    for view in views:
        cube, metric = cube_and_metric_from_view(
            view, tables, priority_taken, "european_football_2"
        )
        cubes.append(cube)
        if metric:
            metrics.append(metric)
    apply_name_priority(tables, views, cubes, metrics)
    # case-insensitive FS: table Player yields to cube player → Player_table
    assert "Player_table" in tables
    assert tables["Player_table"]["table"] == "Player"
    assert any(c["name"] == "player" for c in cubes)
    assert {c["name"].lower() for c in cubes}.isdisjoint({t.lower() for t in tables})


def test_parse_semantic_plan_hubs() -> None:
    text = """
hubs:
  - table: results
    cube: f1_race_results
    dimensions:
      - races
      - drivers
  - table: event
    cube: club_event
    dimensions: []
"""
    plan = parse_semantic_plan(text)
    assert [h["table"] for h in plan["hubs"]] == ["results", "event"]
    assert plan["hubs"][0]["cube"] == "f1_race_results"
    assert plan["hubs"][0]["dimensions"] == ["races", "drivers"]
    assert plan["hubs"][1]["dimensions"] == []


def test_load_semantic_plan_hub_counts() -> None:
    expected = {
        "formula_1": 7,
        "toxicology": 4,
        "california_schools": 2,
        "student_club": 6,
        "thrombosis_prediction": 3,
        "superhero": 3,
    }
    for db_id, n in expected.items():
        plan = load_semantic_plan(ROOT / db_id)
        assert plan is not None, db_id
        assert len(plan["hubs"]) == n, db_id
        assert all(h["table"] and h["cube"] for h in plan["hubs"])
        assert not any(h["cube"].endswith("_all") for h in plan["hubs"])


def test_hub_plan_splits_sibling_facts_and_skips_db_all() -> None:
    tables = {
        "races": _table(
            "races",
            ["raceId", "year", "circuitId"],
            ["raceId"],
            fk=[
                {
                    "columns": ["circuitId"],
                    "references": {"table": "circuits", "columns": ["circuitId"]},
                }
            ],
        ),
        "circuits": _table("circuits", ["circuitId", "name"], ["circuitId"]),
        "results": _table(
            "results",
            ["resultId", "raceId", "time"],
            ["resultId"],
            fk=[
                {
                    "columns": ["raceId"],
                    "references": {"table": "races", "columns": ["raceId"]},
                }
            ],
        ),
        "lapTimes": _table(
            "lapTimes",
            ["raceId", "lap", "milliseconds"],
            ["raceId", "lap"],
            fk=[
                {
                    "columns": ["raceId"],
                    "references": {"table": "races", "columns": ["raceId"]},
                }
            ],
        ),
    }
    views = views_from_tables(tables, "formula_1")
    assert len(views) == 1
    assert views[0]["name"] == "formula_1_all"
    assert {s["table"] for s in views[0]["sources"]} == {
        "races",
        "circuits",
        "results",
        "lapTimes",
    }

    plan = {
        "hubs": [
            {
                "table": "results",
                "cube": "f1_race_results",
                "dimensions": ["races", "circuits"],
            },
            {
                "table": "lapTimes",
                "cube": "f1_lap_times",
                "dimensions": ["races"],
            },
        ]
    }
    views = views_from_tables(tables, "formula_1", plan=plan)
    by = {v["name"]: v for v in views}
    assert "formula_1_all" not in by
    assert set(by) == {"f1_race_results", "f1_lap_times"}
    res_src = {s["table"] for s in by["f1_race_results"]["sources"]}
    assert res_src == {"results", "races", "circuits"}
    assert "lapTimes" not in res_src
    lap_src = {s["table"] for s in by["f1_lap_times"]["sources"]}
    assert lap_src == {"lapTimes", "races"}
    assert "circuits" not in lap_src and "results" not in lap_src


def test_hub_plan_single_table_and_bridge_reuse() -> None:
    tables = {
        "molecule": _table("molecule", ["molecule_id", "label"], ["molecule_id"]),
        "atom": _table(
            "atom",
            ["atom_id", "molecule_id"],
            ["atom_id"],
            fk=[
                {
                    "columns": ["molecule_id"],
                    "references": {"table": "molecule", "columns": ["molecule_id"]},
                }
            ],
        ),
        "bond": _table(
            "bond",
            ["bond_id", "molecule_id"],
            ["bond_id"],
            fk=[
                {
                    "columns": ["molecule_id"],
                    "references": {"table": "molecule", "columns": ["molecule_id"]},
                }
            ],
        ),
        "connected": _table(
            "connected",
            ["atom_id", "bond_id"],
            [],
            fk=[
                {
                    "columns": ["atom_id"],
                    "references": {"table": "atom", "columns": ["atom_id"]},
                },
                {
                    "columns": ["bond_id"],
                    "references": {"table": "bond", "columns": ["bond_id"]},
                },
            ],
        ),
        "event": _table("event", ["event_id", "event_name"], ["event_id"]),
    }
    plan = {
        "hubs": [
            {"table": "molecule", "cube": "tox_molecule", "dimensions": []},
            {"table": "atom", "cube": "tox_atom", "dimensions": ["molecule"]},
            {"table": "bond", "cube": "tox_bond", "dimensions": ["molecule"]},
            {
                "table": "connected",
                "cube": "tox_connected",
                "dimensions": ["atom", "bond"],
            },
            {"table": "event", "cube": "club_event", "dimensions": []},
        ]
    }
    views = views_from_tables(tables, "toxicology", plan=plan)
    by = {v["name"]: v for v in views}
    assert by["tox_molecule"]["joins"] == []
    assert {s["table"] for s in by["tox_molecule"]["sources"]} == {"molecule"}
    assert {s["table"] for s in by["tox_atom"]["sources"]} == {"atom", "molecule"}
    assert "bond" not in {s["table"] for s in by["tox_atom"]["sources"]}
    conn_src = {s["table"] for s in by["tox_connected"]["sources"]}
    assert conn_src == {"connected", "atom", "bond"}
    assert "molecule" not in conn_src
    assert by["club_event"]["joins"] == []
    assert {s["table"] for s in by["club_event"]["sources"]} == {"event"}
    atom_join = by["tox_atom"]["joins"][0]
    assert atom_join["relationship"] == "many_to_one"
    assert {j["relationship"] for j in by["tox_connected"]["joins"]} == {"many_to_one"}
    assert "molecule" not in conn_src


def test_hub_fk_to_dimension_is_many_to_one_and_sibling_is_not_absorbed() -> None:
    tables = {
        "results": _table(
            "results",
            ["resultId", "raceId", "time"],
            ["resultId"],
            fk=[{"columns": ["raceId"], "references": {"table": "races", "columns": ["raceId"]}}],
        ),
        "races": _table(
            "races",
            ["raceId", "year", "circuitId"],
            ["raceId"],
            fk=[
                {
                    "columns": ["circuitId"],
                    "references": {"table": "circuits", "columns": ["circuitId"]},
                }
            ],
        ),
        "circuits": _table("circuits", ["circuitId", "name"], ["circuitId"]),
        "lapTimes": _table(
            "lapTimes",
            ["raceId", "lap", "milliseconds"],
            ["raceId", "lap"],
            fk=[{"columns": ["raceId"], "references": {"table": "races", "columns": ["raceId"]}}],
        ),
    }
    plan = {
        "hubs": [
            {
                "table": "results",
                "cube": "f1_race_results",
                "dimensions": ["races", "circuits", "lapTimes"],
            },
            {"table": "lapTimes", "cube": "f1_lap_times", "dimensions": ["races"]},
        ]
    }
    views = views_from_tables(tables, "formula_1", plan=plan)
    by = {v["name"]: v for v in views}
    assert "lapTimes" not in {s["table"] for s in by["f1_race_results"]["sources"]}
    rels = {j["relationship"] for j in by["f1_race_results"]["joins"]}
    assert rels == {"many_to_one"}
    cube, _metric = cube_and_metric_from_view(
        by["f1_race_results"], tables, set(), "formula_1"
    )
    assert cube["grain"] == ["resultId"]


def test_non_pk_link_is_not_one_join_and_unspecified_relationship_is_unset() -> None:
    tables = {
        "orders": _table("orders", ["order_id", "note"], ["order_id"]),
        "items": _table(
            "items",
            ["item_id", "note"],
            ["item_id"],
            fk=[{"columns": ["note"], "references": {"table": "orders", "columns": ["note"]}}],
        ),
    }
    assert not edge_is_key_relationship(
        tables, "items", "orders", [("note", "note")]
    )
    plan = {
        "hubs": [
            {"table": "orders", "cube": "orders_hub", "dimensions": ["items"]},
        ]
    }
    views = views_from_tables(tables, "shop", plan=plan)
    assert views[0]["joins"] == []
    assert (
        classify_join_relationship(tables, "orders", "items", [("note", "note")])
        is None
    )
    both_pk = {
        "frpm": _table("frpm", ["CDSCode"], ["CDSCode"]),
        "schools": _table("schools", ["CDSCode"], ["CDSCode"]),
    }
    assert (
        classify_join_relationship(
            both_pk, "frpm", "schools", [("CDSCode", "CDSCode")]
        )
        == "one_to_one"
    )
    frpm_fk = {
        "frpm": _table(
            "frpm",
            ["CDSCode"],
            ["CDSCode"],
            fk=[
                {
                    "columns": ["CDSCode"],
                    "references": {"table": "schools", "columns": ["CDSCode"]},
                }
            ],
        ),
        "schools": _table("schools", ["CDSCode"], ["CDSCode"]),
    }
    assert (
        classify_join_relationship(
            frpm_fk,
            "frpm",
            "schools",
            [("CDSCode", "CDSCode")],
            hub_dimensions={"schools"},
        )
        == "many_to_one"
    )
    child = {
        "orders": _table("orders", ["order_id"], ["order_id"]),
        "items": _table(
            "items",
            ["item_id", "order_id"],
            ["item_id"],
            fk=[
                {
                    "columns": ["order_id"],
                    "references": {"table": "orders", "columns": ["order_id"]},
                }
            ],
        ),
    }
    assert (
        classify_join_relationship(
            child, "orders", "items", [("order_id", "order_id")]
        )
        == "one_to_many"
    )


def test_annotate_adds_relationship_and_grain_without_unlinking(tmp_path: Path) -> None:
    db = tmp_path / "toy"
    meta = db / "meta"
    meta.mkdir(parents=True)
    (db / "semantic_plan.yaml").write_text(
        "hubs:\n  - table: orders\n    cube: orders_hub\n    dimensions:\n      - customer\n",
        encoding="utf-8",
    )
    orders = _table(
        "orders",
        ["order_id", "customer_id"],
        ["order_id"],
        fk=[
            {
                "columns": ["customer_id"],
                "references": {"table": "customer", "columns": ["customer_id"]},
            }
        ],
    )
    customer = _table("customer", ["customer_id", "name"], ["customer_id"])
    view = {
        "kind": "view",
        "name": "orders_hub_view",
        "target": "toy",
        "combine": "join",
        "description": "keep me",
        "sources": [
            {"kind": "table", "table": "orders"},
            {"kind": "table", "table": "customer"},
        ],
        "joins": [
            {
                "left": "orders",
                "right": "customer",
                "type": "inner",
                "on": [{"left_column": "customer_id", "right_column": "customer_id"}],
            }
        ],
        "columns": [
            {
                "name": "order_id",
                "mappings": [{"source": "orders", "column": "order_id"}],
            }
        ],
    }
    locked = {
        "kind": "view",
        "name": "locked_view",
        "joins": [
            {
                "left": "orders",
                "right": "customer",
                "type": "left",
                "relationship": "one_to_one",
                "on": [{"left_column": "customer_id", "right_column": "customer_id"}],
            }
        ],
    }
    cube = {
        "kind": "cube",
        "name": "orders_hub",
        "target": "toy",
        "view": "orders_hub_view",
        "description": "keep cube",
        "dimensions": [],
        "facts": [],
    }
    extra = meta / "note.json"
    extra.write_text("{}\n", encoding="utf-8")
    for name, doc in {
        "orders.json": orders,
        "customer.json": customer,
        "orders_hub_view.json": view,
        "orders_hub.json": cube,
        "locked_view.json": locked,
    }.items():
        (meta / name).write_text(
            __import__("json").dumps(doc, indent=2) + "\n", encoding="utf-8"
        )
    changed = annotate_hub_relationships(db)
    assert extra.read_text(encoding="utf-8") == "{}\n"
    assert {p.name for p in changed} == {"orders_hub.json", "orders_hub_view.json"}
    view_out = __import__("json").loads((meta / "orders_hub_view.json").read_text())
    assert view_out["description"] == "keep me"
    assert view_out["joins"][0]["type"] == "inner"
    assert view_out["joins"][0]["relationship"] == "many_to_one"
    cube_out = __import__("json").loads((meta / "orders_hub.json").read_text())
    assert cube_out["description"] == "keep cube"
    assert cube_out["grain"] == ["order_id"]
    locked_out = __import__("json").loads((meta / "locked_view.json").read_text())
    assert locked_out["joins"][0]["relationship"] == "one_to_one"
    again = annotate_hub_relationships(db)
    assert again == []
    renamed = {
        "customer_table": _table("customer_table", ["customer_id"], ["customer_id"])
    }
    renamed["customer_table"]["table"] = "customer"
    assert resolve_catalog_table(renamed, "customer") == "customer_table"


if __name__ == "__main__":
    test_no_pk_to_pk_inference_for_superhero_style()
    test_preserves_cyclic_fk_edges_and_distinct_pk_fk_names()
    test_prefers_exact_name_fk_and_short_api_id()
    test_auto_cube_descriptions_are_empty()
    test_infers_table_id_column_joins_to_table_id()
    test_role_playing_lookup_expansion_for_superhero_colours()
    print("ok")
