//! Schema Meta-Model Types.
//! 
//! Defines the structure of User-Defined Data.
//! 
//! # Invariant
//! Tables are Resources. Fields are Sub-resources.
//! Schema mutations are capability-gated operations.

pub mod validation;

use serde::{Serialize, Deserialize};

/// Defines a User Table (Model).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDefinition {
    /// Stable, internal ID (e.g. UUID). Immutable.
    pub id: String,
    
    /// User-facing name (e.g. "users"). Mutable (via rename op).
    pub name: String,
    
    /// Namespace (e.g. "public", "internal").
    pub namespace: String,
    
    /// Creation timestamp.
    pub created_at: u64,
}

/// Defines a Field (Column) in a Model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    /// Stable internal ID.
    pub id: String,
    
    /// Column name.
    pub name: String,
    
    /// Field Data Type.
    pub field_type: FieldType,
    
    /// Is required (NOT NULL).
    pub required: bool,
    
    /// Is unique.
    pub unique: bool,
    
    /// Default value (JSON).
    pub default: Option<serde_json::Value>,
    
    /// Creation timestamp.
    pub created_at: u64,
}

/// Logical Data Types for Fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "config")] // e.g. { "type": "Ref", "config": { "model_id": "..." } }
pub enum FieldType {
    String,
    Int,
    Float,
    Bool,
    Json,
    /// Semantic Reference to another Model (Foreign Key).
    Ref { model_id: String },
}

/// Explicit Schema Operations.
/// Privilege: `schema.*`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SchemaOp {
    /// Create a new Model.
    CreateModel(ModelDefinition),
    
    /// Add a Field to an existing Model.
    AddField { 
        model_id: String, 
        field: FieldDefinition 
    },
    
    /// Remove a Field from a Model.
    /// Destructive!
    DropField { 
        model_id: String, 
        field_id: String 
    },
    
    // Future: RenameModel, RenameField, AttachFolder, etc.
}
