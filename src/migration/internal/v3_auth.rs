use crate::migration::InternalMigration;
use crate::state::State;
use crate::transport::error::TransportError;

/// V3: Auth tables.
/// Creates tables for identity provider: __auth_users, __auth_secrets.
pub struct V3Auth;

impl InternalMigration for V3Auth {
    fn version(&self) -> u64 {
        3
    }

    fn name(&self) -> &str {
        "auth_tables"
    }

    fn apply(&self, state: &mut dyn State) -> Result<(), TransportError> {
        // Create auth users table
        state.execute_ddl(
            "CREATE TABLE IF NOT EXISTS __auth_users (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                email TEXT,
                email_verified INTEGER DEFAULT 0,
                created_at INTEGER NOT NULL
            );"
        ).map_err(|e| TransportError::Internal(e.to_string()))?;

        // Create auth secrets table (password hashes)
        state.execute_ddl(
            "CREATE TABLE IF NOT EXISTS __auth_secrets (
                user_id TEXT PRIMARY KEY,
                password_hash TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                FOREIGN KEY (user_id) REFERENCES __auth_users(id)
            );"
        ).map_err(|e| TransportError::Internal(e.to_string()))?;

        // Create index on username for fast lookups
        state.execute_ddl(
            "CREATE INDEX IF NOT EXISTS idx_auth_users_username ON __auth_users(username);"
        ).map_err(|e| TransportError::Internal(e.to_string()))?;

        // Create index on email for fast lookups
        state.execute_ddl(
            "CREATE INDEX IF NOT EXISTS idx_auth_users_email ON __auth_users(email);"
        ).map_err(|e| TransportError::Internal(e.to_string()))?;

        Ok(())
    }
}
