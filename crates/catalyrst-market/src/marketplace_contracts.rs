//! Direct port of `marketplace-server/src/logic/contracts.ts:getMarketplaceContracts`.
//!
//! Hardcoded LAND/Estate/Name + 40-ish historical mainnet wearable-collection
//! contracts that pre-date the on-chain registry. The upstream also defines
//! Goerli/Sepolia tables for those same names; both are dev-only and the
//! catalyrst-market deployment runs against mainnet only, so we omit them.
//! Re-add by mirroring the TS switch arm if a test needs them.

use crate::dcl_schemas::{ChainId, Contract, Network, NftCategory};

pub fn get_marketplace_contracts(chain_id: ChainId) -> Vec<Contract> {
    match chain_id {
        ChainId::EthereumMainnet => ethereum_mainnet_contracts(),
        _ => Vec::new(),
    }
}

pub fn get_collection_store_address(polygon_chain_id: ChainId) -> &'static str {
    match polygon_chain_id {
        ChainId::MaticMainnet => "0x214ffc0f0103735728dc66b61a22e4f163e275ae",
        ChainId::MaticAmoy => "0xe36abc9ec616c83caaa386541380829106149d68",
        _ => "",
    }
}

fn ethereum_mainnet_contracts() -> Vec<Contract> {
    use NftCategory::*;
    const ETH: ChainId = ChainId::EthereumMainnet;
    const NET: Network = Network::Ethereum;

    let mk = |name: &str, address: &str, category: NftCategory| Contract {
        name: name.to_string(),
        address: address.to_string(),
        category,
        network: NET,
        chain_id: ETH,
    };

    vec![
        mk("LAND", "0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d", Parcel),
        mk("Estates", "0x959e104e1a4db6317fa58f8295f586e1a978c297", Estate),
        mk("Names", "0x2a187453064356c898cae034eaed119e1663acb8", Ens),
        mk("Atari Launch", "0x4c290f486bae507719c562b6b524bdb71a2570c9", Wearable),
        mk("Binance Us", "0xa8ee490e4c4da48cc1653502c1a77479d4d818de", Wearable),
        mk("China Flying", "0x90958d4531258ca11d18396d4174a007edbc2b42", Wearable),
        mk("Community Contest", "0x32b7495895264ac9d0b12d32afd435453458b1c6", Wearable),
        mk("Cybermike CyberSoldier Set", "0x24d538a6265b006d4b53c45ba91af5ef60dca6cb", Wearable),
        mk("CZ Mercenary MTZ", "0xc3ca6c364b854fd0a653a43f8344f8c22ddfdd26", Wearable),
        mk("Dappcraft Moonminer", "0x1e1d4e6262787c8a8783a37fee698bd42aa42bec", Wearable),
        mk("DCG", "0x3163d2cfee3183f9874e2869942cc62649eeb004", Wearable),
        mk("DCL Launch", "0xd35147be6401dcb20811f2104c33de8e97ed6818", Wearable),
        mk("DC Meta", "0xe7a64f6a239ed7f5bf18caa1ce2920d0c1278129", Wearable),
        mk("DC Niftyblocksmith", "0x102daabd1e9d294d4436ec4c521dce7b1f15499e", Wearable),
        mk("DG Fall 2020", "0x7038e9d2c6f5f84469a84cf9bc5f4909bb6ac5e0", Wearable),
        mk("DG Summer", "0xbf53c33235cbfc22cef5a61a83484b86342679c5", Wearable),
        mk("Dgtble Headspace", "0x574f64ac2e7215cba9752b85fc73030f35166bc0", Wearable),
        mk("Digital Alchemy", "0x5cf39e64392c615fd8086838883958752a11b486", Wearable),
        mk("Ethermon Wearables", "0x54266bcf2ffa841af934f003d144957d5934f3ab", Wearable),
        mk("Exclusive Masks", "0xc04528c14c8ffd84c7c1fb6719b4a89853035cdd", Wearable),
        mk("Dillon Francis Atari", "0x51e0b1afe5da0c038fc93a3fc8e11cf7a238b40b", Wearable),
        mk("Halloween 2019", "0xc1f4b0eea2bd6690930e6c66efd3e197d620b9c2", Wearable),
        mk("Halloween 2020", "0xfeb52cbf71b9adac957c6f948a6cf9980ac8c907", Wearable),
        mk("MCH", "0xf64dc33a192e056bb5f0e5049356a0498b502d50", Wearable),
        mk("Meme don't buy this", "0x1a57f6afc902d25792c53b8f19b7e17ef84222d5", Wearable),
        mk("MF Sammichgamer", "0x30d3387ff3de2a21bef7032f82d00ff7739e403c", Wearable),
        mk("ML Liondance", "0x0b1c6c75d511fae05e7dc696f4cf14129a9c43c9", Wearable),
        mk("ML Pekingopera", "0x60d8271c501501c4b8cd9ed5343ac59d1b79d993", Wearable),
        mk("Moonshot", "0x6a99abebb48819d2abe92c5e4dc4f48dc09a3ee8", Wearable),
        mk("PM Dreamverse Eminence", "0x09305998a531fade369ebe30adf868c96a34e813", Wearable),
        mk("PM Outtathisworld", "0x75a3752579dc2d63ca229eebbe3537fbabf85a12", Wearable),
        mk("RAC Basics", "0x68e139552c4077ce5c9ab929c7e18ca721ffff00", Wearable),
        mk("Release the Kraken", "0xffc5043d9a00865d089d5eefa5b3d1625aec6763", Wearable),
        mk("RTFKT X Atari", "0x6b47e7066c7db71aa04a1d5872496fe05c4c331f", Wearable),
        mk("Stay Safe", "0x201c3af8c471e5842428b74d1e7c0249adda2a92", Wearable),
        mk("Sugarclub Yumi", "0xb5d14052d1e2bce2a2d7459d0379256e632b855d", Wearable),
        mk("Tech Tribal Marc0matic", "0x480a0f4e360e8964e68858dd231c2922f1df45ef", Wearable),
        mk("3LAUBasics", "0xe1ecb4e5130f493551c7d6df96ad19e5b431a0a9", Wearable),
        mk("Winklevoss Capital", "0xc82a864a94db3550bc71fcb4ce07228bcec21f1a", Wearable),
        mk("Wonderzone Meteorcharser", "0x34ed0aa248f60f54dd32fbc9883d6137a491f4f3", Wearable),
        mk("Wonderzone Steampunk", "0xb96697fa4a3361ba35b774a42c58daccaad1b8e1", Wearable),
        mk("WZ Wonderbot", "0x5df4602e7f38a91ea7724fc167f0c67f61604b1e", Wearable),
        mk("Xmas2019", "0xc3af02c0fd486c8e9da5788b915d6fff3f049866", Wearable),
        mk("Xmas2020", "0xecf073f91101ce5628669c487aee8f5822a101b1", Wearable),
        mk("XmashUp", "0xdd9c7bc159dacb19c9f6b9d7e23948c87aa2397f", Wearable),
    ]
}
