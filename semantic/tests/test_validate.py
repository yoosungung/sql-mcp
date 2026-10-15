import json
from pathlib import Path

import pytest

from sql_mcp_semantic.validate import SchemaError, validate_document, validate_file, validate_tree

EXAMPLES = Path(__file__).resolve().parents[1] / "examples"


def test_example_tree_registers():
    docs = validate_tree(EXAMPLES)
    kinds = sorted(doc["kind"] for doc in docs)
    assert kinds == [
        "cube",
        "metric",
        "segment",
        "table",
        "table",
        "table",
        "view",
        "view",
        "view",
    ]


def test_cube_requires_description():
    with pytest.raises(SchemaError) as raised:
        validate_document(
            {
                "kind": "cube",
                "name": "sales",
                "view": "sales",
                "dimensions": [],
                "facts": [],
            }
        )
    assert any("description" in error for error in raised.value.errors)


def test_pk_column_must_exist():
    document = _table()
    document["pk"] = ["missing"]
    with pytest.raises(SchemaError) as raised:
        validate_document(document)
    assert any("pk" in error for error in raised.value.errors)


def test_combine_rejects_from():
    document = _from_view()
    document["combine"] = "from"
    with pytest.raises(SchemaError) as raised:
        validate_document(document)
    assert any("combine" in error for error in raised.value.errors)


def test_view_column_rejects_same_flags():
    document = _join_view()
    document["columns"][0]["same_name"] = True
    with pytest.raises(SchemaError) as raised:
        validate_document(document)
    assert any("same_name" in error for error in raised.value.errors)


def test_bundled_columns_use_the_same_column_name():
    document = _join_view()
    document["columns"][0]["mappings"][1]["column"] = "other_id"
    with pytest.raises(SchemaError) as raised:
        validate_document(document)
    assert any("같은 이름" in error for error in raised.value.errors)


def test_select_requires_a_summary_source():
    document = _select_view()
    for source in document["sources"]:
        source["summary"] = False
    with pytest.raises(SchemaError) as raised:
        validate_document(document)
    assert any("summary" in error for error in raised.value.errors)


def test_union_column_maps_every_source():
    document = _join_view()
    document["combine"] = "union"
    document.pop("joins")
    document["columns"][0]["mappings"] = [document["columns"][0]["mappings"][0]]
    with pytest.raises(SchemaError) as raised:
        validate_document(document)
    assert any("union" in error for error in raised.value.errors)


def test_filename_must_match_name(tmp_path: Path):
    path = tmp_path / "tables" / "other.json"
    path.parent.mkdir()
    path.write_text(json.dumps(_table()), encoding="utf-8")
    with pytest.raises(SchemaError) as raised:
        validate_file(path)
    assert any("파일명" in error for error in raised.value.errors)


def test_registry_rejects_unknown_table_reference():
    view = _from_view()
    view["sources"][0]["name"] = "missing"
    for column in view["columns"]:
        for mapping in column["mappings"]:
            mapping["source"] = "missing"
    with pytest.raises(SchemaError) as raised:
        validate_tree_documents([_table(), view])
    assert any("missing" in error for error in raised.value.errors)


def test_table_role_defaults_to_all():
    document = _table()
    validate_document(document)
    document["role"] = "all"
    validate_document(document)
    document["role"] = ["finance"]
    validate_document(document)
    document["role"] = []
    with pytest.raises(SchemaError) as raised:
        validate_document(document)
    assert any("role" in error for error in raised.value.errors)


def test_segment_role_defaults_to_all():
    document = {
        "kind": "segment",
        "name": "paid",
        "cube": "sales",
        "filter": {"expression": "status_code = 'PAID'"},
    }
    validate_document(document)
    document["role"] = "all"
    validate_document(document)
    document["role"] = ["finance"]
    validate_document(document)
    document["role"] = []
    with pytest.raises(SchemaError) as raised:
        validate_document(document)
    assert any("role" in error for error in raised.value.errors)


def test_metric_role_defaults_to_all():
    document = {
        "kind": "metric",
        "name": "revenue",
        "cube": "sales",
        "expression": "sum(sale_amount)",
        "aggregation": "group",
        "group_by": ["order_id"],
        "filter": None,
    }
    validate_document(document)
    document["role"] = "all"
    validate_document(document)
    document["role"] = ["finance"]
    validate_document(document)
    document["role"] = []
    with pytest.raises(SchemaError) as raised:
        validate_document(document)
    assert any("role" in error for error in raised.value.errors)


def test_role_defaults_to_all():
    document = _cube()
    del document["dimensions"][0]["role"]
    del document["facts"][0]["role"]
    validate_document(document)
    document["dimensions"][0]["role"] = "all"
    validate_document(document)
    document["facts"][0]["role"] = []
    with pytest.raises(SchemaError) as raised:
        validate_document(document)
    assert any("role" in error for error in raised.value.errors)


def test_dimension_type_is_code_date_time_datetime_or_string():
    document = _cube()
    document["dimensions"][0].pop("type")
    document["dimensions"][0]["code"] = False
    document["dimensions"][0]["date"] = True
    document["dimensions"][0]["time"] = False
    with pytest.raises(SchemaError) as raised:
        validate_document(document)
    assert any("type" in error for error in raised.value.errors)
    document = _cube()
    for dimension_type in ("code", "date", "time", "datetime", "string"):
        document["dimensions"][0]["type"] = dimension_type
        validate_document(document)


def test_dimension_and_fact_are_column_or_expression():
    both = _cube()
    both["dimensions"][0]["expression"] = "order_id"
    with pytest.raises(SchemaError):
        validate_document(both)
    neither = _cube()
    del neither["facts"][0]["column"]
    with pytest.raises(SchemaError):
        validate_document(neither)
    calculated = _cube()
    calculated["dimensions"].append(
        {
            "name": "order_label",
            "type": "string",
            "expression": "cast(order_id as varchar)",
            "default_filter": None,
            "encryption": False,
            "role": "all",
        }
    )
    calculated["facts"].append(
        {
            "name": "net_amount",
            "expression": "amount",
            "amount": True,
            "fx": False,
            "default_filter": None,
            "encryption": False,
            "role": "all",
        }
    )
    validate_document(calculated)


def test_expression_member_skips_view_column_check():
    from sql_mcp_semantic.validate import validate_registry

    cube = _cube()
    cube["facts"] = [
        {
            "name": "net_amount",
            "expression": "missing_column * 1",
            "amount": True,
            "fx": False,
            "default_filter": None,
            "encryption": False,
            "role": "all",
        }
    ]
    validate_registry([_table(), _from_view(), cube])


def test_group_by_requires_view_columns():
    document = {
        "kind": "metric",
        "name": "revenue",
        "cube": "sales",
        "expression": "sum(amount)",
        "aggregation": "group",
        "filter": None,
    }
    with pytest.raises(SchemaError) as raised:
        validate_document(document)
    assert any("group_by" in error for error in raised.value.errors)
    document["group_by"] = ["missing"]
    validate_document(document)
    from sql_mcp_semantic.validate import validate_registry

    with pytest.raises(SchemaError) as raised:
        validate_registry([_table(), _from_view(), _cube(), document])
    assert any("missing" in error for error in raised.value.errors)


def test_metric_aggregation_is_group_or_rollup():
    document = {
        "kind": "metric",
        "name": "revenue",
        "cube": "sales",
        "expression": "sum(sale_amount)",
        "aggregation": "sum",
        "filter": None,
    }
    with pytest.raises(SchemaError) as raised:
        validate_document(document)
    assert any("aggregation" in error for error in raised.value.errors)
    for aggregation in ("group", "rollup", "cube", "grouping_sets"):
        document["aggregation"] = aggregation
        if aggregation == "group":
            document["group_by"] = ["order_id"]
        else:
            document.pop("group_by", None)
        validate_document(document)


def test_metric_name_must_not_match_cube_column():
    from sql_mcp_semantic.validate import validate_registry

    with pytest.raises(SchemaError) as raised:
        validate_registry(
            [
                _table(),
                _from_view(),
                _cube(),
                {
                    "kind": "metric",
                    "name": "amount",
                    "cube": "sales",
                    "expression": "sum(amount)",
                    "aggregation": "group",
                    "group_by": ["order_id"],
                    "filter": None,
                },
            ]
        )
    assert any("amount" in error for error in raised.value.errors)


def validate_tree_documents(documents: list[dict]):
    from sql_mcp_semantic.validate import validate_registry

    for document in documents:
        validate_document(document)
    validate_registry(documents)


def _table() -> dict:
    return {
        "kind": "table",
        "name": "orders",
        "db": "dw",
        "schema": "sales",
        "table": "orders",
        "columns": [
            {"name": "order_id", "type": "bigint", "nullable": False},
            {"name": "amount", "type": "numeric", "nullable": False},
        ],
        "pk": ["order_id"],
        "fk": [],
        "permission_filters": [],
        "scope_filter": None,
        "default_filter": None,
    }


def _from_view() -> dict:
    return {
        "kind": "view",
        "name": "orders_view",
        "sources": [{"kind": "table", "name": "orders"}],
        "columns": [
            {
                "name": "order_id",
                "mappings": [{"source": "orders", "column": "order_id"}],
            },
            {
                "name": "amount",
                "mappings": [{"source": "orders", "column": "amount"}],
            },
        ],
    }


def _cube() -> dict:
    return {
        "kind": "cube",
        "name": "sales",
        "view": "orders_view",
        "description": "주문",
        "dimensions": [
            {
                "name": "order_id",
                "type": "string",
                "column": "order_id",
                "default_filter": None,
                "encryption": False,
                "role": "all",
            }
        ],
        "facts": [
            {
                "name": "amount",
                "column": "amount",
                "amount": True,
                "fx": False,
                "default_filter": None,
                "encryption": False,
                "role": "all",
            }
        ],
    }


def _join_view() -> dict:
    return {
        "kind": "view",
        "name": "orders_customer",
        "combine": "join",
        "sources": [
            {"kind": "table", "name": "orders"},
            {"kind": "table", "name": "customer"},
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
                "name": "customer_id",
                "mappings": [
                    {"source": "orders", "column": "customer_id"},
                    {"source": "customer", "column": "customer_id"},
                ],
            }
        ],
    }


def _select_view() -> dict:
    return {
        "kind": "view",
        "name": "sales",
        "combine": "select",
        "prefer": "summary",
        "sources": [
            {"kind": "view", "name": "monthly_orders", "summary": True},
            {"kind": "view", "name": "orders_customer", "summary": False},
        ],
        "columns": [
            {
                "name": "amount",
                "mappings": [
                    {"source": "monthly_orders", "column": "amount"},
                    {"source": "orders_customer", "column": "amount"},
                ],
            }
        ],
    }
