use std::collections::{HashMap, HashSet};

use crate::proto::{Connection, QuestDefinition};

#[derive(Debug, thiserror::Error)]
pub enum QuestValidationError {
    #[error("Missing the definition for the quest")]
    InvalidDefinition,
    #[error("Missing a starting node for the quest")]
    NoStartingNode,
    #[error("Missing a end node for the quest")]
    NoEndNode,
    #[error("Step must have a description: {0}")]
    MissingDescriptionForStep(String),
    #[error("Task must have a description: {0}")]
    MissingDescriptionForTask(String),
    #[error("Connection half has no defined step - Step ID: {0}")]
    MissingStepDefinition(String),
    #[error("Step ID is not unique - Step ID: {0}")]
    NotUniqueIDForStep(String),
    #[error("Step's Task ID is not unique - Step ID: {0}")]
    NotUniqueIDForStepTask(String),
    #[error("Step {0} doesn't have tasks defined")]
    MissingTasksForStep(String),
    #[error("Action Item's type is not valid: {0}")]
    ActionItemTypeNotValid(String),
    #[error("Action Item's parameters are not valid: {0}")]
    ActionItemParametersNotValid(String),
}

fn contains_step(definition: &QuestDefinition, step_id: &str) -> bool {
    definition.steps.iter().any(|step| step.id == step_id)
}

fn steps_without_to(definition: &QuestDefinition) -> HashSet<String> {
    let mut connections = HashMap::new();
    for connection in &definition.connections {
        connections.insert(connection.step_from.clone(), connection.step_to.clone());
    }
    definition
        .steps
        .iter()
        .filter(|step| !connections.contains_key(&step.id))
        .map(|step| step.id.clone())
        .collect()
}

fn steps_without_from(definition: &QuestDefinition) -> HashSet<String> {
    let mut connections = HashMap::new();
    for connection in &definition.connections {
        connections.insert(connection.step_to.clone(), connection.step_from.clone());
    }
    definition
        .steps
        .iter()
        .filter(|step| !connections.contains_key(&step.id))
        .map(|step| step.id.clone())
        .collect()
}

pub fn validate_definition(definition: &QuestDefinition) -> Result<(), QuestValidationError> {
    if definition.steps.is_empty() {
        return Err(QuestValidationError::InvalidDefinition);
    }

    for Connection { step_from, step_to } in &definition.connections {
        if !contains_step(definition, step_from) {
            return Err(QuestValidationError::MissingStepDefinition(
                step_from.clone(),
            ));
        }
        if !contains_step(definition, step_to) {
            return Err(QuestValidationError::MissingStepDefinition(step_to.clone()));
        }
    }

    if steps_without_from(definition).is_empty() {
        return Err(QuestValidationError::NoStartingNode);
    }

    if steps_without_to(definition).is_empty() {
        return Err(QuestValidationError::NoEndNode);
    }

    let mut unique_task_ids: HashSet<String> = HashSet::new();
    let mut unique_step_ids: HashSet<String> = HashSet::new();

    for step in &definition.steps {
        if step.tasks.is_empty() {
            return Err(QuestValidationError::MissingTasksForStep(step.id.clone()));
        }

        if !unique_step_ids.insert(step.id.clone()) {
            return Err(QuestValidationError::NotUniqueIDForStep(step.id.clone()));
        }

        if step.description.is_empty() {
            return Err(QuestValidationError::MissingDescriptionForStep(
                step.id.clone(),
            ));
        }

        for task in &step.tasks {
            if !unique_task_ids.insert(task.id.clone()) {
                return Err(QuestValidationError::NotUniqueIDForStepTask(
                    step.id.clone(),
                ));
            }

            if task.description.is_empty() {
                return Err(QuestValidationError::MissingDescriptionForTask(
                    task.id.clone(),
                ));
            }

            for action_item in &task.action_items {
                match action_item.r#type.as_str() {
                    "CUSTOM" => {
                        if action_item.parameters.is_empty() {
                            return Err(QuestValidationError::ActionItemParametersNotValid(
                                "CUSTOM".to_string(),
                            ));
                        }
                    }
                    "LOCATION" => {
                        if !action_item.parameters.contains_key("x")
                            || !action_item.parameters.contains_key("y")
                        {
                            return Err(QuestValidationError::ActionItemParametersNotValid(
                                "LOCATION".to_string(),
                            ));
                        }
                    }
                    "EMOTE" => {
                        if !action_item.parameters.contains_key("x")
                            || !action_item.parameters.contains_key("y")
                            || !action_item.parameters.contains_key("id")
                        {
                            return Err(QuestValidationError::ActionItemParametersNotValid(
                                "EMOTE".to_string(),
                            ));
                        }
                    }
                    "JUMP" => {
                        if !action_item.parameters.contains_key("x")
                            || !action_item.parameters.contains_key("y")
                        {
                            return Err(QuestValidationError::ActionItemParametersNotValid(
                                "JUMP".to_string(),
                            ));
                        }
                    }
                    other => {
                        return Err(QuestValidationError::ActionItemTypeNotValid(
                            other.to_string(),
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}
