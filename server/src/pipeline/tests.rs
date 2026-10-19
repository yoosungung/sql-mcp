    use super::*;
    use serde_json::{Value, json};

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

    fn docs_with_second_cube(target: &str) -> Vec<Value> {
        let mut docs = example_docs();
        // Second cube same view/target for JOIN/UNION tests.
        let mut sales_b: Value = docs
            .iter()
            .find(|d| d.get("name").and_then(|v| v.as_str()) == Some("sales"))
            .cloned()
            .unwrap();
        sales_b["name"] = json!("sales_b");
        sales_b["target"] = json!(target);
        docs.push(sales_b);
        docs.push(json!({
            "kind": "metric",
            "name": "revenue_b",
            "description": "sales_b 지역·일자별 판매 금액 합계.",
            "cube": "sales_b",
            "expression": "sum(sale_amount)",
            "aggregation": "group",
            "group_by": ["region_code", "order_date"],
            "filter": null,
            "role": "all"
        }));
        docs
    }

    #[test]
    fn compiles_revenue_with_segment() {
        let docs = example_docs();
        let sql = compile_semantic_sql(
            &docs,
            "SELECT revenue FROM sales WHERE paid",
            Some("branch"),
        )
        .unwrap();
        assert!(sql.to_ascii_lowercase().contains("sum("));
        assert!(sql.contains("PAID") || sql.contains("paid") || sql.contains("'PAID'"));
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn compiles_union_all_same_target() {
        let docs = docs_with_second_cube("dw");
        let sql = compile_semantic_sql(
            &docs,
            "SELECT revenue FROM sales UNION ALL SELECT revenue_b FROM sales_b",
            None,
        )
        .unwrap();
        let u = sql.to_ascii_uppercase();
        assert!(u.contains("UNION ALL"));
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn rejects_union_different_target() {
        let mut docs = docs_with_second_cube("other");
        // sales stays dw; sales_b is other
        let err = compile_semantic_sql(
            &docs,
            "SELECT revenue FROM sales UNION ALL SELECT revenue_b FROM sales_b",
            None,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)), "{err:?}");
        let _ = &mut docs;
    }

    #[test]
    fn compiles_join_same_target() {
        let docs = docs_with_second_cube("dw");
        let sql = compile_semantic_sql(
            &docs,
            "SELECT sales.revenue, sales_b.revenue_b \
             FROM sales JOIN sales_b ON sales.region_code = sales_b.region_code",
            None,
        )
        .unwrap();
        let u = sql.to_ascii_uppercase();
        assert!(u.contains("JOIN"));
        assert!(u.contains("REGION_CODE"));
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn rejects_join_different_target() {
        let docs = docs_with_second_cube("other");
        let err = compile_semantic_sql(
            &docs,
            "SELECT sales.revenue, sales_b.revenue_b \
             FROM sales JOIN sales_b ON sales.region_code = sales_b.region_code",
            None,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)), "{err:?}");
    }

    #[test]
    fn rejects_physical_from() {
        let docs = example_docs();
        let err = compile_semantic_sql(
            &docs,
            "SELECT order_id FROM sales.orders",
            None,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            AppError::Validation(_) | AppError::NotFound(_)
        ));
    }

    #[test]
    fn rejects_bare_physical_select() {
        let docs = example_docs();
        assert!(compile_semantic_sql(&docs, "SELECT 1", None).is_err());
    }

    #[test]
    fn mysql_dialect_adapts_identifier_quotes() {
        let docs = example_docs();
        let sql = compile_semantic_sql_with_binds(
            &docs,
            "SELECT revenue FROM sales WHERE paid",
            Some("branch"),
            None,
            DialectProfile::Mysql,
        )
        .unwrap();
        assert!(sql.contains('`'), "expected MySQL backticks in {sql}");
        assert!(
            !sql.contains('"'),
            "canonical double quotes should be adapted away: {sql}"
        );
        assert!(sql.to_ascii_lowercase().contains("sum("));
    }

    #[test]
    fn compiles_multiple_metrics_with_distinct_filters_and_dynamic_group_by() {
        let mut docs = example_docs();
        docs.push(json!({
            "kind": "metric",
            "name": "paid_revenue",
            "description": "결제 완료 판매 금액.",
            "cube": "sales",
            "expression": "sum(sale_amount)",
            "aggregation": "group",
            "group_by": ["region_code", "order_date"],
            "filter": {"expression": "status_code = 'PAID'"},
            "role": "all"
        }));
        let mut binds = HashMap::new();
        binds.insert("caller_region".into(), "KR".into());
        let sql = compile_semantic_sql_with_binds(
            &docs,
            "SELECT region_code AS reg, revenue AS total_rev, paid_revenue AS paid_rev \
             FROM sales \
             WHERE region_code = :caller_region \
             HAVING revenue > 100 \
             ORDER BY total_rev DESC \
             LIMIT 10 OFFSET 5",
            Some("branch"),
            Some(&binds),
            DialectProfile::Postgres,
        )
        .unwrap();
        let upper = sql.to_ascii_uppercase();
        assert!(upper.contains("CASE WHEN"), "expected CASE WHEN for distinct metric filters: {sql}");
        assert!(upper.contains("HAVING"), "expected HAVING clause: {sql}");
        assert!(upper.contains("ORDER BY \"TOTAL_REV\" DESC"), "expected ORDER BY total_rev DESC: {sql}");
        assert!(upper.contains("LIMIT 10"), "expected LIMIT 10: {sql}");
        assert!(upper.contains("OFFSET 5"), "expected OFFSET 5: {sql}");
        assert!(sql.contains("'KR'"), "expected bind :caller_region resolved to 'KR': {sql}");
        // Dynamic group_by: only region_code was projected, so order_date should not be in GROUP BY
        assert!(!sql.contains("order_date\" AS"), "order_date should not be projected: {sql}");
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn enforces_metric_segment_and_dimension_rbac_and_encryption_guards() {
        let mut docs = example_docs();
        docs.push(json!({
            "kind": "metric",
            "name": "finance_metric",
            "description": "재무 전용 지표.",
            "cube": "sales",
            "expression": "sum(sale_amount)",
            "aggregation": "group",
            "group_by": ["region_code"],
            "filter": null,
            "role": ["finance"]
        }));
        // Restricted metric rejected for branch role
        assert!(matches!(
            compile_semantic_sql(&docs, "SELECT finance_metric FROM sales", Some("branch")),
            Err(AppError::Forbidden(_))
        ));
        // Allowed for finance role
        assert!(compile_semantic_sql(&docs, "SELECT finance_metric FROM sales", Some("finance")).is_ok());

        // Mark customer_id as encrypted in sales cube and verify WHERE filter is rejected
        for doc in &mut docs {
            if doc.get("kind").and_then(|v| v.as_str()) == Some("cube")
                && doc.get("name").and_then(|v| v.as_str()) == Some("sales")
            {
                if let Some(dims) = doc.get_mut("dimensions").and_then(|v| v.as_array_mut()) {
                    for d in dims {
                        if d.get("name").and_then(|v| v.as_str()) == Some("customer_id") {
                            d["encryption"] = json!(true);
                        }
                    }
                }
            }
        }
        assert!(matches!(
            compile_semantic_sql(
                &docs,
                "SELECT revenue FROM sales WHERE customer_id = 'C1'",
                Some("branch")
            ),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn rejects_unknown_member_in_join_on() {
        let docs = docs_with_second_cube("dw");
        let err = compile_semantic_sql(
            &docs,
            "SELECT sales.revenue, sales_b.revenue_b \
             FROM sales JOIN sales_b ON sales.nonexistent = sales_b.region_code",
            None,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)), "{err:?}");
    }

    #[test]
    fn compiles_dim_only_ungrouped() {
        let docs = example_docs();
        let sql = compile_semantic_sql(
            &docs,
            "SELECT region_code FROM sales WHERE region_code = 'KR'",
            Some("finance"),
        )
        .unwrap();
        let upper = sql.to_ascii_uppercase();
        assert!(!upper.contains("GROUP BY"), "ungrouped must omit GROUP BY: {sql}");
        assert!(!upper.contains("SUM("), "ungrouped must not aggregate: {sql}");
        assert!(
            sql.contains("region_code") || sql.contains("\"region_code\""),
            "{sql}"
        );
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn compiles_fact_and_dim_ungrouped() {
        let docs = example_docs();
        let sql = compile_semantic_sql(
            &docs,
            "SELECT region_code, sale_amount FROM sales LIMIT 5",
            Some("finance"),
        )
        .unwrap();
        let upper = sql.to_ascii_uppercase();
        assert!(!upper.contains("GROUP BY"), "{sql}");
        assert!(upper.contains("LIMIT"), "{sql}");
        assert!(
            sql.contains("sale_amount") || sql.contains("\"sale_amount\""),
            "{sql}"
        );
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn rejects_having_in_dim_fact_ungrouped() {
        let docs = example_docs();
        let err = compile_semantic_sql(
            &docs,
            "SELECT region_code FROM sales HAVING region_code = 'KR'",
            Some("finance"),
        )
        .unwrap_err();
        assert!(
            matches!(err, AppError::Validation(ref m) if m.to_ascii_lowercase().contains("having") || m.contains("ungrouped")),
            "{err:?}"
        );
    }

    #[test]
    fn compiles_dim_only_group_by_and_having_and_agg_order_by() {
        let docs = example_docs();
        let sql = compile_semantic_sql(
            &docs,
            "SELECT region_code FROM sales GROUP BY region_code",
            Some("finance"),
        )
        .unwrap();
        let upper = sql.to_ascii_uppercase();
        assert!(upper.contains("GROUP BY"), "expected GROUP BY in {sql}");
        assert_select_only(&sql).unwrap();

        let sql_having = compile_semantic_sql(
            &docs,
            "SELECT region_code FROM sales WHERE status_code <> 'CANCEL' \
             GROUP BY region_code HAVING COUNT(*) > 20 ORDER BY SUM(sale_amount) DESC LIMIT 1",
            Some("finance"),
        )
        .unwrap();
        let u2 = sql_having.to_ascii_uppercase();
        assert!(u2.contains("GROUP BY"), "{sql_having}");
        assert!(u2.contains("HAVING COUNT(*) > 20"), "{sql_having}");
        assert!(u2.contains("ORDER BY SUM("), "{sql_having}");
        assert_select_only(&sql_having).unwrap();
    }

    #[test]
    fn rejects_ungrouped_cube_join_without_distinct_and_allows_distinct_or_grouped() {
        let docs = docs_with_second_cube("dw");
        // Non-DISTINCT ungrouped Cube JOIN is rejected to prevent Chasm Trap fan-out.
        let err = compile_semantic_sql(
            &docs,
            "SELECT sales.region_code, sales_b.region_code \
             FROM sales JOIN sales_b ON sales.region_code = sales_b.region_code",
            Some("finance"),
        )
        .unwrap_err();
        assert!(
            matches!(err, AppError::Validation(ref m) if m.contains("DISTINCT") || m.contains("fan-out")),
            "{err:?}"
        );

        // SELECT DISTINCT ungrouped Cube JOIN is allowed.
        let distinct_sql = compile_semantic_sql(
            &docs,
            "SELECT DISTINCT sales.region_code, sales_b.region_code AS reg_b \
             FROM sales JOIN sales_b ON sales.region_code = sales_b.region_code",
            Some("finance"),
        )
        .unwrap();
        let u = distinct_sql.to_ascii_uppercase();
        assert!(u.contains("SELECT DISTINCT"), "{distinct_sql}");
        assert!(u.contains("JOIN"), "{distinct_sql}");
        assert_select_only(&distinct_sql).unwrap();

        // Explicit GROUP BY Cube JOIN without metrics is also allowed.
        let grouped_sql = compile_semantic_sql(
            &docs,
            "SELECT sales.region_code, sales_b.region_code AS reg_b \
             FROM sales JOIN sales_b ON sales.region_code = sales_b.region_code \
             GROUP BY sales.region_code, sales_b.region_code",
            Some("finance"),
        )
        .unwrap();
        assert!(grouped_sql.to_ascii_uppercase().contains("GROUP BY"), "{grouped_sql}");
        assert_select_only(&grouped_sql).unwrap();
    }

    #[test]
    fn compiles_adhoc_sum_with_group_by() {
        let docs = example_docs();
        let sql = compile_semantic_sql(
            &docs,
            "SELECT region_code, sum(sale_amount) AS total \
             FROM sales GROUP BY region_code",
            Some("finance"),
        )
        .unwrap();
        let upper = sql.to_ascii_uppercase();
        assert!(upper.contains("SUM("), "expected SUM in {sql}");
        assert!(sql.contains("region_code") || sql.contains("\"region_code\""), "{sql}");
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn compiles_adhoc_global_aggregate() {
        let docs = example_docs();
        let sql = compile_semantic_sql(
            &docs,
            "SELECT sum(sale_amount) AS total FROM sales",
            Some("finance"),
        )
        .unwrap();
        assert!(sql.to_ascii_uppercase().contains("SUM("), "{sql}");
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn compiles_mixing_named_metric_and_adhoc_and_allows_query_grain_override() {
        let docs = example_docs();
        let sql = compile_semantic_sql(
            &docs,
            "SELECT region_code, revenue, sum(sale_amount) AS total, \
             revenue / count(*) AS avg_rev, round(revenue, 2) AS rev_round \
             FROM sales GROUP BY region_code",
            Some("finance"),
        )
        .unwrap();
        let upper = sql.to_ascii_uppercase();
        assert!(upper.contains("SUM("), "{sql}");
        assert!(upper.contains("COUNT(*)"), "{sql}");
        assert!(upper.contains("ROUND("), "{sql}");
        assert_select_only(&sql).unwrap();

        // D24: `revenue.group_by` is default grain only — slicing by `customer_id` (not in that list) is allowed.
        let override_sql = compile_semantic_sql(
            &docs,
            "SELECT customer_id, revenue, count(*) AS cnt FROM sales",
            Some("finance"),
        )
        .unwrap();
        let o = override_sql.to_ascii_lowercase();
        assert!(o.contains("group by"), "{override_sql}");
        assert!(o.contains("customer_id"), "{override_sql}");
        assert_select_only(&override_sql).unwrap();

        // Bare named metric still uses metadata default group_by axes.
        let bare = compile_semantic_sql(&docs, "SELECT revenue FROM sales", Some("finance")).unwrap();
        let b = bare.to_ascii_lowercase();
        assert!(b.contains("group by"), "{bare}");
        assert!(b.contains("region_code") || b.contains("region"), "{bare}");
        assert!(b.contains("order_date") || b.contains("order"), "{bare}");
        assert_select_only(&bare).unwrap();

        // Pure ad-hoc aggregate without `revenue` can group by `customer_id`.
        let adhoc_ok = compile_semantic_sql(
            &docs,
            "SELECT customer_id, count(*) AS cnt FROM sales",
            Some("finance"),
        )
        .unwrap();
        assert!(adhoc_ok.to_ascii_uppercase().contains("GROUP BY"), "{adhoc_ok}");
    }

    #[test]
    fn rejects_adhoc_when_fact_role_denied() {
        let docs = example_docs();
        let err = compile_semantic_sql(
            &docs,
            "SELECT sum(sale_amount) AS total FROM sales",
            Some("branch"),
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Forbidden(_)), "{err:?}");
    }

    #[test]
    fn compiles_mixing_named_and_adhoc_in_cube_join_and_filter_only_side() {
        let docs = docs_with_second_cube("dw");
        // Mixing named metric + ad-hoc aggregate across or within joined cubes is allowed.
        let sql = compile_semantic_sql(
            &docs,
            "SELECT sales.revenue, sum(sales_b.sale_amount) AS total \
             FROM sales JOIN sales_b ON sales.region_code = sales_b.region_code",
            Some("finance"),
        )
        .unwrap();
        let u = sql.to_ascii_uppercase();
        assert!(u.contains("JOIN"), "{sql}");
        assert!(u.contains("SUM("), "{sql}");
        assert_select_only(&sql).unwrap();

        // Joined side `sales_b` with 0 metrics (used only for join/filtering) compiles as a grouped subquery.
        let filter_side_sql = compile_semantic_sql(
            &docs,
            "SELECT sales.revenue, sum(sales.sale_amount) AS total \
             FROM sales JOIN sales_b ON sales.region_code = sales_b.region_code \
             WHERE sales_b.status_code = 'PAID'",
            Some("finance"),
        )
        .unwrap();
        assert!(filter_side_sql.to_ascii_uppercase().contains("JOIN"), "{filter_side_sql}");
        assert_select_only(&filter_side_sql).unwrap();
    }

    #[test]
    fn compiles_adhoc_cube_join() {
        let docs = docs_with_second_cube("dw");
        let sql = compile_semantic_sql(
            &docs,
            "SELECT sales.region_code, sum(sales.sale_amount) AS a, sum(sales_b.sale_amount) AS b \
             FROM sales JOIN sales_b ON sales.region_code = sales_b.region_code \
             GROUP BY sales.region_code",
            Some("finance"),
        )
        .unwrap();
        let u = sql.to_ascii_uppercase();
        assert!(u.contains("JOIN"), "{sql}");
        assert!(u.contains("SUM("), "{sql}");
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn compiles_scalar_expressions_and_unprojected_order_by() {
        let docs = example_docs();
        // Scalar date_trunc in SELECT + GROUP BY by alias + positional ORDER BY
        let sql_grouped = compile_semantic_sql(
            &docs,
            "SELECT date_trunc('month', order_date) AS m, sum(sale_amount) AS total \
             FROM sales GROUP BY m ORDER BY 2 DESC",
            Some("finance"),
        )
        .unwrap();
        let ug = sql_grouped.to_ascii_uppercase();
        assert!(ug.contains("DATE_TRUNC"), "{sql_grouped}");
        assert!(ug.contains("GROUP BY"), "{sql_grouped}");
        assert!(ug.contains("ORDER BY 2 DESC"), "{sql_grouped}");
        assert_select_only(&sql_grouped).unwrap();

        // Ungrouped scalar arithmetic in SELECT + unprojected cube member in ORDER BY
        let sql_ungrouped = compile_semantic_sql(
            &docs,
            "SELECT sale_amount * fx_rate AS krw_amount \
             FROM sales WHERE status_code = 'PAID' ORDER BY order_date DESC LIMIT 5",
            Some("finance"),
        )
        .unwrap();
        let uu = sql_ungrouped.to_ascii_uppercase();
        assert!(!uu.contains("GROUP BY"), "{sql_ungrouped}");
        assert!(uu.contains("ORDER BY \"ORDERS\".\"ORDER_DATE\" DESC"), "{sql_ungrouped}");
        assert_select_only(&sql_ungrouped).unwrap();
    }

    #[test]
    fn sqlite_dialect_compiles_helpers_date_trunc_and_float_division() {
        let docs = example_docs();
        let sql = compile_semantic_sql_with_binds(
            &docs,
            "SELECT sales.region_code, \
                    SUM(IIF(status_code = 'PAID', 1, 0)) * 100 / COUNT(*) AS paid_pct, \
                    DIVIDE(SUM(sale_amount), COUNT(*)) AS avg_sale \
             FROM sales \
             WHERE date_trunc('year', order_date) = 2024 AND SUBSTR(order_date, 1, 4) = '2024' \
             GROUP BY sales.region_code",
            Some("finance"),
            None,
            DialectProfile::Sqlite,
        )
        .unwrap();
        assert!(sql.contains("AS REAL)"), "expected REAL cast for SQLite division: {sql}");
        assert!(sql.contains("NULLIF("), "expected NULLIF from DIVIDE helper: {sql}");
        assert!(
            sql.contains(r#"CAST(SUBSTR(CAST("orders"."order_date" AS TEXT), 1, 4) AS INTEGER) = 2024"#),
            "expected integer year comparison rewrite in SQLite: {sql}"
        );
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn compiles_metadata_metric_and_segment_subqueries_and_rejects_adhoc_subqueries() {
        let mut docs = example_docs();
        docs.push(json!({
            "kind": "metric",
            "name": "revenue_ratio",
            "cube": "sales",
            "description": "전체 매출 대비 비중.",
            "expression": "SUM(sale_amount) / (SELECT SUM(sale_amount) FROM sales)",
            "aggregation": "group",
            "group_by": ["region_code"],
            "role": "all"
        }));
        docs.push(json!({
            "kind": "segment",
            "name": "high_value_region",
            "cube": "sales",
            "description": "고매출 지역 세그먼트.",
            "filter": {
                "expression": "region_code IN (SELECT region_code FROM sales GROUP BY region_code HAVING SUM(sale_amount) > 1000)"
            },
            "role": "all"
        }));

        // 1. Metadata metric & segment with semantic subqueries compile cleanly (including NOT segment)
        let sql = compile_semantic_sql(
            &docs,
            "SELECT region_code, revenue_ratio FROM sales WHERE high_value_region GROUP BY region_code",
            Some("finance"),
        )
        .unwrap();
        assert!(
            sql.contains(r#"SUM("orders"."sale_amount")"#),
            "expected compiled subquery in {sql}"
        );
        assert!(
            sql.contains(r#""orders"."region_code" IN (SELECT"#),
            "expected compiled IN (SELECT ...) segment in {sql}"
        );
        assert_select_only(&sql).unwrap();

        let sql_not_seg = compile_semantic_sql(
            &docs,
            "SELECT region_code, revenue_ratio FROM sales WHERE NOT high_value_region GROUP BY region_code",
            Some("finance"),
        )
        .unwrap();
        assert!(
            sql_not_seg.contains(r#"NOT ("orders"."region_code" IN (SELECT"#)
                || sql_not_seg.contains(r#"NOT "orders"."region_code" IN (SELECT"#),
            "expected negated segment subquery in {sql_not_seg}"
        );
        assert_select_only(&sql_not_seg).unwrap();

        // 2. Caller ad-hoc subqueries in WHERE or SELECT are still rejected
        let err_where = compile_semantic_sql(
            &docs,
            "SELECT region_code, revenue FROM sales WHERE region_code IN (SELECT region_code FROM sales)",
            Some("finance"),
        )
        .unwrap_err();
        assert!(
            err_where.to_string().contains("subqueries are not allowed"),
            "expected ad-hoc WHERE subquery rejection, got: {err_where}"
        );

        let err_select = compile_semantic_sql(
            &docs,
            "SELECT region_code, (SELECT MAX(sale_amount) FROM sales) AS mx FROM sales GROUP BY region_code",
            Some("finance"),
        )
        .unwrap_err();
        assert!(
            err_select.to_string().contains("subqueries are not allowed"),
            "expected ad-hoc SELECT subquery rejection, got: {err_select}"
        );
    }

    #[test]
    fn compiles_window_functions_in_grouped_and_ungrouped_modes() {
        let docs = example_docs();

        // 1. Grouped mode with window over named metric + cube dimension in PARTITION BY
        let sql_named_win = compile_semantic_sql(
            &docs,
            "SELECT region_code, revenue, RANK() OVER (PARTITION BY region_code ORDER BY revenue DESC) AS rnk \
             FROM sales GROUP BY region_code",
            Some("finance"),
        )
        .unwrap();
        assert!(
            sql_named_win.contains(r#"RANK() OVER (PARTITION BY "orders"."region_code" ORDER BY (sum("orders"."sale_amount")) DESC)"#),
            "unexpected named metric window SQL: {sql_named_win}"
        );
        assert_select_only(&sql_named_win).unwrap();

        // 2. Grouped mode with window over ad-hoc aggregate (share of total)
        let sql_adhoc_win = compile_semantic_sql(
            &docs,
            "SELECT region_code, SUM(sale_amount) / SUM(SUM(sale_amount)) OVER () AS share \
             FROM sales GROUP BY region_code",
            Some("finance"),
        )
        .unwrap();
        assert!(
            sql_adhoc_win.contains(r#"SUM("orders"."sale_amount") / SUM(SUM("orders"."sale_amount")) OVER ()"#),
            "unexpected ad-hoc share window SQL: {sql_adhoc_win}"
        );
        assert_select_only(&sql_adhoc_win).unwrap();

        // 3. Ungrouped mode with ROW_NUMBER() OVER (PARTITION BY ... ORDER BY ...)
        let sql_ungrouped_win = compile_semantic_sql(
            &docs,
            "SELECT region_code, sale_amount, ROW_NUMBER() OVER (PARTITION BY region_code ORDER BY sale_amount DESC) AS rn \
             FROM sales",
            Some("finance"),
        )
        .unwrap();
        let u = sql_ungrouped_win.to_ascii_uppercase();
        assert!(!u.contains("GROUP BY"), "ungrouped window query must not have GROUP BY: {sql_ungrouped_win}");
        assert!(
            sql_ungrouped_win.contains(r#"ROW_NUMBER() OVER (PARTITION BY "orders"."region_code" ORDER BY "orders"."sale_amount" DESC)"#),
            "unexpected ungrouped window SQL: {sql_ungrouped_win}"
        );
        assert_select_only(&sql_ungrouped_win).unwrap();

        // 4. Window function in GROUP BY is rejected
        let err_gb = compile_semantic_sql(
            &docs,
            "SELECT ROW_NUMBER() OVER (ORDER BY region_code) AS rn, SUM(sale_amount) FROM sales GROUP BY 1",
            Some("finance"),
        )
        .unwrap_err();
        assert!(
            err_gb.to_string().contains("GROUP BY"),
            "expected window function in GROUP BY rejection, got: {err_gb}"
        );
    }
