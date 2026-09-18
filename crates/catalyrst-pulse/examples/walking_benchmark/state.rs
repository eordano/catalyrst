use catalyrst_pulse::{
    batch::decode_baseline_batch,
    decentraland::pulse as p,
    interest::{ParcelEncoder, ParcelEncoderOptions},
};
use std::{collections::HashMap, time::Instant};

pub fn walking(index: usize, frame: u32) -> p::PlayerState {
    let frame = frame + index as u32 * 7;
    let active_frames = frame / 100 * 80 + (frame % 100).min(80);
    let phase = index as f32 * 0.37 + active_frames as f32 / 10.0 * 0.55;
    let moving = frame % 100 < 80;
    let x = 8.0 + 3.0 * phase.sin();
    let z = 8.0 + 3.0 * phase.cos();
    let mut state = p::PlayerState {
        parcel_index: ParcelEncoder::new(ParcelEncoderOptions::default()).encode(0, 0),
        state_flags: p::PlayerAnimationFlags::Grounded as u32,
        ..Default::default()
    };
    state.set_position_x_f(x);
    state.set_position_y_f(1.0);
    state.set_position_z_f(z);
    state.set_velocity_x_f(if moving { 1.65 * phase.cos() } else { 0.0 });
    state.set_velocity_z_f(if moving { -1.65 * phase.sin() } else { 0.0 });
    state.set_rotation_y_f((90.0 + phase.to_degrees()).rem_euclid(360.0));
    state.set_movement_blend_f(if moving { 1.0 } else { 0.0 });
    state
}

pub struct Subject {
    pub index: usize,
    pub sequence: u32,
    pub state: p::PlayerState,
}
pub struct Receiver {
    pub subjects: HashMap<u32, Subject>,
    wallets: HashMap<String, usize>,
    pub latencies: Vec<f64>,
    pub updates: usize,
    pub batches: usize,
    pub dictionary_batches: usize,
    pub resyncs: usize,
    pub failures: Vec<String>,
}
impl Receiver {
    pub fn new(wallets: HashMap<String, usize>) -> Self {
        Self {
            subjects: HashMap::new(),
            wallets,
            latencies: vec![],
            updates: 0,
            batches: 0,
            dictionary_batches: 0,
            resyncs: 0,
            failures: vec![],
        }
    }
    pub fn handle(
        &mut self,
        message: p::ServerMessage,
        arrived: Instant,
        history: &[Vec<(Instant, p::PlayerState)>],
    ) -> Vec<p::ResyncRequest> {
        let mut resync = vec![];
        match message.message.unwrap() {
            p::server_message::Message::PlayerJoined(join) => {
                let full = join.state.unwrap();
                let index = self.wallets[&join.user_id];
                self.subjects.insert(
                    full.subject_id,
                    Subject {
                        index,
                        sequence: full.sequence,
                        state: full.state.unwrap(),
                    },
                );
            }
            p::server_message::Message::PlayerStateFull(full) => {
                if let Some(s) = self.subjects.get_mut(&full.subject_id) {
                    s.sequence = full.sequence;
                    s.state = full.state.unwrap();
                    self.verify(full.subject_id, arrived, history);
                }
            }
            p::server_message::Message::PlayerStateDelta(delta) => {
                self.delta(delta, arrived, history, &mut resync);
            }
            p::server_message::Message::PlayerStateDeltaBatchBaseline(batch) => {
                self.batches += 1;
                self.dictionary_batches += usize::from(batch.payload[0] & 128 == 0);
                for s in decode_baseline_batch(batch.subject_count, &batch.payload).unwrap() {
                    self.delta(s.to_delta(batch.server_tick), arrived, history, &mut resync);
                }
            }
            p::server_message::Message::PlayerProfileVersionAnnounced(_) => {}
            other => panic!("unexpected gameplay packet {other:?}"),
        }
        resync
    }
    fn delta(
        &mut self,
        d: p::PlayerStateDeltaTier0,
        arrived: Instant,
        history: &[Vec<(Instant, p::PlayerState)>],
        resync: &mut Vec<p::ResyncRequest>,
    ) {
        let Some(s) = self.subjects.get_mut(&d.subject_id) else {
            resync.push(p::ResyncRequest {
                subject_id: d.subject_id,
                known_seq: 0,
            });
            self.resyncs += 1;
            return;
        };
        if d.new_seq == s.sequence || d.new_seq.wrapping_sub(s.sequence) >= 1 << 31 {
            return;
        }
        if d.baseline_seq != s.sequence {
            resync.push(p::ResyncRequest {
                subject_id: d.subject_id,
                known_seq: s.sequence,
            });
            self.resyncs += 1;
            return;
        }
        macro_rules! apply { ($($field:ident),*) => { $(if let Some(value)=d.$field {s.state.$field=value;})* }; }
        apply!(
            parcel_index,
            position_x,
            position_y,
            position_z,
            velocity_x,
            velocity_y,
            velocity_z,
            rotation_y,
            movement_blend,
            slide_blend,
            state_flags,
            glide_state,
            jump_count
        );
        macro_rules! optional { ($($field:ident),*) => { $(if d.$field.is_some() {s.state.$field=d.$field;})* }; }
        optional!(head_yaw, head_pitch, point_at_x, point_at_y, point_at_z);
        s.sequence = d.new_seq;
        self.verify(d.subject_id, arrived, history);
    }
    fn verify(&mut self, id: u32, arrived: Instant, history: &[Vec<(Instant, p::PlayerState)>]) {
        let s = &self.subjects[&id];
        let Some((sent, expected)) = history[s.index].get(s.sequence as usize) else {
            self.failures
                .push(format!("unknown sequence {} for {}", s.sequence, s.index));
            return;
        };
        if s.sequence >= 2 {
            self.updates += 1;
            self.latencies
                .push(arrived.saturating_duration_since(*sent).as_secs_f64() * 1000.0);
            if &s.state != expected {
                self.failures.push(format!(
                    "state mismatch index={} seq={} actual={:?} expected={expected:?}",
                    s.index, s.sequence, s.state
                ));
            }
        }
    }
}
