pub fn recalculate_normals(positions: &[[f64; 3]], indices: &[u32]) -> Vec<[f64; 3]> {
    let n = positions.len();

    let p: Vec<[f32; 3]> = positions
        .iter()
        .map(|v| [v[0] as f32, v[1] as f32, v[2] as f32])
        .collect();

    let mut acc = vec![[0.0f32; 3]; n];
    let m = (indices.len() / 3) * 3;
    let mut k = 0usize;
    while k < m {
        let ia = indices[k] as usize;
        let ib = indices[k + 1] as usize;
        let ic = indices[k + 2] as usize;
        k += 3;
        if ia >= n || ib >= n || ic >= n {
            continue;
        }
        let pa = p[ia];
        let pb = p[ib];
        let pc = p[ic];
        let e1 = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let e2 = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];

        let fnx = e1[1] * e2[2] - e1[2] * e2[1];
        let fny = e1[2] * e2[0] - e1[0] * e2[2];
        let fnz = e1[0] * e2[1] - e1[1] * e2[0];
        for &v in &[ia, ib, ic] {
            acc[v][0] += fnx;
            acc[v][1] += fny;
            acc[v][2] += fnz;
        }
    }

    let mut out = Vec::with_capacity(n);
    for a in acc.iter() {
        let magsq = a[0] * a[0] + a[1] * a[1] + a[2] * a[2];
        if magsq > 0.0 {
            let inv = 1.0f32 / magsq.sqrt();
            out.push([
                (a[0] * inv) as f64,
                (a[1] * inv) as f64,
                (a[2] * inv) as f64,
            ]);
        } else {
            out.push([0.0, 0.0, 0.0]);
        }
    }
    out
}
