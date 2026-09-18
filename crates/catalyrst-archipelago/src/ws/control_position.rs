use crate::control_v4::V4Owner;
use crate::state::AppState;
use catalyrst_types::control_position::ControlPosition;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn publish(
    state: &AppState,
    owner: &V4Owner,
    realm: &str,
    position: Option<[f32; 3]>,
) -> bool {
    let close = position.is_none();
    let Some(permit) =
        state
            .registry
            .control_position_permit(&owner.address, &owner.session, owner.epoch, close)
    else {
        return false;
    };
    let Ok(previous) = SEQUENCE.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        value.checked_add(1)
    }) else {
        return false;
    };
    let record = ControlPosition {
        audience: state.cfg.server.control_v4_audience.clone(),
        wallet: owner.address.clone(),
        session: owner.session.clone(),
        epoch: owner.epoch,
        sequence: previous + 1,
        realm: realm.to_owned(),
        position,
    };
    crate::nats::ControlPositionAnnouncement::new(record, permit)
        .is_some_and(|announcement| state.publisher.publish_control_position(announcement))
}
