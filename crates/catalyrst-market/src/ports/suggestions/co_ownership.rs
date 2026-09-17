use super::constants::{CO_OWNERSHIP_SHRINKAGE, MIN_CO_OWNERS, NEIGHBORS_PER_ITEM};

/// Per-wallet scratch space. Sized independently of `MAX_WALLET_ITEMS` so this file does not have
/// to track the SQL band; the bounds check in the accumulation loop is what keeps that safe.
const MAX_WALLET_ITEMS_BUFFER: usize = 512;

/// Neighbour columns per accumulation pass. The full matrix would not fit beside the API, so this
/// is the knob that trades memory for passes: peak stays at `item_count * block_width * 8` bytes
/// whatever the catalogue size.
pub const DEFAULT_BLOCK_WIDTH: usize = 128;

/// Acquisitions in compressed row form, one row per wallet. Wallet `w` owns the entries in
/// `[offsets[w], offsets[w + 1])`. `weights` already carries the hoarder damping, so the maths
/// below never needs the wallet's size again.
#[derive(Debug, Default)]
pub struct AcquisitionMatrix {
    pub offsets: Vec<i32>,
    pub items: Vec<i32>,
    pub weights: Vec<f32>,
    pub wallet_count: usize,
    pub item_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeighborRow {
    pub item: usize,
    pub neighbor: usize,
    pub sim: f64,
    pub support: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct CoOwnershipOptions {
    pub min_support: u32,
    pub neighbors_per_item: usize,
    pub shrinkage: f64,
    pub block_width: usize,
}

impl Default for CoOwnershipOptions {
    fn default() -> Self {
        Self {
            min_support: MIN_CO_OWNERS,
            neighbors_per_item: NEIGHBORS_PER_ITEM,
            shrinkage: CO_OWNERSHIP_SHRINKAGE,
            block_width: DEFAULT_BLOCK_WIDTH,
        }
    }
}

/// The per-item weight vector's L2 norm, over the same damped weights the dot products use.
/// Together they make `dot / (norm_a * norm_b)` a plain cosine.
pub fn compute_norms(matrix: &AcquisitionMatrix) -> Vec<f64> {
    let mut norms = vec![0.0f64; matrix.item_count];
    for (i, item) in matrix.items.iter().enumerate() {
        let weight = matrix.weights[i] as f64;
        norms[*item as usize] += weight * weight;
    }
    for norm in norms.iter_mut() {
        *norm = norm.sqrt();
    }
    norms
}

/// Item-item co-ownership neighbours: for every anchor item, the top-K candidate items most often
/// held by the same wallets, as a cosine shrunk by the co-owner count.
///
/// Anchors are EVERY item, not just candidates -- a wallet's profile can contain anything,
/// including items that were never sellable -- while neighbours are restricted to `is_candidate`,
/// because a row whose neighbour can never be recommended is dead weight in the table.
///
/// The accumulator is blocked over neighbour columns rather than allocated whole: one pass per
/// block, each anchor's block-local best merged into a running top-K.
pub fn build_co_ownership_neighbors(
    matrix: &AcquisitionMatrix,
    is_candidate: &[bool],
    options: CoOwnershipOptions,
) -> Vec<NeighborRow> {
    let item_count = matrix.item_count;
    let k = options.neighbors_per_item.max(1);
    let norms = compute_norms(matrix);

    let mut columns: Vec<usize> = Vec::new();
    let mut column_of = vec![-1i32; item_count];
    for item in 0..item_count {
        if is_candidate.get(item).copied().unwrap_or(false) && norms[item] > 0.0 {
            column_of[item] = columns.len() as i32;
            columns.push(item);
        }
    }
    if columns.is_empty() {
        return Vec::new();
    }

    let mut heaps = TopKHeaps::new(item_count, k);
    let width = options.block_width.max(1).min(columns.len());
    let mut dot = vec![0.0f64; item_count * width];
    let mut support = vec![0u32; item_count * width];
    let mut in_block = [0usize; MAX_WALLET_ITEMS_BUFFER];
    let mut in_block_weight = [0.0f64; MAX_WALLET_ITEMS_BUFFER];

    let mut block_start = 0usize;
    while block_start < columns.len() {
        let block_end = (block_start + width).min(columns.len());
        let block_size = block_end - block_start;
        dot.fill(0.0);
        support.fill(0);

        for wallet in 0..matrix.wallet_count {
            let from = matrix.offsets[wallet] as usize;
            let to = matrix.offsets[wallet + 1] as usize;

            let mut hits = 0usize;
            for i in from..to {
                // A wallet larger than the buffer would otherwise write past it. The SQL band
                // keeps wallets under MAX_WALLET_ITEMS, but the two are deliberately independent
                // and the margin is thin, so the guard is what makes that independence safe.
                if hits >= in_block.len() {
                    break;
                }
                let column = column_of[matrix.items[i] as usize];
                if column >= block_start as i32 && column < block_end as i32 {
                    in_block[hits] = (column as usize) - block_start;
                    in_block_weight[hits] = matrix.weights[i] as f64;
                    hits += 1;
                }
            }
            if hits == 0 {
                continue;
            }

            for i in from..to {
                let anchor = matrix.items[i] as usize;
                let anchor_weight = matrix.weights[i] as f64;
                let anchor_column = column_of[anchor] - block_start as i32;
                let base = anchor * width;
                for j in 0..hits {
                    let column = in_block[j];
                    if column as i32 == anchor_column {
                        continue;
                    }
                    dot[base + column] += anchor_weight * in_block_weight[j];
                    support[base + column] += 1;
                }
            }
        }

        for anchor in 0..item_count {
            let anchor_norm = norms[anchor];
            if anchor_norm == 0.0 {
                continue;
            }
            let base = anchor * width;
            for column in 0..block_size {
                let co = support[base + column];
                if co < options.min_support {
                    continue;
                }
                let neighbor = columns[block_start + column];
                let denominator = anchor_norm * norms[neighbor];
                if denominator == 0.0 {
                    continue;
                }
                let co_f = co as f64;
                let sim = (dot[base + column] / denominator) * (co_f / (co_f + options.shrinkage));
                if sim > 0.0 {
                    heaps.offer(anchor, neighbor, sim, co);
                }
            }
        }

        block_start += width;
    }

    heaps.drain()
}

/// One bounded min-heap per anchor, flat so there is no per-item allocation.
struct TopKHeaps {
    sims: Vec<f64>,
    neighbors: Vec<usize>,
    supports: Vec<u32>,
    sizes: Vec<usize>,
    item_count: usize,
    k: usize,
}

impl TopKHeaps {
    fn new(item_count: usize, k: usize) -> Self {
        Self {
            sims: vec![0.0; item_count * k],
            neighbors: vec![0; item_count * k],
            supports: vec![0; item_count * k],
            sizes: vec![0; item_count],
            item_count,
            k,
        }
    }

    fn offer(&mut self, anchor: usize, neighbor: usize, sim: f64, support: u32) {
        let base = anchor * self.k;
        let size = self.sizes[anchor];
        if size < self.k {
            self.sims[base + size] = sim;
            self.neighbors[base + size] = neighbor;
            self.supports[base + size] = support;
            self.sizes[anchor] = size + 1;
            if size + 1 == self.k {
                self.heapify(base);
            }
            return;
        }
        if sim <= self.sims[base] {
            return;
        }
        self.sims[base] = sim;
        self.neighbors[base] = neighbor;
        self.supports[base] = support;
        self.sift_down(base, 0, self.k);
    }

    fn drain(self) -> Vec<NeighborRow> {
        let mut rows: Vec<NeighborRow> = Vec::new();
        for anchor in 0..self.item_count {
            let size = self.sizes[anchor];
            if size == 0 {
                continue;
            }
            let base = anchor * self.k;
            let mut slice: Vec<NeighborRow> = (0..size)
                .map(|i| NeighborRow {
                    item: anchor,
                    neighbor: self.neighbors[base + i],
                    sim: self.sims[base + i],
                    support: self.supports[base + i],
                })
                .collect();
            slice.sort_by(|a, b| {
                b.sim
                    .partial_cmp(&a.sim)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.neighbor.cmp(&b.neighbor))
            });
            rows.extend(slice);
        }
        rows
    }

    fn heapify(&mut self, base: usize) {
        for i in (0..self.k / 2).rev() {
            self.sift_down(base, i, self.k);
        }
    }

    fn sift_down(&mut self, base: usize, start: usize, size: usize) {
        let mut root = start;
        loop {
            let left = 2 * root + 1;
            if left >= size {
                break;
            }
            let mut smallest = left;
            let right = left + 1;
            if right < size && self.sims[base + right] < self.sims[base + left] {
                smallest = right;
            }
            if self.sims[base + smallest] >= self.sims[base + root] {
                break;
            }
            self.sims.swap(base + root, base + smallest);
            self.neighbors.swap(base + root, base + smallest);
            self.supports.swap(base + root, base + smallest);
            root = smallest;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wallets as item-index lists, folded into the compressed form with the hoarder damping the
    /// loader applies: `|items(u)|^(-1/4)`.
    fn matrix(wallets: &[&[usize]], item_count: usize) -> AcquisitionMatrix {
        let mut offsets = vec![0i32];
        let mut items: Vec<i32> = Vec::new();
        let mut weights: Vec<f32> = Vec::new();
        for wallet in wallets {
            let damping = (wallet.len() as f64).powf(-0.25) as f32;
            for item in *wallet {
                items.push(*item as i32);
                weights.push(damping);
            }
            offsets.push(items.len() as i32);
        }
        AcquisitionMatrix {
            offsets,
            items,
            weights,
            wallet_count: wallets.len(),
            item_count,
        }
    }

    fn options(min_support: u32) -> CoOwnershipOptions {
        CoOwnershipOptions {
            min_support,
            ..Default::default()
        }
    }

    #[test]
    fn wallets_holding_both_items_push_each_towards_the_other() {
        let m = matrix(&[&[0, 1], &[0, 1], &[0, 1]], 3);
        let rows = build_co_ownership_neighbors(&m, &[true, true, true], options(3));

        assert!(rows.iter().any(|r| r.item == 0 && r.neighbor == 1));
        assert!(rows.iter().any(|r| r.item == 1 && r.neighbor == 0));
        assert!(rows.iter().all(|r| r.item != 2 && r.neighbor != 2));
        for row in &rows {
            assert_eq!(row.support, 3);
            assert!(row.sim > 0.0 && row.sim <= 1.0, "{row:?}");
        }
    }

    /// A pair below the support floor is not trusted at all.
    #[test]
    fn a_thin_pair_is_dropped_rather_than_ranked() {
        let m = matrix(&[&[0, 1], &[0, 1]], 2);
        assert!(build_co_ownership_neighbors(&m, &[true, true], options(3)).is_empty());
        assert!(!build_co_ownership_neighbors(&m, &[true, true], options(2)).is_empty());
    }

    /// The shrinkage is what keeps a two-co-owner pair from scoring like a two-hundred one.
    #[test]
    fn support_shrinks_the_cosine_towards_zero() {
        let thin = matrix(&[&[0, 1], &[0, 1], &[0, 1]], 2);
        let thick = matrix(&vec![&[0usize, 1usize][..]; 60], 2);

        let thin_sim = build_co_ownership_neighbors(&thin, &[true, true], options(3))[0].sim;
        let thick_sim = build_co_ownership_neighbors(&thick, &[true, true], options(3))[0].sim;
        assert!(thick_sim > thin_sim, "{thick_sim} vs {thin_sim}");
    }

    /// Anchors are every item; only NEIGHBOURS are restricted to what can be recommended.
    #[test]
    fn a_non_candidate_can_anchor_but_never_be_a_neighbour() {
        let m = matrix(&[&[0, 1], &[0, 1], &[0, 1]], 2);
        let rows = build_co_ownership_neighbors(&m, &[false, true], options(3));

        assert!(rows.iter().any(|r| r.item == 0 && r.neighbor == 1));
        assert!(rows.iter().all(|r| r.neighbor != 0));
    }

    /// Blocking is a memory strategy, not a different answer.
    #[test]
    fn blocking_over_columns_does_not_change_the_result() {
        let wallets: Vec<Vec<usize>> = (0..12)
            .map(|w| vec![w % 5, (w + 1) % 5, (w + 2) % 5])
            .collect();
        let borrowed: Vec<&[usize]> = wallets.iter().map(|w| w.as_slice()).collect();
        let m = matrix(&borrowed, 5);
        let candidates = [true; 5];

        let whole = build_co_ownership_neighbors(
            &m,
            &candidates,
            CoOwnershipOptions {
                block_width: 64,
                ..options(2)
            },
        );
        let blocked = build_co_ownership_neighbors(
            &m,
            &candidates,
            CoOwnershipOptions {
                block_width: 1,
                ..options(2)
            },
        );
        assert_eq!(whole.len(), blocked.len());
        for (a, b) in whole.iter().zip(blocked.iter()) {
            assert_eq!(a.item, b.item);
            assert_eq!(a.neighbor, b.neighbor);
            assert!((a.sim - b.sim).abs() < 1e-12, "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn each_anchor_keeps_at_most_k_neighbours_strongest_first() {
        let wallets: Vec<Vec<usize>> = (1..6).map(|item| vec![0, item, item + 5]).collect();
        let borrowed: Vec<&[usize]> = wallets.iter().map(|w| w.as_slice()).collect();
        let m = matrix(&borrowed, 11);
        let rows = build_co_ownership_neighbors(
            &m,
            &[true; 11],
            CoOwnershipOptions {
                neighbors_per_item: 2,
                ..options(1)
            },
        );

        let anchor_rows: Vec<&NeighborRow> = rows.iter().filter(|r| r.item == 0).collect();
        assert_eq!(anchor_rows.len(), 2);
        assert!(anchor_rows[0].sim >= anchor_rows[1].sim);
    }
}
