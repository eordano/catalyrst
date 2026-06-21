//! Quest progression state machine — faithful port of
//! `decentraland/quests` crates/protocol/src/quests/{graph,state}.rs over the
//! generated protobuf types (`Quest`, `Event`, `QuestState`, `StepContent`,
//! `Task`, `Action`).
//!
//! A quest is a DAG of steps wired by `connections`; synthetic `_START_` /
//! `_END_` nodes bracket it. The initial state activates every step reachable
//! from `_START_` (i.e. steps with no incoming connection). Each event removes
//! a matched action-item from a current task; emptying a task completes it;
//! emptying a step's to-dos completes the step, decrements `steps_left`, and
//! activates its successors. `required_steps` are the steps directly pointing
//! to `_END_`. Action matching is case-insensitive on both type and parameters.

use std::collections::{HashMap, HashSet};

use crate::proto::{
    Action, Connection, Event, Quest, QuestDefinition, QuestState, StepContent, Task,
};

pub const START_STEP_ID: &str = "_START_";
pub const END_STEP_ID: &str = "_END_";

pub type StepID = String;

// ---- Quest / QuestDefinition graph helpers (port of quests/mod.rs) ----

fn quest_definition(quest: &Quest) -> Option<&QuestDefinition> {
    quest.definition.as_ref()
}

fn contains_step(quest: &Quest, step_id: &str) -> bool {
    match quest_definition(quest) {
        Some(def) => def.steps.iter().any(|s| s.id == step_id),
        None => false,
    }
}

/// Steps that never appear as the `from` of a connection — they point to `_END_`.
fn steps_without_to(quest: &Quest) -> HashSet<StepID> {
    let mut steps = HashSet::new();
    let Some(def) = quest_definition(quest) else {
        return steps;
    };
    let mut connections = HashMap::new();
    for c in &def.connections {
        connections.insert(c.step_from.clone(), c.step_to.clone());
    }
    for step in &def.steps {
        if !connections.contains_key(&step.id) {
            steps.insert(step.id.clone());
        }
    }
    steps
}

/// Steps that never appear as the `to` of a connection — they branch off `_START_`.
fn steps_without_from(quest: &Quest) -> HashSet<StepID> {
    let mut steps = HashSet::new();
    let Some(def) = quest_definition(quest) else {
        return steps;
    };
    let mut connections = HashMap::new();
    for c in &def.connections {
        connections.insert(c.step_to.clone(), c.step_from.clone());
    }
    for step in &def.steps {
        if !connections.contains_key(&step.id) {
            steps.insert(step.id.clone());
        }
    }
    steps
}

/// Clear all action_items off a task (upstream `Task::hide_actions`).
pub fn hide_task_actions(task: &mut Task) {
    task.action_items.clear();
}

/// Strip action_items off every to-do in the live state (upstream
/// `QuestState::hide_actions`) so streamed updates don't leak the answer set.
pub fn hide_state_actions(state: &mut QuestState) {
    for step in state.current_steps.values_mut() {
        for task in &mut step.to_dos {
            hide_task_actions(task);
        }
    }
}

/// Strip action_items off a quest's full definition (upstream `Quest::hide_actions`).
pub fn hide_quest_actions(quest: &mut Quest) {
    if let Some(def) = quest.definition.as_mut() {
        for step in &mut def.steps {
            for task in &mut step.tasks {
                hide_task_actions(task);
            }
        }
    }
}

// ---- QuestGraph (port of quests/graph.rs) ----

/// Directed adjacency over the quest's steps, bracketed by `_START_` / `_END_`.
pub struct QuestGraph {
    /// step -> successors
    next: HashMap<String, Vec<String>>,
    /// step -> predecessors
    prev: HashMap<String, Vec<String>>,
    /// real step count (excludes the two synthetic nodes)
    total_steps: usize,
    pub tasks_by_step: HashMap<StepID, Vec<Task>>,
}

impl QuestGraph {
    pub fn next(&self, from: &str) -> Option<Vec<String>> {
        self.next.get(from).cloned()
    }

    pub fn prev(&self, from: &str) -> Option<Vec<String>> {
        self.prev.get(from).cloned()
    }

    pub fn required_for_end(&self) -> Option<Vec<StepID>> {
        self.prev(END_STEP_ID)
    }

    pub fn total_steps(&self) -> usize {
        self.total_steps
    }
}

impl From<&Quest> for QuestGraph {
    fn from(quest: &Quest) -> Self {
        let mut next: HashMap<String, Vec<String>> = HashMap::new();
        let mut prev: HashMap<String, Vec<String>> = HashMap::new();
        let mut add_edge = |from: &str, to: &str| {
            next.entry(from.to_string())
                .or_default()
                .push(to.to_string());
            prev.entry(to.to_string())
                .or_default()
                .push(from.to_string());
        };

        let tasks_by_step = build_tasks_by_step(quest);
        let total_steps = quest_definition(quest).map(|d| d.steps.len()).unwrap_or(0);

        let Some(def) = quest_definition(quest) else {
            return Self {
                next,
                prev,
                total_steps,
                tasks_by_step,
            };
        };

        // Inter-step connections. With no connections, every step is isolated
        // (each is both a start and an end node, matching upstream).
        for Connection { step_from, step_to } in &def.connections {
            if contains_step(quest, step_from) && contains_step(quest, step_to) {
                add_edge(step_from, step_to);
            }
        }

        // Edges to the synthetic END node.
        for step in steps_without_to(quest) {
            add_edge(&step, END_STEP_ID);
        }
        // Edges from the synthetic START node.
        for step in steps_without_from(quest) {
            add_edge(START_STEP_ID, &step);
        }

        Self {
            next,
            prev,
            total_steps,
            tasks_by_step,
        }
    }
}

fn build_tasks_by_step(quest: &Quest) -> HashMap<StepID, Vec<Task>> {
    let mut map = HashMap::new();
    if let Some(def) = quest_definition(quest) {
        for step in &def.steps {
            map.insert(step.id.clone(), step.tasks.clone());
        }
    }
    map
}

/// Case-insensitive equality on action type and parameters (port of
/// `quests/graph.rs::matches_action`).
pub fn matches_action(action: &Action, other_action: &Action) -> bool {
    if !action.r#type.eq_ignore_ascii_case(&other_action.r#type) {
        return false;
    }
    if action.parameters.len() != other_action.parameters.len() {
        return false;
    }
    for (key, value) in &action.parameters {
        match other_action.parameters.get(key) {
            Some(other_value) if value.eq_ignore_ascii_case(other_value) => {}
            _ => return false,
        }
    }
    true
}

// ---- QuestState (port of quests/state.rs) ----

/// Whether all `required_steps` are present in `steps_completed`.
pub fn is_completed(state: &QuestState) -> bool {
    state
        .required_steps
        .iter()
        .all(|step| state.steps_completed.contains(step))
}

/// Initial state for a fresh instance — every step branching off `_START_`,
/// `required_steps` from the `_END_` predecessors, `steps_left == total_steps`.
fn initial_state(graph: &QuestGraph) -> QuestState {
    let current_steps = graph
        .next(START_STEP_ID)
        .unwrap_or_default()
        .iter()
        .map(|step| {
            (
                step.clone(),
                StepContent {
                    to_dos: graph.tasks_by_step.get(step).cloned().unwrap_or_default(),
                    tasks_completed: Vec::new(),
                },
            )
        })
        .collect::<HashMap<String, StepContent>>();

    QuestState {
        current_steps,
        required_steps: graph.required_for_end().unwrap_or_default(),
        steps_left: graph.total_steps() as u32,
        steps_completed: Vec::default(),
    }
}

/// Apply a single event to a state, returning the next state (port of
/// `QuestState::apply_event`). A `None` action returns the state unchanged.
pub fn apply_event(state: &QuestState, graph: &QuestGraph, event: &Event) -> QuestState {
    let mut next = state.clone();
    let Some(event_action) = event.action.as_ref() else {
        return next;
    };

    for (step_id, step_content) in &state.current_steps {
        if step_content.to_dos.is_empty() {
            continue;
        }
        for (i, task) in step_content.to_dos.iter().enumerate() {
            if let Some(matched) = task
                .action_items
                .iter()
                .position(|action| matches_action(action, event_action))
            {
                if let Some(step) = next.current_steps.get_mut(step_id) {
                    step.to_dos[i].action_items.remove(matched);
                    if step.to_dos[i].action_items.is_empty() {
                        let completed_task = step.to_dos[i].clone();
                        step.tasks_completed.push(completed_task);
                        step.to_dos.remove(i);
                    }
                }
            }
        }

        if let Some(step) = next.current_steps.get(step_id) {
            if step.to_dos.is_empty() {
                next.current_steps.remove(step_id);
                next.steps_left = next.steps_left.saturating_sub(1);

                for succ in graph.next(step_id).unwrap_or_default() {
                    if succ != END_STEP_ID {
                        let content = StepContent {
                            to_dos: graph.tasks_by_step.get(&succ).cloned().unwrap_or_default(),
                            tasks_completed: Vec::new(),
                        };
                        next.current_steps.insert(succ, content);
                    }
                }
                next.steps_completed.push(step_id.clone());
            }
        }
    }

    next
}

/// Fold the ordered event log over the initial state (port of
/// `quests/state.rs::get_state`).
pub fn get_state(quest: &Quest, events: &[Event]) -> QuestState {
    let graph = QuestGraph::from(quest);
    let initial = initial_state(&graph);
    events
        .iter()
        .fold(initial, |state, event| apply_event(&state, &graph, event))
}

/// Compute an instance's `QuestState` for a pre-decoded quest by folding its
/// stored event log (upstream `get_instance_state` over an already-loaded
/// quest). Used by the RPC service's StartQuest/GetAllQuests success paths.
pub async fn compute_instance_state_quest(
    db: &crate::db::Db,
    quest: &Quest,
    instance_id: &str,
) -> Result<QuestState, crate::quests::QuestError> {
    use crate::proto::ProtocolMessage;
    let stored_events = db.get_events(instance_id).await?;
    let events: Vec<Event> = stored_events
        .iter()
        .filter_map(|e| Event::decode(e.event.as_slice()).ok())
        .collect();
    Ok(get_state(quest, &events))
}

// ---- Test helpers mirroring quests/builders.rs ----

#[cfg(test)]
fn connection(from: &str, to: &str) -> Connection {
    Connection {
        step_from: from.to_string(),
        step_to: to.to_string(),
    }
}

#[cfg(test)]
fn location(x: i32, y: i32) -> Action {
    let mut parameters = std::collections::HashMap::new();
    parameters.insert("x".to_string(), x.to_string());
    parameters.insert("y".to_string(), y.to_string());
    Action {
        r#type: "LOCATION".to_string(),
        parameters,
    }
}

#[cfg(test)]
fn jump(x: i32, y: i32) -> Action {
    let mut parameters = std::collections::HashMap::new();
    parameters.insert("x".to_string(), x.to_string());
    parameters.insert("y".to_string(), y.to_string());
    Action {
        r#type: "JUMP".to_string(),
        parameters,
    }
}

#[cfg(test)]
fn custom(id: &str) -> Action {
    let mut parameters = std::collections::HashMap::new();
    parameters.insert("id".to_string(), id.to_string());
    Action {
        r#type: "CUSTOM".to_string(),
        parameters,
    }
}

#[cfg(test)]
fn event(action: Action) -> Event {
    Event {
        id: uuid::Uuid::new_v4().to_string(),
        address: "0xA".to_string(),
        action: Some(action),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::Step;

    fn linear_quest() -> Quest {
        Quest {
            definition: Some(QuestDefinition {
                connections: vec![connection("A1", "B"), connection("B", "C")],
                steps: vec![
                    Step {
                        id: "A1".to_string(),
                        description: "d".to_string(),
                        tasks: vec![Task {
                            id: "A1_1".to_string(),
                            description: "d".to_string(),
                            action_items: vec![location(10, 10), jump(10, 11)],
                        }],
                    },
                    Step {
                        id: "B".to_string(),
                        description: "d".to_string(),
                        tasks: vec![Task {
                            id: "B_1".to_string(),
                            description: "d".to_string(),
                            action_items: vec![jump(20, 10)],
                        }],
                    },
                    Step {
                        id: "C".to_string(),
                        description: "d".to_string(),
                        tasks: vec![Task {
                            id: "C_1".to_string(),
                            description: "d".to_string(),
                            action_items: vec![jump(20, 20)],
                        }],
                    },
                ],
            }),
            ..Default::default()
        }
    }

    #[test]
    fn initial_state_matches_upstream() {
        let q = linear_quest();
        let graph = QuestGraph::from(&q);
        let s = initial_state(&graph);
        assert!(s.current_steps.contains_key("A1"));
        assert_eq!(s.current_steps.len(), 1);
        assert!(s.steps_completed.is_empty());
        assert_eq!(s.steps_left, 3);
        assert_eq!(s.required_steps, vec!["C".to_string()]);
        assert!(!is_completed(&s));
    }

    #[test]
    fn full_run_completes_and_decrements_steps_left() {
        // Mirrors upstream quest_graph_apply_event_task_simple_works ordering.
        let q = linear_quest();
        let s = get_state(
            &q,
            &[
                event(location(10, 10)), // A1_1 needs both items
                event(jump(10, 11)),     // completes A1 -> activates B, steps_left 2
                event(jump(20, 10)),     // completes B -> activates C, steps_left 1
                event(jump(20, 20)),     // completes C -> END, steps_left 0
            ],
        );
        assert!(s.current_steps.is_empty());
        assert_eq!(s.steps_left, 0);
        assert!(s.steps_completed.contains(&"A1".to_string()));
        assert!(s.steps_completed.contains(&"B".to_string()));
        assert!(s.steps_completed.contains(&"C".to_string()));
        assert!(is_completed(&s));
    }

    #[test]
    fn single_step_no_connections_is_start_and_end() {
        let q = Quest {
            definition: Some(QuestDefinition {
                connections: vec![],
                steps: vec![Step {
                    id: "A".to_string(),
                    description: "d".to_string(),
                    tasks: vec![Task {
                        id: "A_1".to_string(),
                        description: "d".to_string(),
                        action_items: vec![custom("A1_1_ID")],
                    }],
                }],
            }),
            ..Default::default()
        };
        let graph = QuestGraph::from(&q);
        let s = initial_state(&graph);
        assert!(s.current_steps.contains_key("A"));
        assert_eq!(s.steps_left, 1);
        assert_eq!(s.required_steps, vec!["A".to_string()]);

        let s = get_state(&q, &[event(custom("A1_1_ID"))]);
        assert!(s.current_steps.is_empty());
        assert_eq!(s.steps_left, 0);
        assert!(is_completed(&s));
    }

    #[test]
    fn action_match_is_case_insensitive() {
        let mut lower = location(1, 2);
        lower.r#type = "location".to_string();
        assert!(matches_action(&location(1, 2), &lower));
        assert!(!matches_action(&location(1, 2), &location(1, 3)));
        assert!(!matches_action(&location(1, 2), &jump(1, 2)));
    }

    #[test]
    fn wrong_action_does_not_advance() {
        let q = linear_quest();
        let s = get_state(&q, &[event(jump(99, 99))]);
        assert!(s.current_steps.contains_key("A1"));
        assert!(s.steps_completed.is_empty());
        assert_eq!(s.steps_left, 3);
    }
}
