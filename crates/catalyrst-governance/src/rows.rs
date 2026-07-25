use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExtraKeys(pub Map<String, Value>);

#[cfg(feature = "ts")]
impl ts_rs::TS for ExtraKeys {
    type WithoutGenerics = Self;

    fn decl() -> String {
        panic!("ExtraKeys is only used inline, it cannot be declared")
    }

    fn decl_concrete() -> String {
        panic!("ExtraKeys is only used inline, it cannot be declared")
    }

    fn name() -> String {
        "Record<string, unknown>".to_owned()
    }

    fn inline() -> String {
        Self::name()
    }

    fn inline_flattened() -> String {
        Self::name()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ProposalRow {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub proposal_type: String,
    pub status: String,
    pub user: String,
    pub start_at: String,
    pub finish_at: String,
    pub created_at: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub required_to_pass: Number,
    pub snapshot_id: String,
    pub configuration: ProposalConfiguration,
    #[serde(flatten)]
    pub extra: ExtraKeys,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ProposalConfiguration {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub description: Option<String>,
    #[serde(rename = "abstract")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub abstract_: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional, type = "number"))]
    pub size: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub beneficiary: Option<String>,
    #[serde(rename = "paymentToken")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub payment_token: Option<String>,
    #[serde(flatten)]
    pub extra: ExtraKeys,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ProjectRow {
    pub id: String,
    pub proposal_id: String,
    pub title: String,
    pub status: String,
    #[serde(rename = "type")]
    pub project_type: String,
    pub author: String,
    pub configuration: ProjectConfiguration,
    pub funding: ProjectFunding,
    pub latest_update: ProjectLatestUpdate,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: Number,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: Number,
    #[serde(flatten)]
    pub extra: ExtraKeys,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ProjectConfiguration {
    pub category: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub size: Number,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub beneficiary: Option<String>,
    #[serde(flatten)]
    pub extra: ExtraKeys,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ProjectFunding {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub enacted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub vesting: Option<ProjectVesting>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub one_time_payment: Option<ProjectOneTimePayment>,
    #[serde(flatten)]
    pub extra: ExtraKeys,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ProjectVesting {
    pub start_at: String,
    pub finish_at: String,
    pub token: String,
    pub status: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: Number,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub vested: Number,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub released: Number,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub releasable: Number,
    #[serde(flatten)]
    pub extra: ExtraKeys,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ProjectOneTimePayment {
    pub enacting_tx: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional, type = "number"))]
    pub tx_amount: Option<Number>,
    #[serde(flatten)]
    pub extra: ExtraKeys,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ProjectLatestUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub update: Option<ProjectUpdateSummary>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub update_timestamp: Number,
    #[serde(flatten)]
    pub extra: ExtraKeys,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ProjectUpdateSummary {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub index: Number,
    pub introduction: Option<String>,
    pub health: Option<String>,
    pub completion_date: Option<String>,
    #[serde(flatten)]
    pub extra: ExtraKeys,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct BudgetRow {
    pub id: String,
    pub start_at: String,
    pub finish_at: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: Number,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub allocated: Number,
    pub categories: std::collections::BTreeMap<String, BudgetCategory>,
    #[serde(flatten)]
    pub extra: ExtraKeys,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct BudgetCategory {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: Number,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub allocated: Number,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub available: Number,
    #[serde(flatten)]
    pub extra: ExtraKeys,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PROPOSALS_CAPTURE: &str = include_str!("../testdata/gov-proposals.json");
    const PROJECTS_CAPTURE: &str = include_str!("../testdata/gov-projects.json");
    const BUDGETS_CAPTURE: &str = include_str!("../testdata/gov-budgets.json");
    const GRANT_PROPOSAL_ROW: &str = include_str!("../testdata/gov-proposal-grant-row.json");

    fn rows_of(capture: &str) -> Vec<Value> {
        let envelope: Value = serde_json::from_str(capture).expect("capture parses");
        envelope["data"].as_array().expect("data array").clone()
    }

    #[test]
    fn wire_identity_proposal_rows_roundtrip() {
        let rows = rows_of(PROPOSALS_CAPTURE);
        assert!(!rows.is_empty());
        for original in rows {
            let parsed: ProposalRow =
                serde_json::from_value(original.clone()).expect("proposal row parses");
            assert_eq!(serde_json::to_value(&parsed).unwrap(), original);
            assert!(!parsed.extra.0.contains_key("id"));
            assert!(!parsed.extra.0.contains_key("type"));
            assert!(parsed.extra.0.contains_key("textsearch"));
        }
    }

    #[test]
    fn wire_identity_proposal_row_grant_configuration() {
        let original: Value = serde_json::from_str(GRANT_PROPOSAL_ROW).unwrap();
        let parsed: ProposalRow = serde_json::from_value(original.clone()).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), original);
        let cfg = &parsed.configuration;
        assert!(cfg.description.is_some());
        assert!(cfg.abstract_.is_some());
        assert_eq!(cfg.category.as_deref(), Some("Platform"));
        assert_eq!(cfg.tier.as_deref(), Some("Lower Tier"));
        assert_eq!(cfg.size, Some(Number::from(5001)));
        assert!(cfg.beneficiary.is_some());
        assert_eq!(cfg.payment_token.as_deref(), Some("MANA"));
    }

    #[test]
    fn wire_identity_project_rows_roundtrip() {
        let rows = rows_of(PROJECTS_CAPTURE);
        assert!(!rows.is_empty());
        let mut saw_vesting = false;
        let mut saw_otp = false;
        let mut saw_update = false;
        let mut saw_no_update = false;
        for original in rows {
            let parsed: ProjectRow =
                serde_json::from_value(original.clone()).expect("project row parses");
            assert_eq!(serde_json::to_value(&parsed).unwrap(), original);
            saw_vesting |= parsed.funding.vesting.is_some();
            saw_otp |= parsed.funding.one_time_payment.is_some();
            saw_update |= parsed.latest_update.update.is_some();
            saw_no_update |= parsed.latest_update.update.is_none();
        }
        assert!(saw_vesting && saw_otp && saw_update && saw_no_update);
    }

    #[test]
    fn wire_identity_budget_rows_roundtrip() {
        let rows = rows_of(BUDGETS_CAPTURE);
        assert!(!rows.is_empty());
        for original in rows {
            let parsed: BudgetRow =
                serde_json::from_value(original.clone()).expect("budget row parses");
            assert_eq!(serde_json::to_value(&parsed).unwrap(), original);
            assert!(!parsed.categories.is_empty());
        }
    }

    #[test]
    fn wire_identity_project_row_minimal_dto() {
        let row = ProjectRow {
            id: "p-1".into(),
            proposal_id: "prop-1".into(),
            title: "T".into(),
            status: "pending".into(),
            project_type: "grant".into(),
            author: "0xabc".into(),
            configuration: ProjectConfiguration {
                category: "Platform".into(),
                size: Number::from(5000),
                tier: None,
                beneficiary: None,
                extra: ExtraKeys::default(),
            },
            funding: ProjectFunding {
                enacted_at: None,
                vesting: None,
                one_time_payment: None,
                extra: ExtraKeys::default(),
            },
            latest_update: ProjectLatestUpdate {
                update: None,
                update_timestamp: Number::from(0),
                extra: ExtraKeys::default(),
            },
            created_at: Number::from(1_624_379_340_618_i64),
            updated_at: Number::from(1_626_106_858_279_i64),
            extra: ExtraKeys::default(),
        };
        let old = json!({
            "id": "p-1",
            "proposal_id": "prop-1",
            "title": "T",
            "status": "pending",
            "type": "grant",
            "author": "0xabc",
            "configuration": { "category": "Platform", "size": 5000 },
            "funding": {},
            "latest_update": { "update_timestamp": 0 },
            "created_at": 1_624_379_340_618_i64,
            "updated_at": 1_626_106_858_279_i64,
        });
        assert_eq!(serde_json::to_value(&row).unwrap(), old);
    }

    #[test]
    fn wire_identity_update_summary_null_vs_absent() {
        let original = json!({
            "index": 2,
            "introduction": null,
            "health": null,
            "completion_date": null,
            "id": "u-1",
            "status": "pending"
        });
        let parsed: ProjectUpdateSummary = serde_json::from_value(original.clone()).unwrap();
        assert!(parsed.introduction.is_none());
        assert_eq!(serde_json::to_value(&parsed).unwrap(), original);
    }
}
