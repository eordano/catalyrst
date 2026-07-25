#![allow(clippy::all)]
#![allow(unused_imports)]
#![allow(unused_qualifications)]

pub mod decentraland {
    pub mod common {
        include!(concat!(env!("OUT_DIR"), "/decentraland.common.rs"));
    }
    pub mod kernel {
        pub mod comms {
            pub mod v3 {
                include!(concat!(env!("OUT_DIR"), "/decentraland.kernel.comms.v3.rs"));
            }
        }
    }
}

pub use decentraland::common::Position;
pub use decentraland::kernel::comms::v3 as archipelago;
