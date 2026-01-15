//! SQL string extraction and table dependency parsing.
//!
//! This module extracts SQL strings from function bodies and parses them
//! to identify table dependencies at compile time.

use std::collections::HashSet;

use sqlparser::ast::{
    Expr, Query, Select, SelectItem, SetExpr, Statement, TableFactor, TableWithJoins,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use syn::visit::Visit;
use syn::{Expr as SynExpr, ExprCall, ExprLit, ExprMacro, ExprMethodCall, Lit};

/// Visitor that extracts SQL string literals from function bodies.
pub struct SqlStringExtractor {
    /// Collected SQL strings.
    pub sql_strings: Vec<String>,
}

impl SqlStringExtractor {
    pub fn new() -> Self {
        Self {
            sql_strings: Vec::new(),
        }
    }

    /// Check if a string looks like SQL.
    fn looks_like_sql(s: &str) -> bool {
        let upper = s.to_uppercase();
        upper.contains("SELECT")
            || upper.contains("INSERT")
            || upper.contains("UPDATE")
            || upper.contains("DELETE")
            || (upper.contains("FROM") && !upper.contains("import"))
    }

    /// Extract SQL strings from macro tokens.
    /// This handles sqlx macros like query_as!(Type, "SQL", args...)
    fn extract_sql_from_tokens(&mut self, tokens: &proc_macro2::TokenStream) {
        for token in tokens.clone() {
            match token {
                proc_macro2::TokenTree::Literal(lit) => {
                    // Try to parse as a string literal
                    let lit_str = lit.to_string();
                    // Raw string literals look like r#"..."# or r"..."
                    // Regular string literals look like "..."
                    if let Some(sql) = Self::extract_string_content(&lit_str) {
                        if Self::looks_like_sql(&sql) {
                            self.sql_strings.push(sql);
                        }
                    }
                }
                proc_macro2::TokenTree::Group(group) => {
                    // Recurse into groups (parentheses, brackets, braces)
                    self.extract_sql_from_tokens(&group.stream());
                }
                _ => {}
            }
        }
    }

    /// Extract the actual string content from a literal representation.
    /// Handles both regular strings ("...") and raw strings (r#"..."#, r"...").
    fn extract_string_content(lit: &str) -> Option<String> {
        let lit = lit.trim();

        // Raw string literal: r#"..."# or r##"..."## etc.
        if lit.starts_with("r#") || lit.starts_with("r\"") {
            // Find the opening quote pattern
            let quote_start = lit.find('"')?;
            // Count the # symbols before the quote
            let hash_count = lit[1..quote_start].chars().filter(|&c| c == '#').count();
            // Build the closing pattern
            let closing = format!("\"{}", "#".repeat(hash_count));
            // Find the content between opening and closing
            let content_start = quote_start + 1;
            let content_end = lit.rfind(&closing)?;
            if content_start < content_end {
                return Some(lit[content_start..content_end].to_string());
            }
        }
        // Regular string literal: "..."
        else if lit.starts_with('"') && lit.ends_with('"') && lit.len() >= 2 {
            // Remove the surrounding quotes and unescape
            let content = &lit[1..lit.len() - 1];
            // Basic unescaping (handles common cases)
            return Some(
                content
                    .replace("\\n", "\n")
                    .replace("\\t", "\t")
                    .replace("\\\"", "\"")
                    .replace("\\\\", "\\"),
            );
        }

        None
    }
}

impl<'ast> Visit<'ast> for SqlStringExtractor {
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        let method_name = node.method.to_string();

        // Check for sqlx query patterns: .query(), .query_as(), etc.
        if matches!(
            method_name.as_str(),
            "query" | "query_as" | "query_scalar" | "query_as_unchecked"
        ) {
            // The first argument is typically the SQL string
            if let Some(first_arg) = node.args.first() {
                self.visit_expr(first_arg);
            }
        }

        // Continue visiting children
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        // Check for sqlx::query("...") style calls
        if let SynExpr::Path(path) = &*node.func {
            let path_str = path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");

            if path_str.contains("query") || path_str.ends_with("query_as") {
                if let Some(first_arg) = node.args.first() {
                    self.visit_expr(first_arg);
                }
            }
        }

        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_lit(&mut self, node: &'ast ExprLit) {
        // Extract string literals that look like SQL
        if let Lit::Str(lit_str) = &node.lit {
            let value = lit_str.value();

            if Self::looks_like_sql(&value) {
                self.sql_strings.push(value);
            }
        }

        syn::visit::visit_expr_lit(self, node);
    }

    fn visit_expr_macro(&mut self, node: &'ast ExprMacro) {
        // Handle sqlx macros: query!, query_as!, query_scalar!, etc.
        let macro_name = node
            .mac
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();

        if matches!(
            macro_name.as_str(),
            "query" | "query_as" | "query_scalar" | "query_as_unchecked"
        ) {
            // Extract string literals from the macro tokens
            self.extract_sql_from_tokens(&node.mac.tokens);
        }

        syn::visit::visit_expr_macro(self, node);
    }
}

/// Parse SQL strings and extract all referenced tables.
pub fn extract_tables_from_sql(sql_strings: &[String]) -> HashSet<String> {
    let mut tables = HashSet::new();
    let dialect = PostgreSqlDialect {};

    for sql in sql_strings {
        // Try to parse the SQL
        match Parser::parse_sql(&dialect, sql) {
            Ok(statements) => {
                for stmt in statements {
                    extract_tables_from_statement(&stmt, &mut tables);
                }
            }
            Err(_) => {
                // SQL parsing failed - might have placeholders like $1, $2
                // Try extracting with a simple approach
                extract_tables_simple(sql, &mut tables);
            }
        }
    }

    tables
}

/// Extract tables from a parsed SQL statement.
fn extract_tables_from_statement(stmt: &Statement, tables: &mut HashSet<String>) {
    match stmt {
        Statement::Query(query) => {
            extract_tables_from_query(query, tables);
        }
        Statement::Insert(insert) => {
            // INSERT INTO table_name - use table field
            let name = normalize_table_name(&insert.table.to_string());
            tables.insert(name);

            // Also check subqueries in INSERT ... SELECT
            if let Some(src) = &insert.source {
                extract_tables_from_query(src, tables);
            }
        }
        Statement::Update {
            table, selection, ..
        } => {
            // UPDATE table_name
            extract_tables_from_table_with_joins(table, tables);

            // WHERE clause might have subqueries
            if let Some(sel) = selection {
                extract_tables_from_expr(sel, tables);
            }
        }
        Statement::Delete(delete) => {
            // DELETE FROM - handle FromTable enum
            extract_tables_from_from_table(&delete.from, tables);

            if let Some(sel) = &delete.selection {
                extract_tables_from_expr(sel, tables);
            }
        }
        _ => {}
    }
}

/// Extract tables from a FromTable (used in DELETE statements).
fn extract_tables_from_from_table(from: &sqlparser::ast::FromTable, tables: &mut HashSet<String>) {
    match from {
        sqlparser::ast::FromTable::WithFromKeyword(table_with_joins_list) => {
            for twj in table_with_joins_list {
                extract_tables_from_table_with_joins(twj, tables);
            }
        }
        sqlparser::ast::FromTable::WithoutKeyword(table_with_joins_list) => {
            for twj in table_with_joins_list {
                extract_tables_from_table_with_joins(twj, tables);
            }
        }
    }
}

/// Extract tables from a query (SELECT statement).
fn extract_tables_from_query(query: &Query, tables: &mut HashSet<String>) {
    // Handle CTEs (WITH clause)
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            // The CTE itself defines a temporary table, but we need
            // to extract tables from its definition
            extract_tables_from_query(&cte.query, tables);
        }
    }

    // Handle the main query body
    extract_tables_from_set_expr(&query.body, tables);
}

/// Extract tables from a SET expression (UNION, INTERSECT, etc.).
fn extract_tables_from_set_expr(set_expr: &SetExpr, tables: &mut HashSet<String>) {
    match set_expr {
        SetExpr::Select(select) => {
            extract_tables_from_select(select, tables);
        }
        SetExpr::Query(query) => {
            extract_tables_from_query(query, tables);
        }
        SetExpr::SetOperation { left, right, .. } => {
            extract_tables_from_set_expr(left, tables);
            extract_tables_from_set_expr(right, tables);
        }
        SetExpr::Values(_) => {}
        SetExpr::Insert(insert_stmt) => {
            // Handle nested INSERT - insert_stmt is a Box<Statement>
            extract_tables_from_statement(insert_stmt, tables);
        }
        SetExpr::Table(t) => {
            if let Some(name) = &t.table_name {
                tables.insert(normalize_table_name(name));
            }
        }
        SetExpr::Update(_) => {}
    }
}

/// Extract tables from a SELECT statement.
fn extract_tables_from_select(select: &Select, tables: &mut HashSet<String>) {
    // FROM clause
    for table_with_joins in &select.from {
        extract_tables_from_table_with_joins(table_with_joins, tables);
    }

    // SELECT list (might have subqueries)
    for item in &select.projection {
        match item {
            SelectItem::ExprWithAlias { expr, .. } => {
                extract_tables_from_expr(expr, tables);
            }
            SelectItem::UnnamedExpr(expr) => {
                extract_tables_from_expr(expr, tables);
            }
            _ => {}
        }
    }

    // WHERE clause (might have subqueries)
    if let Some(selection) = &select.selection {
        extract_tables_from_expr(selection, tables);
    }

    // HAVING clause
    if let Some(having) = &select.having {
        extract_tables_from_expr(having, tables);
    }
}

/// Extract tables from a table with joins.
fn extract_tables_from_table_with_joins(twj: &TableWithJoins, tables: &mut HashSet<String>) {
    extract_tables_from_table_factor(&twj.relation, tables);

    for join in &twj.joins {
        extract_tables_from_table_factor(&join.relation, tables);
    }
}

/// Extract table name from a table factor.
fn extract_tables_from_table_factor(factor: &TableFactor, tables: &mut HashSet<String>) {
    match factor {
        TableFactor::Table { name, .. } => {
            let table_name = normalize_table_name(&name.to_string());
            tables.insert(table_name);
        }
        TableFactor::Derived { subquery, .. } => {
            extract_tables_from_query(subquery, tables);
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            extract_tables_from_table_with_joins(table_with_joins, tables);
        }
        // Handle all other variants - they don't contain table references we need
        _ => {}
    }
}

/// Extract tables from expressions (for subqueries).
fn extract_tables_from_expr(expr: &Expr, tables: &mut HashSet<String>) {
    match expr {
        Expr::Subquery(query) => {
            extract_tables_from_query(query, tables);
        }
        Expr::InSubquery { subquery, .. } => {
            extract_tables_from_query(subquery, tables);
        }
        Expr::Exists { subquery, .. } => {
            extract_tables_from_query(subquery, tables);
        }
        Expr::BinaryOp { left, right, .. } => {
            extract_tables_from_expr(left, tables);
            extract_tables_from_expr(right, tables);
        }
        Expr::UnaryOp { expr, .. } => {
            extract_tables_from_expr(expr, tables);
        }
        Expr::Nested(expr) => {
            extract_tables_from_expr(expr, tables);
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            extract_tables_from_expr(expr, tables);
            extract_tables_from_expr(low, tables);
            extract_tables_from_expr(high, tables);
        }
        Expr::Case {
            operand,
            conditions,
            results,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                extract_tables_from_expr(op, tables);
            }
            for cond in conditions {
                extract_tables_from_expr(cond, tables);
            }
            for res in results {
                extract_tables_from_expr(res, tables);
            }
            if let Some(else_r) = else_result {
                extract_tables_from_expr(else_r, tables);
            }
        }
        Expr::Function(func) => {
            // Check function arguments for subqueries
            if let sqlparser::ast::FunctionArguments::List(arg_list) = &func.args {
                for arg in &arg_list.args {
                    if let sqlparser::ast::FunctionArg::Unnamed(
                        sqlparser::ast::FunctionArgExpr::Expr(e),
                    ) = arg
                    {
                        extract_tables_from_expr(e, tables);
                    }
                }
            }
        }
        Expr::InList { list, .. } => {
            for e in list {
                extract_tables_from_expr(e, tables);
            }
        }
        Expr::IsFalse(e)
        | Expr::IsNotFalse(e)
        | Expr::IsTrue(e)
        | Expr::IsNotTrue(e)
        | Expr::IsNull(e)
        | Expr::IsNotNull(e)
        | Expr::IsUnknown(e)
        | Expr::IsNotUnknown(e) => {
            extract_tables_from_expr(e, tables);
        }
        _ => {}
    }
}

/// Normalize a table name by removing schema qualifiers.
/// e.g., "public.users" -> "users", "\"Users\"" -> "Users"
fn normalize_table_name(name: &str) -> String {
    let name = name.trim();

    // Remove quotes
    let name = name.trim_matches('"').trim_matches('\'');

    // Handle schema.table format - take the last part
    if let Some(pos) = name.rfind('.') {
        name[pos + 1..].trim_matches('"').to_string()
    } else {
        name.to_string()
    }
}

/// Simple extraction for SQL that fails to parse (e.g., with placeholders).
fn extract_tables_simple(sql: &str, tables: &mut HashSet<String>) {
    // Remove string literals to avoid false matches
    let sql = remove_string_literals(sql);

    // Simple patterns for common cases
    let patterns = ["FROM", "JOIN", "INTO", "UPDATE"];

    let upper = sql.to_uppercase();

    for keyword in &patterns {
        let mut search_start = 0;
        while let Some(pos) = upper[search_start..].find(keyword) {
            let abs_pos = search_start + pos + keyword.len();
            if abs_pos < sql.len() {
                // Skip whitespace
                let rest = &sql[abs_pos..];
                let trimmed = rest.trim_start();

                // Extract the table name (until whitespace, comma, or parenthesis)
                let table_end = trimmed
                    .find(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == ')')
                    .unwrap_or(trimmed.len());

                if table_end > 0 {
                    let table_name = &trimmed[..table_end];
                    // Skip if it's a keyword or looks like a subquery
                    let table_upper = table_name.to_uppercase();
                    if !matches!(
                        table_upper.as_str(),
                        "SELECT" | "WHERE" | "SET" | "VALUES" | "ON" | "AS" | "AND" | "OR"
                    ) && !table_name.starts_with('(')
                        && !table_name.starts_with('$')
                    {
                        tables.insert(normalize_table_name(table_name));
                    }
                }
            }
            search_start = abs_pos;
        }
    }
}

/// Remove string literals from SQL to avoid false matches.
fn remove_string_literals(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());
    let mut in_string = false;
    let mut string_char = ' ';

    for c in sql.chars() {
        if in_string {
            if c == string_char {
                in_string = false;
            }
            // Skip character
        } else if c == '\'' || c == '"' {
            in_string = true;
            string_char = c;
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_select() {
        let tables = extract_tables_from_sql(&["SELECT * FROM users".to_string()]);
        assert!(tables.contains("users"));
    }

    #[test]
    fn test_join() {
        let tables = extract_tables_from_sql(&[
            "SELECT u.*, p.name FROM users u JOIN projects p ON u.id = p.user_id".to_string(),
        ]);
        assert!(tables.contains("users"));
        assert!(tables.contains("projects"));
    }

    #[test]
    fn test_left_join() {
        let tables = extract_tables_from_sql(&[
            "SELECT * FROM users u LEFT JOIN orders o ON u.id = o.user_id".to_string(),
        ]);
        assert!(tables.contains("users"));
        assert!(tables.contains("orders"));
    }

    #[test]
    fn test_schema_qualified() {
        let tables = extract_tables_from_sql(&["SELECT * FROM public.users".to_string()]);
        assert!(tables.contains("users"));
    }

    #[test]
    fn test_subquery_in_where() {
        let tables = extract_tables_from_sql(&[
            "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders)".to_string(),
        ]);
        assert!(tables.contains("users"));
        assert!(tables.contains("orders"));
    }

    #[test]
    fn test_exists_subquery() {
        let tables = extract_tables_from_sql(&[
            "SELECT * FROM users u WHERE EXISTS(SELECT 1 FROM orders o WHERE o.user_id = u.id)"
                .to_string(),
        ]);
        assert!(tables.contains("users"));
        assert!(tables.contains("orders"));
    }

    #[test]
    fn test_cte() {
        let tables = extract_tables_from_sql(&[
            "WITH active AS (SELECT * FROM users WHERE active = true) SELECT * FROM active JOIN projects ON active.id = projects.user_id".to_string()
        ]);
        assert!(tables.contains("users"));
        assert!(tables.contains("projects"));
    }

    #[test]
    fn test_multiple_joins() {
        let tables = extract_tables_from_sql(&[
            "SELECT * FROM users u INNER JOIN projects p ON u.id = p.user_id LEFT JOIN tasks t ON p.id = t.project_id".to_string()
        ]);
        assert!(tables.contains("users"));
        assert!(tables.contains("projects"));
        assert!(tables.contains("tasks"));
    }

    #[test]
    fn test_insert() {
        let tables =
            extract_tables_from_sql(&["INSERT INTO users (name) VALUES ('test')".to_string()]);
        assert!(tables.contains("users"));
    }

    #[test]
    fn test_insert_select() {
        let tables = extract_tables_from_sql(&[
            "INSERT INTO audit_log (user_id) SELECT id FROM users".to_string(),
        ]);
        assert!(tables.contains("audit_log"));
        assert!(tables.contains("users"));
    }

    #[test]
    fn test_update() {
        let tables =
            extract_tables_from_sql(&["UPDATE users SET name = 'test' WHERE id = 1".to_string()]);
        assert!(tables.contains("users"));
    }

    #[test]
    fn test_delete() {
        let tables = extract_tables_from_sql(&["DELETE FROM users WHERE id = 1".to_string()]);
        assert!(tables.contains("users"));
    }

    #[test]
    fn test_union() {
        let tables = extract_tables_from_sql(&[
            "SELECT id FROM users UNION SELECT id FROM admins".to_string(),
        ]);
        assert!(tables.contains("users"));
        assert!(tables.contains("admins"));
    }

    #[test]
    fn test_subquery_in_from() {
        let tables = extract_tables_from_sql(&[
            "SELECT * FROM (SELECT * FROM users WHERE active = true) AS active_users".to_string(),
        ]);
        assert!(tables.contains("users"));
    }

    #[test]
    fn test_normalize_quoted() {
        assert_eq!(normalize_table_name("\"Users\""), "Users");
        assert_eq!(normalize_table_name("'users'"), "users");
    }

    #[test]
    fn test_normalize_schema() {
        assert_eq!(normalize_table_name("public.users"), "users");
        assert_eq!(normalize_table_name("schema.\"Table\""), "Table");
    }

    #[test]
    fn test_multiple_sql_strings() {
        let tables = extract_tables_from_sql(&[
            "SELECT * FROM users".to_string(),
            "SELECT * FROM projects".to_string(),
        ]);
        assert!(tables.contains("users"));
        assert!(tables.contains("projects"));
    }

    #[test]
    fn test_sql_with_placeholders() {
        // This might fail to parse but should fall back to simple extraction
        let tables = extract_tables_from_sql(&[
            "SELECT * FROM users WHERE id = $1 AND name = $2".to_string(),
        ]);
        assert!(tables.contains("users"));
    }

    #[test]
    fn test_complex_query_with_placeholders() {
        let tables = extract_tables_from_sql(&[
            "SELECT r.*, s.current_streak FROM rituals r LEFT JOIN streaks s ON r.id = s.ritual_id WHERE r.user_id = $1".to_string()
        ]);
        assert!(tables.contains("rituals"));
        assert!(tables.contains("streaks"));
    }

    #[test]
    fn test_extract_string_content_regular() {
        assert_eq!(
            SqlStringExtractor::extract_string_content(r#""SELECT * FROM users""#),
            Some("SELECT * FROM users".to_string())
        );
    }

    #[test]
    fn test_extract_string_content_raw() {
        assert_eq!(
            SqlStringExtractor::extract_string_content(r###"r#"SELECT * FROM users"#"###),
            Some("SELECT * FROM users".to_string())
        );
    }

    #[test]
    fn test_stillpoint_query() {
        // This is the actual SQL from stillpoint's get_rituals query
        let sql = r#"
        SELECT
            r.id,
            r.user_id,
            r.emoji,
            r.title,
            r.description,
            r.sort_order,
            r.is_active,
            r.created_at,
            r.updated_at,
            COALESCE(s.current_streak, 0) as "current_streak!",
            COALESCE(s.longest_streak, 0) as "longest_streak!",
            COALESCE(s.streak_status, 'none') as "streak_status!",
            COALESCE(s.status_emoji, '') as "status_emoji!",
            s.last_completed_at,
            EXISTS(
                SELECT 1 FROM completions c
                WHERE c.ritual_id = r.id AND c.completed_date = $2
            ) as "completed_today!"
        FROM rituals r
        LEFT JOIN streaks s ON s.ritual_id = r.id
        WHERE r.user_id = $1 AND r.is_active = true
        ORDER BY r.sort_order ASC, r.created_at ASC
        "#;
        let tables = extract_tables_from_sql(&[sql.to_string()]);
        assert!(tables.contains("rituals"), "Should contain rituals");
        assert!(tables.contains("streaks"), "Should contain streaks");
        assert!(tables.contains("completions"), "Should contain completions");
    }
}
