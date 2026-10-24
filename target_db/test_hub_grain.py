"""Hub grain: yaml sources ⊆ hub+dims, agent SQL FROM remap."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from hub_grain import (
    allowed_tables,
    cube_for_gold,
    extra_compiled_tables,
    local_view_grain_failures,
    remap_agent_sql,
)


def test_local_hub_views_stay_inside_plan() -> None:
    assert local_view_grain_failures() == []


def test_allowed_tables_include_hub_and_dims() -> None:
    names = allowed_tables(
        {"table": "results", "cube": "f1_race_results", "dimensions": ["races", "drivers"]}
    )
    assert names == {"results", "races", "drivers"}
    assert "laptimes" not in names


def test_cube_for_gold_picks_fact_hub() -> None:
    gold = "SELECT COUNT(*) FROM results JOIN races ON results.raceId = races.raceId"
    assert cube_for_gold("formula_1", gold) == "f1_race_results"
    gold_lap = "SELECT milliseconds FROM lapTimes JOIN drivers ON lapTimes.driverId = drivers.driverId"
    assert cube_for_gold("formula_1", gold_lap) == "f1_lap_times"


def test_remap_replaces_old_cube_from_only() -> None:
    sql = "SELECT COUNT(resultId) FROM formula_1 WHERE year = 2010"
    gold = "SELECT COUNT(*) FROM results WHERE year = 2010"
    out = remap_agent_sql(sql, "formula_1", gold)
    assert "FROM f1_race_results" in out
    assert "FROM formula_1" not in out


def test_remap_leaves_sql_when_no_hub_in_gold() -> None:
    sql = "SELECT 1 FROM formula_1"
    assert remap_agent_sql(sql, "formula_1", "SELECT 1") == sql


def test_extra_compiled_tables_flags_sibling_fact() -> None:
    hub = {
        "table": "results",
        "cube": "f1_race_results",
        "dimensions": ["races", "drivers", "constructors", "circuits", "status"],
    }
    sql = 'SELECT 1 FROM "results" INNER JOIN "lapTimes" ON 1=1'
    assert extra_compiled_tables(sql, hub) == {"laptimes"}
    clean = 'SELECT 1 FROM "results" INNER JOIN "races" ON 1=1'
    assert extra_compiled_tables(clean, hub) == set()
