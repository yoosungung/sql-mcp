import json
from pathlib import Path

import pytest

from sql_mcp_semantic.catalog import Catalog, CatalogError
from sql_mcp_semantic.validate import SchemaError


def test_create_persists_valid_table(tmp_path: Path):
    catalog = Catalog.open(tmp_path)
    created = catalog.create(_table())
    assert created["name"] == "orders"
    stored = json.loads((tmp_path / "tables" / "orders.json").read_text(encoding="utf-8"))
    assert stored["pk"] == ["order_id"]
    reopened = Catalog.open(tmp_path)
    assert reopened.read("table", "orders")["table"] == "orders"


def test_create_rejects_invalid_json_without_writing(tmp_path: Path):
    catalog = Catalog.open(tmp_path)
    with pytest.raises(SchemaError) as raised:
        catalog.create({"kind": "cube", "name": "sales", "view": "sales", "dimensions": [], "facts": []})
    assert any("description" in error for error in raised.value.errors)
    assert list(tmp_path.rglob("*.json")) == []


def test_create_rejects_missing_connection_without_writing(tmp_path: Path):
    catalog = Catalog.open(tmp_path)
    with pytest.raises(SchemaError) as raised:
        catalog.create(_view("missing"))
    assert any("missing" in error for error in raised.value.errors)
    assert list(tmp_path.rglob("*.json")) == []


def test_update_rejects_broken_link_and_keeps_file(tmp_path: Path):
    catalog = Catalog.open(tmp_path)
    catalog.create(_table())
    catalog.create(_view("orders"))
    catalog.create(_cube())
    broken = _view("orders")
    broken["columns"][0]["name"] = "other_id"
    with pytest.raises(SchemaError) as raised:
        catalog.update(broken)
    assert any("order_id" in error for error in raised.value.errors)
    assert catalog.read("view", "orders_view")["columns"][0]["name"] == "order_id"


def test_delete_rejects_when_still_referenced(tmp_path: Path):
    catalog = Catalog.open(tmp_path)
    catalog.create(_table())
    catalog.create(_view("orders"))
    with pytest.raises(SchemaError) as raised:
        catalog.delete("table", "orders")
    assert any("orders" in error for error in raised.value.errors)
    assert (tmp_path / "tables" / "orders.json").exists()
    catalog.delete("view", "orders_view")
    catalog.delete("table", "orders")
    assert catalog.list() == []


def test_duplicate_create_and_missing_read(tmp_path: Path):
    catalog = Catalog.open(tmp_path)
    catalog.create(_table())
    with pytest.raises(CatalogError) as raised:
        catalog.create(_table())
    assert any("이미 있다" in error for error in raised.value.errors)
    with pytest.raises(CatalogError) as raised:
        catalog.read("metric", "revenue")
    assert any("없다" in error for error in raised.value.errors)


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


def _view(table_name: str) -> dict:
    return {
        "kind": "view",
        "name": "orders_view",
        "sources": [{"kind": "table", "name": table_name}],
        "columns": [
            {
                "name": "order_id",
                "mappings": [{"source": table_name, "column": "order_id"}],
            }
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
        "facts": [],
    }
