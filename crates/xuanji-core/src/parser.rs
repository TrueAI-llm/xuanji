use crate::error::CoreError;
use crate::types::WorkflowDef;

/// Parse a YAML string into a WorkflowDef.
pub fn parse_workflow(yaml_str: &str) -> Result<WorkflowDef, CoreError> {
    let workflow: WorkflowDef = serde_yml::from_str(yaml_str)
        .map_err(|e| CoreError::YamlParse(format!("{}", e)))?;

    validate_workflow(&workflow)?;

    Ok(workflow)
}

/// Validate a parsed workflow definition.
pub fn validate_workflow(workflow: &WorkflowDef) -> Result<(), CoreError> {
    if workflow.tasks.is_empty() {
        return Err(CoreError::DagValidation("workflow has no tasks".into()));
    }

    // Check all depends_on references point to existing tasks
    for (name, task) in &workflow.tasks {
        for dep in &task.depends_on {
            if !workflow.tasks.contains_key(dep) {
                return Err(CoreError::DagValidation(format!(
                    "task '{}' depends on '{}', which does not exist",
                    name, dep
                )));
            }
        }
        // Self-dependency check
        if task.depends_on.contains(name) {
            return Err(CoreError::DagValidation(format!(
                "task '{}' depends on itself",
                name
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_workflow() {
        let yaml = r#"
name: test
tasks:
  hello:
    tool: shell.run
    arguments:
      command: echo hello
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert_eq!(wf.name, "test");
        assert_eq!(wf.tasks.len(), 1);
        assert_eq!(wf.tasks["hello"].tool, "shell.run");
    }

    #[test]
    fn test_parse_workflow_with_dependencies() {
        let yaml = r#"
name: build
tasks:
  test:
    tool: shell.run
    arguments: { command: "cargo test" }
  build:
    tool: shell.run
    arguments: { command: "cargo build" }
    depends_on: [test]
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert_eq!(wf.tasks["build"].depends_on, vec!["test"]);
    }

    #[test]
    fn test_parse_workflow_with_inputs() {
        let yaml = r#"
name: deploy
inputs:
  environment:
    type: string
    default: staging
tasks:
  deploy:
    tool: shell.run
    arguments: { command: "deploy ${{ inputs.environment }}" }
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert!(wf.inputs.contains_key("environment"));
        assert_eq!(wf.inputs["environment"].default.as_ref().unwrap(), "staging");
    }

    #[test]
    fn test_parse_empty_tasks_rejected() {
        let yaml = r#"
name: empty
tasks: {}
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(matches!(err, CoreError::DagValidation(_)));
    }

    #[test]
    fn test_parse_missing_dependency_rejected() {
        let yaml = r#"
name: bad-dep
tasks:
  deploy:
    tool: shell.run
    arguments: { command: "deploy" }
    depends_on: [nonexistent]
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(matches!(err, CoreError::DagValidation(_)));
    }

    #[test]
    fn test_parse_self_dependency_rejected() {
        let yaml = r#"
name: self-dep
tasks:
  loop:
    tool: shell.run
    arguments: { command: "echo" }
    depends_on: [loop]
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(matches!(err, CoreError::DagValidation(_)));
    }

    #[test]
    fn test_parse_invalid_yaml_rejected() {
        let yaml = r#"
name: [invalid
  tasks: {{{"
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(matches!(err, CoreError::YamlParse(_)));
    }
}
