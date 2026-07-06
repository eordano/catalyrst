#[derive(Clone, Debug, Default)]
pub struct SimplifyReport {
    pub tris_before: usize,
    pub tris_after: usize,
    pub ratios_run: Vec<f64>,
    pub aggressive_final: bool,
    pub passthrough: bool,
    pub unsimplified: bool,
}

impl SimplifyReport {
    pub fn summary(&self) -> String {
        format!(
            "tris {} -> {} (ratios {:?}{}{}{})",
            self.tris_before,
            self.tris_after,
            self.ratios_run,
            if self.aggressive_final { ", -sa" } else { "" },
            if self.passthrough {
                ", passthrough"
            } else {
                ""
            },
            if self.unsimplified {
                ", UNSIMPLIFIED"
            } else {
                ""
            },
        )
    }
}
