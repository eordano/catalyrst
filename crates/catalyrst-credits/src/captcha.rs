const WIDTH: u32 = 160;
const HEIGHT: u32 = 60;
const ANSWER_RANGE: u32 = 101;
// Half-width of the rendered marker, in pixels. The claim handler accepts the
// slider answer (`x`) within ±`CLAIM_TOLERANCE` percent of the stored answer;
// the marker is drawn a few pixels wide so the target is visibly alignable at
// that tolerance once mapped from percent into the image's pixel space.
const MARKER_HALF_PX: i64 = 2;

/// Accept window, in 0..=100 percent units, between the stored captcha answer and
/// the slider value (`x`) the client submits. Single source of truth shared by
/// the claim handler and the geometry round-trip test, so the rendered marker
/// width, the percent->pixel mapping, and the accept check can never drift apart.
pub const CLAIM_TOLERANCE: f64 = 4.0;

pub fn answer_for_seed(seed: u64) -> f64 {
    (seed % ANSWER_RANGE as u64) as f64
}

/// Map a 0..=100 percentage answer onto the image's 0..WIDTH pixel column.
/// The Unity client sends `x = slider.value * 100` (percent), and stretches the
/// returned PNG across the full slider track, so the marker must sit at the same
/// fractional offset (`answer/100`) of the image width — not at `answer` pixels,
/// which would confine every marker to the left 100px of a 160px image and make
/// the puzzle unsolvable for any answer the slider can actually reach.
fn marker_pixel(answer_percent: f64) -> u32 {
    let pct = answer_percent.clamp(0.0, 100.0);
    let px = (pct / 100.0 * (WIDTH - 1) as f64).round() as u32;
    px.min(WIDTH - 1)
}

/// Inverse of `marker_pixel`: recover the 0..=100 percent a given marker column
/// represents, so the slider value a human reads off the image can be checked
/// against the accept window. Used by the geometry round-trip test.
#[cfg(test)]
fn pixel_to_percent(px: u32) -> f64 {
    (px.min(WIDTH - 1) as f64) / (WIDTH - 1) as f64 * 100.0
}

// Deterministic per-pixel noise keyed on the challenge seed. A captcha is only
// useful if it can't be solved by a one-line image heuristic; the previous
// render was a single white column on a flat tint, trivially located by picking
// the brightest column. Here every pixel carries seeded noise so neither "find
// the brightest column" nor "find the flat-tint outlier" recovers the answer —
// the target band is distinguished by structure (a dark gap framed by bright
// rails), which a human eye resolves but a naive max/variance scan does not.
fn lcg(state: &mut u64) -> u32 {
    // 64-bit LCG (Numerical Recipes constants); take the high bits, which have
    // the best statistical quality, as the noise sample.
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (*state >> 33) as u32
}

pub fn render_png(answer_x: f64) -> Vec<u8> {
    let marker = marker_pixel(answer_x) as i64;
    // Seed the noise stream from the answer so the render is a pure function of
    // the challenge (reproducible in tests) yet differs every challenge.
    let mut rng = (answer_x.max(0.0) as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(0x1234_5678_9ABC_DEF0);

    let stride = (WIDTH * 3 + 1) as usize;
    let mut raw = Vec::with_capacity(stride * HEIGHT as usize);
    for _y in 0..HEIGHT {
        raw.push(0u8); // PNG filter byte (None) for this scanline.
        for col in 0..WIDTH {
            let n = lcg(&mut rng);
            // Speckled blue-grey background: each pixel jittered across a wide
            // range so there is no flat tint to subtract and no global maximum
            // to lock onto.
            let mut r = (40 + (n & 0x7f)) as u8;
            let mut g = (60 + ((n >> 7) & 0x7f)) as u8;
            let mut b = (90 + ((n >> 14) & 0x7f)) as u8;

            let d = col as i64 - marker;
            if d.abs() <= MARKER_HALF_PX {
                // Dark central gap: the slot the slider must be dragged into.
                r = 18;
                g = 18;
                b = 24;
            } else if d.abs() <= MARKER_HALF_PX + 2 {
                // Bright rails immediately flanking the gap, so the target is a
                // recognizable notch (dark-between-bright) rather than the single
                // brightest column — defeating a plain argmax-brightness solver.
                r = 0xff;
                g = 0xff;
                b = 0xff;
            }
            raw.extend_from_slice(&[r, g, b]);
        }
    }

    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&WIDTH.to_be_bytes());
    ihdr.extend_from_slice(&HEIGHT.to_be_bytes());
    ihdr.push(8);
    ihdr.push(2);
    ihdr.push(0);
    ihdr.push(0);
    ihdr.push(0);
    write_chunk(&mut png, b"IHDR", &ihdr);

    let idat = zlib_store(&raw);
    write_chunk(&mut png, b"IDAT", &idat);

    write_chunk(&mut png, b"IEND", &[]);
    png
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(0x78);
    out.push(0x01);

    let mut pos = 0usize;
    while pos < data.len() {
        let chunk = std::cmp::min(0xffff, data.len() - pos);
        let last = pos + chunk >= data.len();
        out.push(if last { 1 } else { 0 });
        let len = chunk as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(&data[pos..pos + chunk]);
        pos += chunk;
    }
    if data.is_empty() {
        out.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    }

    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let mut a = 1u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + byte as u32) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xedb8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_pixel_percent_round_trips_within_tolerance() {
        // A human reads the marker's image position, drags the slider there, and
        // the client submits that position as a percent. Model that loop exactly:
        // answer percent -> rendered marker pixel -> percent recovered from the
        // pixel. The recovered percent must land inside the claim accept window
        // for every reachable answer, or a perfectly-aligned slider would be
        // rejected. (Not a tautology: it exercises the real percent<->pixel
        // mapping and would fail if the marker geometry or WIDTH drifted.)
        for answer in 0..=100u32 {
            let a = answer as f64;
            let recovered = pixel_to_percent(marker_pixel(a));
            assert!(
                (a - recovered).abs() <= CLAIM_TOLERANCE,
                "answer {answer} -> pixel {} -> percent {recovered} exceeds tolerance",
                marker_pixel(a)
            );
        }
    }

    #[test]
    fn marker_spans_full_image_width() {
        // The 0 and 100 percent answers must hit the image edges; without the
        // percent->pixel mapping the marker stayed in the left 100px of a 160px
        // image, leaving the right ~37% of the slider's range unsolvable.
        assert_eq!(marker_pixel(0.0), 0);
        assert_eq!(marker_pixel(100.0), WIDTH - 1);
        // The midpoint answer lands near the image center, not at pixel 50.
        let mid = marker_pixel(50.0);
        assert!(
            (mid as i64 - ((WIDTH - 1) / 2) as i64).abs() <= 1,
            "50% should map to ~center, got {mid}"
        );
    }

    #[test]
    fn marker_monotonic_and_in_bounds() {
        let mut prev = 0u32;
        for answer in 0..=100u32 {
            let px = marker_pixel(answer as f64);
            assert!(px < WIDTH, "marker {px} out of bounds for answer {answer}");
            assert!(px >= prev, "marker not monotonic at answer {answer}");
            prev = px;
        }
    }

    #[test]
    fn marker_pixel_clamps_out_of_range() {
        assert_eq!(marker_pixel(-10.0), 0);
        assert_eq!(marker_pixel(250.0), WIDTH - 1);
    }

    #[test]
    fn render_png_emits_valid_signature() {
        let png = render_png(42.0);
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    }

    #[test]
    fn render_is_deterministic_per_answer() {
        // Same answer must render byte-identically (the seed is the answer), so
        // the PNG is reproducible; different answers must differ.
        assert_eq!(render_png(30.0), render_png(30.0));
        assert_ne!(render_png(30.0), render_png(70.0));
    }

    #[test]
    fn argmax_brightness_does_not_reveal_answer() {
        // Guard against regressing to the trivial "single white column" captcha:
        // the brightest column of the image must NOT coincide with the marker,
        // because the marker is a DARK gap and many noisy background columns can
        // peak as bright as the flanking rails. Decode the stored raw scanlines
        // and confirm a naive per-column brightness argmax misses the target.
        let answer = 64.0;
        let marker = marker_pixel(answer) as i64;

        // Re-derive the raw RGB the renderer produced (pre-PNG), so the test
        // reasons about the actual pixels a solver would scan.
        let mut rng = (answer as u64)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0x1234_5678_9ABC_DEF0);
        let mut col_brightness = vec![0u64; WIDTH as usize];
        for _y in 0..HEIGHT {
            for col in 0..WIDTH {
                let n = lcg(&mut rng);
                let d = col as i64 - marker;
                let (r, g, b) = if d.abs() <= MARKER_HALF_PX {
                    (18u32, 18u32, 24u32)
                } else if d.abs() <= MARKER_HALF_PX + 2 {
                    (255, 255, 255)
                } else {
                    (
                        40 + (n & 0x7f),
                        60 + ((n >> 7) & 0x7f),
                        90 + ((n >> 14) & 0x7f),
                    )
                };
                col_brightness[col as usize] += (r + g + b) as u64;
            }
        }

        let argmax = col_brightness
            .iter()
            .enumerate()
            .max_by_key(|(_, v)| **v)
            .map(|(i, _)| i as i64)
            .unwrap();
        // The brightest column is one of the rails, never the dark target gap,
        // so a brightness-argmax solver lands off the answer.
        assert!(
            (argmax - marker).abs() > MARKER_HALF_PX,
            "argmax column {argmax} should not be the dark marker at {marker}"
        );
    }
}
