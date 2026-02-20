use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::note::Note;

#[derive(Debug, Deserialize, Default)]
pub struct Schema {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub fields: HashMap<String, FieldSchema>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Constraint {
    pub name: String,
    pub message: String,
    /// SPARQL SELECT query that returns violating notes (must select ?note and ?title)
    pub query: String,
}

#[derive(Debug, Deserialize)]
pub struct FieldSchema {
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    List,
    Enum,
    Date,
    Number,
}

#[derive(Debug)]
pub struct Violation {
    pub note_path: String,
    pub field: String,
    pub message: String,
}

impl Schema {
    /// Load schema from .kbase/schema.yaml, or return empty schema
    pub fn load(vault_path: &Path) -> Result<Self> {
        let schema_path = vault_path.join(".kbase").join("schema.yaml");

        if !schema_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&schema_path)
            .with_context(|| format!("Failed to read {}", schema_path.display()))?;

        let schema: Schema = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", schema_path.display()))?;

        Ok(schema)
    }

    /// Validate a note against the schema
    pub fn validate(&self, note: &Note) -> Vec<Violation> {
        let mut violations = Vec::new();
        let note_path = note.path.to_string_lossy().to_string();

        // Check required fields
        for field in &self.required {
            let has_field = match field.as_str() {
                "title" => !note.title.is_empty(),
                "tags" => !note.tags.is_empty(),
                _ => note.fields.contains_key(field),
            };

            if !has_field {
                violations.push(Violation {
                    note_path: note_path.clone(),
                    field: field.clone(),
                    message: format!("missing required field '{}'", field),
                });
            }
        }

        // Check field types
        for (field_name, field_schema) in &self.fields {
            if let Some(value) = note.fields.get(field_name) {
                if let Some(violation) =
                    self.validate_field(&note_path, field_name, value, field_schema)
                {
                    violations.push(violation);
                }
            }
        }

        // Validate tags field type if specified
        if let Some(field_schema) = self.fields.get("tags") {
            if field_schema.field_type != FieldType::List {
                violations.push(Violation {
                    note_path: note_path.clone(),
                    field: "tags".to_string(),
                    message: "tags field must be type 'list'".to_string(),
                });
            }
        }

        violations
    }

    /// Generate frontmatter YAML for a new note based on schema.
    /// Includes title, tags, required fields, and enum fields.
    pub fn generate_frontmatter(&self, title: &str) -> String {
        let mut lines = Vec::new();

        // Title is always first
        lines.push(format!("title: \"{}\"", title));

        // Tags are always included
        let tags_in_required = self.required.contains(&"tags".to_string());
        if !tags_in_required {
            lines.push("tags: []".to_string());
        }

        // Track which fields we've added
        let mut added_fields = std::collections::HashSet::new();
        added_fields.insert("title".to_string());
        added_fields.insert("tags".to_string());

        // Add required fields with defaults
        for field in &self.required {
            if added_fields.contains(field) {
                continue;
            }
            added_fields.insert(field.clone());

            let default_value = if let Some(schema) = self.fields.get(field) {
                self.default_value_for_type(&schema.field_type, &schema.values)
            } else {
                // No schema defined, assume string
                "\"\"".to_string()
            };

            if let Some(schema) = self.fields.get(field) {
                if schema.field_type == FieldType::Enum && !schema.values.is_empty() {
                    lines.push(format!("{}: {} # {}", field, default_value, schema.values.join(", ")));
                } else {
                    lines.push(format!("{}: {}", field, default_value));
                }
            } else {
                lines.push(format!("{}: {}", field, default_value));
            }
        }

        // Add non-required enum fields so user can see options
        for (field, schema) in &self.fields {
            if added_fields.contains(field) {
                continue;
            }
            if schema.field_type == FieldType::Enum && !schema.values.is_empty() {
                added_fields.insert(field.clone());
                lines.push(format!("{}: \"\" # {}", field, schema.values.join(", ")));
            }
        }

        lines.join("\n")
    }

    fn default_value_for_type(&self, field_type: &FieldType, values: &[String]) -> String {
        match field_type {
            FieldType::String => "\"\"".to_string(),
            FieldType::List => "[]".to_string(),
            FieldType::Enum => {
                if let Some(first) = values.first() {
                    format!("\"{}\"", first)
                } else {
                    "\"\"".to_string()
                }
            }
            FieldType::Date => {
                // Use today's date
                let now = std::time::SystemTime::now();
                let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                let secs = duration.as_secs();
                // Simple date calculation (approximate, good enough for defaults)
                let days = secs / 86400;
                let year = 1970 + days / 365;
                let day_of_year = days % 365;
                let month = (day_of_year / 30) + 1;
                let day = (day_of_year % 30) + 1;
                format!("\"{:04}-{:02}-{:02}\"", year, month.min(12), day.min(28))
            }
            FieldType::Number => "0".to_string(),
        }
    }

    fn validate_field(
        &self,
        note_path: &str,
        field_name: &str,
        value: &serde_yaml::Value,
        schema: &FieldSchema,
    ) -> Option<Violation> {
        match schema.field_type {
            FieldType::String => {
                if !value.is_string() {
                    return Some(Violation {
                        note_path: note_path.to_string(),
                        field: field_name.to_string(),
                        message: format!("'{}' must be a string", field_name),
                    });
                }
            }
            FieldType::List => {
                if !value.is_sequence() {
                    return Some(Violation {
                        note_path: note_path.to_string(),
                        field: field_name.to_string(),
                        message: format!("'{}' must be a list", field_name),
                    });
                }
            }
            FieldType::Enum => {
                if let Some(s) = value.as_str() {
                    if !schema.values.contains(&s.to_string()) {
                        return Some(Violation {
                            note_path: note_path.to_string(),
                            field: field_name.to_string(),
                            message: format!(
                                "'{}' must be one of: {}",
                                field_name,
                                schema.values.join(", ")
                            ),
                        });
                    }
                } else {
                    return Some(Violation {
                        note_path: note_path.to_string(),
                        field: field_name.to_string(),
                        message: format!("'{}' must be a string (enum)", field_name),
                    });
                }
            }
            FieldType::Date => {
                if let Some(s) = value.as_str() {
                    // Simple date validation: YYYY-MM-DD
                    if !is_valid_date(s) {
                        return Some(Violation {
                            note_path: note_path.to_string(),
                            field: field_name.to_string(),
                            message: format!("'{}' must be a valid date (YYYY-MM-DD)", field_name),
                        });
                    }
                } else {
                    return Some(Violation {
                        note_path: note_path.to_string(),
                        field: field_name.to_string(),
                        message: format!("'{}' must be a date string", field_name),
                    });
                }
            }
            FieldType::Number => {
                if !value.is_number() {
                    return Some(Violation {
                        note_path: note_path.to_string(),
                        field: field_name.to_string(),
                        message: format!("'{}' must be a number", field_name),
                    });
                }
            }
        }
        None
    }
}

fn is_valid_date(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return false;
    }

    let year = parts[0].parse::<u32>().ok();
    let month = parts[1].parse::<u32>().ok();
    let day = parts[2].parse::<u32>().ok();

    matches!((year, month, day), (Some(y), Some(m), Some(d)) if y >= 1000 && y <= 9999 && m >= 1 && m <= 12 && d >= 1 && d <= 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_valid_date_accepts_valid() {
        assert!(is_valid_date("2026-01-15"));
        assert!(is_valid_date("1999-12-31"));
    }

    #[test]
    fn is_valid_date_rejects_invalid() {
        assert!(!is_valid_date("not-a-date"));
        assert!(!is_valid_date("2026-13-01"));
        assert!(!is_valid_date("2026-01-32"));
        assert!(!is_valid_date("26-01-15"));
    }

    #[test]
    fn generate_frontmatter_basic() {
        let schema = Schema::default();
        let fm = schema.generate_frontmatter("My Note");
        assert!(fm.contains("title: \"My Note\""));
        assert!(fm.contains("tags: []"));
    }

    #[test]
    fn generate_frontmatter_with_required_fields() {
        let mut fields = HashMap::new();
        fields.insert(
            "status".to_string(),
            FieldSchema {
                field_type: FieldType::Enum,
                values: vec!["draft".to_string(), "published".to_string()],
            },
        );
        fields.insert(
            "author".to_string(),
            FieldSchema {
                field_type: FieldType::String,
                values: vec![],
            },
        );

        let schema = Schema {
            required: vec!["status".to_string(), "author".to_string()],
            fields,
            constraints: vec![],
        };

        let fm = schema.generate_frontmatter("Test");
        assert!(fm.contains("title: \"Test\""));
        assert!(fm.contains("tags: []"));
        assert!(fm.contains("status: \"draft\" # draft, published"));
        assert!(fm.contains("author: \"\""));
    }

    #[test]
    fn generate_frontmatter_includes_non_required_enums() {
        let mut fields = HashMap::new();
        fields.insert(
            "priority".to_string(),
            FieldSchema {
                field_type: FieldType::Enum,
                values: vec!["high".to_string(), "medium".to_string(), "low".to_string()],
            },
        );

        let schema = Schema {
            required: vec![],
            fields,
            constraints: vec![],
        };

        let fm = schema.generate_frontmatter("Test");
        assert!(fm.contains("priority: \"\" # high, medium, low"));
    }
}
