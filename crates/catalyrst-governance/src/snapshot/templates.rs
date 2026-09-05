use serde::Deserialize;
use serde_json::Value;

pub const DEFAULT_CHOICES: [&str; 3] = ["yes", "no", "abstain"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalKind {
    Catalyst,
    Hiring,
    Tender,
    Bid,
    LinkedWearables,
    Governance,
    CouncilDecisionVeto,
}

impl ProposalKind {
    pub fn from_path(raw: &str) -> Option<Self> {
        match raw {
            "catalyst" => Some(Self::Catalyst),
            "hiring" => Some(Self::Hiring),
            "tender" => Some(Self::Tender),
            "bid" => Some(Self::Bid),
            "linked-wearables" => Some(Self::LinkedWearables),
            "governance" => Some(Self::Governance),
            "council-decision-veto" => Some(Self::CouncilDecisionVeto),
            _ => None,
        }
    }

    pub fn as_path(&self) -> &'static str {
        match self {
            Self::Catalyst => "catalyst",
            Self::Hiring => "hiring",
            Self::Tender => "tender",
            Self::Bid => "bid",
            Self::LinkedWearables => "linked-wearables",
            Self::Governance => "governance",
            Self::CouncilDecisionVeto => "council-decision-veto",
        }
    }

    pub fn is_pending_on_creation(&self) -> bool {
        matches!(self, Self::Tender)
    }
}

pub struct RenderContext<'a> {
    pub author: &'a str,
    pub governance_url: &'a str,
    pub snapshot_web_url: &'a str,
    pub space_council: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub title: String,
    pub body: String,
}

pub fn render(
    kind: ProposalKind,
    payload: &Value,
    ctx: &RenderContext<'_>,
) -> Result<Rendered, String> {
    let rendered = match kind {
        ProposalKind::Catalyst => catalyst(parse(payload)?)?,
        ProposalKind::Hiring => hiring(parse(payload)?)?,
        ProposalKind::Tender => tender(parse(payload)?, ctx),
        ProposalKind::LinkedWearables => linked_wearables(parse(payload)?),
        ProposalKind::Governance => governance(parse(payload)?, ctx),
        ProposalKind::CouncilDecisionVeto => council_decision_veto(parse(payload)?, ctx)?,
        ProposalKind::Bid => {
            return Err("bid proposals are not submitted to snapshot at creation time".to_string())
        }
    };

    if rendered.title.trim().is_empty() {
        return Err("the proposal payload produced an empty title".to_string());
    }

    Ok(Rendered {
        title: rendered.title,
        body: with_author(&rendered.body, ctx.author),
    })
}

fn parse<T: for<'de> Deserialize<'de>>(payload: &Value) -> Result<T, String> {
    serde_json::from_value(payload.clone()).map_err(|e| format!("invalid proposal payload: {e}"))
}

fn with_author(body: &str, author: &str) -> String {
    format!("> by {author}\n\n{}", body.trim())
}

fn first_line(value: &str) -> String {
    value.split('\n').next().unwrap_or("").trim().to_string()
}

fn markdown(value: &str) -> String {
    value
        .trim()
        .lines()
        .map(demote_heading)
        .collect::<Vec<_>>()
        .join("\n")
}

fn demote_heading(line: &str) -> String {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes >= 3 {
        return line.to_string();
    }
    let rest = &trimmed[hashes..];
    if !rest.starts_with(' ') {
        return line.to_string();
    }
    format!("###{rest}")
}

fn bullet_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("- {item}\n"))
        .collect::<String>()
}

fn linked_proposal(id: &str, governance_url: &str) -> String {
    let base = governance_url.trim_end_matches('/');
    format!("[{id}]({base}/proposal/?id={id})")
}

fn snapshot_proposal_url(web_url: &str, space: &str, id: &str) -> String {
    let base = web_url.trim_end_matches('/');
    format!("{base}/#/{space}/proposal/{id}")
}

#[derive(Debug, Deserialize)]
struct CatalystPayload {
    #[serde(rename = "type")]
    kind: String,
    owner: String,
    domain: String,
    description: String,
}

fn catalyst(p: CatalystPayload) -> Result<Rendered, String> {
    let (title, body_verb) = match p.kind.as_str() {
        "catalyst_add" => (
            format!(
                "Add catalyst node with domain {} to the catalyst network",
                p.domain
            ),
            "added to",
        ),
        "catalyst_remove" => (
            format!(
                "Remove catalyst node with domain {} from the catalyst network",
                p.domain
            ),
            "removed from",
        ),
        other => return Err(format!("unknown catalyst proposal type: {other}")),
    };
    let body = format!(
        "Should the catalyst node with the domain {} and owner {} be {} Decentraland's Catalyst Network?\n\n## Description\n\n{}",
        p.domain,
        p.owner,
        body_verb,
        markdown(&p.description)
    );
    Ok(Rendered { title, body })
}

#[derive(Debug, Deserialize)]
struct HiringPayload {
    #[serde(rename = "type")]
    kind: String,
    committee: String,
    address: String,
    #[serde(default)]
    name: Option<String>,
    reasons: String,
    evidence: String,
}

fn short_address(address: &str) -> String {
    let chars: Vec<char> = address.chars().collect();
    if chars.len() < 10 {
        return address.to_string();
    }
    let prefix: String = chars[..6].iter().collect();
    let suffix: String = chars[chars.len() - 4..].iter().collect();
    format!("{prefix}...{suffix}")
}

fn hiring(p: HiringPayload) -> Result<Rendered, String> {
    let name = p
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| short_address(&p.address));

    let (title, question, reason_verb) = match p.kind.as_str() {
        "hiring_add" => (
            format!("Add {name} to {}", p.committee),
            format!("Should {name} be added to {}?", p.committee),
            "adding",
        ),
        "hiring_remove" => (
            format!("Remove {name} from {}", p.committee),
            format!("Should {name} be removed from {}?", p.committee),
            "removing",
        ),
        other => return Err(format!("unknown hiring proposal type: {other}")),
    };

    let body = format!(
        "{question}\n\n## Address\n{}\n\n## Reasons for {reason_verb}\n{}\n\n## Evidence\n{}",
        p.address,
        markdown(&p.reasons),
        markdown(&p.evidence)
    );
    Ok(Rendered { title, body })
}

#[derive(Debug, Deserialize)]
struct TenderPayload {
    linked_proposal_id: String,
    project_name: String,
    summary: String,
    problem_statement: String,
    technical_specification: String,
    use_cases: String,
    deliverables: String,
    target_release_quarter: String,
}

fn tender(p: TenderPayload, ctx: &RenderContext<'_>) -> Rendered {
    let body = format!(
        "Should funds from the DAO Treasury be allocated to finance a new community-led project addressing issues outlined herein?\n\n\
         ## Linked Pitch Proposal\n{}\n\n\
         ## Summary\n\n{}\n\n\
         ## Problem Statement\n\n{}\n\n\
         ## Technical Specification\n\n{}\n\n\
         ## Use Cases\n\n{}\n\n\
         ## Deliverables\n\n{}\n\n\
         ## Target Release Quarter\n\n{}",
        linked_proposal(&p.linked_proposal_id, ctx.governance_url),
        markdown(&p.summary),
        markdown(&p.problem_statement),
        markdown(&p.technical_specification),
        markdown(&p.use_cases),
        markdown(&p.deliverables),
        p.target_release_quarter
    );
    Rendered {
        title: first_line(&p.project_name),
        body,
    }
}

#[derive(Debug, Deserialize)]
struct GovernancePayload {
    linked_proposal_id: String,
    title: String,
    summary: String,
    r#abstract: String,
    motivation: String,
    specification: String,
    impacts: String,
    implementation_pathways: String,
    conclusion: String,
}

fn governance(p: GovernancePayload, ctx: &RenderContext<'_>) -> Rendered {
    let body = format!(
        "## Linked Draft Proposal\n{}\n\n\
         ## Summary\n\n{}\n\n\
         ## Abstract\n\n{}\n\n\
         ## Motivation\n\n{}\n\n\
         ## Specification\n\n{}\n\n\
         ## Impacts\n\n{}\n\n\
         ## Implementation Pathways\n\n{}\n\n\
         ## Conclusion\n\n{}",
        linked_proposal(&p.linked_proposal_id, ctx.governance_url),
        markdown(&p.summary),
        markdown(&p.r#abstract),
        markdown(&p.motivation),
        markdown(&p.specification),
        markdown(&p.impacts),
        markdown(&p.implementation_pathways),
        markdown(&p.conclusion)
    );
    Rendered {
        title: first_line(&p.title),
        body,
    }
}

#[derive(Debug, Deserialize)]
struct LinkedWearablesPayload {
    name: String,
    marketplace_link: String,
    links: Vec<String>,
    nft_collections: String,
    motivation: String,
    items: u64,
    governance: String,
    smart_contract: Vec<String>,
    managers: Vec<String>,
    programmatically_generated: bool,
    #[serde(default)]
    method: String,
}

fn linked_wearables(p: LinkedWearablesPayload) -> Rendered {
    let contract_plural = if p.smart_contract.len() > 1 { "es" } else { "" };
    let manager_plural = if p.managers.len() > 1 { "es" } else { "" };
    let generated = if p.programmatically_generated {
        "Yes"
    } else {
        "No"
    };
    let method_section = if p.method.trim().is_empty() {
        String::new()
    } else {
        format!("\n\n## Method\n\n{}", markdown(&p.method))
    };

    let body = format!(
        "Should {} be added to the Linked Wearables Registry?\n\n\
         ## NFT Marketplace Listing\n\n{}\n\
         ## Relevant Links\n\n{}\n\
         ## NFT Collections Description\n\n{}\n\n\
         ## Motivation\n\n{}\n\n\
         ## Items to be Uploaded\n\n{}\n\n\
         ## Intellectual Property\n\n{}\n\n\
         ## Smart Contract Address{contract_plural}\n\n{}\n\
         ## Manager Address{manager_plural}\n\n{}\n\
         ## Is this collection generated programmatically?\n- {generated}{method_section}",
        p.name,
        bullet_list(std::slice::from_ref(&p.marketplace_link)),
        bullet_list(&p.links),
        markdown(&p.nft_collections),
        markdown(&p.motivation),
        p.items,
        markdown(&p.governance),
        bullet_list(&p.smart_contract),
        bullet_list(&p.managers),
    );

    Rendered {
        title: format!("Add {} to the Linked Wearables Registry", p.name),
        body,
    }
}

#[derive(Debug, Deserialize)]
struct CouncilVetoPayload {
    #[serde(default)]
    title: Option<String>,
    decision_snapshot_id: String,
    reasons: String,
    #[serde(default)]
    suggestions: Option<String>,
}

fn council_decision_veto(
    p: CouncilVetoPayload,
    ctx: &RenderContext<'_>,
) -> Result<Rendered, String> {
    let decision_title = p
        .title
        .as_deref()
        .map(first_line)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "council veto payload has no `title`: the vetoed council decision's title is required \
             and this server cannot read it from snapshot"
                .to_string()
        })?;

    let space = ctx
        .space_council
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "SNAPSHOT_SPACE_COUNCIL is unset, so the vetoed decision cannot be linked".to_string()
        })?;

    let suggestions = p
        .suggestions
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("\n\n## Suggestions to the Council\n{}", markdown(s)))
        .unwrap_or_default();

    let body = format!(
        "Should we veto the Council Decision \"{decision_title}\"?\n\n\
         ## Decision URL\n{}\n\n\
         ## Reasons to Veto\n{}{suggestions}",
        snapshot_proposal_url(ctx.snapshot_web_url, space, &p.decision_snapshot_id),
        markdown(&p.reasons)
    );

    Ok(Rendered {
        title: format!("Veto \"{decision_title}\""),
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx() -> RenderContext<'static> {
        RenderContext {
            author: "0x1111111111111111111111111111111111111111",
            governance_url: "https://decentraland.org/governance",
            snapshot_web_url: "https://snapshot.org",
            space_council: Some("council.dcl.eth"),
        }
    }

    #[test]
    fn kind_round_trips_through_its_path_segment() {
        for path in [
            "catalyst",
            "hiring",
            "tender",
            "bid",
            "linked-wearables",
            "governance",
            "council-decision-veto",
        ] {
            let kind = ProposalKind::from_path(path).expect("known kind");
            assert_eq!(kind.as_path(), path);
        }
        assert!(ProposalKind::from_path("grant").is_none());
    }

    #[test]
    fn catalyst_add_renders_the_upstream_title_and_body() {
        let payload = json!({
            "request": "add",
            "type": "catalyst_add",
            "owner": "0x2222222222222222222222222222222222222222",
            "domain": "peer.example.org",
            "description": "A new node.",
            "coAuthors": [],
        });
        let out = render(ProposalKind::Catalyst, &payload, &ctx()).expect("render");
        assert_eq!(
            out.title,
            "Add catalyst node with domain peer.example.org to the catalyst network"
        );
        assert!(out.body.starts_with(
            "> by 0x1111111111111111111111111111111111111111\n\nShould the catalyst node with the domain peer.example.org and owner 0x2222222222222222222222222222222222222222 be added to Decentraland's Catalyst Network?"
        ));
        assert!(out.body.contains("## Description\n\nA new node."));
    }

    #[test]
    fn catalyst_remove_flips_the_verbs() {
        let payload = json!({
            "type": "catalyst_remove",
            "owner": "0x2222222222222222222222222222222222222222",
            "domain": "peer.example.org",
            "description": "Gone.",
        });
        let out = render(ProposalKind::Catalyst, &payload, &ctx()).expect("render");
        assert_eq!(
            out.title,
            "Remove catalyst node with domain peer.example.org from the catalyst network"
        );
        assert!(out
            .body
            .contains("be removed from Decentraland's Catalyst Network?"));
    }

    #[test]
    fn an_unknown_catalyst_type_is_refused() {
        let payload = json!({
            "type": "catalyst_maybe",
            "owner": "0x2222222222222222222222222222222222222222",
            "domain": "peer.example.org",
            "description": "?",
        });
        let err = render(ProposalKind::Catalyst, &payload, &ctx()).unwrap_err();
        assert!(err.contains("unknown catalyst proposal type"), "got: {err}");
    }

    #[test]
    fn hiring_without_a_name_shortens_the_address() {
        let payload = json!({
            "type": "hiring_add",
            "committee": "Security Advisory Board",
            "address": "0x3333333333333333333333333333333333333333",
            "reasons": "Because.",
            "evidence": "Links.",
        });
        let out = render(ProposalKind::Hiring, &payload, &ctx()).expect("render");
        assert_eq!(out.title, "Add 0x3333...3333 to Security Advisory Board");
        assert!(out.body.contains("## Reasons for adding\nBecause."));
    }

    #[test]
    fn a_payload_missing_a_required_field_is_refused() {
        let err = render(
            ProposalKind::Hiring,
            &json!({ "type": "hiring_add" }),
            &ctx(),
        )
        .unwrap_err();
        assert!(err.contains("invalid proposal payload"), "got: {err}");
    }

    #[test]
    fn tender_titles_from_the_first_line_of_the_project_name() {
        let payload = json!({
            "type": "tender",
            "linked_proposal_id": "abc-123",
            "project_name": "Better Roads\nsecond line",
            "summary": "s",
            "problem_statement": "p",
            "technical_specification": "t",
            "use_cases": "u",
            "deliverables": "d",
            "target_release_quarter": "Q4 2026",
            "coAuthors": [],
        });
        let out = render(ProposalKind::Tender, &payload, &ctx()).expect("render");
        assert_eq!(out.title, "Better Roads");
        assert!(out.body.contains(
            "## Linked Pitch Proposal\n[abc-123](https://decentraland.org/governance/proposal/?id=abc-123)"
        ));
    }

    #[test]
    fn linked_wearables_pluralises_address_headings() {
        let payload = json!({
            "type": "linked_wearables",
            "name": "Acme",
            "marketplace_link": "https://opensea.io/acme",
            "links": ["https://acme.example"],
            "image_previews": [],
            "nft_collections": "c",
            "motivation": "m",
            "items": 12,
            "governance": "g",
            "smart_contract": ["0xaaa", "0xbbb"],
            "managers": ["0xccc"],
            "programmatically_generated": false,
            "method": "",
            "coAuthors": [],
        });
        let out = render(ProposalKind::LinkedWearables, &payload, &ctx()).expect("render");
        assert_eq!(out.title, "Add Acme to the Linked Wearables Registry");
        assert!(out.body.contains("## Smart Contract Addresses"));
        assert!(out.body.contains("## Manager Address\n"));
        assert!(out.body.contains("- No"));
        assert!(!out.body.contains("## Method"));
    }

    #[test]
    fn governance_renders_every_body_section() {
        let payload = json!({
            "type": "governance",
            "linked_proposal_id": "draft-1",
            "title": "A policy",
            "summary": "s",
            "abstract": "a",
            "motivation": "m",
            "specification": "sp",
            "impacts": "i",
            "implementation_pathways": "ip",
            "conclusion": "c",
            "coAuthors": [],
        });
        let out = render(ProposalKind::Governance, &payload, &ctx()).expect("render");
        assert_eq!(out.title, "A policy");
        for heading in [
            "## Summary",
            "## Abstract",
            "## Motivation",
            "## Specification",
            "## Impacts",
            "## Implementation Pathways",
            "## Conclusion",
        ] {
            assert!(out.body.contains(heading), "missing {heading}");
        }
    }

    #[test]
    fn council_veto_without_a_decision_title_is_refused() {
        let payload = json!({
            "type": "council_decision_veto",
            "decision_snapshot_id": "0xdead",
            "reasons": "r",
            "coAuthors": [],
        });
        let err = render(ProposalKind::CouncilDecisionVeto, &payload, &ctx()).unwrap_err();
        assert!(err.contains("no `title`"), "got: {err}");
    }

    #[test]
    fn council_veto_links_the_decision_in_the_council_space() {
        let payload = json!({
            "type": "council_decision_veto",
            "title": "Fund the thing",
            "decision_snapshot_id": "0xdead",
            "reasons": "r",
            "suggestions": "s",
        });
        let out = render(ProposalKind::CouncilDecisionVeto, &payload, &ctx()).expect("render");
        assert_eq!(out.title, "Veto \"Fund the thing\"");
        assert!(out
            .body
            .contains("https://snapshot.org/#/council.dcl.eth/proposal/0xdead"));
        assert!(out.body.contains("## Suggestions to the Council"));
    }

    #[test]
    fn bid_is_not_a_snapshot_write() {
        let err = render(ProposalKind::Bid, &json!({}), &ctx()).unwrap_err();
        assert!(err.contains("not submitted to snapshot"), "got: {err}");
    }

    #[test]
    fn author_supplied_headings_are_demoted_below_the_section_headings() {
        assert_eq!(markdown("# Title\ntext"), "### Title\ntext");
        assert_eq!(markdown("## Title"), "### Title");
        assert_eq!(markdown("### Title"), "### Title");
        assert_eq!(markdown("#nothashed"), "#nothashed");
    }
}
