#![allow(clippy::all)]
#![allow(unused_imports)]
#![allow(unused_qualifications)]

pub mod decentraland {
    pub mod common {
        include!(concat!(env!("OUT_DIR"), "/decentraland.common.rs"));
    }
    pub mod social_service {
        include!(concat!(env!("OUT_DIR"), "/decentraland.social_service.rs"));
        pub mod v2 {
            include!(concat!(env!("OUT_DIR"), "/decentraland.social_service.v2.rs"));
        }
    }
}

pub use decentraland::common;
pub use decentraland::social_service as errors;
pub use decentraland::social_service::v2;
