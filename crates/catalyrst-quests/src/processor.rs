//! Event processor — faithful port of `decentraland/quests`
//! crates/system/src/event_processing.rs. Drains the in-process events queue,
//! tests each event against every active instance of the event's user, persists
//! matched events, fires reward hooks + completion, and publishes the resulting
//! `QuestStateUpdate` / `EventIgnored` `UserUpdate`s.

use crate::context::Context;
use crate::proto::{
    user_update, Event, ProtocolMessage, Quest, QuestState, QuestStateUpdate, UserUpdate,
};
use crate::quests::give_rewards_to_user;
use crate::state::{apply_event, get_state, hide_state_actions, is_completed, QuestGraph};
use tokio::sync::mpsc::UnboundedReceiver;

/// Spawn the processing loop. Mirrors `start_event_processing`: an infinite
/// loop popping events and processing each.
pub fn spawn_event_processor(ctx: Context, mut rx: UnboundedReceiver<Event>) {
    tokio::spawn(async move {
        tracing::info!("quests event processor listening");
        while let Some(event) = rx.recv().await {
            process_event(&ctx, event).await;
        }
    });
}

/// `process_event` + `do_process_event`.
async fn process_event(ctx: &Context, event: Event) {
    let user_address = event.address.clone();
    let instances = match ctx
        .db()
        .get_active_user_quest_instances(&user_address)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "processing event > couldn't load active instances");
            // Upstream re-pushes on failure so the event isn't lost.
            ctx.push_event(event);
            return;
        }
    };

    let mut applied = 0usize;
    for instance in instances {
        // Recompute the instance's current state from its event log.
        let quest = match ctx
            .db()
            .get_quest_with_decoded_definition(&instance.quest_id)
            .await
        {
            Ok(q) => q,
            Err(e) => {
                tracing::error!(error = %e, instance = %instance.id, "processing event > load quest failed");
                continue;
            }
        };
        let stored_events = match ctx.db().get_events(&instance.id).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, instance = %instance.id, "processing event > load events failed");
                continue;
            }
        };
        let decoded: Vec<Event> = stored_events
            .iter()
            .filter_map(|e| Event::decode(e.event.as_slice()).ok())
            .collect();
        let quest_state = fold_state(&quest, &decoded);

        if is_completed(&quest_state) {
            continue;
        }

        let graph = QuestGraph::from(&quest);
        let new_state = apply_event(&quest_state, &graph, &event);
        if state_changed(&quest_state, &new_state)
            && add_event_and_notify(ctx, &event, &quest.id, &instance.id, new_state).await
        {
            applied += 1;
        }
    }

    if applied == 0 {
        ctx.pubsub().publish(UserUpdate {
            user_address: user_address.clone(),
            message: Some(user_update::Message::EventIgnored(event.id.clone())),
        });
        tracing::info!("processing event > event was ignored");
    }
}

/// `add_event_and_notify`: persist the event, on completion fire rewards +
/// record completion, then publish the (hidden-actions) QuestStateUpdate.
async fn add_event_and_notify(
    ctx: &Context,
    event: &Event,
    quest_id: &str,
    instance_id: &str,
    mut quest_state: QuestState,
) -> bool {
    if let Err(e) = ctx
        .db()
        .add_event(
            &event.id,
            &event.address,
            &event.encode_to_vec(),
            instance_id,
        )
        .await
    {
        tracing::error!(error = %e, instance = %instance_id, "processing event > add_event failed");
        return false;
    }

    if is_completed(&quest_state) {
        give_rewards_to_user(ctx.db(), quest_id, &event.address).await;
        if let Err(e) = ctx.db().complete_quest_instance(instance_id).await {
            tracing::error!(error = %e, instance = %instance_id, "processing event > record completion failed");
        }
    }

    hide_state_actions(&mut quest_state);
    ctx.pubsub().publish(UserUpdate {
        message: Some(user_update::Message::QuestStateUpdate(QuestStateUpdate {
            instance_id: instance_id.to_string(),
            quest_state: Some(quest_state),
            event_id: event.id.clone(),
        })),
        user_address: event.address.clone(),
    });
    true
}

fn fold_state(quest: &Quest, events: &[Event]) -> QuestState {
    get_state(quest, events)
}

/// Structural state inequality (the prost types don't derive PartialEq, so
/// compare on the protobuf-encoded bytes — exactly what differs after an event).
fn state_changed(a: &QuestState, b: &QuestState) -> bool {
    a.encode_to_vec() != b.encode_to_vec()
}
