use super::*;

#[cfg(not(target_arch = "nvptx64"))]
pub mod probe {
    use super::{
        BC7E_2SUBSET_CHECKERBOARD_PARTITION_INDEX, G_ALPHA_INDEX_BITCOUNT, G_ALPHA_PRECISION_TABLE,
        G_ANCHOR_2ND, G_ANCHOR_3RD_1, G_ANCHOR_3RD_2, G_COLOR_INDEX_BITCOUNT,
        G_COLOR_PRECISION_TABLE, G_MODE_HAS_P_BITS, G_MODE_HAS_SHARED_P_BITS, G_NUM_SUBSETS,
        G_PARTITION2, G_PARTITION3, G_PARTITION_BITS, G_WEIGHTS2, G_WEIGHTS2X, G_WEIGHTS3,
        G_WEIGHTS3X, G_WEIGHTS4, G_WEIGHTS4X, MODE0_IDX, MODE1_IDX, MODE4_IDX2, MODE4_IDX3,
        MODE5_IDX, MODE6_IDX, MODE7_IDX, PB_WEIGHT, PR_WEIGHT, SUBSET_IDX2, SUBSET_IDX3,
    };

    pub fn weights2() -> &'static [u32; 4] {
        &G_WEIGHTS2
    }
    pub fn weights3() -> &'static [u32; 8] {
        &G_WEIGHTS3
    }
    pub fn weights4() -> &'static [u32; 16] {
        &G_WEIGHTS4
    }
    pub fn weights2x() -> &'static [[f32; 4]; 4] {
        &G_WEIGHTS2X
    }
    pub fn weights3x() -> &'static [[f32; 4]; 8] {
        &G_WEIGHTS3X
    }
    pub fn weights4x() -> &'static [[f32; 4]; 16] {
        &G_WEIGHTS4X
    }
    pub fn partition2() -> &'static [u8; 64 * 16] {
        &G_PARTITION2
    }
    pub fn partition3() -> &'static [u8; 64 * 16] {
        &G_PARTITION3
    }
    pub fn anchor_2nd() -> &'static [i32; 64] {
        &G_ANCHOR_2ND
    }
    pub fn anchor_3rd_1() -> &'static [i32; 64] {
        &G_ANCHOR_3RD_1
    }
    pub fn anchor_3rd_2() -> &'static [i32; 64] {
        &G_ANCHOR_3RD_2
    }
    pub fn num_subsets() -> &'static [usize; 8] {
        &G_NUM_SUBSETS
    }
    pub fn partition_bits() -> &'static [u32; 8] {
        &G_PARTITION_BITS
    }
    pub fn color_index_bitcount() -> &'static [u32; 8] {
        &G_COLOR_INDEX_BITCOUNT
    }
    pub fn alpha_index_bitcount() -> &'static [i32; 8] {
        &G_ALPHA_INDEX_BITCOUNT
    }
    pub fn mode_has_p_bits() -> &'static [i32; 8] {
        &G_MODE_HAS_P_BITS
    }
    pub fn mode_has_shared_p_bits() -> &'static [i32; 8] {
        &G_MODE_HAS_SHARED_P_BITS
    }
    pub fn color_precision_table() -> &'static [u32; 8] {
        &G_COLOR_PRECISION_TABLE
    }
    pub fn alpha_precision_table() -> &'static [u32; 8] {
        &G_ALPHA_PRECISION_TABLE
    }
    pub fn pr_weight() -> f32 {
        PR_WEIGHT
    }
    pub fn pb_weight() -> f32 {
        PB_WEIGHT
    }
    pub fn mode_idx_words() -> [u32; 7] {
        [
            MODE0_IDX as u32,
            MODE1_IDX as u32,
            MODE6_IDX as u32,
            MODE5_IDX as u32,
            MODE4_IDX3 as u32,
            MODE4_IDX2 as u32,
            MODE7_IDX as u32,
        ]
    }
    pub fn checkerboard_partition_index() -> u32 {
        BC7E_2SUBSET_CHECKERBOARD_PARTITION_INDEX as u32
    }
    pub fn subset_idx2(partition: usize) -> ([[i32; 16]; 3], [u32; 3]) {
        let s = &SUBSET_IDX2[partition];
        (
            s.idx,
            [s.total[0] as u32, s.total[1] as u32, s.total[2] as u32],
        )
    }
    pub fn subset_idx3(partition: usize) -> ([[i32; 16]; 3], [u32; 3]) {
        let s = &SUBSET_IDX3[partition];
        (
            s.idx,
            [s.total[0] as u32, s.total[1] as u32, s.total[2] as u32],
        )
    }

    pub fn saturate(v: f32) -> f32 {
        super::saturate(v)
    }
    pub fn itrunc(f: f32) -> i32 {
        super::itrunc(f)
    }
    pub fn iabs32(v: i32) -> i32 {
        super::iabs32(v)
    }
    pub fn sq(s: f32) -> f32 {
        super::sq(s)
    }
    pub fn vec4f_dot(a: [f32; 4], b: [f32; 4]) -> f32 {
        super::vec4f_dot(&super::Vec4F { c: a }, &super::Vec4F { c: b })
    }
    pub fn vec4f_normalize(v: [f32; 4]) -> [f32; 4] {
        let mut x = super::Vec4F { c: v };
        super::vec4f_normalize(&mut x);
        x.c
    }
    pub fn scale_color(c: [i32; 4], comp_bits: u32, has_pbits: bool) -> [i32; 4] {
        let mut p = super::CCParams::clear();
        p.comp_bits = comp_bits;
        p.has_pbits = has_pbits;
        super::scale_color(&super::ColorI { c }, &p).c
    }
    pub fn dist_rgb(e1: [i32; 4], e2: [i32; 4], perceptual: bool, w: [u32; 4]) -> u64 {
        super::compute_color_distance_rgb_scalar(
            &super::ColorI { c: e1 },
            &super::ColorI { c: e2 },
            perceptual,
            &w,
        )
    }
    pub fn dist_rgba(e1: [i32; 4], e2: [i32; 4], perceptual: bool, w: [u32; 4]) -> u64 {
        super::compute_color_distance_rgba_scalar(
            &super::ColorI { c: e1 },
            &super::ColorI { c: e2 },
            perceptual,
            &w,
        )
    }
    fn weightsx_table(idx: usize) -> &'static [[f32; 4]] {
        match idx {
            0 => &G_WEIGHTS2X,
            1 => &G_WEIGHTS3X,
            _ => &G_WEIGHTS4X,
        }
    }
    pub fn weightsx_table_len(idx: usize) -> usize {
        weightsx_table(idx).len()
    }
    fn colors_from(colors: &[[i32; 4]; 16]) -> [super::ColorI; 16] {
        let mut out = [super::ColorI::default(); 16];
        for i in 0..16 {
            out[i] = super::ColorI { c: colors[i] };
        }
        out
    }
    pub fn lsq_rgba(
        n: usize,
        sel: &[i32; 16],
        table_idx: usize,
        colors: &[[i32; 4]; 16],
    ) -> ([f32; 4], [f32; 4]) {
        let cols = colors_from(colors);
        let mut xl = super::Vec4F::default();
        let mut xh = super::Vec4F::default();
        super::compute_lsq_endpoints_rgba(
            n,
            sel,
            weightsx_table(table_idx),
            &mut xl,
            &mut xh,
            &cols,
        );
        (xl.c, xh.c)
    }
    pub fn lsq_rgb(
        n: usize,
        sel: &[i32; 16],
        table_idx: usize,
        colors: &[[i32; 4]; 16],
    ) -> ([f32; 4], [f32; 4]) {
        let cols = colors_from(colors);
        let mut xl = super::Vec4F::default();
        let mut xh = super::Vec4F::default();
        super::compute_lsq_endpoints_rgb_scalar(
            n,
            sel,
            weightsx_table(table_idx),
            &mut xl,
            &mut xh,
            &cols,
        );
        (xl.c, xh.c)
    }
    pub fn lsq_a(
        n: usize,
        sel: &[i32; 16],
        table_idx: usize,
        colors: &[[i32; 4]; 16],
    ) -> (f32, f32) {
        let cols = colors_from(colors);
        let mut xl = 0f32;
        let mut xh = 0f32;
        super::compute_lsq_endpoints_a(n, sel, weightsx_table(table_idx), &mut xl, &mut xh, &cols);
        (xl, xh)
    }
    pub fn luma_weights() -> [f32; 3] {
        [0.2126, 0.7152, 0.0722]
    }

    fn ccparams_full(
        nsw: u32,
        tbl: usize,
        comp_bits: u32,
        weights: [u32; 4],
        has_alpha: bool,
        has_pbits: bool,
        share_pbit: bool,
        perceptual: bool,
    ) -> super::CCParams {
        let mut p = super::CCParams::clear();
        p.num_selector_weights = nsw;
        p.psel_weights = match tbl {
            0 => &G_WEIGHTS2,
            1 => &G_WEIGHTS3,
            _ => &G_WEIGHTS4,
        };
        p.psel_weightsx = weightsx_table(tbl);
        p.comp_bits = comp_bits;
        p.weights = weights;
        p.has_alpha = has_alpha;
        p.has_pbits = has_pbits;
        p.endpoints_share_pbit = share_pbit;
        p.perceptual = perceptual;
        p
    }

    pub type PackOut = ([i32; 4], [i32; 4], [u32; 2], [i32; 16], u64);

    fn pack_out(res: &super::CCResults, e: u64) -> PackOut {
        (res.low.c, res.high.c, res.pbits, res.selectors, e)
    }

    pub fn pack_mode0_one_color(
        perceptual: bool,
        weights: [u32; 4],
        rgba: [usize; 4],
        num_pixels: usize,
        pixels: &[[i32; 4]; 16],
        t: &super::OptTables,
    ) -> PackOut {
        let p = ccparams_full(8, 1, 4, weights, false, true, false, perceptual);
        let cols = colors_from(pixels);
        let mut res = super::CCResults::new();
        let e = super::pack_mode0_to_one_color(
            &p, t, &mut res, rgba[0], rgba[1], rgba[2], num_pixels, &cols,
        );
        pack_out(&res, e)
    }

    pub fn pack_mode1_one_color(
        perceptual: bool,
        weights: [u32; 4],
        rgba: [usize; 4],
        num_pixels: usize,
        pixels: &[[i32; 4]; 16],
        t: &super::OptTables,
    ) -> PackOut {
        let p = ccparams_full(8, 1, 6, weights, false, true, true, perceptual);
        let cols = colors_from(pixels);
        let mut res = super::CCResults::new();
        let e = super::pack_mode1_to_one_color(
            &p, t, &mut res, rgba[0], rgba[1], rgba[2], num_pixels, &cols,
        );
        pack_out(&res, e)
    }

    pub fn pack_mode24_one_color(
        nsw: u32,
        perceptual: bool,
        weights: [u32; 4],
        rgba: [usize; 4],
        num_pixels: usize,
        pixels: &[[i32; 4]; 16],
        t: &super::OptTables,
    ) -> PackOut {
        let tbl = if nsw == 8 { 1 } else { 0 };
        let p = ccparams_full(nsw, tbl, 5, weights, false, false, false, perceptual);
        let cols = colors_from(pixels);
        let mut res = super::CCResults::new();
        let e = super::pack_mode24_to_one_color(
            &p, t, &mut res, rgba[0], rgba[1], rgba[2], num_pixels, &cols,
        );
        pack_out(&res, e)
    }

    pub fn pack_mode6_one_color(
        perceptual: bool,
        weights: [u32; 4],
        rgba: [usize; 4],
        num_pixels: usize,
        pixels: &[[i32; 4]; 16],
        t: &super::OptTables,
    ) -> PackOut {
        let p = ccparams_full(16, 2, 7, weights, true, true, false, perceptual);
        let cols = colors_from(pixels);
        let mut res = super::CCResults::new();
        let e = super::pack_mode6_to_one_color(
            &p, t, &mut res, rgba[0], rgba[1], rgba[2], rgba[3], num_pixels, &cols,
        );
        pack_out(&res, e)
    }

    pub fn pack_mode7_one_color(
        perceptual: bool,
        weights: [u32; 4],
        rgba: [usize; 4],
        num_pixels: usize,
        pixels: &[[i32; 4]; 16],
        t: &super::OptTables,
    ) -> PackOut {
        let p = ccparams_full(4, 0, 5, weights, true, true, false, perceptual);
        let cols = colors_from(pixels);
        let mut res = super::CCResults::new();
        let e = super::pack_mode7_to_one_color(
            &p, t, &mut res, rgba[0], rgba[1], rgba[2], rgba[3], num_pixels, &cols,
        );
        pack_out(&res, e)
    }

    pub fn fix_degenerate(
        mode: usize,
        tmin: [i32; 4],
        tmax: [i32; 4],
        xl: [f32; 4],
        xh: [f32; 4],
        iscale: i32,
    ) -> ([i32; 4], [i32; 4]) {
        let mut a = super::ColorI { c: tmin };
        let mut b = super::ColorI { c: tmax };
        super::fix_degenerate_endpoints(
            mode,
            &mut a,
            &mut b,
            &super::Vec4F { c: xl },
            &super::Vec4F { c: xh },
            iscale,
        );
        (a.c, b.c)
    }

    #[derive(Clone, Copy)]
    pub struct EvalCase {
        pub low: [i32; 4],
        pub high: [i32; 4],
        pub pbits: [u32; 2],
        pub nsw: u32,
        pub tbl: usize,
        pub comp_bits: u32,
        pub weights: [u32; 4],
        pub has_alpha: bool,
        pub has_pbits: bool,
        pub share_pbit: bool,
        pub perceptual: bool,
        pub init_err: u64,
        pub num_pixels: usize,
    }

    pub type EvalOut = (u64, u64, [i32; 4], [i32; 4], [u32; 2], [i32; 16], [i32; 16]);

    pub fn eval_solution(c: &EvalCase, pixels: &[[i32; 4]; 16]) -> EvalOut {
        let p = ccparams_full(
            c.nsw,
            c.tbl,
            c.comp_bits,
            c.weights,
            c.has_alpha,
            c.has_pbits,
            c.share_pbit,
            c.perceptual,
        );
        let cols = colors_from(pixels);
        let mut res = super::CCResults::new();
        res.best_overall_err = c.init_err;
        let e = super::evaluate_solution(
            &super::ColorI { c: c.low },
            &super::ColorI { c: c.high },
            &c.pbits,
            &p,
            &mut res,
            c.num_pixels,
            &cols,
        );
        (
            e,
            res.best_overall_err,
            res.low.c,
            res.high.c,
            res.pbits,
            res.selectors,
            res.selectors_temp,
        )
    }

    pub fn eval_n16_rgb(
        num_pixels: usize,
        pixels: &[[i32; 4]; 16],
        wc: &[[f32; 4]; 16],
        w: [f32; 3],
        d: [f32; 3],
        l: [f32; 3],
        f: f32,
        n: usize,
    ) -> (f32, [i32; 16]) {
        let cols = colors_from(pixels);
        let mut sel = [0i32; 16];
        let e = super::eval_solution_n16_rgb_scalar(
            num_pixels, &cols, wc, w[0], w[1], w[2], d[0], d[1], d[2], l[0], l[1], l[2], f, n,
            &mut sel,
        );
        (e, sel)
    }

    #[derive(Clone, Copy)]
    pub struct CCP {
        pub nsw: u32,
        pub tbl: usize,
        pub comp_bits: u32,
        pub weights: [u32; 4],
        pub has_alpha: bool,
        pub has_pbits: bool,
        pub share_pbit: bool,
        pub perceptual: bool,
    }

    #[derive(Clone, Copy)]
    pub struct CCInit {
        pub err: u64,
        pub low: [i32; 4],
        pub high: [i32; 4],
        pub pbits: [u32; 2],
    }

    pub type CCOut = (u64, [i32; 4], [i32; 4], [u32; 2], [i32; 16], [i32; 16]);

    fn ccp_params(c: &CCP) -> super::CCParams {
        ccparams_full(
            c.nsw,
            c.tbl,
            c.comp_bits,
            c.weights,
            c.has_alpha,
            c.has_pbits,
            c.share_pbit,
            c.perceptual,
        )
    }

    fn res_from(init: &CCInit) -> super::CCResults {
        let mut r = super::CCResults::new();
        r.best_overall_err = init.err;
        r.low = super::ColorI { c: init.low };
        r.high = super::ColorI { c: init.high };
        r.pbits = init.pbits;
        r
    }

    fn cc_out(res: &super::CCResults) -> CCOut {
        (
            res.best_overall_err,
            res.low.c,
            res.high.c,
            res.pbits,
            res.selectors,
            res.selectors_temp,
        )
    }

    pub fn eval_4way(
        c: &CCP,
        lo: [[i32; 4]; 2],
        hi: [[i32; 4]; 2],
        init: &CCInit,
        num_pixels: usize,
        pixels: &[[i32; 4]; 16],
    ) -> CCOut {
        let p = ccp_params(c);
        let cols = colors_from(pixels);
        let mut res = res_from(init);
        let lo2 = [super::ColorI { c: lo[0] }, super::ColorI { c: lo[1] }];
        let hi2 = [super::ColorI { c: hi[0] }, super::ColorI { c: hi[1] }];
        super::eval_4way_pbit_with_tiebreak(&lo2, &hi2, &p, &mut res, num_pixels, &cols);
        cc_out(&res)
    }

    pub fn find_optimal(
        mode: usize,
        xl: [f32; 4],
        xh: [f32; 4],
        c: &CCP,
        pbit_search: bool,
        init: &CCInit,
        num_pixels: usize,
        pixels: &[[i32; 4]; 16],
    ) -> (u64, CCOut) {
        let p = ccp_params(c);
        let cols = colors_from(pixels);
        let mut res = res_from(init);
        let e = super::find_optimal_solution(
            mode,
            &super::Vec4F { c: xl },
            &super::Vec4F { c: xh },
            &p,
            &mut res,
            pbit_search,
            num_pixels,
            &cols,
        );
        (e, cc_out(&res))
    }

    pub fn est_idx(
        mode: usize,
        weights: [u32; 4],
        idxs: &[i32; 16],
        num_pixels: usize,
        pixels: &[[i32; 4]; 16],
    ) -> u64 {
        let mut p = super::CCParams::clear();
        p.weights = weights;
        let cols = colors_from(pixels);
        super::ccc_est_idx_scalar(mode, &p, idxs, num_pixels, &cols)
    }

    pub fn est_mode7_idx(
        weights: [u32; 4],
        perceptual: bool,
        idxs: &[i32; 16],
        num_pixels: usize,
        pixels: &[[i32; 4]; 16],
    ) -> u64 {
        let mut p = super::CCParams::clear();
        p.weights = weights;
        p.perceptual = perceptual;
        let cols = colors_from(pixels);
        super::ccc_est_mode7_idx_scalar(&p, idxs, num_pixels, &cols)
    }

    pub fn est_params(mode: usize, cp: &super::Params) -> ([u32; 4], u32, u32, bool) {
        let p = super::make_est_params(mode, cp);
        let tbl = if p.psel_weights.len() == 4 { 0 } else { 1 };
        (p.weights, p.num_selector_weights, tbl, p.perceptual)
    }

    pub type SolList = ([u32; 8], [u64; 8], usize);

    fn sol_list(l: &super::SolutionList) -> SolList {
        let mut idx = [0u32; 8];
        let mut errs = [0u64; 8];
        for i in 0..super::SOL_CAP {
            idx[i] = l.sols[i].index;
            errs[i] = l.sols[i].err;
        }
        (idx, errs, l.len)
    }

    pub fn estimate_partition(mode: usize, cp: &super::Params, pixels: &[[i32; 4]; 16]) -> u32 {
        let cols = colors_from(pixels);
        let lanes: [&[super::ColorI; 16]; 1] = [&cols];
        super::estimate_partition_group(mode, &lanes, cp)[0]
    }

    pub fn estimate_partition_list(
        mode: usize,
        cp: &super::Params,
        max_solutions: i32,
        pixels: &[[i32; 4]; 16],
    ) -> SolList {
        let cols = colors_from(pixels);
        let lanes: [&[super::ColorI; 16]; 1] = [&cols];
        let mut out = [super::SolutionList::new(); 1];
        super::estimate_partition_list_group(mode, &lanes, cp, max_solutions, &mut out);
        sol_list(&out[0])
    }

    pub struct PlanOut {
        pub part0: u32,
        pub part13: u32,
        pub part2: u32,
        pub use_list13: bool,
        pub use_list2: bool,
        pub use_list0: bool,
        pub list13: SolList,
        pub list2: SolList,
        pub list0: SolList,
        pub list7: SolList,
    }

    pub fn build_plans(cp: &super::Params, pixels: &[[i32; 4]; 16]) -> PlanOut {
        let cols = colors_from(pixels);
        let lanes: [&[super::ColorI; 16]; 1] = [&cols];
        let mut plans = [super::PartitionPlan::new(); 1];
        super::build_partition_plans(&lanes, cp, &mut plans);
        let p = &plans[0];
        PlanOut {
            part0: p.part0,
            part13: p.part13,
            part2: p.part2,
            use_list13: p.use_list13,
            use_list2: p.use_list2,
            use_list0: p.use_list0,
            list13: sol_list(&p.list13),
            list2: sol_list(&p.list2),
            list0: sol_list(&p.list0),
            list7: sol_list(&p.list7),
        }
    }

    pub fn mode_tree_hint(pixels: &[[i32; 4]; 16], cp: &super::Params) -> (bool, super::Params) {
        let cols = colors_from(pixels);
        match super::apply_mode_tree_hint(&cols, cp) {
            Some(p) => (true, p),
            None => (false, cp.clone()),
        }
    }

    pub fn ccc(
        mode: usize,
        c: &CCP,
        pbit_search: bool,
        refinement_passes: u32,
        uber_level: u32,
        uber1_mask: u32,
        refinement: bool,
        num_pixels: usize,
        pixels: &[[i32; 4]; 16],
        t: &super::OptTables,
    ) -> CCOut {
        let p = ccp_params(c);
        let mut cp = super::Params::slow(c.perceptual);
        cp.pbit_search = pbit_search;
        cp.refinement_passes = refinement_passes;
        cp.uber_level = uber_level;
        cp.uber1_mask = uber1_mask;
        let cols = colors_from(pixels);
        let mut res = super::CCResults::new();
        let e = super::color_cell_compression(
            mode, &p, &mut res, &cp, num_pixels, &cols, refinement, t,
        );
        assert_eq!(e, res.best_overall_err);
        cc_out(&res)
    }

    #[derive(Clone, Copy)]
    pub struct OptIn {
        pub mode: u32,
        pub partition: u32,
        pub selectors: [i32; 16],
        pub alpha_selectors: [i32; 16],
        pub low: [[i32; 4]; 3],
        pub high: [[i32; 4]; 3],
        pub pbits: [[u32; 2]; 3],
        pub rotation: u32,
        pub index_selector: u32,
    }

    fn opt_results(o: &OptIn) -> super::OptResults {
        let mut r = super::OptResults::new();
        r.mode = o.mode as usize;
        r.partition = o.partition;
        r.selectors = o.selectors;
        r.alpha_selectors = o.alpha_selectors;
        for k in 0..3 {
            r.low[k] = super::ColorI { c: o.low[k] };
            r.high[k] = super::ColorI { c: o.high[k] };
            r.pbits[k] = o.pbits[k];
        }
        r.rotation = o.rotation;
        r.index_selector = o.index_selector;
        r
    }

    pub fn encode_block_bits(o: &OptIn) -> [u8; 16] {
        super::encode_bc7_block_bits(&opt_results(o))
    }

    pub fn encode_block_mode6(o: &OptIn) -> [u8; 16] {
        super::encode_bc7_block_mode6(&opt_results(o))
    }

    pub fn block_solid(cr: usize, cg: usize, cb: usize, ca: i32, t: &super::OptTables) -> [u8; 16] {
        super::handle_block_solid(cr, cg, cb, ca, t)
    }

    pub type AlphaOut = (u64, u32, [i32; 4], [i32; 4], [i32; 16], [i32; 16]);

    pub fn alpha_mode4(
        weights: [u32; 4],
        cp: &super::Params,
        lo_a: i32,
        hi_a: i32,
        init_err: u64,
        pixels: &[[i32; 4]; 16],
        t: &super::OptTables,
    ) -> AlphaOut {
        let cols = colors_from(pixels);
        let mut params = super::CCParams::clear();
        params.weights = weights;
        let mut opt = super::OptResults::new();
        let mut err = init_err;
        super::handle_alpha_block_mode4(&cols, cp, &mut params, lo_a, hi_a, &mut opt, &mut err, t);
        (
            err,
            opt.index_selector,
            opt.low[0].c,
            opt.high[0].c,
            opt.selectors,
            opt.alpha_selectors,
        )
    }

    pub fn alpha_mode5(
        weights: [u32; 4],
        cp: &super::Params,
        lo_a: i32,
        hi_a: i32,
        pixels: &[[i32; 4]; 16],
        t: &super::OptTables,
    ) -> AlphaOut {
        let cols = colors_from(pixels);
        let mut params = super::CCParams::clear();
        params.weights = weights;
        let mut opt = super::OptResults::new();
        let mut err = 0u64;
        super::handle_alpha_block_mode5(&cols, cp, &mut params, lo_a, hi_a, &mut opt, &mut err, t);
        (
            err,
            opt.index_selector,
            opt.low[0].c,
            opt.high[0].c,
            opt.selectors,
            opt.alpha_selectors,
        )
    }
}
