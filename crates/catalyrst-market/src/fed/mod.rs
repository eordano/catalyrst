pub mod apply;
pub mod authority;
pub mod ids;
pub mod messages;
pub mod replay;

use catalyrst_fed::sig::Eip712Domain;

pub fn market_domain() -> Eip712Domain {
    Eip712Domain {
        name: "DecentralandMarket".into(),
        version: "1".into(),
        chain_id: 137,
        verifying_contract: "0x0000000000000000000000000000000000000000".into(),
    }
}
