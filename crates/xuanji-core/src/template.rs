use crate::error::CoreError;
use crate::types::TaskResult;
use regex::Regex;
use std::collections::HashMap;

/// Context for template variable resolution.
#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    pub inputs: HashMap<String, serde_json::Value>,
    pub tasks: HashMap<String, TaskResult>,
    pub env: HashMap<String, String>,
}

/// Resolve all `${{ expr }}` template variables in a JSON value.
///
/// - If the entire string is a single `${{ }}` expression, the resolved value
///   keeps its original type (number, bool, etc.).
/// - If `${{ }}` is embedded in surrounding text, the resolved value is
///   converted to a string and substituted inline.
/// - Recursively resolves inside objects and arrays.
pub fn resolve_templates(
    value: &serde_json::Value,
    ctx: &TemplateContext,
) -> Result<serde_json::Value, CoreError> {
    match value {
        serde_json::Value::String(s) => resolve_string(s, ctx),
        serde_json::Value::Object(map) => {
            let resolved: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| Ok((k.clone(), resolve_templates(v, ctx)?)))
                .collect::<Result<_, CoreError>>()?;
            Ok(serde_json::Value::Object(resolved))
        }
        serde_json::Value::Array(arr) => {
            let resolved: Vec<serde_json::Value> = arr
                .iter()
                .map(|v| resolve_templates(v, ctx))
                .collect::<Result<_, CoreError>>()?;
            Ok(serde_json::Value::Array(resolved))
        }
        other => Ok(other.clone()),
    }
}

fn resolve_string(s: &str, ctx: &TemplateContext) -> Result<serde_json::Value, CoreError> {
    let re = Regex::new(r"\$\{\{\s*([^}]+)\s*\}\}").unwrap();

    if !re.is_match(s) {
        return Ok(serde_json::Value::String(s.to_string()));
    }

    // If the entire string is a single template expression, return the typed value
    let trimmed = s.trim();
    if let Some(caps) = re.captures(trimmed) {
        if caps.get(0).unwrap().as_str() == trimmed {
            let path = caps.get(1).unwrap().as_str().trim();
            return resolve_path(path, ctx);
        }
    }

    // Otherwise, substitute all occurrences as strings
    let result = re.replace_all(s, |caps: &regex::Captures| {
        let path = caps.get(1).unwrap().as_str().trim();
        match resolve_path(path, ctx) {
            Ok(val) => val.as_str().unwrap_or(&val.to_string()).to_string(),
            Err(_) => format!("${{{{{}}}}}", path),
        }
    });

    Ok(serde_json::Value::String(result.to_string()))
}

fn resolve_path(path: &str, ctx: &TemplateContext) -> Result<serde_json::Value, CoreError> {
    let parts: Vec<&str> = path.split('.').collect();

    match parts.first() {
        Some(&"inputs") => {
            let key = parts.get(1).ok_or_else(|| {
                CoreError::Template(format!("invalid template path: {}", path))
            })?;
            ctx.inputs
                .get(*key)
                .cloned()
                .ok_or_else(|| CoreError::Template(format!("input '{}' not found", key)))
        }
        Some(&"tasks") => {
            let task_name = parts.get(1).ok_or_else(|| {
                CoreError::Template(format!("invalid template path: {}", path))
            })?;
            let field = parts.get(2).ok_or_else(|| {
                CoreError::Template(format!("invalid template path: {}", path))
            })?;
            let result = ctx.tasks.get(*task_name).ok_or_else(|| {
                CoreError::Template(format!("task '{}' result not found", task_name))
            })?;
            match *field {
                "output" => Ok(result.output.clone()),
                "status" => Ok(serde_json::Value::String(result.status.to_string())),
                _ => Err(CoreError::Template(format!(
                    "unknown task field: '{}'",
                    field
                ))),
            }
        }
        Some(&"env") => {
            let key = parts.get(1).ok_or_else(|| {
                CoreError::Template(format!("invalid template path: {}", path))
            })?;
            ctx.env
                .get(*key)
                .map(|v| serde_json::Value::String(v.clone()))
                .ok_or_else(|| CoreError::Template(format!("env var '{}' not found", key)))
        }
        _ => Err(CoreError::Template(format!(
            "unknown template variable namespace in: {}",
            path
        ))),
    }
}

/// Evaluate a `when` condition expression.
///
/// After template resolution, the result is compared against truthy values.
pub fn evaluate_when(expr: &str, ctx: &TemplateContext) -> Result<bool, CoreError> {
    let resolved = resolve_templates(&serde_json::Value::String(expr.to_string()), ctx)?;

    let s = match resolved {
        serde_json::Value::Bool(b) => return Ok(b),
        serde_json::Value::String(s) => s.to_lowercase(),
        other => other.to_string(),
    };

    match s.as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" | "" => Ok(false),
        _ => Err(CoreError::Template(format!(
            "cannot evaluate '{}' as boolean",
            s
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TaskStatus;

    fn make_ctx() -> TemplateContext {
        let mut ctx = TemplateContext::default();
        ctx.inputs.insert("name".into(), serde_json::json!("world"));
        ctx.inputs.insert("count".into(), serde_json::json!(42));
        ctx.inputs.insert("flag".into(), serde_json::json!(true));
        ctx.env.insert("HOME".into(), "/home/user".into());
        ctx.tasks.insert(
            "build".into(),
            TaskResult {
                status: TaskStatus::Done,
                output: serde_json::json!("build succeeded"),
            },
        );
        ctx
    }

    #[test]
    fn test_simple_string_substitution() {
        let ctx = make_ctx();
        let val = serde_json::json!("hello ${{ inputs.name }}");
        let resolved = resolve_templates(&val, &ctx).unwrap();
        assert_eq!(resolved, serde_json::json!("hello world"));
    }

    #[test]
    fn test_whole_value_preserves_type() {
        let ctx = make_ctx();
        let val = serde_json::json!("${{ inputs.count }}");
        let resolved = resolve_templates(&val, &ctx).unwrap();
        assert_eq!(resolved, serde_json::json!(42));
    }

    #[test]
    fn test_whole_value_bool() {
        let ctx = make_ctx();
        let val = serde_json::json!("${{ inputs.flag }}");
        let resolved = resolve_templates(&val, &ctx).unwrap();
        assert_eq!(resolved, serde_json::json!(true));
    }

    #[test]
    fn test_nested_json() {
        let ctx = make_ctx();
        let val = serde_json::json!({
            "url": "https://${{ inputs.name }}/api",
            "count": "${{ inputs.count }}"
        });
        let resolved = resolve_templates(&val, &ctx).unwrap();
        assert_eq!(resolved["url"], "https://world/api");
        assert_eq!(resolved["count"], 42);
    }

    #[test]
    fn test_task_output_reference() {
        let ctx = make_ctx();
        let val = serde_json::json!("${{ tasks.build.output }}");
        let resolved = resolve_templates(&val, &ctx).unwrap();
        assert_eq!(resolved, serde_json::json!("build succeeded"));
    }

    #[test]
    fn test_task_status_reference() {
        let ctx = make_ctx();
        let val = serde_json::json!("${{ tasks.build.status }}");
        let resolved = resolve_templates(&val, &ctx).unwrap();
        assert_eq!(resolved, serde_json::json!("done"));
    }

    #[test]
    fn test_env_variable() {
        let ctx = make_ctx();
        let val = serde_json::json!("${{ env.HOME }}");
        let resolved = resolve_templates(&val, &ctx).unwrap();
        assert_eq!(resolved, serde_json::json!("/home/user"));
    }

    #[test]
    fn test_missing_input_returns_error() {
        let ctx = TemplateContext::default();
        let val = serde_json::json!("${{ inputs.missing }}");
        let result = resolve_templates(&val, &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_no_templates_passthrough() {
        let ctx = make_ctx();
        let val = serde_json::json!("plain text");
        let resolved = resolve_templates(&val, &ctx).unwrap();
        assert_eq!(resolved, serde_json::json!("plain text"));
    }

    #[test]
    fn test_evaluate_when_true() {
        let ctx = make_ctx();
        assert!(evaluate_when("true", &ctx).unwrap());
        assert!(evaluate_when("${{ inputs.flag }}", &ctx).unwrap());
    }

    #[test]
    fn test_evaluate_when_false() {
        let ctx = make_ctx();
        assert!(!evaluate_when("false", &ctx).unwrap());
    }
}
