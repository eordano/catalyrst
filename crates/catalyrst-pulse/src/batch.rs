use crate::decentraland::pulse::PlayerStateDeltaTier0;
use crate::messages::spec;

pub const SUBJECT_ID_BITS: u32 = 13;
pub const SEQ_DELTA_BITS: u32 = 6;
pub const SEQ_DELTA_ESCAPE: u32 = (1 << SEQ_DELTA_BITS) - 1;
pub const PRESENCE_BITS: u32 = 17;
pub const STATE_FLAGS_BITS: u32 = 16;
const ABSOLUTE_SEQ_BITS: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SeqEncoding {
    Delta,
    #[default]
    Absolute,
}

pub const SEQ_ENCODING_HEADER_BITS: u32 = 1;

impl SeqEncoding {
    fn from_bit(bit: u32) -> Self {
        if bit == 0 {
            SeqEncoding::Delta
        } else {
            SeqEncoding::Absolute
        }
    }
}

pub const WRAP_OVERHEAD: usize = 15;

pub const MAX_BATCH_BYTES: usize =
    crate::transport::webtransport::config::DEFAULT_MAX_DATAGRAM_BYTES - WRAP_OVERHEAD;

const _: () = assert!(crate::transport::webtransport::config::DEFAULT_MAX_DATAGRAM_BYTES <= 16383);

const GLIDE_STATE_BITS: u32 = 2;
const JUMP_COUNT_BITS: u32 = 16;
const PARCEL_INDEX_BITS: u32 = 17;

pub const FIELD_COUNT: usize = 17;

const FIELD_WIDTHS: [u32; FIELD_COUNT] = [
    PARCEL_INDEX_BITS,
    spec::POSITION_X.bits,
    spec::POSITION_Y.bits,
    spec::POSITION_Z.bits,
    spec::VELOCITY_X.bits,
    spec::VELOCITY_Y.bits,
    spec::VELOCITY_Z.bits,
    spec::ROTATION_Y.bits,
    spec::MOVEMENT_BLEND.bits,
    spec::SLIDE_BLEND.bits,
    spec::HEAD_YAW.bits,
    spec::HEAD_PITCH.bits,
    GLIDE_STATE_BITS,
    JUMP_COUNT_BITS,
    spec::POINT_AT_X.bits,
    spec::POINT_AT_Y.bits,
    spec::POINT_AT_Z.bits,
];

const _: () = assert!(FIELD_COUNT == PRESENCE_BITS as usize);

#[derive(Debug, Default)]
pub struct BitWriter {
    bytes: Vec<u8>,
    nbits: u64,
}

impl BitWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bit_len(&self) -> u64 {
        self.nbits
    }

    pub fn write_bits(&mut self, value: u32, width: u32) {
        for shift in (0..width).rev() {
            let bit = ((value >> shift) & 1) as u8;
            let byte_idx = (self.nbits / 8) as usize;
            let bit_in_byte = 7 - (self.nbits % 8) as u32;
            if byte_idx == self.bytes.len() {
                self.bytes.push(0);
            }
            if bit == 1 {
                self.bytes[byte_idx] |= 1 << bit_in_byte;
            }
            self.nbits += 1;
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

pub struct BitReader<'a> {
    bytes: &'a [u8],
    nbits: u64,
    total: u64,
}

impl<'a> BitReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            nbits: 0,
            total: (bytes.len() as u64) * 8,
        }
    }

    pub fn read_bits(&mut self, width: u32) -> Result<u32, BatchError> {
        if self.nbits + width as u64 > self.total {
            return Err(BatchError::UnexpectedEof);
        }
        let mut value = 0u32;
        for _ in 0..width {
            let byte_idx = (self.nbits / 8) as usize;
            let bit_in_byte = 7 - (self.nbits % 8) as u32;
            let bit = (self.bytes[byte_idx] >> bit_in_byte) & 1;
            value = (value << 1) | bit as u32;
            self.nbits += 1;
        }
        Ok(value)
    }

    pub fn bits_remaining(&self) -> u64 {
        self.total - self.nbits
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchError {
    UnexpectedEof,
}

impl std::fmt::Display for BatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BatchError::UnexpectedEof => write!(f, "batch payload ended mid-field"),
        }
    }
}

impl std::error::Error for BatchError {}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BatchSubject {
    pub subject_id: u32,
    pub baseline_seq: u32,
    pub new_seq: u32,
    pub state_flags: u32,
    pub fields: [Option<u32>; FIELD_COUNT],
}

impl BatchSubject {
    pub fn from_delta(delta: &PlayerStateDeltaTier0, state_flags: u32) -> Self {
        Self {
            subject_id: delta.subject_id,
            baseline_seq: delta.baseline_seq,
            new_seq: delta.new_seq,
            state_flags,
            fields: [
                delta.parcel_index.map(|v| v as u32),
                delta.position_x,
                delta.position_y,
                delta.position_z,
                delta.velocity_x,
                delta.velocity_y,
                delta.velocity_z,
                delta.rotation_y,
                delta.movement_blend,
                delta.slide_blend,
                delta.head_yaw,
                delta.head_pitch,
                delta.glide_state.map(|v| v as u32),
                delta.jump_count.map(|v| v as u32),
                delta.point_at_x,
                delta.point_at_y,
                delta.point_at_z,
            ],
        }
    }

    pub fn to_delta(&self, server_tick: u32) -> PlayerStateDeltaTier0 {
        PlayerStateDeltaTier0 {
            subject_id: self.subject_id,
            baseline_seq: self.baseline_seq,
            new_seq: self.new_seq,
            server_tick,
            parcel_index: self.fields[0].map(|v| v as i32),
            position_x: self.fields[1],
            position_y: self.fields[2],
            position_z: self.fields[3],
            velocity_x: self.fields[4],
            velocity_y: self.fields[5],
            velocity_z: self.fields[6],
            rotation_y: self.fields[7],
            movement_blend: self.fields[8],
            slide_blend: self.fields[9],
            head_yaw: self.fields[10],
            head_pitch: self.fields[11],
            state_flags: Some(self.state_flags),
            glide_state: self.fields[12].map(|v| v as i32),
            jump_count: self.fields[13].map(|v| v as i32),
            point_at_x: self.fields[14],
            point_at_y: self.fields[15],
            point_at_z: self.fields[16],
        }
    }

    pub fn present_field_count(&self) -> usize {
        self.fields.iter().filter(|f| f.is_some()).count()
    }

    fn seq_delta(&self) -> u32 {
        self.new_seq.wrapping_sub(self.baseline_seq)
    }

    pub fn bit_len(&self, mode: SeqEncoding) -> u64 {
        let mut n = (SUBJECT_ID_BITS + PRESENCE_BITS + STATE_FLAGS_BITS) as u64;
        n += match mode {
            SeqEncoding::Delta => {
                if self.seq_delta() >= SEQ_DELTA_ESCAPE {
                    (SEQ_DELTA_BITS + ABSOLUTE_SEQ_BITS) as u64
                } else {
                    SEQ_DELTA_BITS as u64
                }
            }
            SeqEncoding::Absolute => ABSOLUTE_SEQ_BITS as u64,
        };
        for (i, field) in self.fields.iter().enumerate() {
            if field.is_some() {
                n += FIELD_WIDTHS[i] as u64;
            }
        }
        n
    }

    fn encode_into(&self, w: &mut BitWriter, mode: SeqEncoding) {
        w.write_bits(self.subject_id, SUBJECT_ID_BITS);
        match mode {
            SeqEncoding::Delta => {
                let seq_delta = self.seq_delta();
                if seq_delta >= SEQ_DELTA_ESCAPE {
                    w.write_bits(SEQ_DELTA_ESCAPE, SEQ_DELTA_BITS);
                    w.write_bits(self.new_seq, ABSOLUTE_SEQ_BITS);
                } else {
                    w.write_bits(seq_delta, SEQ_DELTA_BITS);
                }
            }
            SeqEncoding::Absolute => w.write_bits(self.new_seq, ABSOLUTE_SEQ_BITS),
        }
        let mut mask = 0u32;
        for (i, field) in self.fields.iter().enumerate() {
            if field.is_some() {
                mask |= 1 << (PRESENCE_BITS - 1 - i as u32);
            }
        }
        w.write_bits(mask, PRESENCE_BITS);
        w.write_bits(self.state_flags, STATE_FLAGS_BITS);
        for (i, field) in self.fields.iter().enumerate() {
            if let Some(v) = field {
                w.write_bits(*v, FIELD_WIDTHS[i]);
            }
        }
    }

    fn decode_from(
        r: &mut BitReader<'_>,
        mode: SeqEncoding,
        last_known_seq: &mut impl FnMut(u32) -> u32,
    ) -> Result<Self, BatchError> {
        let subject_id = r.read_bits(SUBJECT_ID_BITS)?;
        let (baseline_seq, new_seq) = match mode {
            SeqEncoding::Delta => {
                let seq_delta = r.read_bits(SEQ_DELTA_BITS)?;
                let baseline_seq = last_known_seq(subject_id);
                let new_seq = if seq_delta == SEQ_DELTA_ESCAPE {
                    r.read_bits(ABSOLUTE_SEQ_BITS)?
                } else {
                    baseline_seq.wrapping_add(seq_delta)
                };
                (baseline_seq, new_seq)
            }
            SeqEncoding::Absolute => {
                let new_seq = r.read_bits(ABSOLUTE_SEQ_BITS)?;
                (new_seq, new_seq)
            }
        };
        let mask = r.read_bits(PRESENCE_BITS)?;
        let state_flags = r.read_bits(STATE_FLAGS_BITS)?;
        let mut fields = [None; FIELD_COUNT];
        for (i, field) in fields.iter_mut().enumerate() {
            if mask & (1 << (PRESENCE_BITS - 1 - i as u32)) != 0 {
                *field = Some(r.read_bits(FIELD_WIDTHS[i])?);
            }
        }
        Ok(Self {
            subject_id,
            baseline_seq,
            new_seq,
            state_flags,
            fields,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedBatch {
    pub server_tick: u32,
    pub subject_count: u32,
    pub payload: Vec<u8>,
}

fn open_batch(mode: SeqEncoding) -> BitWriter {
    let mut w = BitWriter::new();
    w.write_bits(mode as u32, SEQ_ENCODING_HEADER_BITS);
    w
}

pub fn peek_seq_encoding(payload: &[u8]) -> Result<SeqEncoding, BatchError> {
    let mut r = BitReader::new(payload);
    Ok(SeqEncoding::from_bit(
        r.read_bits(SEQ_ENCODING_HEADER_BITS)?,
    ))
}

pub fn encode_batches(
    server_tick: u32,
    subjects: &[BatchSubject],
    max_bytes: usize,
    mode: SeqEncoding,
) -> Vec<EncodedBatch> {
    let mut out = Vec::new();
    let mut writer = open_batch(mode);
    let mut count = 0u32;

    for s in subjects {
        let projected_bits = writer.bit_len() + s.bit_len(mode);
        let projected_bytes = projected_bits.div_ceil(8) as usize;
        if count > 0 && projected_bytes > max_bytes {
            out.push(EncodedBatch {
                server_tick,
                subject_count: count,
                payload: std::mem::replace(&mut writer, open_batch(mode)).into_bytes(),
            });
            count = 0;
        }
        s.encode_into(&mut writer, mode);
        count += 1;
    }
    if count > 0 {
        out.push(EncodedBatch {
            server_tick,
            subject_count: count,
            payload: writer.into_bytes(),
        });
    }
    out
}

pub fn decode_batch(
    subject_count: u32,
    payload: &[u8],
    mut last_known_seq: impl FnMut(u32) -> u32,
) -> Result<Vec<BatchSubject>, BatchError> {
    let mut reader = BitReader::new(payload);
    let mode = SeqEncoding::from_bit(reader.read_bits(SEQ_ENCODING_HEADER_BITS)?);
    let mut out = Vec::with_capacity(subject_count as usize);
    for _ in 0..subject_count {
        out.push(BatchSubject::decode_from(
            &mut reader,
            mode,
            &mut last_known_seq,
        )?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
