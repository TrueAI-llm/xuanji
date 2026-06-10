use crate::error::CoreError;
use crate::types::WorkflowDef;
use petgraph::algo::{is_cyclic_directed, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;

/// Build a directed graph from a workflow definition.
///
/// Returns the graph and a mapping from task names to node indices.
pub fn build_dag(workflow: &WorkflowDef) -> Result<(DiGraph<String, ()>, HashMap<String, NodeIndex>), CoreError> {
    let mut graph = DiGraph::new();
    let mut name_to_idx = HashMap::new();

    // Add nodes
    for name in workflow.tasks.keys() {
        let idx = graph.add_node(name.clone());
        name_to_idx.insert(name.clone(), idx);
    }

    // Add edges: dependency → task
    for (name, task) in &workflow.tasks {
        let task_idx = name_to_idx[name];
        for dep in &task.depends_on {
            let dep_idx = name_to_idx[dep];
            graph.add_edge(dep_idx, task_idx, ());
        }
    }

    // Cycle detection
    if is_cyclic_directed(&graph) {
        return Err(CoreError::DagCycle);
    }

    Ok((graph, name_to_idx))
}

/// Get a topological ordering of the DAG nodes.
pub fn topological_sort(graph: &DiGraph<String, ()>) -> Result<Vec<NodeIndex>, CoreError> {
    toposort(graph, None)
        .map_err(|_| CoreError::DagCycle)
}

/// Find all tasks that are ready to execute:
/// - Status is Pending
/// - All dependencies are Done or Skipped
pub fn find_ready_tasks(
    graph: &DiGraph<String, ()>,
    name_to_idx: &HashMap<String, NodeIndex>,
    statuses: &HashMap<String, crate::types::TaskStatus>,
    workflow: &WorkflowDef,
) -> Vec<String> {
    let mut ready = Vec::new();

    for (name, idx) in name_to_idx {
        let status = statuses.get(name).unwrap_or(&crate::types::TaskStatus::Pending);
        if !matches!(status, crate::types::TaskStatus::Pending) {
            continue;
        }

        // Check all dependencies are satisfied
        let mut deps_satisfied = true;
        for dep_idx in graph.neighbors_directed(*idx, petgraph::Direction::Incoming) {
            let dep_name = &graph[dep_idx];
            let dep_status = statuses.get(dep_name).unwrap_or(&crate::types::TaskStatus::Pending);
            if !matches!(dep_status, crate::types::TaskStatus::Done | crate::types::TaskStatus::Skipped) {
                deps_satisfied = false;
                break;
            }
        }

        if deps_satisfied {
            ready.push(name.clone());
        }
    }

    ready
}

/// Propagate failure: mark all downstream dependents of a failed task as Blocked.
pub fn propagate_failure(
    failed_task: &str,
    graph: &DiGraph<String, ()>,
    name_to_idx: &HashMap<String, NodeIndex>,
    statuses: &mut HashMap<String, crate::types::TaskStatus>,
) {
    if let Some(&start_idx) = name_to_idx.get(failed_task) {
        // BFS through outgoing edges (downstream)
        let mut stack = vec![start_idx];
        let mut visited = std::collections::HashSet::new();
        visited.insert(start_idx);

        while let Some(idx) = stack.pop() {
            for neighbor in graph.neighbors_directed(idx, petgraph::Direction::Outgoing) {
                if visited.insert(neighbor) {
                    let name = &graph[neighbor];
                    let status = statuses.get(name).unwrap_or(&crate::types::TaskStatus::Pending);
                    if matches!(status, crate::types::TaskStatus::Pending) {
                        statuses.insert(name.clone(), crate::types::TaskStatus::Blocked);
                    }
                    stack.push(neighbor);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TaskDef;

    fn make_workflow(tasks: Vec<(&str, Vec<&str>)>) -> WorkflowDef {
        WorkflowDef {
            name: "test".to_string(),
            description: String::new(),
            inputs: HashMap::new(),
            triggers: Vec::new(),
            tasks: tasks.into_iter().map(|(name, deps)| {
                (name.to_string(), TaskDef {
                    tool: "shell.run".to_string(),
                    arguments: serde_json::json!({}),
                    depends_on: deps.into_iter().map(String::from).collect(),
                    timeout: None,
                    retry: None,
                    confirm: false,
                    when: None,
                })
            }).collect(),
        }
    }

    #[test]
    fn test_build_dag_linear() {
        let wf = make_workflow(vec![
            ("a", vec![]),
            ("b", vec!["a"]),
            ("c", vec!["b"]),
        ]);
        let (graph, map) = build_dag(&wf).unwrap();
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);
    }

    #[test]
    fn test_build_dag_diamond() {
        let wf = make_workflow(vec![
            ("a", vec![]),
            ("b", vec!["a"]),
            ("c", vec!["a"]),
            ("d", vec!["b", "c"]),
        ]);
        let (graph, _) = build_dag(&wf).unwrap();
        assert_eq!(graph.node_count(), 4);
        assert_eq!(graph.edge_count(), 4);
    }

    #[test]
    fn test_detect_cycle() {
        let wf = make_workflow(vec![
            ("a", vec!["b"]),
            ("b", vec!["c"]),
            ("c", vec!["a"]),
        ]);
        let result = build_dag(&wf);
        assert!(matches!(result.unwrap_err(), CoreError::DagCycle));
    }

    #[test]
    fn test_topological_sort_order() {
        let wf = make_workflow(vec![
            ("a", vec![]),
            ("b", vec!["a"]),
            ("c", vec!["b"]),
        ]);
        let (graph, _) = build_dag(&wf).unwrap();
        let order = topological_sort(&graph).unwrap();
        let names: Vec<&str> = order.iter().map(|idx| graph[*idx].as_str()).collect();
        // "a" must come before "b", "b" before "c"
        let pos_a = names.iter().position(|&n| n == "a").unwrap();
        let pos_b = names.iter().position(|&n| n == "b").unwrap();
        let pos_c = names.iter().position(|&n| n == "c").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_find_ready_tasks() {
        let wf = make_workflow(vec![
            ("a", vec![]),
            ("b", vec!["a"]),
            ("c", vec!["a"]),
        ]);
        let (graph, map) = build_dag(&wf).unwrap();
        let statuses = HashMap::new();
        let ready = find_ready_tasks(&graph, &map, &statuses, &wf);
        assert_eq!(ready, vec!["a"]);
    }

    #[test]
    fn test_propagate_failure() {
        let wf = make_workflow(vec![
            ("a", vec![]),
            ("b", vec!["a"]),
            ("c", vec!["b"]),
        ]);
        let (graph, map) = build_dag(&wf).unwrap();
        let mut statuses = HashMap::new();
        statuses.insert("a".into(), crate::types::TaskStatus::Failed);
        statuses.insert("b".into(), crate::types::TaskStatus::Pending);
        statuses.insert("c".into(), crate::types::TaskStatus::Pending);

        propagate_failure("a", &graph, &map, &mut statuses);

        assert_eq!(statuses["b"], crate::types::TaskStatus::Blocked);
        assert_eq!(statuses["c"], crate::types::TaskStatus::Blocked);
    }
}
