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
}
