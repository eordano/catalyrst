use crate::decentraland::pulse::PlayerStateDeltaTier0;
use crate::messages::spec;

pub const SUBJECT_ID_BITS: u32 = 13;
pub const SEQ_DELTA_BITS: u32 = 6;
pub const SEQ_DELTA_ESCAPE: u32 = (1 << SEQ_DELTA_BITS) - 1;
pub const PRESENCE_BITS: u32 = 17;
pub const STATE_FLAGS_BITS: u32 = 16;
const ABSOLUTE_SEQ_BITS: u32 = 32;
pub const SAMPLE_AGE_LIMIT: u32 = 1 << 31;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SeqEncoding {
    Delta,
    #[default]
    Absolute,
    AbsoluteBaseline,
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

fn varint_bits(value: u32) -> u32 {
    (32 - value.leading_zeros()).max(1).div_ceil(7) * 8
}

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

    fn write_varint(&mut self, mut value: u32) {
        while value >= 128 {
            self.write_bits((value & 127) | 128, 8);
            value >>= 7;
        }
        self.write_bits(value, 8);
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

    fn read_varint(&mut self) -> Result<u32, BatchError> {
        let mut value = 0;
        for shift in (0..35).step_by(7) {
            let byte = self.read_bits(8)?;
            if (shift == 28 && byte > 15) || (shift > 0 && byte == 0) {
                return Err(BatchError::UnexpectedEof);
            }
            value |= (byte & 127) << shift;
            if byte < 128 {
                return Ok(value);
            }
        }
        Err(BatchError::UnexpectedEof)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchError {
    UnexpectedEof,
    InvalidData,
}

impl std::fmt::Display for BatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BatchError::UnexpectedEof => write!(f, "batch payload ended mid-field"),
            BatchError::InvalidData => write!(f, "invalid batch data"),
        }
    }
}

impl std::error::Error for BatchError {}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BatchSubject {
    pub subject_id: u32,
    pub baseline_seq: u32,
    pub new_seq: u32,
    pub sample_tick: u32,
    pub state_flags: u32,
    pub state_flags_present: bool,
    pub fields: [Option<u32>; FIELD_COUNT],
}

impl BatchSubject {
    pub fn from_delta(delta: &PlayerStateDeltaTier0, state_flags: u32) -> Self {
        Self {
            subject_id: delta.subject_id,
            baseline_seq: delta.baseline_seq,
            new_seq: delta.new_seq,
            sample_tick: delta.server_tick,
            state_flags,
            state_flags_present: delta.state_flags.is_some(),
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
            state_flags: self.state_flags_present.then_some(self.state_flags),
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

    pub fn is_representable(&self) -> bool {
        self.subject_id < (1 << SUBJECT_ID_BITS)
            && self.state_flags < (1 << STATE_FLAGS_BITS)
            && self.seq_delta() > 0
            && self.seq_delta() < (1 << 31)
            && self
                .fields
                .iter()
                .zip(FIELD_WIDTHS)
                .all(|(value, width)| value.is_none_or(|value| value < (1 << width)))
    }

    fn seq_delta(&self) -> u32 {
        self.new_seq.wrapping_sub(self.baseline_seq)
    }

    fn sample_age(&self, server_tick: u32, sample_tick: bool) -> Option<u32> {
        sample_tick.then(|| {
            let age = server_tick.wrapping_sub(self.sample_tick);
            if age < SAMPLE_AGE_LIMIT {
                age
            } else {
                0
            }
        })
    }

    pub fn bit_len(&self, mode: SeqEncoding) -> u64 {
        self.bit_len_with(mode, None)
    }

    fn bit_len_with(&self, mode: SeqEncoding, age: Option<u32>) -> u64 {
        let flags = if mode == SeqEncoding::AbsoluteBaseline {
            1 + if self.state_flags_present {
                STATE_FLAGS_BITS
            } else {
                0
            }
        } else {
            STATE_FLAGS_BITS
        };
        let mut n = (SUBJECT_ID_BITS + PRESENCE_BITS + flags) as u64;
        n += match mode {
            SeqEncoding::Delta => {
                if self.seq_delta() >= SEQ_DELTA_ESCAPE {
                    (SEQ_DELTA_BITS + ABSOLUTE_SEQ_BITS) as u64
                } else {
                    SEQ_DELTA_BITS as u64
                }
            }
            SeqEncoding::Absolute => ABSOLUTE_SEQ_BITS as u64,
            SeqEncoding::AbsoluteBaseline => {
                (varint_bits(self.new_seq)
                    + SEQ_DELTA_BITS
                    + if self.seq_delta() >= SEQ_DELTA_ESCAPE {
                        varint_bits(self.baseline_seq)
                    } else {
                        0
                    }
                    + age.map_or(0, varint_bits)) as u64
            }
        };
        for (i, field) in self.fields.iter().enumerate() {
            if field.is_some() {
                n += FIELD_WIDTHS[i] as u64;
            }
        }
        n
    }

    fn encode_into(&self, w: &mut BitWriter, mode: SeqEncoding, age: Option<u32>) {
        self.encode_with_mask(w, mode, None, false, age);
    }

    fn presence_mask(&self) -> u32 {
        self.fields.iter().enumerate().fold(0, |mask, (i, field)| {
            mask | (u32::from(field.is_some()) << (PRESENCE_BITS - 1 - i as u32))
        })
    }

    fn encode_with_mask(
        &self,
        w: &mut BitWriter,
        mode: SeqEncoding,
        mask_index: Option<(u32, u32)>,
        unit_gap: bool,
        age: Option<u32>,
    ) {
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
            SeqEncoding::AbsoluteBaseline => {
                w.write_varint(self.new_seq);
                let distance = self.seq_delta();
                if !unit_gap {
                    w.write_bits(distance.min(SEQ_DELTA_ESCAPE), SEQ_DELTA_BITS);
                }
                if distance >= SEQ_DELTA_ESCAPE {
                    w.write_varint(self.baseline_seq);
                }
                if let Some(age) = age {
                    w.write_varint(age);
                }
            }
        }
        let (mask, width) = mask_index.unwrap_or((self.presence_mask(), PRESENCE_BITS));
        w.write_bits(mask, width);
        if mode == SeqEncoding::AbsoluteBaseline {
            w.write_bits(u32::from(self.state_flags_present), 1);
        }
        if mode != SeqEncoding::AbsoluteBaseline || self.state_flags_present {
            w.write_bits(self.state_flags, STATE_FLAGS_BITS);
        }
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
        Self::decode_with_masks(r, mode, last_known_seq, &[], false, 0, false)
    }

    fn decode_with_masks(
        r: &mut BitReader<'_>,
        mode: SeqEncoding,
        last_known_seq: &mut impl FnMut(u32) -> u32,
        masks: &[u32],
        unit_gap: bool,
        server_tick: u32,
        sample_tick: bool,
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
            SeqEncoding::AbsoluteBaseline => {
                let new_seq = r.read_varint()?;
                let distance = if unit_gap {
                    1
                } else {
                    r.read_bits(SEQ_DELTA_BITS)?
                };
                let baseline = if distance == SEQ_DELTA_ESCAPE {
                    r.read_varint()?
                } else {
                    new_seq.wrapping_sub(distance)
                };
                (baseline, new_seq)
            }
        };
        let sample_tick = if sample_tick {
            let age = r.read_varint()?;
            if age >= SAMPLE_AGE_LIMIT {
                return Err(BatchError::InvalidData);
            }
            server_tick.wrapping_sub(age)
        } else {
            server_tick
        };
        let mask = if masks.is_empty() {
            r.read_bits(PRESENCE_BITS)?
        } else {
            *masks
                .get(r.read_bits(mask_index_bits(masks.len()))? as usize)
                .ok_or(BatchError::InvalidData)?
        };
        let state_flags_present = mode != SeqEncoding::AbsoluteBaseline || r.read_bits(1)? != 0;
        let state_flags = if state_flags_present {
            r.read_bits(STATE_FLAGS_BITS)?
        } else {
            0
        };
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
            sample_tick,
            state_flags,
            state_flags_present,
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
    w.write_bits(
        u32::from(mode != SeqEncoding::Delta),
        SEQ_ENCODING_HEADER_BITS,
    );
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
    encode_batches_with(server_tick, subjects, max_bytes, mode, false)
}

pub fn encode_batches_with(
    server_tick: u32,
    subjects: &[BatchSubject],
    max_bytes: usize,
    mode: SeqEncoding,
    sample_tick: bool,
) -> Vec<EncodedBatch> {
    let sample_tick = sample_tick && mode == SeqEncoding::AbsoluteBaseline;
    let mut out = Vec::new();
    let mut writer = open_batch(mode);
    let mut count = 0u32;

    for s in subjects {
        let age = s.sample_age(server_tick, sample_tick);
        let projected_bits = writer.bit_len() + s.bit_len_with(mode, age);
        let projected_bytes = projected_bits.div_ceil(8) as usize;
        if count > 0 && projected_bytes > max_bytes {
            out.push(EncodedBatch {
                server_tick,
                subject_count: count,
                payload: std::mem::replace(&mut writer, open_batch(mode)).into_bytes(),
            });
            count = 0;
        }
        s.encode_into(&mut writer, mode, age);
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

pub fn decode_baseline_batch(
    subject_count: u32,
    payload: &[u8],
) -> Result<Vec<BatchSubject>, BatchError> {
    decode_baseline_batch_with(0, subject_count, payload, false)
}

pub fn decode_baseline_batch_with(
    server_tick: u32,
    subject_count: u32,
    payload: &[u8],
    sample_tick: bool,
) -> Result<Vec<BatchSubject>, BatchError> {
    let min_subject_bits = if sample_tick { 30 } else { 22 };
    if payload.is_empty()
        || payload.len() > 1200
        || subject_count == 0
        || subject_count as usize > (payload.len() * 8 - 1) / min_subject_bits
    {
        return Err(BatchError::InvalidData);
    }
    let mut reader = BitReader::new(payload);
    let mut masks = Vec::new();
    let mut unit_gap = false;
    if reader.read_bits(1)? == 0 {
        unit_gap = reader.read_bits(1)? != 0;
        let count = reader.read_bits(3)? + 1;
        for _ in 0..count {
            let mask = reader.read_bits(PRESENCE_BITS)?;
            if masks.contains(&mask) {
                return Err(BatchError::InvalidData);
            }
            masks.push(mask);
        }
    }
    let mut out: Vec<BatchSubject> = Vec::with_capacity(subject_count as usize);
    for _ in 0..subject_count {
        let subject = BatchSubject::decode_with_masks(
            &mut reader,
            SeqEncoding::AbsoluteBaseline,
            &mut |_| unreachable!(),
            &masks,
            unit_gap,
            server_tick,
            sample_tick,
        )?;
        if !subject.is_representable() || out.iter().any(|s| s.subject_id == subject.subject_id) {
            return Err(BatchError::InvalidData);
        }
        out.push(subject);
    }
    let remaining = reader.bits_remaining();
    if remaining > 7 || reader.read_bits(remaining as u32)? != 0 {
        return Err(BatchError::InvalidData);
    }
    Ok(out)
}

fn mask_index_bits(count: usize) -> u32 {
    usize::BITS - (count - 1).leading_zeros()
}

pub fn encode_dictionary_batches(
    server_tick: u32,
    subjects: &[BatchSubject],
    max_bytes: usize,
) -> Vec<EncodedBatch> {
    encode_dictionary_batches_with(server_tick, subjects, max_bytes, false)
}

pub fn encode_dictionary_batches_with(
    server_tick: u32,
    subjects: &[BatchSubject],
    max_bytes: usize,
    sample_tick: bool,
) -> Vec<EncodedBatch> {
    let mut masks = Vec::new();
    for subject in subjects {
        let mask = subject.presence_mask();
        if !masks.contains(&mask) {
            masks.push(mask);
        }
        if masks.len() > 8 {
            return encode_batches_with(
                server_tick,
                subjects,
                max_bytes,
                SeqEncoding::AbsoluteBaseline,
                sample_tick,
            );
        }
    }
    if subjects.is_empty() {
        return Vec::new();
    }
    let index_bits = mask_index_bits(masks.len());
    let unit_gap = subjects.iter().all(|s| s.seq_delta() == 1);
    let open = || {
        let mut writer = BitWriter::new();
        writer.write_bits(0, 1);
        writer.write_bits(u32::from(unit_gap), 1);
        writer.write_bits((masks.len() - 1) as u32, 3);
        for mask in &masks {
            writer.write_bits(*mask, PRESENCE_BITS);
        }
        writer
    };
    let mut writer = open();
    let mut count = 0;
    let mut start = 0;
    let mut out = Vec::new();
    for (index, subject) in subjects.iter().enumerate() {
        let age = subject.sample_age(server_tick, sample_tick);
        let bits = subject.bit_len_with(SeqEncoding::AbsoluteBaseline, age) - PRESENCE_BITS as u64
            + index_bits as u64
            - if unit_gap { SEQ_DELTA_BITS as u64 } else { 0 };
        if count > 0 && (writer.bit_len() + bits).div_ceil(8) as usize > max_bytes {
            finish_dictionary_chunk(
                &mut out,
                server_tick,
                &subjects[start..index],
                writer,
                count,
                max_bytes,
                sample_tick,
            );
            writer = open();
            count = 0;
            start = index;
        }
        let mask = masks
            .iter()
            .position(|mask| *mask == subject.presence_mask())
            .unwrap() as u32;
        subject.encode_with_mask(
            &mut writer,
            SeqEncoding::AbsoluteBaseline,
            Some((mask, index_bits)),
            unit_gap,
            age,
        );
        count += 1;
    }
    finish_dictionary_chunk(
        &mut out,
        server_tick,
        &subjects[start..],
        writer,
        count,
        max_bytes,
        sample_tick,
    );
    out
}

fn finish_dictionary_chunk(
    out: &mut Vec<EncodedBatch>,
    server_tick: u32,
    subjects: &[BatchSubject],
    writer: BitWriter,
    count: u32,
    max_bytes: usize,
    sample_tick: bool,
) {
    let plain_bits = 1 + subjects
        .iter()
        .map(|subject| {
            subject.bit_len_with(
                SeqEncoding::AbsoluteBaseline,
                subject.sample_age(server_tick, sample_tick),
            )
        })
        .sum::<u64>();
    if plain_bits.div_ceil(8) <= writer.bit_len().div_ceil(8)
        || writer.bit_len().div_ceil(8) as usize > max_bytes
    {
        out.extend(encode_batches_with(
            server_tick,
            subjects,
            max_bytes,
            SeqEncoding::AbsoluteBaseline,
            sample_tick,
        ));
    } else {
        out.push(EncodedBatch {
            server_tick,
            subject_count: count,
            payload: writer.into_bytes(),
        });
    }
}

#[cfg(test)]
mod tests;
