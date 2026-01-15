//! Planner error types.
//!
//! Explicit errors for good UI feedback and correct HTTP status codes.

use std::fmt;

/// Errors that occur during plan construction.
///
/// These are all **client errors** (400), not authorization failures (403).
/// Policy rejection happens *after* successful planning.
#[derive(Debug, Clone, PartialEq)]
pub enum PlannerError {
    /// Model does not exist in schema.
    UnknownModel { name: String },

    /// Field does not exist on the specified model.
    UnknownField { model: String, field: String },

    /// Relation does not exist in __relations.
    UnknownRelation { name: String },

    /// Join is invalid (wrong direction, missing FK, etc.).
    InvalidJoin { relation: String, reason: String },

    /// Filter is malformed (empty AND/OR, invalid field reference).
    InvalidFilter { reason: String },

    /// Bulk operation requires WHERE clause.
    MissingWhereForBulk { operation: String },

    /// Multiple joins use the same alias.
    DuplicateAlias { alias: String },

    /// Field reference uses an undeclared alias.
    UndeclaredAlias { alias: String },

    /// Operation is not supported for this model type.
    UnsupportedOperation { operation: String, reason: String },

    /// Insert has no rows.
    EmptyInsert,

    /// Select list is empty (for query).
    EmptySelect,

    /// Limit/offset is invalid.
    InvalidPagination { reason: String },
}

impl fmt::Display for PlannerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownModel { name } => {
                write!(f, "model '{}' does not exist", name)
            }
            Self::UnknownField { model, field } => {
                write!(f, "field '{}' does not exist on model '{}'", field, model)
            }
            Self::UnknownRelation { name } => {
                write!(f, "relation '{}' does not exist", name)
            }
            Self::InvalidJoin { relation, reason } => {
                write!(f, "invalid join on relation '{}': {}", relation, reason)
            }
            Self::InvalidFilter { reason } => {
                write!(f, "invalid filter: {}", reason)
            }
            Self::MissingWhereForBulk { operation } => {
                write!(
                    f,
                    "bulk {} requires WHERE clause to prevent accidental data loss",
                    operation
                )
            }
            Self::DuplicateAlias { alias } => {
                write!(f, "duplicate join alias '{}'", alias)
            }
            Self::UndeclaredAlias { alias } => {
                write!(f, "field reference uses undeclared alias '{}'", alias)
            }
            Self::UnsupportedOperation { operation, reason } => {
                write!(f, "operation '{}' not supported: {}", operation, reason)
            }
            Self::EmptyInsert => {
                write!(f, "insert requires at least one row")
            }
            Self::EmptySelect => {
                write!(f, "query requires at least one field in select")
            }
            Self::InvalidPagination { reason } => {
                write!(f, "invalid pagination: {}", reason)
            }
        }
    }
}

impl std::error::Error for PlannerError {}
