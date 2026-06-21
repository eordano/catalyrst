use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Default, Clone)]
pub struct SectionRules {
    pub ignore: Vec<String>,
    pub ignore_whole_response: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Volatility {
    sections: HashMap<String, SectionRules>,
}

impl Volatility {
    pub fn load_or_default(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => match parse_toml(&contents) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!(
                        "  ~ volatility.toml parse error ({}), using built-in defaults",
                        e
                    );
                    Self::defaults()
                }
            },
            Err(_) => Self::defaults(),
        }
    }

    pub fn defaults() -> Self {
        let mut sections = HashMap::new();
        sections.insert(
            "about".to_string(),
            SectionRules {
                ignore: vec![
                    "configurations.realmName".to_string(),
                    "configurations.networkId".to_string(),
                    "content.publicUrl".to_string(),
                    "lambdas.publicUrl".to_string(),
                    "comms.publicUrl".to_string(),
                    "comms.adapter".to_string(),
                    "bff.publicUrl".to_string(),
                    "comms".to_string(),
                ],
                ignore_whole_response: false,
            },
        );
        sections.insert(
            "status".to_string(),
            SectionRules {
                ignore: vec![
                    "lastSyncWithDAO".to_string(),
                    "lastDeploymentAt".to_string(),
                    "synchronizationStatus.lastSyncWithOtherServers[].lastSyncTimestamp"
                        .to_string(),
                    "synchronizationStatus.lastSyncWithOtherServers[].lastDeploymentTimestamp"
                        .to_string(),
                    "synchronizationStatus.lastSyncWithOtherServers[].url".to_string(),
                    "synchronizationStatus.otherServers[]".to_string(),
                    "synchronizationStatus.lastHeartbeat".to_string(),
                    "synchronizationStatus.syncFrontier".to_string(),
                    "synchronizationStatus.up".to_string(),
                ],
                ignore_whole_response: false,
            },
        );
        sections.insert(
            "challenge".to_string(),
            SectionRules {
                ignore: vec!["challengeText".to_string()],
                ignore_whole_response: false,
            },
        );
        sections.insert(
            "audit".to_string(),
            SectionRules {
                ignore: vec!["localTimestamp".to_string()],
                ignore_whole_response: false,
            },
        );
        sections.insert(
            "snapshots".to_string(),
            SectionRules {
                ignore: vec![],
                ignore_whole_response: true,
            },
        );
        sections.insert(
            "failed-deployments".to_string(),
            SectionRules {
                ignore: vec![],
                ignore_whole_response: true,
            },
        );
        sections.insert(
            "deployments".to_string(),
            SectionRules {
                ignore: vec![
                    "deployments[].localTimestamp".to_string(),
                    "pagination.next".to_string(),
                    "pagination.self".to_string(),
                ],
                ignore_whole_response: false,
            },
        );
        sections.insert(
            "lambdas-status".to_string(),
            SectionRules {
                ignore: vec![
                    "version".to_string(),
                    "currentTime".to_string(),
                    "commitHash".to_string(),
                ],
                ignore_whole_response: false,
            },
        );
        Self { sections }
    }

    pub fn is_ignored(&self, section: &str, json_path: &str) -> bool {
        let Some(rules) = self.sections.get(section) else {
            return false;
        };
        rules.ignore.iter().any(|pat| glob_match(pat, json_path))
    }

    pub fn ignore_whole(&self, section: &str) -> bool {
        self.sections
            .get(section)
            .map(|r| r.ignore_whole_response)
            .unwrap_or(false)
    }
}

fn glob_match(pattern: &str, path: &str) -> bool {
    if matches_inner(pattern, path) {
        return true;
    }
    if let Some(stripped) = path.split_once('.').map(|(_, rest)| rest) {
        if matches_inner(pattern, stripped) {
            return true;
        }
    }
    false
}

fn matches_inner(pattern: &str, path: &str) -> bool {
    let p_segs = tokenize(pattern);
    let v_segs = tokenize(path);
    if p_segs.len() != v_segs.len() {
        return false;
    }
    for (ps, vs) in p_segs.iter().zip(v_segs.iter()) {
        if !segment_match(ps, vs) {
            return false;
        }
    }
    true
}

fn tokenize(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch == '.' {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn segment_match(pattern: &str, segment: &str) -> bool {
    let (p_name, p_idx) = split_bracket(pattern);
    let (v_name, v_idx) = split_bracket(segment);

    let name_ok = p_name == "*" || p_name == v_name;
    if !name_ok {
        return false;
    }
    match (p_idx, v_idx) {
        (None, None) => true,
        (Some(""), Some(_)) => true,
        (Some(p), Some(v)) => p == v,
        _ => false,
    }
}

fn split_bracket(s: &str) -> (&str, Option<&str>) {
    if let Some(open) = s.find('[') {
        if let Some(close_rel) = s[open..].find(']') {
            let close = open + close_rel;
            let name = &s[..open];
            let idx = &s[open + 1..close];
            return (name, Some(idx));
        }
    }
    (s, None)
}

fn parse_toml(input: &str) -> Result<Volatility, String> {
    let mut sections: HashMap<String, SectionRules> = HashMap::new();
    let mut current: Option<String> = None;

    let logical_lines = fold_arrays(input);

    for (lineno, line) in logical_lines {
        let line = strip_comment(&line).trim().to_string();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') && !line.contains('=') {
            let name = line[1..line.len() - 1].trim().to_string();
            sections.entry(name.clone()).or_default();
            current = Some(name);
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("line {}: expected key=value", lineno));
        };
        let key = key.trim();
        let value = value.trim();

        let section = current
            .as_ref()
            .ok_or_else(|| format!("line {}: key outside any [section]", lineno))?;
        let rules = sections.entry(section.clone()).or_default();

        match key {
            "ignore" => {
                rules.ignore =
                    parse_string_array(value).map_err(|e| format!("line {}: {}", lineno, e))?;
            }
            "ignore_whole_response" => {
                rules.ignore_whole_response = value == "true";
            }
            other => {
                return Err(format!("line {}: unknown key '{}'", lineno, other));
            }
        }
    }
    Ok(Volatility { sections })
}

fn fold_arrays(input: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut buf = String::new();
    let mut buf_start_lineno = 0usize;
    let mut depth = 0i32;

    for (idx, raw) in input.lines().enumerate() {
        let lineno = idx + 1;
        if depth == 0 {
            buf.clear();
            buf.push_str(raw);
            buf_start_lineno = lineno;
        } else {
            buf.push(' ');
            buf.push_str(raw);
        }

        let stripped = strip_comment(raw);
        let mut in_str = false;
        for ch in stripped.chars() {
            match ch {
                '"' => in_str = !in_str,
                '[' if !in_str => depth += 1,
                ']' if !in_str => depth -= 1,
                _ => {}
            }
        }

        if depth <= 0 {
            depth = 0;
            out.push((buf_start_lineno, std::mem::take(&mut buf)));
        }
    }
    if !buf.is_empty() {
        out.push((buf_start_lineno, buf));
    }
    out
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(i) => &line[..i],
        None => line,
    }
}

fn parse_string_array(value: &str) -> Result<Vec<String>, String> {
    let value = value.trim();
    if !value.starts_with('[') || !value.ends_with(']') {
        return Err(format!("expected array, got `{}`", value));
    }
    let inner = &value[1..value.len() - 1];
    let mut out = Vec::new();
    let mut in_str = false;
    let mut cur = String::new();
    for ch in inner.chars() {
        match ch {
            '"' => {
                if in_str {
                    out.push(std::mem::take(&mut cur));
                    in_str = false;
                } else {
                    in_str = true;
                }
            }
            _ if in_str => cur.push(ch),
            ',' | ' ' | '\t' | '\n' => {}
            other => return Err(format!("unexpected char '{}' in array", other)),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_array_wildcard() {
        assert!(glob_match(
            "deployments[].localTimestamp",
            "deployments[3].localTimestamp"
        ));
        assert!(!glob_match(
            "deployments[].localTimestamp",
            "deployments[3].entityId"
        ));
    }

    #[test]
    fn glob_star_segment() {
        assert!(glob_match("configurations.*", "configurations.realmName"));
        assert!(!glob_match("configurations.*", "configurations.a.b"));
    }

    #[test]
    fn glob_with_section_prefix() {
        assert!(glob_match(
            "configurations.realmName",
            "configurations.realmName"
        ));
        assert!(glob_match(
            "configurations.realmName",
            "/about.configurations.realmName"
        ));
    }

    #[test]
    fn glob_top_level_only_patterns() {
        assert!(glob_match(
            "localTimestamp",
            "/audit/scene/bafkreieo2bzxurwq2zyu5wbhr2c6x7mez2tjcksphjazvqmhhuc7lr2jem.localTimestamp"
        ));
        assert!(!glob_match(
            "localTimestamp",
            "/audit/scene/x.foo.localTimestamp"
        ));
        assert!(glob_match("comms", "/about.comms"));
        assert!(glob_match(
            "synchronizationStatus.up",
            "/status.synchronizationStatus.up"
        ));
    }

    #[test]
    fn defaults_have_snapshots_whole() {
        let v = Volatility::defaults();
        assert!(v.ignore_whole("snapshots"));
        assert!(v.ignore_whole("failed-deployments"));
        assert!(!v.ignore_whole("about"));
    }

    #[test]
    fn shipped_toml_parses() {
        let src = include_str!("../volatility.toml");
        let v = parse_toml(src).unwrap();
        assert!(v.is_ignored("about", "/about.comms"));
        assert!(v.is_ignored("status", "/status.synchronizationStatus.up"));
        assert!(v.is_ignored("audit", "/audit/scene/abc.localTimestamp"));
        assert!(v.ignore_whole("snapshots"));
    }

    #[test]
    fn parse_small_toml() {
        let src = r#"
            [about]
            ignore = ["configurations.realmName", "content.publicUrl"]

            [snapshots]
            ignore_whole_response = true
        "#;
        let v = parse_toml(src).unwrap();
        assert!(v.is_ignored("about", "configurations.realmName"));
        assert!(v.ignore_whole("snapshots"));
    }
}
