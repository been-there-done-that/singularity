//! SQL generation and execution.
//!
//! This module handles the "fast path" — pure SQL rewrite
//! where all predicates can be compiled to SQL.

mod builder;
mod filter;

use rusqlite::{params_from_iter, Connection};

use crate::planner::{LogicalPlan, MutationPlan};
use crate::protocol::data::{DataAction, FilterOp, PlanGrant, RowPredicate};

use super::error::ExecutionError;
use super::result::{ExecutionResult, Row};

use builder::SqlBuilder;
use filter::compile_filter;

/// Execute a query with SQL rewrite.
pub fn execute_query(
    plan: &LogicalPlan,
    grant: &PlanGrant,
    conn: &Connection,
    subject_id: Option<&str>,
) -> Result<ExecutionResult, ExecutionError> {
    let mut builder = SqlBuilder::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    // SELECT clause
    let select_fields: Vec<&str> = plan
        .selection
        .base_fields
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    
    builder.select(&select_fields);
    builder.from(&plan.base_model.name);

    // Add JOINs
    for (i, join_plan) in plan.joins.iter().enumerate() {
        let join_grant = grant.joins.get(i);
        
        // Build ON clause
        let on_clause = format!(
            "{}.{} = {}.{}",
            plan.base_model.name,
            join_plan.relation.from_field.name,
            join_plan.alias,
            join_plan.relation.to_field.name
        );
        
        // Add join predicate if present
        let full_on = if let Some(jg) = join_grant {
            if let RowPredicate::Sql { ref filter } = jg.row_predicate {
                let (sql, filter_params) = compile_filter(filter, Some(&join_plan.alias), subject_id)?;
                params.extend(filter_params);
                format!("({}) AND ({})", on_clause, sql)
            } else {
                on_clause
            }
        } else {
            on_clause
        };

        builder.left_join(
            &join_plan.relation.to_model.name,
            &join_plan.alias,
            &full_on,
        );

        // Add selected fields from join
        for field in &join_plan.selected_fields {
            builder.add_select(&format!("{}.{}", join_plan.alias, field.name));
        }
    }

    // WHERE clause: combine user filter + row predicate
    let mut where_parts: Vec<String> = Vec::new();

    // User-supplied filter
    if let Some(ref filter) = plan.filter {
        let (sql, filter_params) = compile_filter(filter, None, subject_id)?;
        where_parts.push(sql);
        params.extend(filter_params);
    }

    // Row predicate from grant
    match &grant.row_predicate {
        RowPredicate::Always => {
            // No additional filter
        }
        RowPredicate::Never => {
            // This should have been caught in authorization
            where_parts.push("0 = 1".to_string());
        }
        RowPredicate::Sql { filter } => {
            let (sql, filter_params) = compile_filter(filter, None, subject_id)?;
            where_parts.push(sql);
            params.extend(filter_params);
        }
        RowPredicate::Dynamic { .. } => {
            // Should not reach here in fast path
            return Err(ExecutionError::UnsupportedOperation {
                reason: "dynamic predicate in fast path".into(),
            });
        }
    }

    if !where_parts.is_empty() {
        builder.where_clause(&where_parts.join(" AND "));
    }

    // ORDER BY
    for order in &plan.ordering {
        let dir = if order.descending { "DESC" } else { "ASC" };
        builder.order_by(&format!("{} {}", order.field.name, dir));
    }

    // LIMIT/OFFSET
    if let Some(ref pagination) = plan.pagination {
        builder.limit(pagination.limit);
        builder.offset(pagination.offset);
    }

    // Execute
    let sql = builder.build();
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    
    let mut stmt = conn.prepare(&sql)?;
    let column_names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect();

    let rows: Vec<Row> = stmt
        .query_map(params_from_iter(param_refs), |row| {
            let mut result = Row::new();
            for (i, name) in column_names.iter().enumerate() {
                let value: rusqlite::types::Value = row.get(i)?;
                result.set(name, sqlite_to_json(value));
            }
            Ok(result)
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ExecutionResult::Rows(rows))
}

/// Execute a count query.
pub fn execute_count(
    plan: &LogicalPlan,
    grant: &PlanGrant,
    conn: &Connection,
    subject_id: Option<&str>,
) -> Result<ExecutionResult, ExecutionError> {
    let mut builder = SqlBuilder::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    builder.select(&["COUNT(*)"]);
    builder.from(&plan.base_model.name);

    // WHERE clause: combine user filter + row predicate
    let mut where_parts: Vec<String> = Vec::new();

    if let Some(ref filter) = plan.filter {
        let (sql, filter_params) = compile_filter(filter, None, subject_id)?;
        where_parts.push(sql);
        params.extend(filter_params);
    }

    match &grant.row_predicate {
        RowPredicate::Always => {}
        RowPredicate::Never => {
            where_parts.push("0 = 1".to_string());
        }
        RowPredicate::Sql { filter } => {
            let (sql, filter_params) = compile_filter(filter, None, subject_id)?;
            where_parts.push(sql);
            params.extend(filter_params);
        }
        RowPredicate::Dynamic { .. } => {
            return Err(ExecutionError::UnsupportedOperation {
                reason: "dynamic predicate in fast path".into(),
            });
        }
    }

    if !where_parts.is_empty() {
        builder.where_clause(&where_parts.join(" AND "));
    }

    let sql = builder.build();
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let count: i64 = conn.query_row(&sql, params_from_iter(param_refs), |row| row.get(0))?;

    Ok(ExecutionResult::Count(count as u64))
}

/// Execute an insert.
pub fn execute_insert(
    plan: &LogicalPlan,
    grant: &PlanGrant,
    conn: &Connection,
) -> Result<ExecutionResult, ExecutionError> {
    let mutation = plan.mutation.as_ref().ok_or_else(|| ExecutionError::PlanMismatch {
        reason: "insert plan missing mutation".into(),
    })?;

    let (rows, returning) = match mutation {
        MutationPlan::Insert { rows, returning } => (rows, returning),
        _ => {
            return Err(ExecutionError::PlanMismatch {
                reason: "expected insert mutation".into(),
            });
        }
    };

    // Check bulk limits
    if let Some(max) = grant.bulk.max_rows {
        if rows.len() as u32 > max {
            return Err(ExecutionError::BulkLimitExceeded {
                limit: max,
                actual: rows.len() as u64,
            });
        }
    }

    let mut affected = 0u64;
    let mut returning_rows = Vec::new();

    for row in rows {
        let obj = row.data.as_object().ok_or_else(|| ExecutionError::PlanMismatch {
            reason: "insert row is not an object".into(),
        })?;

        let columns: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
        let placeholders: Vec<&str> = columns.iter().map(|_| "?").collect();

        let mut sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            plan.base_model.name,
            columns.join(", "),
            placeholders.join(", ")
        );

        if !returning.is_empty() {
            let ret_cols: Vec<&str> = returning.iter().map(|f| f.name.as_str()).collect();
            sql.push_str(&format!(" RETURNING {}", ret_cols.join(", ")));
        }

        let params: Vec<Box<dyn rusqlite::ToSql>> = obj
            .values()
            .map(|v| json_to_sqlite(v.clone()))
            .collect();
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        if returning.is_empty() {
            conn.execute(&sql, params_from_iter(param_refs))?;
        } else {
            let column_names: Vec<String> = returning.iter().map(|f| f.name.clone()).collect();
            let mut stmt = conn.prepare(&sql)?;
            let rows: Vec<Row> = stmt
                .query_map(params_from_iter(param_refs), |row| {
                    let mut result = Row::new();
                    for (i, name) in column_names.iter().enumerate() {
                        let value: rusqlite::types::Value = row.get(i)?;
                        result.set(name, sqlite_to_json(value));
                    }
                    Ok(result)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            returning_rows.extend(rows);
        }

        affected += 1;
    }

    Ok(ExecutionResult::Affected {
        rows: affected,
        returning: returning_rows,
    })
}

/// Execute an update.
pub fn execute_update(
    plan: &LogicalPlan,
    grant: &PlanGrant,
    conn: &Connection,
    subject_id: Option<&str>,
) -> Result<ExecutionResult, ExecutionError> {
    // Check requires_where
    if grant.bulk.requires_where && plan.filter.is_none() {
        return Err(ExecutionError::WhereRequired {
            operation: "UPDATE".into(),
        });
    }

    let mutation = plan.mutation.as_ref().ok_or_else(|| ExecutionError::PlanMismatch {
        reason: "update plan missing mutation".into(),
    })?;

    let (set, returning) = match mutation {
        MutationPlan::Update { set, returning, .. } => (set, returning),
        _ => {
            return Err(ExecutionError::PlanMismatch {
                reason: "expected update mutation".into(),
            });
        }
    };

    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    // SET clause
    let set_parts: Vec<String> = set
        .iter()
        .map(|(field, value)| {
            params.push(json_to_sqlite(value.clone()));
            format!("{} = ?", field.name)
        })
        .collect();

    let mut sql = format!(
        "UPDATE {} SET {}",
        plan.base_model.name,
        set_parts.join(", ")
    );

    // WHERE clause
    let mut where_parts: Vec<String> = Vec::new();

    if let Some(ref filter) = plan.filter {
        let (filter_sql, filter_params) = compile_filter(filter, None, subject_id)?;
        where_parts.push(filter_sql);
        params.extend(filter_params);
    }

    match &grant.row_predicate {
        RowPredicate::Always => {}
        RowPredicate::Never => {
            where_parts.push("0 = 1".to_string());
        }
        RowPredicate::Sql { filter } => {
            let (filter_sql, filter_params) = compile_filter(filter, None, subject_id)?;
            where_parts.push(filter_sql);
            params.extend(filter_params);
        }
        RowPredicate::Dynamic { .. } => {
            return Err(ExecutionError::UnsupportedOperation {
                reason: "dynamic predicate in fast path".into(),
            });
        }
    }

    if !where_parts.is_empty() {
        sql.push_str(&format!(" WHERE {}", where_parts.join(" AND ")));
    }

    // RETURNING
    let mut returning_rows: Vec<Row> = Vec::new();
    if !returning.is_empty() {
        let ret_cols: Vec<&str> = returning.iter().map(|f| f.name.as_str()).collect();
        sql.push_str(&format!(" RETURNING {}", ret_cols.join(", ")));
    }

    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    if returning.is_empty() {
        let affected = conn.execute(&sql, params_from_iter(param_refs))?;

        // Check bulk limit
        if let Some(max) = grant.bulk.max_rows {
            if affected as u32 > max {
                return Err(ExecutionError::BulkLimitExceeded {
                    limit: max,
                    actual: affected as u64,
                });
            }
        }

        Ok(ExecutionResult::affected(affected as u64))
    } else {
        let column_names: Vec<String> = returning.iter().map(|f| f.name.clone()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows: Vec<Row> = stmt
            .query_map(params_from_iter(param_refs), |row| {
                let mut result = Row::new();
                for (i, name) in column_names.iter().enumerate() {
                    let value: rusqlite::types::Value = row.get(i)?;
                    result.set(name, sqlite_to_json(value));
                }
                Ok(result)
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let affected = rows.len() as u64;

        // Check bulk limit
        if let Some(max) = grant.bulk.max_rows {
            if affected as u32 > max {
                return Err(ExecutionError::BulkLimitExceeded {
                    limit: max,
                    actual: affected,
                });
            }
        }

        Ok(ExecutionResult::affected_with_returning(affected, rows))
    }
}

/// Execute a delete.
pub fn execute_delete(
    plan: &LogicalPlan,
    grant: &PlanGrant,
    conn: &Connection,
    subject_id: Option<&str>,
) -> Result<ExecutionResult, ExecutionError> {
    // Check requires_where
    if grant.bulk.requires_where && plan.filter.is_none() {
        return Err(ExecutionError::WhereRequired {
            operation: "DELETE".into(),
        });
    }

    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    let mut sql = format!("DELETE FROM {}", plan.base_model.name);

    // WHERE clause
    let mut where_parts: Vec<String> = Vec::new();

    if let Some(ref filter) = plan.filter {
        let (filter_sql, filter_params) = compile_filter(filter, None, subject_id)?;
        where_parts.push(filter_sql);
        params.extend(filter_params);
    }

    match &grant.row_predicate {
        RowPredicate::Always => {}
        RowPredicate::Never => {
            where_parts.push("0 = 1".to_string());
        }
        RowPredicate::Sql { filter } => {
            let (filter_sql, filter_params) = compile_filter(filter, None, subject_id)?;
            where_parts.push(filter_sql);
            params.extend(filter_params);
        }
        RowPredicate::Dynamic { .. } => {
            return Err(ExecutionError::UnsupportedOperation {
                reason: "dynamic predicate in fast path".into(),
            });
        }
    }

    if !where_parts.is_empty() {
        sql.push_str(&format!(" WHERE {}", where_parts.join(" AND ")));
    }

    // Get returning if mutation specifies it
    let returning = plan
        .mutation
        .as_ref()
        .and_then(|m| match m {
            MutationPlan::Delete { returning } => Some(returning),
            _ => None,
        })
        .cloned()
        .unwrap_or_default();

    let mut returning_rows: Vec<Row> = Vec::new();
    if !returning.is_empty() {
        let ret_cols: Vec<&str> = returning.iter().map(|f| f.name.as_str()).collect();
        sql.push_str(&format!(" RETURNING {}", ret_cols.join(", ")));
    }

    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    if returning.is_empty() {
        let affected = conn.execute(&sql, params_from_iter(param_refs))?;

        // Check bulk limit
        if let Some(max) = grant.bulk.max_rows {
            if affected as u32 > max {
                return Err(ExecutionError::BulkLimitExceeded {
                    limit: max,
                    actual: affected as u64,
                });
            }
        }

        Ok(ExecutionResult::affected(affected as u64))
    } else {
        let column_names: Vec<String> = returning.iter().map(|f| f.name.clone()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows: Vec<Row> = stmt
            .query_map(params_from_iter(param_refs), |row| {
                let mut result = Row::new();
                for (i, name) in column_names.iter().enumerate() {
                    let value: rusqlite::types::Value = row.get(i)?;
                    result.set(name, sqlite_to_json(value));
                }
                Ok(result)
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let affected = rows.len() as u64;

        // Check bulk limit
        if let Some(max) = grant.bulk.max_rows {
            if affected as u32 > max {
                return Err(ExecutionError::BulkLimitExceeded {
                    limit: max,
                    actual: affected,
                });
            }
        }

        Ok(ExecutionResult::affected_with_returning(affected, rows))
    }
}

/// Convert SQLite value to JSON.
fn sqlite_to_json(value: rusqlite::types::Value) -> serde_json::Value {
    use rusqlite::types::Value;
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Integer(i) => serde_json::json!(i),
        Value::Real(f) => serde_json::json!(f),
        Value::Text(s) => serde_json::json!(s),
        Value::Blob(b) => serde_json::json!(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b
        )),
    }
}

/// Convert JSON value to SQLite parameter.
fn json_to_sqlite(value: serde_json::Value) -> Box<dyn rusqlite::ToSql> {
    use serde_json::Value;
    match value {
        Value::Null => Box::new(Option::<String>::None),
        Value::Bool(b) => Box::new(if b { 1i64 } else { 0i64 }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Box::new(i)
            } else if let Some(f) = n.as_f64() {
                Box::new(f)
            } else {
                Box::new(n.to_string())
            }
        }
        Value::String(s) => Box::new(s),
        Value::Array(arr) => Box::new(serde_json::to_string(&arr).unwrap_or_default()),
        Value::Object(obj) => Box::new(serde_json::to_string(&obj).unwrap_or_default()),
    }
}
