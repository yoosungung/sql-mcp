use super::*;
use serde_json::json;
use std::collections::HashSet;

fn example_docs() -> Vec<Value> {
    [
        include_str!("../../../semantic/examples/tables/customer.json"),
        include_str!("../../../semantic/examples/tables/orders.json"),
        include_str!("../../../semantic/examples/tables/orders_month.json"),
        include_str!("../../../semantic/examples/tables/region_codes.json"),
        include_str!("../../../semantic/examples/tables/status_codes.json"),
        include_str!("../../../semantic/examples/views/orders_customer.json"),
        include_str!("../../../semantic/examples/cubes/sales.json"),
        include_str!("../../../semantic/examples/metrics/revenue.json"),
        include_str!("../../../semantic/examples/segments/paid.json"),
    ]
    .into_iter()
    .map(|s| serde_json::from_str(s).unwrap())
    .collect()
}

#[test]
fn as_code_join_after_aggregation() {
    let docs = example_docs();
    let sql = compile_metric_select(&docs, "revenue", &["paid"], Some("branch")).unwrap();
    assert!(
        sql.contains("region_code__value") || sql.contains("\"region_code__value\""),
        "expected region_code__value in {sql}"
    );
    assert!(
        sql.contains("_sem_agg") || sql.contains("\"_sem_agg\""),
        "expected outer wrap alias in {sql}"
    );
    assert!(
        sql.to_ascii_uppercase().contains("LEFT JOIN"),
        "expected LEFT JOIN code dict in {sql}"
    );
    assert!(
        sql.contains("lang = 'ko'") || sql.contains("lang = \"ko\"") || sql.contains("lang='ko'"),
        "expected as_code.where in {sql}"
    );
    // Join must be outside GROUP BY aggregation
    let upper = sql.to_ascii_uppercase();
    let group_pos = upper.find("GROUP BY").expect("GROUP BY");
    let left_pos = upper.rfind("LEFT JOIN").expect("LEFT JOIN");
    assert!(
        left_pos > group_pos,
        "LEFT JOIN should wrap after GROUP BY: {sql}"
    );
    assert_select_only(&sql).unwrap();
}

#[test]
fn compiles_orders_with_filters() {
    let orders: Value = serde_json::from_str(include_str!(
        "../../../semantic/examples/tables/orders.json"
    ))
    .unwrap();
    let sql = compile_table_select(&orders, &["order_id", "region_code"], Some("branch")).unwrap();
    assert!(sql.contains("\"sales\".\"orders\""));
    assert!(sql.contains("order_date >="));
    assert!(sql.contains("status_code <>"));
    assert!(sql.contains("region_code = :caller_region"));
    assert_select_only(&sql).unwrap();
}

#[test]
fn rejects_bad_ident() {
    assert!(quote_ident("a;drop").is_err());
}

#[test]
fn physical_from_ok() {
    let doc = json!({"schema": "sales", "table": "orders"});
    assert_eq!(physical_from(&doc).unwrap(), "\"sales\".\"orders\"");
}

#[test]
fn compiles_revenue_metric_with_segment() {
    let docs = example_docs();
    let sql = compile_metric_select(&docs, "revenue", &["paid"], Some("branch")).unwrap();
    assert!(sql.contains("sum("));
    assert!(sql.contains("GROUP BY"));
    assert!(sql.contains("PAID"));
    // Step 6: table filters pushed into derived table
    assert!(sql.contains("SELECT * FROM"));
    assert!(sql.contains("region_code = :caller_region"));
    assert_select_only(&sql).unwrap();
}

#[test]
fn prunes_unused_customer_join() {
    let docs = example_docs();
    let sql = compile_metric_select(&docs, "revenue", &["paid"], Some("branch")).unwrap();
    // region_code/order_date/sale_amount/status map to orders first → customer pruned
    assert!(
        !sql.contains("AS \"customer\""),
        "expected customer prune, got {sql}"
    );
    assert!(sql.contains("AS \"orders\"") || sql.contains("\"orders\""));
}

#[test]
fn prunes_root_source_when_all_inner_joins_and_resolves_multi_mapped_col() {
    let mut docs = example_docs();
    for d in &mut docs {
        if d.get("kind").and_then(|v| v.as_str()) == Some("cube")
            && d.get("name").and_then(|v| v.as_str()) == Some("sales")
        {
            d.get_mut("dimensions")
                .and_then(|v| v.as_array_mut())
                .unwrap()
                .push(json!({
                    "name": "customer_status_code",
                    "description": "고객 상태 코드.",
                    "type": "code",
                    "column": "customer_status_code",
                    "encryption": false,
                    "role": "all"
                }));
        }
    }
    // Query only customer_status_code (from customer) and customer_id (multi-mapped: orders, customer).
    // Since all joins in orders_customer are inner and only customer is exclusively required,
    // orders (sources[0]) must be pruned and customer_id must resolve to "customer"."customer_id".
    let spec = CubeQuerySpec {
        projections: vec![
            CubeProjection {
                member: "customer_id".into(),
                alias: "customer_id".into(),
            },
            CubeProjection {
                member: "customer_status_code".into(),
                alias: "customer_status_code".into(),
            },
        ],
        dimensions: vec!["customer_id".into(), "customer_status_code".into()],
        ungrouped: true,
        distinct: false,
        ..Default::default()
    };
    let sql = compile_cube_query(&docs, "sales", &spec, Some("branch"), None).unwrap();
    assert!(
        !sql.contains("AS \"orders\""),
        "expected root source orders to be pruned when querying only customer columns, got: {sql}"
    );
    assert!(
        sql.contains("\"customer\".\"customer_id\""),
        "expected multi-mapped customer_id to resolve to included table customer, got: {sql}"
    );
}

#[test]
fn cyclic_view_joins_connect_directly_without_redundant_join_edges() {
    // 3 tables in a triangle: event, budget, attendance, member
    let docs = vec![
        json!({
            "kind": "table", "name": "member", "target": "dw", "db": "dw", "schema": "main", "table": "member",
            "columns": [{"name": "member_id", "type": "INT", "nullable": false}],
            "pk": ["member_id"], "fk": [], "permission_filters": [], "scope_filter": null, "default_filter": null
        }),
        json!({
            "kind": "table", "name": "event", "target": "dw", "db": "dw", "schema": "main", "table": "event",
            "columns": [{"name": "event_id", "type": "INT", "nullable": false}, {"name": "event_name", "type": "TEXT", "nullable": true}],
            "pk": ["event_id"], "fk": [], "permission_filters": [], "scope_filter": null, "default_filter": null
        }),
        json!({
            "kind": "table", "name": "budget", "target": "dw", "db": "dw", "schema": "main", "table": "budget",
            "columns": [{"name": "budget_id", "type": "INT", "nullable": false}, {"name": "spent", "type": "REAL", "nullable": true}, {"name": "link_to_event", "type": "INT", "nullable": true}],
            "pk": ["budget_id"], "fk": [], "permission_filters": [], "scope_filter": null, "default_filter": null
        }),
        json!({
            "kind": "view", "name": "club_view", "target": "dw", "combine": "join",
            "sources": [
                {"kind": "table", "table": "member"},
                {"kind": "table", "table": "event"},
                {"kind": "table", "table": "budget"}
            ],
            "joins": [
                {"left": "member", "right": "event", "type": "inner", "on": [{"left_column": "member_id", "right_column": "event_id"}]},
                {"left": "member", "right": "budget", "type": "inner", "on": [{"left_column": "member_id", "right_column": "budget_id"}]},
                {"left": "budget", "right": "event", "type": "inner", "on": [{"left_column": "link_to_event", "right_column": "event_id"}]}
            ],
            "columns": [
                {"name": "member_id", "mappings": [{"source": "member", "column": "member_id"}]},
                {"name": "event_name", "mappings": [{"source": "event", "column": "event_name"}]},
                {"name": "spent", "mappings": [{"source": "budget", "column": "spent"}]}
            ]
        }),
        json!({
            "kind": "cube", "name": "club", "target": "dw", "view": "club_view", "description": "club",
            "dimensions": [{"name": "event_name", "description": "event", "type": "string", "column": "event_name", "encryption": false}],
            "facts": [{"name": "spent", "description": "spent", "type": "amount", "column": "spent", "encryption": false}]
        }),
    ];
    let spec = CubeQuerySpec {
        projections: vec![
            CubeProjection {
                member: "event_name".into(),
                alias: "event_name".into(),
            },
            CubeProjection {
                member: "spent".into(),
                alias: "spent".into(),
            },
        ],
        dimensions: vec!["event_name".into(), "spent".into()],
        ungrouped: true,
        distinct: false,
        ..Default::default()
    };
    let sql = compile_cube_query(&docs, "club", &spec, None, None).unwrap();
    assert!(
        !sql.contains("\"member\""),
        "expected direct join between event and budget without root member, got: {sql}"
    );
    assert!(
        sql.contains("\"event\"") && sql.contains("\"budget\""),
        "expected event and budget in SQL, got: {sql}"
    );
}

#[test]
fn summary_view_routing_prefers_summary() {
    let mut docs = example_docs();
    docs.push(json!({
        "kind": "view",
        "name": "orders_only",
        "combine": "join",
        "sources": [{"kind": "table", "table": "orders"}],
        "joins": [],
        "columns": [
            {"name": "region_code", "mappings": [{"source": "orders", "column": "region_code"}]},
            {"name": "order_date", "mappings": [{"source": "orders", "column": "order_date"}]},
            {"name": "sale_amount", "mappings": [{"source": "orders", "column": "sale_amount"}]},
            {"name": "status_code", "mappings": [{"source": "orders", "column": "status_code"}]},
            {"name": "fx_rate", "mappings": [{"source": "orders", "column": "fx_rate"}]},
            {"name": "customer_id", "mappings": [{"source": "orders", "column": "customer_id"}]}
        ]
    }));
    docs.push(json!({
        "kind": "view",
        "name": "sales_select",
        "combine": "select",
        "prefer": "summary",
        "sources": [
            {"kind": "view", "table": "orders_only", "summary": true},
            {"kind": "view", "table": "orders_customer", "summary": false}
        ],
        "columns": []
    }));
    for d in &mut docs {
        if d.get("kind").and_then(|v| v.as_str()) == Some("cube")
            && d.get("name").and_then(|v| v.as_str()) == Some("sales")
        {
            d["view"] = json!("sales_select");
        }
    }
    let sql = compile_metric_select(&docs, "revenue", &["paid"], Some("branch")).unwrap();
    assert!(
        !sql.contains("AS \"customer\""),
        "summary route should use orders_only: {sql}"
    );
    assert!(sql.contains("sum("));
}

#[test]
fn rejects_unknown_metric_ident() {
    let mut docs = example_docs();
    for d in &mut docs {
        if d.get("kind").and_then(|v| v.as_str()) == Some("metric")
            && d.get("name").and_then(|v| v.as_str()) == Some("revenue")
        {
            d["expression"] = json!("sum(not_a_fact)");
        }
    }
    assert!(compile_metric_select(&docs, "revenue", &[], None).is_err());
}

#[test]
fn prunes_union_branch_with_bind_params() {
    let mut docs = example_docs();
    for d in &mut docs {
        if d.get("kind").and_then(|v| v.as_str()) == Some("table")
            && d.get("name").and_then(|v| v.as_str()) == Some("orders")
        {
            d["scope_filter"] = json!({
                "expression": "order_date >= '2020-01-01' AND order_date < '2021-01-01'"
            });
        }
        if d.get("kind").and_then(|v| v.as_str()) == Some("table")
            && d.get("name").and_then(|v| v.as_str()) == Some("orders_month")
        {
            d["scope_filter"] = json!({
                "expression": "order_date >= '2024-01-01' AND order_date < '2025-01-01'"
            });
        }
    }
    docs.push(json!({
        "kind": "view",
        "name": "orders_union",
        "combine": "union",
        "sources": [
            {"kind": "table", "table": "orders"},
            {"kind": "table", "table": "orders_month"}
        ],
        "columns": [
            {"name": "region_code", "mappings": [
                {"source": "orders", "column": "region_code"},
                {"source": "orders_month", "column": "region_code"}
            ]},
            {"name": "order_date", "mappings": [
                {"source": "orders", "column": "order_date"},
                {"source": "orders_month", "column": "order_month"}
            ]},
            {"name": "sale_amount", "mappings": [
                {"source": "orders", "column": "sale_amount"},
                {"source": "orders_month", "column": "sale_amount"}
            ]},
            {"name": "status_code", "mappings": [
                {"source": "orders", "column": "status_code"},
                {"source": "orders_month", "column": "status_code"}
            ]},
            {"name": "fx_rate", "mappings": [
                {"source": "orders", "column": "fx_rate"},
                {"source": "orders_month", "column": "fx_rate"}
            ]},
            {"name": "customer_id", "mappings": [
                {"source": "orders", "column": "customer_id"},
                {"source": "orders_month", "column": "region_code"}
            ]}
        ]
    }));
    for d in &mut docs {
        if d.get("kind").and_then(|v| v.as_str()) == Some("cube") {
            d["view"] = json!("orders_union");
        }
        if d.get("kind").and_then(|v| v.as_str()) == Some("metric")
            && d.get("name").and_then(|v| v.as_str()) == Some("revenue")
        {
            d["filter"] = json!({
                "expression": "order_date >= :q_from AND order_date < :q_to"
            });
        }
    }
    let mut binds = HashMap::new();
    binds.insert("q_from".into(), "2020-06-01".into());
    binds.insert("q_to".into(), "2020-07-01".into());
    let sql = compile_metric_select_with_binds(&docs, "revenue", &[], Some("branch"), Some(&binds))
        .unwrap();
    assert!(
        !sql.contains("orders_month"),
        "bind-resolved 2020 window should prune 2024 branch: {sql}"
    );
}

#[test]
fn prunes_union_branch_on_date_disjoint() {
    let mut docs = example_docs();
    for d in &mut docs {
        if d.get("kind").and_then(|v| v.as_str()) == Some("table")
            && d.get("name").and_then(|v| v.as_str()) == Some("orders")
        {
            d["scope_filter"] =
                json!({ "expression": "order_date >= '2020-01-01' AND order_date < '2021-01-01'" });
        }
        if d.get("kind").and_then(|v| v.as_str()) == Some("table")
            && d.get("name").and_then(|v| v.as_str()) == Some("orders_month")
        {
            d["scope_filter"] = json!({
                "expression": "order_date >= '2024-01-01' AND order_date < '2025-01-01'"
            });
            // align column name for interval extract
            if let Some(cols) = d.get_mut("columns").and_then(|v| v.as_array_mut()) {
                for c in cols.iter_mut() {
                    if c.get("name").and_then(|v| v.as_str()) == Some("order_month") {
                        c["name"] = json!("order_date");
                    }
                }
            }
        }
    }
    docs.push(json!({
        "kind": "view",
        "name": "orders_union",
        "combine": "union",
        "sources": [
            {"kind": "table", "table": "orders"},
            {"kind": "table", "table": "orders_month"}
        ],
        "columns": [
            {"name": "region_code", "mappings": [
                {"source": "orders", "column": "region_code"},
                {"source": "orders_month", "column": "region_code"}
            ]},
            {"name": "order_date", "mappings": [
                {"source": "orders", "column": "order_date"},
                {"source": "orders_month", "column": "order_date"}
            ]},
            {"name": "sale_amount", "mappings": [
                {"source": "orders", "column": "sale_amount"},
                {"source": "orders_month", "column": "sale_amount"}
            ]},
            {"name": "status_code", "mappings": [
                {"source": "orders", "column": "status_code"},
                {"source": "orders_month", "column": "status_code"}
            ]},
            {"name": "fx_rate", "mappings": [
                {"source": "orders", "column": "fx_rate"},
                {"source": "orders_month", "column": "fx_rate"}
            ]},
            {"name": "customer_id", "mappings": [
                {"source": "orders", "column": "customer_id"},
                {"source": "orders_month", "column": "region_code"}
            ]}
        ]
    }));
    for d in &mut docs {
        if d.get("kind").and_then(|v| v.as_str()) == Some("cube")
            && d.get("name").and_then(|v| v.as_str()) == Some("sales")
        {
            d["view"] = json!("orders_union");
        }
        if d.get("kind").and_then(|v| v.as_str()) == Some("metric")
            && d.get("name").and_then(|v| v.as_str()) == Some("revenue")
        {
            d["filter"] = json!({
                "expression": "order_date >= '2020-06-01' AND order_date < '2020-07-01'"
            });
        }
    }
    let sql = compile_metric_select(&docs, "revenue", &[], Some("branch")).unwrap();
    assert!(
        !sql.contains("orders_month"),
        "2024 branch should be pruned vs 2020 query: {sql}"
    );
    assert!(sql.contains("2020") || sql.contains("orders"), "{sql}");
    assert_select_only(&sql).unwrap();
}

#[test]
fn compiles_union_view_and_prunes_dead_scope() {
    let mut docs = example_docs();
    // Dead branch: unsatisfiable scope
    for d in &mut docs {
        if d.get("kind").and_then(|v| v.as_str()) == Some("table")
            && d.get("name").and_then(|v| v.as_str()) == Some("orders_month")
        {
            d["scope_filter"] = json!({ "expression": "1=0" });
        }
    }
    docs.push(json!({
        "kind": "view",
        "name": "orders_union",
        "combine": "union",
        "sources": [
            {"kind": "table", "table": "orders"},
            {"kind": "table", "table": "orders_month"}
        ],
        "columns": [
            {"name": "region_code", "mappings": [
                {"source": "orders", "column": "region_code"},
                {"source": "orders_month", "column": "region_code"}
            ]},
            {"name": "order_date", "mappings": [
                {"source": "orders", "column": "order_date"},
                {"source": "orders_month", "column": "order_month"}
            ]},
            {"name": "sale_amount", "mappings": [
                {"source": "orders", "column": "sale_amount"},
                {"source": "orders_month", "column": "sale_amount"}
            ]},
            {"name": "status_code", "mappings": [
                {"source": "orders", "column": "status_code"},
                {"source": "orders_month", "column": "status_code"}
            ]},
            {"name": "fx_rate", "mappings": [
                {"source": "orders", "column": "fx_rate"},
                {"source": "orders_month", "column": "fx_rate"}
            ]},
            {"name": "customer_id", "mappings": [
                {"source": "orders", "column": "customer_id"},
                {"source": "orders_month", "column": "region_code"}
            ]}
        ]
    }));
    for d in &mut docs {
        if d.get("kind").and_then(|v| v.as_str()) == Some("cube")
            && d.get("name").and_then(|v| v.as_str()) == Some("sales")
        {
            d["view"] = json!("orders_union");
        }
    }
    let sql = compile_metric_select(&docs, "revenue", &["paid"], Some("branch")).unwrap();
    assert!(
        sql.contains("UNION ALL") || !sql.contains("orders_month"),
        "{sql}"
    );
    assert!(
        !sql.contains("orders_month") || sql.matches("orders_month").count() == 0,
        "dead scope branch should be pruned: {sql}"
    );
    assert!(
        sql.contains("sum(") || sql.to_ascii_lowercase().contains("sum"),
        "{sql}"
    );
    assert_select_only(&sql).unwrap();
}

#[test]
fn ungrouped_detail_bypasses_summary_view_while_distinct_allows_it() {
    let mut docs = example_docs();
    docs.push(json!({
            "kind": "view",
            "name": "orders_month_summary_view",
            "combine": "join",
            "sources": [{"kind": "table", "table": "orders_month"}],
            "joins": [],
            "columns": [
                {"name": "region_code", "mappings": [{"source": "orders_month", "column": "region_code"}]},
                {"name": "sale_amount", "mappings": [{"source": "orders_month", "column": "sale_amount"}]}
            ]
        }));
    docs.push(json!({
        "kind": "view",
        "name": "sales_select",
        "combine": "select",
        "prefer": "summary",
        "sources": [
            {"kind": "view", "table": "orders_month_summary_view", "summary": true},
            {"kind": "view", "table": "orders_customer", "summary": false}
        ],
        "columns": []
    }));
    for d in &mut docs {
        if d.get("kind").and_then(|v| v.as_str()) == Some("cube")
            && d.get("name").and_then(|v| v.as_str()) == Some("sales")
        {
            d["view"] = json!("sales_select");
        }
    }

    // Detail ungrouped query (non-DISTINCT) must bypass summary: true and use orders_customer (orders).
    let detail_spec = CubeQuerySpec {
        projections: vec![CubeProjection {
            member: "region_code".into(),
            alias: "region_code".into(),
        }],
        dimensions: vec!["region_code".into()],
        ungrouped: true,
        distinct: false,
        ..Default::default()
    };
    let detail_sql =
        compile_cube_query(&docs, "sales", &detail_spec, Some("branch"), None).unwrap();
    assert!(
        !detail_sql.contains("orders_month"),
        "non-DISTINCT ungrouped detail query must bypass summary view: {detail_sql}"
    );
    assert!(detail_sql.contains("orders"), "{detail_sql}");

    // SELECT DISTINCT ungrouped query is allowed to route to summary: true view.
    let distinct_spec = CubeQuerySpec {
        distinct: true,
        ..detail_spec
    };
    let distinct_sql =
        compile_cube_query(&docs, "sales", &distinct_spec, Some("branch"), None).unwrap();
    assert!(
        distinct_sql.contains("SELECT DISTINCT"),
        "expected SELECT DISTINCT: {distinct_sql}"
    );
    assert!(
        distinct_sql.contains("orders_month"),
        "SELECT DISTINCT ungrouped query may route to summary view: {distinct_sql}"
    );
}

#[test]
fn role_playing_alias_joins_same_catalog_table_twice() {
    let docs = vec![
        json!({
            "kind": "table", "name": "Team", "target": "dw", "db": "dw", "schema": "main", "table": "Team",
            "columns": [
                {"name": "team_api_id", "type": "INT", "nullable": false},
                {"name": "team_long_name", "type": "TEXT", "nullable": true}
            ],
            "pk": ["team_api_id"], "fk": [], "permission_filters": [], "scope_filter": null, "default_filter": null
        }),
        json!({
            "kind": "table", "name": "Match", "target": "dw", "db": "dw", "schema": "main", "table": "Match",
            "columns": [
                {"name": "id", "type": "INT", "nullable": false},
                {"name": "home_team_api_id", "type": "INT", "nullable": true},
                {"name": "away_team_api_id", "type": "INT", "nullable": true},
                {"name": "home_team_goal", "type": "INT", "nullable": true}
            ],
            "pk": ["id"], "fk": [], "permission_filters": [], "scope_filter": null, "default_filter": null
        }),
        json!({
            "kind": "view", "name": "match_view", "target": "dw", "combine": "join",
            "sources": [
                {"kind": "table", "table": "Match"},
                {"kind": "table", "table": "Team"},
                {"kind": "table", "table": "Team", "alias": "away_team"}
            ],
            "joins": [
                {"left": "Match", "right": "Team", "type": "inner",
                 "on": [{"left_column": "home_team_api_id", "right_column": "team_api_id"}]},
                {"left": "Match", "right": "away_team", "type": "inner",
                 "on": [{"left_column": "away_team_api_id", "right_column": "team_api_id"}]}
            ],
            "columns": [
                {"name": "home_team_goal", "mappings": [{"source": "Match", "column": "home_team_goal"}]},
                {"name": "team_long_name", "mappings": [{"source": "Team", "column": "team_long_name"}]},
                {"name": "away_team_long_name", "mappings": [{"source": "away_team", "column": "team_long_name"}]}
            ]
        }),
        json!({
            "kind": "cube", "name": "match", "target": "dw", "view": "match_view", "description": "",
            "dimensions": [
                {"name": "team_long_name", "description": "", "type": "string", "column": "team_long_name", "encryption": false},
                {"name": "away_team_long_name", "description": "", "type": "string", "column": "away_team_long_name", "encryption": false}
            ],
            "facts": [
                {"name": "home_team_goal", "description": "", "type": "amount", "column": "home_team_goal", "encryption": false}
            ]
        }),
    ];
    let spec = CubeQuerySpec {
        projections: vec![
            CubeProjection {
                member: "team_long_name".into(),
                alias: "team_long_name".into(),
            },
            CubeProjection {
                member: "away_team_long_name".into(),
                alias: "away_team_long_name".into(),
            },
            CubeProjection {
                member: "home_team_goal".into(),
                alias: "home_team_goal".into(),
            },
        ],
        dimensions: vec!["team_long_name".into(), "away_team_long_name".into()],
        ungrouped: false,
        distinct: false,
        ..Default::default()
    };
    let sql = compile_cube_query(&docs, "match", &spec, None, None).unwrap();
    assert!(sql.contains("\"Team\""), "home Team alias missing: {sql}");
    assert!(
        sql.contains("\"away_team\""),
        "away_team alias missing: {sql}"
    );
    let team_as = sql.matches("AS \"Team\"").count() + sql.matches("AS \"away_team\"").count();
    assert!(
        team_as >= 2,
        "expected Team phys joined twice under two aliases: {sql}"
    );
    assert_select_only(&sql).unwrap();
}

fn grain_view(relationship: &str) -> Value {
    json!({
        "combine": "join",
        "joins": [
            {
                "left": "orders",
                "right": "items",
                "type": "inner",
                "relationship": relationship,
                "on": [{"left_column": "order_id", "right_column": "order_id"}]
            },
            {
                "left": "orders",
                "right": "payments",
                "type": "inner",
                "relationship": "many_to_one",
                "on": [{"left_column": "payment_id", "right_column": "payment_id"}]
            }
        ]
    })
}

#[test]
fn many_to_one_aggregate_and_missing_relationship_pass() {
    let included = HashSet::from(["orders".into(), "items".into(), "payments".into()]);
    let mut view = grain_view("many_to_one");
    super::view_prune::assert_no_fanout_or_chasm(&view, &included, true).unwrap();
    view["joins"][0]
        .as_object_mut()
        .unwrap()
        .remove("relationship");
    super::view_prune::assert_no_fanout_or_chasm(&view, &included, true).unwrap();
    let detail = grain_view("one_to_many");
    super::view_prune::assert_no_fanout_or_chasm(&detail, &included, false).unwrap();
}

#[test]
fn aggregating_one_to_many_is_fanout_and_two_are_chasm() {
    let included = HashSet::from(["orders".into(), "items".into(), "payments".into()]);
    let err =
        super::view_prune::assert_no_fanout_or_chasm(&grain_view("one_to_many"), &included, true)
            .unwrap_err()
            .to_string();
    assert!(err.contains("Fan-out rejected"), "{err}");
    assert!(err.contains("orders"), "{err}");
    let mut chasm = grain_view("one_to_many");
    chasm["joins"][1]["relationship"] = json!("one_to_many");
    let err = super::view_prune::assert_no_fanout_or_chasm(&chasm, &included, true)
        .unwrap_err()
        .to_string();
    assert!(err.contains("Chasm rejected"), "{err}");
    let pruned = HashSet::from(["orders".into()]);
    super::view_prune::assert_no_fanout_or_chasm(&grain_view("one_to_many"), &pruned, true)
        .unwrap();
}

fn docs_with_relationship(rel: &str) -> Vec<Value> {
    let mut docs = example_docs();
    for doc in &mut docs {
        if doc.get("name").and_then(|v| v.as_str()) == Some("orders_customer") {
            doc["joins"][0]["relationship"] = json!(rel);
        }
        if rel == "one_to_many" && doc.get("name").and_then(|v| v.as_str()) == Some("sales") {
            doc["dimensions"].as_array_mut().unwrap().push(json!({
                "name": "customer_status_code",
                "description": "Customer status on the customer table.",
                "type": "string",
                "column": "customer_status_code",
                "encryption": false,
                "role": "all"
            }));
        }
    }
    docs
}

#[test]
fn many_to_one_label_does_not_change_physical_sql() {
    let before =
        compile_metric_select(&example_docs(), "revenue", &["paid"], Some("branch")).unwrap();
    std::env::set_var("SQL_MCP_GRAIN_GUARD", "dw");
    let after = compile_metric_select(
        &docs_with_relationship("many_to_one"),
        "revenue",
        &["paid"],
        Some("branch"),
    )
    .unwrap();
    std::env::remove_var("SQL_MCP_GRAIN_GUARD");
    assert_eq!(before, after);
}

#[test]
fn grain_guard_defaults_off_and_rejects_only_when_target_is_listed() {
    std::env::remove_var("SQL_MCP_GRAIN_GUARD");
    let docs = docs_with_relationship("one_to_many");
    let spec = CubeQuerySpec {
        metrics: vec!["revenue".into()],
        dimensions: vec!["customer_status_code".into()],
        ..Default::default()
    };
    let off = compile_cube_query(&docs, "sales", &spec, Some("branch"), None).unwrap();
    assert!(off.to_ascii_uppercase().contains("SELECT"), "{off}");
    std::env::set_var("SQL_MCP_GRAIN_GUARD", "other_db");
    let other = compile_cube_query(&docs, "sales", &spec, Some("branch"), None).unwrap();
    assert_eq!(off, other);
    std::env::set_var("SQL_MCP_GRAIN_GUARD", "dw");
    let err = compile_cube_query(&docs, "sales", &spec, Some("branch"), None)
        .unwrap_err()
        .to_string();
    std::env::remove_var("SQL_MCP_GRAIN_GUARD");
    assert!(err.contains("Fan-out rejected"), "{err}");
}
