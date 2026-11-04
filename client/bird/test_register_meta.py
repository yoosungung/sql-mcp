"""register_meta prune 대상 계산."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from register_meta import PRUNE_KINDS, stale_docs_to_prune


def test_prune_drops_old_cube_and_all_view_on_same_target() -> None:
    local = [
        {"kind": "table", "name": "results", "target": "formula_1"},
        {"kind": "view", "name": "f1_race_results_view", "target": "formula_1"},
        {"kind": "cube", "name": "f1_race_results", "target": "formula_1", "view": "f1_race_results_view"},
        {"kind": "metric", "name": "avg_points", "cube": "f1_race_results"},
    ]
    catalog = [
        {"kind": "table", "name": "results", "target": "formula_1"},
        {"kind": "view", "name": "formula_1_all", "target": "formula_1"},
        {"kind": "cube", "name": "formula_1", "target": "formula_1", "view": "formula_1_all"},
        {"kind": "metric", "name": "formula_1_count", "cube": "formula_1"},
        {"kind": "segment", "name": "old_seg", "cube": "formula_1"},
        {"kind": "cube", "name": "card_games", "target": "card_games", "view": "card_games_all"},
        {"kind": "view", "name": "card_games_all", "target": "card_games"},
    ]
    stale = stale_docs_to_prune(catalog, local)
    kinds_names = [(d["kind"], d["name"]) for d in stale]
    assert kinds_names == [
        ("segment", "old_seg"),
        ("metric", "formula_1_count"),
        ("cube", "formula_1"),
        ("view", "formula_1_all"),
    ]


def test_prune_skips_view_still_held_by_kept_cube() -> None:
    local = [
        {"kind": "view", "name": "superhero_view", "target": "superhero"},
        {"kind": "cube", "name": "superhero", "target": "superhero", "view": "superhero_view"},
    ]
    catalog = [
        {"kind": "view", "name": "superhero_all", "target": "superhero"},
        {"kind": "cube", "name": "superhero", "target": "superhero", "view": "superhero_all"},
        {"kind": "cube", "name": "card_games", "target": "card_games"},
    ]
    stale = stale_docs_to_prune(catalog, local)
    assert stale == []


def test_prune_drops_old_view_after_cube_retarget() -> None:
    local = [
        {"kind": "view", "name": "superhero_view", "target": "superhero"},
        {"kind": "cube", "name": "superhero", "target": "superhero", "view": "superhero_view"},
    ]
    catalog = [
        {"kind": "view", "name": "superhero_all", "target": "superhero"},
        {"kind": "cube", "name": "superhero", "target": "superhero", "view": "superhero_view"},
        {"kind": "cube", "name": "card_games", "target": "card_games"},
    ]
    stale = stale_docs_to_prune(catalog, local)
    assert [(d["kind"], d["name"]) for d in stale] == [("view", "superhero_all")]


def test_prune_kinds_order_is_dependents_first() -> None:
    assert PRUNE_KINDS == ("segment", "metric", "cube", "view")


def test_prune_detaches_local_named_metric_from_dying_cube() -> None:
    local = [
        {"kind": "cube", "name": "tox_atom", "target": "toxicology", "view": "tox_atom_view"},
        {"kind": "view", "name": "tox_atom_view", "target": "toxicology"},
        {"kind": "metric", "name": "carbon_atom_pct", "cube": "tox_atom"},
    ]
    catalog = [
        {"kind": "cube", "name": "toxicology", "target": "toxicology", "view": "toxicology_all"},
        {"kind": "view", "name": "toxicology_all", "target": "toxicology"},
        {"kind": "metric", "name": "carbon_atom_pct", "cube": "toxicology"},
    ]
    stale = stale_docs_to_prune(catalog, local)
    assert [(d["kind"], d["name"]) for d in stale] == [
        ("metric", "carbon_atom_pct"),
        ("cube", "toxicology"),
        ("view", "toxicology_all"),
    ]
