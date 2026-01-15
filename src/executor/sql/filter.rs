//! FilterOp → SQL compilation.
//!
//! Compiles the DSL filter grammar into prepared SQL statements
//! with proper parameterization to prevent SQL injection.

use crate::executor::error::ExecutionError;
use crate::protocol::data::{FilterOp, FilterValue};

/// Compile a FilterOp into SQL with parameters.
///
/// Returns the SQL string and a list of boxed parameters.
pub fn compile_filter(
    filter: &FilterOp,
    table_alias: Option<&str>,
    subject_id: Option<&str>,
) -> Result<(String, Vec<Box<dyn rusqlite::ToSql>>), ExecutionError> {
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let sql = compile_filter_inner(filter, table_alias, subject_id, &mut params)?;
    Ok((sql, params))
}

fn compile_filter_inner(
    filter: &FilterOp,
    table_alias: Option<&str>,
    subject_id: Option<&str>,
    params: &mut Vec<Box<dyn rusqlite::ToSql>>,
) -> Result<String, ExecutionError> {
    match filter {
        FilterOp::And(filters) => {
            if filters.is_empty() {
                return Ok("1 = 1".to_string());
            }
            let parts: Result<Vec<_>, _> = filters
                .iter()
                .map(|f| compile_filter_inner(f, table_alias, subject_id, params))
                .collect();
            Ok(format!("({})", parts?.join(" AND ")))
        }
        FilterOp::Or(filters) => {
            if filters.is_empty() {
                return Ok("0 = 1".to_string());
            }
            let parts: Result<Vec<_>, _> = filters
                .iter()
                .map(|f| compile_filter_inner(f, table_alias, subject_id, params))
                .collect();
            Ok(format!("({})", parts?.join(" OR ")))
        }
        FilterOp::Not(inner) => {
            let inner_sql = compile_filter_inner(inner, table_alias, subject_id, params)?;
            Ok(format!("NOT ({})", inner_sql))
        }
        FilterOp::Eq(field, value) => {
            let col = qualified_column(field, table_alias);
            let param = compile_value(value, subject_id)?;
            params.push(param);
            Ok(format!("{} = ?", col))
        }
        FilterOp::Ne(field, value) => {
            let col = qualified_column(field, table_alias);
            let param = compile_value(value, subject_id)?;
            params.push(param);
            Ok(format!("{} != ?", col))
        }
        FilterOp::Gt(field, value) => {
            let col = qualified_column(field, table_alias);
            let param = compile_value(value, subject_id)?;
            params.push(param);
            Ok(format!("{} > ?", col))
        }
        FilterOp::Gte(field, value) => {
            let col = qualified_column(field, table_alias);
            let param = compile_value(value, subject_id)?;
            params.push(param);
            Ok(format!("{} >= ?", col))
        }
        FilterOp::Lt(field, value) => {
            let col = qualified_column(field, table_alias);
            let param = compile_value(value, subject_id)?;
            params.push(param);
            Ok(format!("{} < ?", col))
        }
        FilterOp::Lte(field, value) => {
            let col = qualified_column(field, table_alias);
            let param = compile_value(value, subject_id)?;
            params.push(param);
            Ok(format!("{} <= ?", col))
        }
        FilterOp::In(field, values) => {
            let col = qualified_column(field, table_alias);
            if values.is_empty() {
                return Ok("0 = 1".to_string());
            }
            let placeholders: Vec<&str> = values.iter().map(|_| "?").collect();
            for v in values {
                params.push(compile_value(v, subject_id)?);
            }
            Ok(format!("{} IN ({})", col, placeholders.join(", ")))
        }
        FilterOp::Like(field, pattern) => {
            let col = qualified_column(field, table_alias);
            params.push(Box::new(pattern.clone()));
            Ok(format!("{} LIKE ?", col))
        }
        FilterOp::ILike(field, pattern) => {
            let col = qualified_column(field, table_alias);
            params.push(Box::new(pattern.to_lowercase()));
            // SQLite doesn't have ILIKE, use LOWER()
            Ok(format!("LOWER({}) LIKE LOWER(?)", col))
        }
        FilterOp::IsNull(field) => {
            let col = qualified_column(field, table_alias);
            Ok(format!("{} IS NULL", col))
        }
        FilterOp::IsNotNull(field) => {
            let col = qualified_column(field, table_alias);
            Ok(format!("{} IS NOT NULL", col))
        }
    }
}

/// Get qualified column name with optional table alias.
fn qualified_column(field: &str, table_alias: Option<&str>) -> String {
    // Check if field already has a qualifier (e.g., "owner.email")
    if field.contains('.') {
        return field.to_string();
    }
    
    match table_alias {
        Some(alias) => format!("{}.{}", alias, field),
        None => field.to_string(),
    }
}

/// Compile a FilterValue into a SQL parameter.
fn compile_value(
    value: &FilterValue,
    subject_id: Option<&str>,
) -> Result<Box<dyn rusqlite::ToSql>, ExecutionError> {
    match value {
        FilterValue::Null => Ok(Box::new(Option::<String>::None)),
        FilterValue::Bool(b) => Ok(Box::new(if *b { 1i64 } else { 0i64 })),
        FilterValue::Int(i) => Ok(Box::new(*i)),
        FilterValue::Float(f) => Ok(Box::new(*f)),
        FilterValue::String(s) => Ok(Box::new(s.clone())),
        FilterValue::DateTime(epoch) => Ok(Box::new(*epoch)),
        FilterValue::Subject => {
            // $subject resolves to the current user's internal ID
            match subject_id {
                Some(id) => Ok(Box::new(id.to_string())),
                None => Err(ExecutionError::PredicateViolation {
                    reason: "$subject used but no subject_id provided".into(),
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_eq() {
        let filter = FilterOp::Eq("status".into(), FilterValue::String("active".into()));
        let (sql, params) = compile_filter(&filter, None, None).unwrap();
        
        assert_eq!(sql, "status = ?");
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_compile_with_alias() {
        let filter = FilterOp::Eq("email".into(), FilterValue::String("test@test.com".into()));
        let (sql, params) = compile_filter(&filter, Some("owner"), None).unwrap();
        
        assert_eq!(sql, "owner.email = ?");
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_compile_and() {
        let filter = FilterOp::And(vec![
            FilterOp::Eq("status".into(), FilterValue::String("active".into())),
            FilterOp::Gt("price".into(), FilterValue::Int(100)),
        ]);
        let (sql, params) = compile_filter(&filter, None, None).unwrap();
        
        assert_eq!(sql, "(status = ? AND price > ?)");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_compile_or() {
        let filter = FilterOp::Or(vec![
            FilterOp::Eq("status".into(), FilterValue::String("draft".into())),
            FilterOp::Eq("status".into(), FilterValue::String("pending".into())),
        ]);
        let (sql, params) = compile_filter(&filter, None, None).unwrap();
        
        assert_eq!(sql, "(status = ? OR status = ?)");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_compile_not() {
        let filter = FilterOp::Not(Box::new(
            FilterOp::Eq("deleted".into(), FilterValue::Bool(true)),
        ));
        let (sql, _) = compile_filter(&filter, None, None).unwrap();
        
        assert_eq!(sql, "NOT (deleted = ?)");
    }

    #[test]
    fn test_compile_in() {
        let filter = FilterOp::In(
            "status".into(),
            vec![
                FilterValue::String("a".into()),
                FilterValue::String("b".into()),
                FilterValue::String("c".into()),
            ],
        );
        let (sql, params) = compile_filter(&filter, None, None).unwrap();
        
        assert_eq!(sql, "status IN (?, ?, ?)");
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn test_compile_subject() {
        let filter = FilterOp::Eq("owner_id".into(), FilterValue::Subject);
        let (sql, params) = compile_filter(&filter, None, Some("user-123")).unwrap();
        
        assert_eq!(sql, "owner_id = ?");
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_compile_subject_missing() {
        let filter = FilterOp::Eq("owner_id".into(), FilterValue::Subject);
        let result = compile_filter(&filter, None, None);
        
        assert!(result.is_err());
    }

    #[test]
    fn test_compile_is_null() {
        let filter = FilterOp::IsNull("deleted_at".into());
        let (sql, params) = compile_filter(&filter, None, None).unwrap();
        
        assert_eq!(sql, "deleted_at IS NULL");
        assert_eq!(params.len(), 0);
    }

    #[test]
    fn test_compile_like() {
        let filter = FilterOp::Like("title".into(), "%rust%".into());
        let (sql, params) = compile_filter(&filter, None, None).unwrap();
        
        assert_eq!(sql, "title LIKE ?");
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_compile_ilike() {
        let filter = FilterOp::ILike("title".into(), "%RUST%".into());
        let (sql, params) = compile_filter(&filter, None, None).unwrap();
        
        assert_eq!(sql, "LOWER(title) LIKE LOWER(?)");
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_qualified_column_with_dot() {
        // Field already has qualifier
        assert_eq!(qualified_column("owner.email", Some("x")), "owner.email");
    }

    #[test]
    fn test_empty_and() {
        let filter = FilterOp::And(vec![]);
        let (sql, _) = compile_filter(&filter, None, None).unwrap();
        assert_eq!(sql, "1 = 1");
    }

    #[test]
    fn test_empty_or() {
        let filter = FilterOp::Or(vec![]);
        let (sql, _) = compile_filter(&filter, None, None).unwrap();
        assert_eq!(sql, "0 = 1");
    }

    #[test]
    fn test_empty_in() {
        let filter = FilterOp::In("status".into(), vec![]);
        let (sql, _) = compile_filter(&filter, None, None).unwrap();
        assert_eq!(sql, "0 = 1");
    }
}
