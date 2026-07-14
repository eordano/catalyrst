use alloy::primitives::{Address, B256};
use catalyrst_crypto::eip712::{
    hash_array_of_structs, hash_dynamic, struct_hash as hash_struct, typed_data_digest,
    word_address, word_u64,
};
use serde_json::{json, Value};

pub const DOMAIN_NAME: &str = "snapshot";
pub const DOMAIN_VERSION: &str = "0.1.4";

pub const VOTING_TYPE: &str = "single-choice";
pub const APP_NAME: &str = "decentraland-governance";

pub const PROPOSAL_FIELDS: [(&str, &str); 13] = [
    ("from", "address"),
    ("space", "string"),
    ("timestamp", "uint64"),
    ("type", "string"),
    ("title", "string"),
    ("body", "string"),
    ("discussion", "string"),
    ("choices", "string[]"),
    ("start", "uint64"),
    ("end", "uint64"),
    ("snapshot", "uint64"),
    ("plugins", "string"),
    ("app", "string"),
];

const DOMAIN_TYPE: &str = "EIP712Domain(string name,string version)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalMessage {
    pub from: Address,
    pub space: String,
    pub timestamp: u64,
    pub voting_type: String,
    pub title: String,
    pub body: String,
    pub discussion: String,
    pub choices: Vec<String>,
    pub start: u64,
    pub end: u64,
    pub snapshot: u64,
    pub plugins: String,
    pub app: String,
}

pub fn proposal_type_string() -> String {
    let fields = PROPOSAL_FIELDS
        .iter()
        .map(|(name, ty)| format!("{ty} {name}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("Proposal({fields})")
}

pub fn domain_json() -> Value {
    json!({ "name": DOMAIN_NAME, "version": DOMAIN_VERSION })
}

pub fn types_json() -> Value {
    let fields: Vec<Value> = PROPOSAL_FIELDS
        .iter()
        .map(|(name, ty)| json!({ "name": name, "type": ty }))
        .collect();
    json!({ "Proposal": fields })
}

pub fn domain_separator() -> B256 {
    B256::from(hash_struct(
        hash_dynamic(DOMAIN_TYPE.as_bytes()),
        &[
            hash_dynamic(DOMAIN_NAME.as_bytes()),
            hash_dynamic(DOMAIN_VERSION.as_bytes()),
        ],
    ))
}

impl ProposalMessage {
    pub fn struct_hash(&self) -> B256 {
        let choices: Vec<[u8; 32]> = self
            .choices
            .iter()
            .map(|choice| hash_dynamic(choice.as_bytes()))
            .collect();
        B256::from(hash_struct(
            hash_dynamic(proposal_type_string().as_bytes()),
            &[
                word_address(self.from),
                hash_dynamic(self.space.as_bytes()),
                word_u64(self.timestamp),
                hash_dynamic(self.voting_type.as_bytes()),
                hash_dynamic(self.title.as_bytes()),
                hash_dynamic(self.body.as_bytes()),
                hash_dynamic(self.discussion.as_bytes()),
                hash_array_of_structs(&choices),
                word_u64(self.start),
                word_u64(self.end),
                word_u64(self.snapshot),
                hash_dynamic(self.plugins.as_bytes()),
                hash_dynamic(self.app.as_bytes()),
            ],
        ))
    }

    pub fn digest(&self) -> B256 {
        B256::from(typed_data_digest(
            domain_separator().0,
            self.struct_hash().0,
        ))
    }

    pub fn message_json(&self) -> Value {
        json!({
            "from": self.from.to_checksum(None),
            "space": self.space,
            "timestamp": self.timestamp,
            "type": self.voting_type,
            "title": self.title,
            "body": self.body,
            "discussion": self.discussion,
            "choices": self.choices,
            "start": self.start,
            "end": self.end,
            "snapshot": self.snapshot,
            "plugins": self.plugins,
            "app": self.app,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAPSHOT_JS_PROPOSAL_TYPE: &str = "Proposal(address from,string space,uint64 timestamp,string type,string title,string body,string discussion,string[] choices,uint64 start,uint64 end,uint64 snapshot,string plugins,string app)";

    #[test]
    fn encode_type_matches_snapshot_js() {
        assert_eq!(proposal_type_string(), SNAPSHOT_JS_PROPOSAL_TYPE);
    }

    fn golden_message() -> ProposalMessage {
        ProposalMessage {
            from: "0x1a642f0E3c3aF545E7AcBD38b07251B3990914F1"
                .parse()
                .expect("poster address"),
            space: "gate.dcl.eth".to_string(),
            timestamp: 1_700_000_040,
            voting_type: VOTING_TYPE.to_string(),
            title: "Add catalyst node with domain peer.example.org to the catalyst network"
                .to_string(),
            body: "> by 0x1111111111111111111111111111111111111111\n\nShould the catalyst node with the domain peer.example.org and owner 0x3333333333333333333333333333333333333333 be added to Decentraland's Catalyst Network?\n\n## Description\n\nA new node for the network."
                .to_string(),
            discussion: String::new(),
            choices: vec!["yes".to_string(), "no".to_string(), "abstain".to_string()],
            start: 1_700_000_040,
            end: 1_700_000_640,
            snapshot: 22_000_000,
            plugins: "{}".to_string(),
            app: APP_NAME.to_string(),
        }
    }

    #[test]
    fn the_domain_separator_matches_ethers() {
        assert_eq!(
            format!("{:#x}", domain_separator()),
            "0x484fce18f892e8535a4b6700e197a8026f4213f809d23ae117da03b497e18670"
        );
    }

    #[test]
    fn the_struct_hash_and_digest_match_ethers_over_snapshot_js_types() {
        let message = golden_message();
        assert_eq!(
            format!("{:#x}", message.struct_hash()),
            "0xb065c4ade9b2bb4675be9e9a99630a04bd43e17b32a00191cd9e5c9b802819c3"
        );
        assert_eq!(
            format!("{:#x}", message.digest()),
            "0x9e426671d3aaae26c4c9f72f60a68553900999dcd35f75f090778c90f5c60c25"
        );
    }

    #[test]
    fn types_json_field_order_matches_the_encode_type() {
        let types = types_json();
        let fields = types["Proposal"].as_array().expect("Proposal array");
        assert_eq!(fields.len(), PROPOSAL_FIELDS.len());
        for (idx, (name, ty)) in PROPOSAL_FIELDS.iter().enumerate() {
            assert_eq!(fields[idx]["name"], *name);
            assert_eq!(fields[idx]["type"], *ty);
        }
    }
}
