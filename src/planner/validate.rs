//! Stage 1: Structural validation.
//!
//! Validates DSL structure before schema resolution:
//! - Required fields present
//! - Empty AND/OR rejected
//! - Bulk ops require WHERE
//! - Limit/offset sane

use crate::protocol::data::{
    DeleteInput, FilterOp, FilterValidationError, InsertInput, QueryInput, UpdateInput,
};

use super::error::PlannerError;

/// Validate query input structure.
pub fn validate_query_structure(input: &QueryInput) -> Result<(), PlannerError> {
    // Select must not be empty
    if input.select.is_empty() {
        return Err(PlannerError::EmptySelect);
    }

    // Validate filter if present
    validate_filter_structure(input.r#where.as_ref())?;

    // Validate join filters
    for join in &input.joins {
        validate_filter_structure(join.r#where.as_ref())?;
    }

    // Validate pagination
    if let (Some(limit), Some(offset)) = (input.limit, input.offset) {
        if limit == 0 {
            return Err(PlannerError::InvalidPagination {
                reason: "limit must be greater than 0".into(),
            });
        }
        // offset can be 0, that's fine
        let _ = offset;
    }

    // Check for duplicate aliases
    let mut aliases: Vec<&str> = vec![];
    for join in &input.joins {
        if aliases.contains(&join.r#as.as_str()) {
            return Err(PlannerError::DuplicateAlias {
                alias: join.r#as.clone(),
            });
        }
        aliases.push(&join.r#as);
    }

    Ok(())
}

/// Validate filter structure.
pub fn validate_filter_structure(filter: Option<&FilterOp>) -> Result<(), PlannerError> {
    if let Some(f) = filter {
        f.validate().map_err(|e| match e {
            FilterValidationError::EmptyAnd => PlannerError::InvalidFilter {
                reason: "AND filter must have at least one element".into(),
            },
            FilterValidationError::EmptyOr => PlannerError::InvalidFilter {
                reason: "OR filter must have at least one element".into(),
            },
            FilterValidationError::InvalidField(field) => PlannerError::InvalidFilter {
                reason: format!("invalid field reference: {}", field),
            },
            FilterValidationError::UnknownAlias(alias) => PlannerError::UndeclaredAlias { alias },
        })?;
    }
    Ok(())
}

/// Validate insert input structure.
pub fn validate_insert_structure(input: &InsertInput) -> Result<(), PlannerError> {
    if input.rows.is_empty() {
        return Err(PlannerError::EmptyInsert);
    }

    // Each row must be an object
    for (i, row) in input.rows.iter().enumerate() {
        if !row.is_object() {
            return Err(PlannerError::InvalidFilter {
                reason: format!("insert row {} must be a JSON object", i),
            });
        }
    }

    Ok(())
}

/// Validate update input structure.
pub fn validate_update_structure(input: &UpdateInput) -> Result<(), PlannerError> {
    // WHERE is required (already enforced by type, but validate structure)
    validate_filter_structure(Some(&input.r#where))?;

    // Set must be an object
    if !input.set.is_object() {
        return Err(PlannerError::InvalidFilter {
            reason: "update 'set' must be a JSON object".into(),
        });
    }

    // Set must not be empty
    if input.set.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        return Err(PlannerError::InvalidFilter {
            reason: "update 'set' must have at least one field".into(),
        });
    }

    Ok(())
}

/// Validate delete input structure.
pub fn validate_delete_structure(input: &DeleteInput) -> Result<(), PlannerError> {
    // WHERE is required (already enforced by type, but validate structure)
    validate_filter_structure(Some(&input.r#where))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::data::{FilterValue, JoinSpec, OrderSpec};

    #[test]
    fn test_empty_select_rejected() {
        let input = QueryInput {
            select: vec![],
            r#where: None,
            joins: vec![],
            order_by: vec![],
            limit: None,
            offset: None,
        };
        assert!(matches!(
            validate_query_structure(&input),
            Err(PlannerError::EmptySelect)
        ));
    }

    #[test]
    fn test_valid_query_passes() {
        let input = QueryInput {
            select: vec!["id".into(), "name".into()],
            r#where: Some(FilterOp::Eq("status".into(), FilterValue::String("active".into()))),
            joins: vec![],
            order_by: vec![],
            limit: Some(50),
            offset: Some(0),
        };
        assert!(validate_query_structure(&input).is_ok());
    }

    #[test]
    fn test_duplicate_alias_rejected() {
        let input = QueryInput {
            select: vec!["id".into()],
            r#where: None,
            joins: vec![
                JoinSpec {
                    relation: "a.b".into(),
                    r#as: "owner".into(),
                    r#type: Default::default(),
                    select: vec!["id".into()],
                    r#where: None,
                },
                JoinSpec {
                    relation: "a.c".into(),
                    r#as: "owner".into(), // duplicate!
                    r#type: Default::default(),
                    select: vec!["id".into()],
                    r#where: None,
                },
            ],
            order_by: vec![],
            limit: None,
            offset: None,
        };
        assert!(matches!(
            validate_query_structure(&input),
            Err(PlannerError::DuplicateAlias { .. })
        ));
    }

    #[test]
    fn test_empty_and_rejected() {
        let input = QueryInput {
            select: vec!["id".into()],
            r#where: Some(FilterOp::And(vec![])),
            joins: vec![],
            order_by: vec![],
            limit: None,
            offset: None,
        };
        assert!(matches!(
            validate_query_structure(&input),
            Err(PlannerError::InvalidFilter { .. })
        ));
    }

    #[test]
    fn test_empty_insert_rejected() {
        let input = InsertInput {
            rows: vec![],
            returning: vec![],
        };
        assert!(matches!(
            validate_insert_structure(&input),
            Err(PlannerError::EmptyInsert)
        ));
    }

    #[test]
    fn test_update_empty_set_rejected() {
        let input = UpdateInput {
            r#where: FilterOp::Eq("id".into(), FilterValue::String("1".into())),
            set: serde_json::json!({}),
            returning: vec![],
        };
        assert!(matches!(
            validate_update_structure(&input),
            Err(PlannerError::InvalidFilter { .. })
        ));
    }

    #[test]
    fn test_zero_limit_rejected() {
        let input = QueryInput {
            select: vec!["id".into()],
            r#where: None,
            joins: vec![],
            order_by: vec![],
            limit: Some(0),
            offset: Some(0),
        };
        assert!(matches!(
            validate_query_structure(&input),
            Err(PlannerError::InvalidPagination { .. })
        ));
    }
}
