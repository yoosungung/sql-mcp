//! Extract simple date intervals from SQL filter expressions for union pruning.

use sqlparser::ast::{BinaryOperator, Expr, Value as AstValue};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

/// Half-open interval on one column: `[start, end)` in days since Unix epoch.
/// Missing bound = ±∞.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateInterval {
    pub column: String,
    pub start: Option<i64>,
    pub end: Option<i64>,
}

impl DateInterval {
    pub fn overlaps(&self, other: &DateInterval) -> bool {
        if self.column != other.column {
            return true; // different columns — do not treat as disjoint
        }
        let a0 = self.start.unwrap_or(i64::MIN / 4);
        let a1 = self.end.unwrap_or(i64::MAX / 4);
        let b0 = other.start.unwrap_or(i64::MIN / 4);
        let b1 = other.end.unwrap_or(i64::MAX / 4);
        a1 > b0 && b1 > a0
    }
}

/// True when same-column intervals exist on both sides and none overlap.
pub fn is_disjoint(scope: &[DateInterval], query: &[DateInterval]) -> bool {
    if scope.is_empty() || query.is_empty() {
        return false;
    }
    let mut saw_comparable = false;
    for s in scope {
        for q in query {
            if s.column == q.column {
                saw_comparable = true;
                if s.overlaps(q) {
                    return false;
                }
            }
        }
    }
    saw_comparable
}

/// Parse filter expr and collect date bounds (`col >= 'YYYY-MM-DD'`, `col < '...'`, AND/OR nested).
pub fn extract_intervals(expr: &str) -> Vec<DateInterval> {
    let wrapped = format!("SELECT 1 WHERE {expr}");
    let dialect = GenericDialect {};
    let Ok(stmts) = Parser::parse_sql(&dialect, &wrapped) else {
        return vec![];
    };
    let Some(sqlparser::ast::Statement::Query(q)) = stmts.into_iter().next() else {
        return vec![];
    };
    let sqlparser::ast::SetExpr::Select(select) = *q.body else {
        return vec![];
    };
    let Some(selection) = select.selection else {
        return vec![];
    };
    let mut bounds: Vec<(String, Option<i64>, Option<i64>)> = Vec::new();
    collect_bounds(&selection, &mut bounds);
    merge_bounds(bounds)
}

fn collect_bounds(expr: &Expr, out: &mut Vec<(String, Option<i64>, Option<i64>)>) {
    match expr {
        Expr::BinaryOp { left, op, right } => match op {
            BinaryOperator::And => {
                collect_bounds(left, out);
                collect_bounds(right, out);
            }
            BinaryOperator::GtEq | BinaryOperator::Gt => {
                if let (Some(col), Some(d)) = (col_name(left), date_days(right)) {
                    let start = if matches!(op, BinaryOperator::Gt) {
                        Some(d + 1)
                    } else {
                        Some(d)
                    };
                    out.push((col, start, None));
                }
            }
            BinaryOperator::Lt | BinaryOperator::LtEq => {
                if let (Some(col), Some(d)) = (col_name(left), date_days(right)) {
                    let end = if matches!(op, BinaryOperator::LtEq) {
                        Some(d + 1)
                    } else {
                        Some(d)
                    };
                    out.push((col, None, end));
                }
            }
            _ => {}
        },
        Expr::Nested(inner) => collect_bounds(inner, out),
        _ => {}
    }
}

fn col_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(id) => Some(id.value.to_ascii_lowercase()),
        Expr::CompoundIdentifier(parts) => parts.last().map(|p| p.value.to_ascii_lowercase()),
        Expr::Nested(inner) => col_name(inner),
        _ => None,
    }
}

fn date_days(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Value(v) => match &v.value {
            AstValue::SingleQuotedString(s) | AstValue::DoubleQuotedString(s) => parse_ymd(s),
            _ => None,
        },
        Expr::TypedString { value, .. } => match &value.value {
            AstValue::SingleQuotedString(s) | AstValue::DoubleQuotedString(s) => parse_ymd(s),
            _ => None,
        },
        Expr::Nested(inner) => date_days(inner),
        // skip bind params / functions
        _ => None,
    }
}

fn parse_ymd(s: &str) -> Option<i64> {
    let s = s.trim();
    let mut parts = s.split('-');
    let y: i32 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Civil to days via Rata Die (Howard Hinnant)
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = (y.rem_euclid(400)) as i64;
    let mp = if m > 2 { m as i64 - 3 } else { m as i64 + 9 };
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era as i64) * 146097 + doe - 719468)
}

fn merge_bounds(bounds: Vec<(String, Option<i64>, Option<i64>)>) -> Vec<DateInterval> {
    use std::collections::HashMap;
    let mut map: HashMap<String, (Option<i64>, Option<i64>)> = HashMap::new();
    for (col, start, end) in bounds {
        let e = map.entry(col).or_insert((None, None));
        if let Some(s) = start {
            e.0 = Some(e.0.map(|x| x.max(s)).unwrap_or(s));
        }
        if let Some(en) = end {
            e.1 = Some(e.1.map(|x| x.min(en)).unwrap_or(en));
        }
    }
    map.into_iter()
        .map(|(column, (start, end))| DateInterval { column, start, end })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_half_open_range() {
        let ivs = extract_intervals("order_date >= '2020-01-01' AND order_date < '2021-01-01'");
        assert_eq!(ivs.len(), 1);
        assert_eq!(ivs[0].column, "order_date");
        assert!(ivs[0].start.is_some());
        assert!(ivs[0].end.is_some());
    }

    #[test]
    fn detects_disjoint_years() {
        let a = extract_intervals("order_date >= '2020-01-01' AND order_date < '2021-01-01'");
        let b = extract_intervals("order_date >= '2024-01-01' AND order_date < '2025-01-01'");
        assert!(is_disjoint(&a, &b));
        assert!(!is_disjoint(&a, &a));
    }

    #[test]
    fn bind_params_yield_no_intervals() {
        let ivs = extract_intervals("order_date >= :scope_from AND order_date < :scope_to");
        assert!(ivs.is_empty());
    }
}
