//! Data access DSL types.
//!
//! Canonical request format for all data operations:
//! - Query (read with filters, joins, ordering)
//! - Count (filtered count)
//! - Insert (single or bulk)
//! - Update (filtered bulk)
//! - Delete (filtered bulk)
//!
//! # Design Principles
//!
//! - **Tuple-style filter syntax** for clean AST compilation
//! - **Explicit joins** via FK relations in __relations
//! - **Policy-first** - PlanGrant encodes all access constraints
//! - **Safe defaults** - bulk ops require WHERE, have max_rows

mod access_config;
mod filter;
mod grant;
mod mutation;
mod query;

pub use access_config::{AccessProfile, PrincipalType, RowScopeType, ScopeSource};
pub use filter::{FilterOp, FilterValidationError, FilterValue};
pub use grant::{
    BulkGrant, DataAction, FieldMask, JoinGrant, PlanGrant, RowPredicate,
};
pub use mutation::{DeleteInput, InsertInput, UpdateInput};
pub use query::{JoinSpec, JoinType, OrderDir, OrderSpec, QueryInput};

