//! SQL query builder.

/// Simple SQL query builder.
#[derive(Debug, Default)]
pub struct SqlBuilder {
    select: Vec<String>,
    from: Option<String>,
    joins: Vec<String>,
    where_clause: Option<String>,
    order_by: Vec<String>,
    limit: Option<u32>,
    offset: Option<u32>,
}

impl SqlBuilder {
    /// Create a new SQL builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set SELECT clause fields.
    pub fn select(&mut self, fields: &[&str]) -> &mut Self {
        self.select = fields.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Add a field to SELECT clause.
    pub fn add_select(&mut self, field: &str) -> &mut Self {
        self.select.push(field.to_string());
        self
    }

    /// Set FROM clause.
    pub fn from(&mut self, table: &str) -> &mut Self {
        self.from = Some(table.to_string());
        self
    }

    /// Add LEFT JOIN.
    pub fn left_join(&mut self, table: &str, alias: &str, on: &str) -> &mut Self {
        self.joins.push(format!(
            "LEFT JOIN {} AS {} ON {}",
            table, alias, on
        ));
        self
    }

    /// Add INNER JOIN.
    pub fn inner_join(&mut self, table: &str, alias: &str, on: &str) -> &mut Self {
        self.joins.push(format!(
            "INNER JOIN {} AS {} ON {}",
            table, alias, on
        ));
        self
    }

    /// Set WHERE clause.
    pub fn where_clause(&mut self, clause: &str) -> &mut Self {
        self.where_clause = Some(clause.to_string());
        self
    }

    /// Add ORDER BY.
    pub fn order_by(&mut self, order: &str) -> &mut Self {
        self.order_by.push(order.to_string());
        self
    }

    /// Set LIMIT.
    pub fn limit(&mut self, limit: u32) -> &mut Self {
        self.limit = Some(limit);
        self
    }

    /// Set OFFSET.
    pub fn offset(&mut self, offset: u32) -> &mut Self {
        self.offset = Some(offset);
        self
    }

    /// Build the final SQL string.
    pub fn build(&self) -> String {
        let mut parts = Vec::new();

        // SELECT
        let select_str = if self.select.is_empty() {
            "*".to_string()
        } else {
            self.select.join(", ")
        };
        parts.push(format!("SELECT {}", select_str));

        // FROM
        if let Some(ref from) = self.from {
            parts.push(format!("FROM {}", from));
        }

        // JOINs
        for join in &self.joins {
            parts.push(join.clone());
        }

        // WHERE
        if let Some(ref where_clause) = self.where_clause {
            parts.push(format!("WHERE {}", where_clause));
        }

        // ORDER BY
        if !self.order_by.is_empty() {
            parts.push(format!("ORDER BY {}", self.order_by.join(", ")));
        }

        // LIMIT
        if let Some(limit) = self.limit {
            parts.push(format!("LIMIT {}", limit));
        }

        // OFFSET
        if let Some(offset) = self.offset {
            parts.push(format!("OFFSET {}", offset));
        }

        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_select() {
        let mut builder = SqlBuilder::new();
        builder.select(&["id", "name"]).from("users");
        
        assert_eq!(builder.build(), "SELECT id, name FROM users");
    }

    #[test]
    fn test_select_with_where() {
        let mut builder = SqlBuilder::new();
        builder
            .select(&["id"])
            .from("items")
            .where_clause("status = 'active'");
        
        assert_eq!(
            builder.build(),
            "SELECT id FROM items WHERE status = 'active'"
        );
    }

    #[test]
    fn test_select_with_join() {
        let mut builder = SqlBuilder::new();
        builder
            .select(&["items.id", "items.title"])
            .from("items")
            .left_join("users", "owner", "items.owner_id = owner.id")
            .add_select("owner.email");
        
        assert_eq!(
            builder.build(),
            "SELECT items.id, items.title, owner.email FROM items LEFT JOIN users AS owner ON items.owner_id = owner.id"
        );
    }

    #[test]
    fn test_select_with_order_limit() {
        let mut builder = SqlBuilder::new();
        builder
            .select(&["*"])
            .from("items")
            .order_by("created_at DESC")
            .limit(50)
            .offset(100);
        
        assert_eq!(
            builder.build(),
            "SELECT * FROM items ORDER BY created_at DESC LIMIT 50 OFFSET 100"
        );
    }
}
