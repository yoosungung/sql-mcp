"""generate_semantic_map diagram-first Markdown 단위 테스트."""

from __future__ import annotations

from pathlib import Path

from generate_semantic_map import (
    build_semantic_map,
    load_meta_docs,
    render_cube_canvas,
    render_view_column_table,
    render_view_graph,
)

FIXTURE = Path(__file__).resolve().parent.parent / "semantic" / "examples"


def _load_examples() -> dict[str, list[dict]]:
    return load_meta_docs(FIXTURE)


def test_load_meta_docs_groups_by_kind() -> None:
    docs = _load_examples()
    assert {d["name"] for d in docs["table"]} >= {"orders", "customer"}
    assert any(d["name"] == "orders_customer" for d in docs["view"])
    assert any(d["name"] == "sales" for d in docs["cube"])
    assert any(d["name"] == "revenue" for d in docs["metric"])
    assert any(d["name"] == "paid" for d in docs["segment"])


def test_view_graph_join_names_only() -> None:
    docs = _load_examples()
    graph = render_view_graph(docs["view"], docs["table"])
    assert "flowchart LR" in graph
    assert "`orders_customer`" in graph
    assert 'orders["orders"]' in graph
    assert 'customer["customer"]' in graph
    assert ' -- "inner · customer_id" --> ' in graph
    assert "**Source columns**" not in graph
    assert "**Column mappings**" in graph
    assert "erDiagram" not in graph


def test_view_column_mapping_pk_fk() -> None:
    docs = _load_examples()
    view = next(v for v in docs["view"] if v["name"] == "orders_customer")
    tables_by_name = {t["name"]: t for t in docs["table"]}
    table = render_view_column_table(view, tables_by_name)
    assert "| view column | orders | customer |" in table
    assert "`customer_id` (FK)" in table or "`customer_id` (PK, FK)" in table
    # orders.customer_id is FK; customer.id / orders.id typically PK
    assert "(PK)" in table
    assert "(FK)" in table


def test_view_column_mapping_table() -> None:
    docs = _load_examples()
    view = next(v for v in docs["view"] if v["name"] == "orders_customer")
    tables_by_name = {t["name"]: t for t in docs["table"]}
    table = render_view_column_table(view, tables_by_name)
    assert "| view column | orders | customer |" in table
    assert "`customer_id`" in table
    graph = render_view_graph(docs["view"], docs["table"])
    assert "**Column mappings**" in graph


def test_view_unmapped_columns_last_row() -> None:
    view = {
        "kind": "view",
        "name": "v",
        "combine": "join",
        "sources": [
            {"kind": "table", "name": "a"},
            {"kind": "table", "name": "b"},
        ],
        "columns": [
            {
                "name": "id",
                "mappings": [
                    {"source": "a", "column": "id"},
                    {"source": "b", "column": "id"},
                ],
            }
        ],
    }
    tables = {
        "a": {
            "name": "a",
            "pk": ["id"],
            "fk": [{"columns": ["orphan_a"]}],
            "columns": [
                {"name": "id", "type": "int", "nullable": False},
                {"name": "orphan_a", "type": "int", "nullable": True},
            ],
        },
        "b": {
            "name": "b",
            "pk": ["id"],
            "columns": [
                {"name": "id", "type": "int", "nullable": False},
                {"name": "orphan_b", "type": "text", "nullable": True},
            ],
        },
    }
    table = render_view_column_table(view, tables)
    assert "| `id` | `id` (PK) | `id` (PK) |" in table
    assert "| _(unmapped)_ | `orphan_a` (FK) | `orphan_b` |" in table


def test_cube_canvas_tables() -> None:
    docs = _load_examples()
    canvas = render_cube_canvas(docs["cube"], docs["metric"], docs["segment"])
    assert "`sales`" in canvas
    assert "| name | type | description | view column |" in canvas
    assert "| `revenue` |" in canvas
    assert "```mermaid" not in canvas


def test_build_semantic_map_structure() -> None:
    docs = _load_examples()
    md = build_semantic_map(docs, target="dw")
    assert "## A. Views" in md
    assert "## B. Cubes" in md
    assert "flowchart LR" in md
    assert "**Source columns**" not in md
    assert "**Column mappings**" in md
    assert "| view column | orders | customer |" in md
    assert "(PK)" in md or "(FK)" in md
