//! Extract date intervals from SQL filters for union pruning (literals, binds, AND/OR).

use std::collections::HashMap;

use sqlparser::ast::{BinaryOperator, Expr, Value as AstValue};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

/// Half-open interval on one column: `[start, end)` in days since Unix epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateInterval {
    pub column: String,
    pub start: Option<i64>,
    pub end: Option<i64>,
}

impl DateInterval {
    pub fn overlaps(&self, other: &DateInterval) -> bool {
        if self.column != other.column {
            return true;
        }
        let a0 = self.start.unwrap_or(i64::MIN / 4);
        let a1 = self.end.unwrap_or(i64::MAX / 4);
        let b0 = other.start.unwrap_or(i64::MIN / 4);
        let b1 = other.end.unwrap_or(i64::MAX / 4);
        a1 > b0 && b1 > a0
    }
}

/// Conjunction of per-column intervals (AND).
pub type AndClause = Vec<DateInterval>;

/// Substitute `:name` bind placeholders with safely escaped single-quoted SQL literals from `binds`.
/// String literals and `::` casts are left untouched. Unknown binds are left as-is.
pub fn substitute_binds(expr: &str, binds: &HashMap<String, String>) -> String {
    if binds.is_empty() {
        return expr.to_string();
    }
    let chars: Vec<char> = expr.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\'' || chars[i] == '"' {
            let quote = chars[i];
            out.push(quote);
            i += 1;
            while i < chars.len() {
                let ch = chars[i];
                out.push(ch);
                i += 1;
                if ch == quote {
                    if i < chars.len() && chars[i] == quote {
                        out.push(quote);
                        i += 1;
                        continue;
                    }
                    break;
                }
            }
            continue;
        }
        // Ignore Postgres `::` type cast
        if chars[i] == ':' && i + 1 < chars.len() && chars[i + 1] == ':' {
            out.push_str("::");
            i += 2;
            continue;
        }
        if chars[i] == ':'
            && i + 1 < chars.len()
            && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_')
        {
            i += 1;
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let name: String = chars[start..i].iter().collect();
            if let Some(val) = binds.get(&name) {
                out.push('\'');
                for vc in val.chars() {
                    if vc == '\'' {
                        out.push_str("''");
                    } else if vc != '\0' {
                        out.push(vc);
                    }
                }
                out.push('\'');
            } else {
                out.push(':');
                out.push_str(&name);
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Return the first unresolved `:name` bind parameter outside string/identifier quotes, if any.
pub fn find_unresolved_bind(sql: &str) -> Option<String> {
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if matches!(chars[i], '\'' | '"' | '`') {
            let quote = chars[i];
            i += 1;
            while i < chars.len() {
                let ch = chars[i];
                i += 1;
                if ch == quote {
                    if i < chars.len() && chars[i] == quote {
                        i += 1;
                        continue;
                    }
                    break;
                }
            }
            continue;
        }
        if chars[i] == ':' && i + 1 < chars.len() && chars[i + 1] == ':' {
            i += 2;
            continue;
        }
        if chars[i] == ':'
            && i + 1 < chars.len()
            && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_')
        {
            i += 1;
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            return Some(chars[start..i].iter().collect());
        }
        i += 1;
    }
    None
}

/// True when every AND-clause pair across DNFs is date-disjoint (safe to prune).
pub fn is_disjoint_dnf(scope: &[AndClause], query: &[AndClause]) -> bool {
    if scope.is_empty() || query.is_empty() {
        return false;
    }
    for s in scope {
        for q in query {
            if !and_clauses_disjoint(s, q) {
                return false;
            }
        }
    }
    true
}

/// Backward-compatible: treat each side as a single AND clause list.
pub fn is_disjoint(scope: &[DateInterval], query: &[DateInterval]) -> bool {
    if scope.is_empty() || query.is_empty() {
        return false;
    }
    and_clauses_disjoint(scope, query)
}

/// Two AND clauses are disjoint if they share a column whose intervals do not overlap.
fn and_clauses_disjoint(a: &[DateInterval], b: &[DateInterval]) -> bool {
    for sa in a {
        for sb in b {
            if sa.column == sb.column && !sa.overlaps(sb) {
                return true;
            }
        }
    }
    false
}

/// Extract merged AND intervals (OR alternatives are unioned into one weak bound — prefer `extract_dnf`).
pub fn extract_intervals(expr: &str) -> Vec<DateInterval> {
    extract_intervals_with_binds(expr, &HashMap::new())
}

pub fn extract_intervals_with_binds(
    expr: &str,
    binds: &HashMap<String, String>,
) -> Vec<DateInterval> {
    let dnf = extract_dnf_with_binds(expr, binds);
    // Flatten: merge all alternatives' bounds per column (union of ranges → hull)
    let mut bounds = Vec::new();
    for clause in dnf {
        for iv in clause {
            bounds.push((iv.column, iv.start, iv.end));
        }
    }
    merge_bounds(bounds)
}

/// DNF: OR of AND-clauses. Use for precise disjoint checks with OR filters.
pub fn extract_dnf(expr: &str) -> Vec<AndClause> {
    extract_dnf_with_binds(expr, &HashMap::new())
}

pub fn extract_dnf_with_binds(
    expr: &str,
    binds: &HashMap<String, String>,
) -> Vec<AndClause> {
    let expr = substitute_binds(expr, binds);
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
    dnf_of(&selection)
        .into_iter()
        .map(merge_bounds)
        .filter(|c| !c.is_empty())
        .collect()
}

type RawBound = (String, Option<i64>, Option<i64>);

fn dnf_of(expr: &Expr) -> Vec<Vec<RawBound>> {
    match expr {
        Expr::BinaryOp { left, op, right } => match op {
            BinaryOperator::Or => {
                let mut out = dnf_of(left);
                out.extend(dnf_of(right));
                out
            }
            BinaryOperator::And => {
                let left = dnf_of(left);
                let right = dnf_of(right);
                let mut out = Vec::new();
                for l in &left {
                    for r in &right {
                        let mut m = l.clone();
                        m.extend(r.iter().cloned());
                        out.push(m);
                    }
                }
                if out.is_empty() {
                    // one side had no date bounds — keep the other
                    if left.is_empty() {
                        right
                    } else {
                        left
                    }
                } else {
                    out
                }
            }
            BinaryOperator::GtEq | BinaryOperator::Gt => {
                if let (Some(col), Some(d)) = (col_name(left), date_days(right)) {
                    let start = if matches!(op, BinaryOperator::Gt) {
                        Some(d + 1)
                    } else {
                        Some(d)
                    };
                    vec![vec![(col, start, None)]]
                } else {
                    vec![vec![]]
                }
            }
            BinaryOperator::Lt | BinaryOperator::LtEq => {
                if let (Some(col), Some(d)) = (col_name(left), date_days(right)) {
                    let end = if matches!(op, BinaryOperator::LtEq) {
                        Some(d + 1)
                    } else {
                        Some(d)
                    };
                    vec![vec![(col, None, end)]]
                } else {
                    vec![vec![]]
                }
            }
            _ => vec![vec![]],
        },
        Expr::Nested(inner) => dnf_of(inner),
        _ => vec![vec![]],
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
            AstValue::Placeholder(_) => None,
            _ => None,
        },
        Expr::TypedString { value, .. } => match &value.value {
            AstValue::SingleQuotedString(s) | AstValue::DoubleQuotedString(s) => parse_ymd(s),
            _ => None,
        },
        Expr::Nested(inner) => date_days(inner),
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
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = (y.rem_euclid(400)) as i64;
    let mp = if m > 2 { m as i64 - 3 } else { m as i64 + 9 };
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era as i64) * 146097 + doe - 719468)
}

fn merge_bounds(bounds: Vec<RawBound>) -> AndClause {
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
    fn unresolved_binds_yield_no_intervals() {
        let ivs = extract_intervals("order_date >= :scope_from AND order_date < :scope_to");
        assert!(ivs.is_empty());
    }

    #[test]
    fn resolves_binds_for_disjoint() {
        let mut binds = HashMap::new();
        binds.insert("scope_from".into(), "2020-06-01".into());
        binds.insert("scope_to".into(), "2020-07-01".into());
        let q = extract_dnf_with_binds(
            "order_date >= :scope_from AND order_date < :scope_to",
            &binds,
        );
        let scope = extract_dnf("order_date >= '2024-01-01' AND order_date < '2025-01-01'");
        assert!(is_disjoint_dnf(&scope, &q));
        let scope2 = extract_dnf("order_date >= '2020-01-01' AND order_date < '2021-01-01'");
        assert!(!is_disjoint_dnf(&scope2, &q));
    }

    #[test]
    fn or_dnf_keeps_overlap_if_any_branch_matches() {
        let scope = extract_dnf("order_date >= '2020-01-01' AND order_date < '2021-01-01'");
        let query = extract_dnf(
            "(order_date >= '2019-01-01' AND order_date < '2019-06-01') OR (order_date >= '2020-06-01' AND order_date < '2020-07-01')",
        );
        assert!(!is_disjoint_dnf(&scope, &query));
        let query2 = extract_dnf(
            "(order_date >= '2018-01-01' AND order_date < '2019-01-01') OR (order_date >= '2024-01-01' AND order_date < '2025-01-01')",
        );
        assert!(is_disjoint_dnf(&scope, &query2));
    }

    #[test]
    fn substitute_binds_escapes_single_quotes_and_preserves_cast() {
        let mut binds = HashMap::new();
        binds.insert("evil".into(), "KR' OR '1'='1".into());
        let out = substitute_binds("region_code = :evil AND amt::numeric > 0", &binds);
        assert_eq!(
            out,
            "region_code = 'KR'' OR ''1''=''1' AND amt::numeric > 0"
        );
    }

    #[test]
    fn find_unresolved_bind_detects_missing_and_ignores_strings_and_casts() {
        assert_eq!(
            find_unresolved_bind("SELECT \"a\"::text, ':not_a_bind' FROM t WHERE r = :caller_region"),
            Some("caller_region".to_string())
        );
        assert_eq!(
            find_unresolved_bind("SELECT \"a\"::text, ':not_a_bind' FROM t WHERE r = 'KR'"),
            None
        );
    }
}
