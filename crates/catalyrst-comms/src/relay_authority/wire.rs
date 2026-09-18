#[derive(Clone, PartialEq, prost::Message)]
pub struct AuthorityRequest {
    #[prost(oneof = "authority_request::Operation", tags = "1, 2, 3")]
    pub operation: Option<authority_request::Operation>,
}

pub mod authority_request {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Operation {
        #[prost(message, tag = "1")]
        Join(super::Join),
        #[prost(message, tag = "2")]
        Renew(super::Renew),
        #[prost(message, tag = "3")]
        Release(super::Release),
    }
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Join {
    #[prost(bytes = "vec", tag = "1")]
    pub pulse_instance: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub connection_generation: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub wallet: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    pub session: Vec<u8>,
    #[prost(string, tag = "5")]
    pub header_payload: String,
    #[prost(bytes = "vec", tag = "6")]
    pub nonce: Vec<u8>,
    #[prost(bytes = "vec", tag = "7")]
    pub proof: Vec<u8>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Renew {
    #[prost(bytes = "vec", tag = "1")]
    pub pulse_instance: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub connection_generation: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub lease_id: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    pub authority_incarnation: Vec<u8>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Release {
    #[prost(bytes = "vec", tag = "1")]
    pub pulse_instance: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub connection_generation: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub lease_id: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    pub authority_incarnation: Vec<u8>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct AuthorityResponse {
    #[prost(oneof = "authority_response::Result", tags = "1, 2, 3")]
    pub result: Option<authority_response::Result>,
}

pub mod authority_response {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Result {
        #[prost(message, tag = "1")]
        Lease(super::Lease),
        #[prost(message, tag = "2")]
        Denied(super::Denied),
        #[prost(message, tag = "3")]
        Released(super::Released),
    }
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Lease {
    #[prost(bytes = "vec", tag = "1")]
    pub lease_id: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub authority_incarnation: Vec<u8>,
    #[prost(uint32, tag = "3")]
    pub ttl_ms: u32,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Denied {
    #[prost(enumeration = "denied::Reason", tag = "1")]
    pub reason: i32,
}

pub mod denied {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
    #[repr(i32)]
    pub enum Reason {
        Unspecified = 0,
        NotAuthorized = 1,
        Expired = 2,
        Capacity = 3,
        Unavailable = 4,
        Invalid = 5,
    }
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Released {}
