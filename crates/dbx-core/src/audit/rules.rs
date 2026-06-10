use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use aho_corasick::{AhoCorasick, MatchKind};
use regex::Regex;
use serde::Deserialize;

use super::types::{AuditKind, AuditLevel, AuditLevelFilter};

const BUILTIN_FIELD_RULES: &[&str] = &[include_str!("rules/fields/dbx-fields.yaml")];
const BUILTIN_CONTENT_RULES: &[&str] =
    &[include_str!("rules/content/dbx-pii.yaml"), include_str!("rules/content/dbx-secrets.yaml")];

static BUILTIN_ENGINE: OnceLock<AuditRuleEngine> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditRuleTarget {
    Field,
    Content,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRuleMatch {
    pub kind: AuditKind,
    pub level: AuditLevel,
    pub rule_id: String,
    pub rule_name: String,
    pub rule_severity: String,
    pub rule_tags: Vec<String>,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct AuditRuleEngine {
    fields: RuleGroup,
    content: RuleGroup,
}

#[derive(Debug, Clone)]
struct RuleGroup {
    rules: Vec<CompiledRule>,
    ac: Option<AhoCorasick>,
    literal_to_rules: BTreeMap<usize, Vec<usize>>,
    fallback_rules: Vec<usize>,
}

#[derive(Debug, Clone)]
struct CompiledRule {
    id: String,
    name: String,
    severity: String,
    tags: Vec<String>,
    kind: AuditKind,
    level: AuditLevel,
    regexes: Vec<Regex>,
    literals: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct TemplateRule {
    id: String,
    info: Option<TemplateInfo>,
    dbx: Option<DbxRule>,
    file: Option<Vec<FileRule>>,
}

#[derive(Debug, Deserialize)]
struct TemplateInfo {
    name: Option<String>,
    severity: Option<String>,
    tags: Option<serde_yaml::Value>,
}

#[derive(Debug, Deserialize)]
struct DbxRule {
    target: Option<String>,
    kind: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct FileRule {
    matchers: Option<Vec<RuleOperator>>,
    extractors: Option<Vec<RuleOperator>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RuleOperator {
    #[serde(rename = "type")]
    operator_type: Option<String>,
    words: Option<Vec<String>>,
    regex: Option<Vec<String>>,
}

impl AuditRuleEngine {
    pub fn builtin() -> &'static Self {
        BUILTIN_ENGINE.get_or_init(|| {
            let mut rules = Vec::new();
            for source in BUILTIN_FIELD_RULES {
                rules.extend(parse_rule_documents(source, Some(AuditRuleTarget::Field)));
            }
            for source in BUILTIN_CONTENT_RULES {
                rules.extend(parse_rule_documents(source, Some(AuditRuleTarget::Content)));
            }
            Self::from_rules(rules)
        })
    }

    pub fn from_template_paths(paths: &[String]) -> Result<Self, String> {
        let mut rules = Self::builtin().all_rules();
        for path in paths.iter().map(|path| path.trim()).filter(|path| !path.is_empty()) {
            for file in collect_yaml_files(Path::new(path))? {
                let content = fs::read_to_string(&file).map_err(|err| format!("{}: {err}", file.display()))?;
                rules.extend(parse_rule_documents(&content, None));
            }
        }
        Ok(Self::from_rules(rules))
    }

    pub fn scan_field(&self, text: &str, level: AuditLevelFilter) -> Vec<AuditRuleMatch> {
        self.fields.scan(text, level)
    }

    pub fn scan_content(&self, text: &str, level: AuditLevelFilter) -> Vec<AuditRuleMatch> {
        self.content.scan(text, level)
    }

    fn from_rules(rules: Vec<(AuditRuleTarget, CompiledRule)>) -> Self {
        let mut field_rules = Vec::new();
        let mut content_rules = Vec::new();
        for (target, rule) in rules {
            match target {
                AuditRuleTarget::Field => field_rules.push(rule),
                AuditRuleTarget::Content => content_rules.push(rule),
            }
        }
        Self { fields: RuleGroup::new(field_rules), content: RuleGroup::new(content_rules) }
    }

    fn all_rules(&self) -> Vec<(AuditRuleTarget, CompiledRule)> {
        self.fields
            .rules
            .iter()
            .cloned()
            .map(|rule| (AuditRuleTarget::Field, rule))
            .chain(self.content.rules.iter().cloned().map(|rule| (AuditRuleTarget::Content, rule)))
            .collect()
    }
}

impl RuleGroup {
    fn new(rules: Vec<CompiledRule>) -> Self {
        let mut literal_lookup = BTreeMap::<String, usize>::new();
        let mut literals = Vec::<String>::new();
        let mut literal_to_rules = BTreeMap::<usize, Vec<usize>>::new();
        let mut fallback_rules = Vec::new();

        for (rule_index, rule) in rules.iter().enumerate() {
            if rule.literals.is_empty() {
                fallback_rules.push(rule_index);
                continue;
            }
            for literal in &rule.literals {
                let literal = literal.to_ascii_lowercase();
                let literal_index = match literal_lookup.get(&literal) {
                    Some(index) => *index,
                    None => {
                        let index = literals.len();
                        literal_lookup.insert(literal.clone(), index);
                        literals.push(literal);
                        index
                    }
                };
                literal_to_rules.entry(literal_index).or_default().push(rule_index);
            }
        }

        let ac = if literals.is_empty() {
            None
        } else {
            AhoCorasick::builder().match_kind(MatchKind::LeftmostLongest).build(&literals).ok()
        };

        Self { rules, ac, literal_to_rules, fallback_rules }
    }

    fn scan(&self, text: &str, level: AuditLevelFilter) -> Vec<AuditRuleMatch> {
        if text.trim().is_empty() {
            return Vec::new();
        }

        let candidates = self.candidate_rules(text);
        if candidates.is_empty() {
            return Vec::new();
        }

        let mut matches = Vec::new();
        for rule_index in candidates {
            let Some(rule) = self.rules.get(rule_index) else {
                continue;
            };
            if !level.allows(rule.level) {
                continue;
            }
            for regex in &rule.regexes {
                for capture in regex.captures_iter(text) {
                    let value = capture
                        .get(1)
                        .or_else(|| capture.get(0))
                        .map(|matched| matched.as_str().to_string())
                        .unwrap_or_default();
                    matches.push(AuditRuleMatch {
                        kind: rule.kind,
                        level: rule.level,
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        rule_severity: rule.severity.clone(),
                        rule_tags: rule.tags.clone(),
                        value,
                    });
                }
            }
        }
        dedupe_matches(matches)
    }

    fn candidate_rules(&self, text: &str) -> Vec<usize> {
        let mut candidates = BTreeSet::<usize>::new();
        for rule_index in &self.fallback_rules {
            candidates.insert(*rule_index);
        }
        if let Some(ac) = &self.ac {
            let lower = text.to_ascii_lowercase();
            for hit in ac.find_iter(lower.as_bytes()) {
                if let Some(rule_indices) = self.literal_to_rules.get(&hit.pattern().as_usize()) {
                    for rule_index in rule_indices {
                        candidates.insert(*rule_index);
                    }
                }
            }
        }
        candidates.into_iter().collect()
    }
}

fn parse_rule_documents(source: &str, forced_target: Option<AuditRuleTarget>) -> Vec<(AuditRuleTarget, CompiledRule)> {
    let mut rules = Vec::new();
    for document in serde_yaml::Deserializer::from_str(source) {
        if let Ok(rule) = TemplateRule::deserialize(document) {
            if let Some(compiled) = compile_template_rule(rule, forced_target) {
                rules.push(compiled);
            }
        }
    }
    rules
}

fn compile_template_rule(
    rule: TemplateRule,
    forced_target: Option<AuditRuleTarget>,
) -> Option<(AuditRuleTarget, CompiledRule)> {
    let info = rule.info;
    let tags = info.as_ref().and_then(|info| info.tags.as_ref()).map(parse_tags).unwrap_or_default();
    let severity = info.as_ref().and_then(|info| info.severity.as_deref()).unwrap_or("medium").to_ascii_lowercase();
    let name = info.as_ref().and_then(|info| info.name.clone()).unwrap_or_else(|| rule.id.clone());
    let target = forced_target.or_else(|| rule.dbx.as_ref().and_then(|dbx| target_from_str(dbx.target.as_deref()?)));
    let target = target.unwrap_or(AuditRuleTarget::Content);
    let kind = rule
        .dbx
        .as_ref()
        .and_then(|dbx| dbx.kind.as_deref())
        .and_then(kind_from_str)
        .unwrap_or_else(|| infer_kind(&rule.id, &name, &tags));
    let level = level_from_severity(&severity).unwrap_or_else(|| kind.level());

    let mut patterns = Vec::<String>::new();
    for file_rule in rule.file.unwrap_or_default() {
        for operator in file_rule.extractors.into_iter().flatten().chain(file_rule.matchers.into_iter().flatten()) {
            match operator.operator_type.as_deref() {
                Some("regex") | None => patterns.extend(operator.regex.unwrap_or_default()),
                Some("word") => {
                    for word in operator.words.unwrap_or_default() {
                        patterns.push(regex::escape(&word));
                    }
                }
                _ => {}
            }
        }
    }
    if patterns.is_empty() {
        return None;
    }

    let mut regexes = Vec::new();
    let mut literals = BTreeSet::<String>::new();
    for pattern in patterns {
        if let Ok(regex) = Regex::new(&pattern) {
            for literal in extract_literals(&pattern) {
                literals.insert(literal);
            }
            regexes.push(regex);
        }
    }
    if regexes.is_empty() {
        return None;
    }

    Some((
        target,
        CompiledRule {
            id: rule.id,
            name,
            severity,
            tags,
            kind,
            level,
            regexes,
            literals: literals.into_iter().collect(),
        },
    ))
}

fn collect_yaml_files(path: &Path) -> Result<Vec<PathBuf>, String> {
    if path.is_file() {
        return Ok(if is_yaml_file(path) { vec![path.to_path_buf()] } else { Vec::new() });
    }
    if !path.is_dir() {
        return Err(format!("规则路径不存在：{}", path.display()));
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(path).map_err(|err| format!("{}: {err}", path.display()))? {
        let entry = entry.map_err(|err| err.to_string())?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            files.extend(collect_yaml_files(&entry_path)?);
        } else if is_yaml_file(&entry_path) {
            files.push(entry_path);
        }
    }
    Ok(files)
}

fn is_yaml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml"))
}

fn parse_tags(value: &serde_yaml::Value) -> Vec<String> {
    match value {
        serde_yaml::Value::String(value) => {
            value.split(',').map(str::trim).filter(|tag| !tag.is_empty()).map(str::to_string).collect()
        }
        serde_yaml::Value::Sequence(values) => values
            .iter()
            .filter_map(|value| value.as_str())
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn target_from_str(value: &str) -> Option<AuditRuleTarget> {
    match value.to_ascii_lowercase().as_str() {
        "field" | "fields" | "field-name" | "metadata" => Some(AuditRuleTarget::Field),
        "content" | "value" | "values" => Some(AuditRuleTarget::Content),
        _ => None,
    }
}

pub fn kind_from_str(value: &str) -> Option<AuditKind> {
    match normalize_key(value).as_str() {
        "phone" | "mobile" => Some(AuditKind::Phone),
        "email" | "mail" => Some(AuditKind::Email),
        "idcard" | "identity" | "idcardnumber" => Some(AuditKind::IdCard),
        "bankcard" | "card" => Some(AuditKind::BankCard),
        "passwordsecret" | "password" | "passwd" | "pwd" => Some(AuditKind::PasswordSecret),
        "tokensecret" | "token" | "apikey" | "api_key" => Some(AuditKind::TokenSecret),
        "address" => Some(AuditKind::Address),
        "username" | "user" => Some(AuditKind::Username),
        "account" => Some(AuditKind::Account),
        "ipaddress" | "ip" => Some(AuditKind::IpAddress),
        "businessidentifier" | "businessid" => Some(AuditKind::BusinessIdentifier),
        "riskevidence" | "risk" | "evidence" => Some(AuditKind::RiskEvidence),
        "secret" => Some(AuditKind::Secret),
        "privatekey" => Some(AuditKind::PrivateKey),
        "cloudcredential" | "cloudkey" | "cloudsecret" => Some(AuditKind::CloudCredential),
        "webhook" => Some(AuditKind::Webhook),
        "connectionstring" | "connection" => Some(AuditKind::ConnectionString),
        _ => None,
    }
}

fn infer_kind(id: &str, name: &str, tags: &[String]) -> AuditKind {
    let haystack = normalize_key(&format!("{id} {name} {}", tags.join(" ")));
    if haystack.contains("privatekey") || haystack.contains("sshkey") {
        AuditKind::PrivateKey
    } else if haystack.contains("webhook") {
        AuditKind::Webhook
    } else if haystack.contains("connectionstring") || haystack.contains("odbc") || haystack.contains("jdbc") {
        AuditKind::ConnectionString
    } else if haystack.contains("aws")
        || haystack.contains("amazon")
        || haystack.contains("azure")
        || haystack.contains("google")
        || haystack.contains("alibaba")
        || haystack.contains("cloud")
    {
        AuditKind::CloudCredential
    } else if haystack.contains("password") || haystack.contains("passwd") || haystack.contains("pwd") {
        AuditKind::PasswordSecret
    } else if haystack.contains("token") || haystack.contains("secret") || haystack.contains("key") {
        AuditKind::TokenSecret
    } else {
        AuditKind::Secret
    }
}

fn level_from_severity(value: &str) -> Option<AuditLevel> {
    match value.to_ascii_lowercase().as_str() {
        "critical" | "high" => Some(AuditLevel::High),
        "medium" => Some(AuditLevel::Medium),
        "low" | "info" | "informational" => Some(AuditLevel::Low),
        _ => None,
    }
}

fn normalize_key(value: &str) -> String {
    value.chars().filter(|ch| ch.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn extract_literals(pattern: &str) -> Vec<String> {
    let mut literals = Vec::new();
    let mut current = String::new();
    let mut chars = pattern.chars().peekable();
    let mut in_class = false;
    let mut in_quantifier = false;
    while let Some(ch) = chars.next() {
        if in_class {
            if ch == ']' {
                in_class = false;
            }
            continue;
        }
        if in_quantifier {
            if ch == '}' {
                in_quantifier = false;
            }
            continue;
        }
        if ch == '[' {
            push_literal(&mut literals, &mut current);
            in_class = true;
        } else if ch == '{' {
            push_literal(&mut literals, &mut current);
            in_quantifier = true;
        } else if ch == '\\' {
            match chars.peek().copied() {
                Some('b' | 'B' | 'd' | 'D' | 's' | 'S' | 'w' | 'W' | 'A' | 'z' | 'Z') => {
                    chars.next();
                    push_literal(&mut literals, &mut current);
                }
                Some(next) => {
                    chars.next();
                    if is_literal_char(next) {
                        current.push(next);
                    } else {
                        push_literal(&mut literals, &mut current);
                    }
                }
                None => push_literal(&mut literals, &mut current),
            }
        } else if is_literal_char(ch) {
            current.push(ch);
        } else {
            push_literal(&mut literals, &mut current);
        }
    }
    push_literal(&mut literals, &mut current);
    literals
}

fn is_literal_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '-' | ':' | '/' | '@' | '.')
}

fn push_literal(literals: &mut Vec<String>, current: &mut String) {
    let literal = current.trim_matches(|ch: char| ch == '_' || ch == '-' || ch == ':' || ch == '/' || ch == '.');
    if literal.chars().count() >= 3 || matches!(literal.to_ascii_lowercase().as_str(), "ip" | "sk" | "ak") {
        literals.push(literal.to_string());
    }
    current.clear();
}

fn dedupe_matches(matches: Vec<AuditRuleMatch>) -> Vec<AuditRuleMatch> {
    let mut seen = BTreeSet::<(String, String)>::new();
    let mut result = Vec::new();
    for item in matches {
        let key = (item.rule_id.clone(), item.value.clone());
        if seen.insert(key) {
            result.push(item);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{AuditRuleEngine, AuditRuleTarget};
    use crate::audit::types::{AuditKind, AuditLevel, AuditLevelFilter};

    #[test]
    fn parses_found_style_content_rules() {
        let engine = AuditRuleEngine::from_rules(super::parse_rule_documents(
            r#"
id: openai-key
info:
  name: OpenAI API Key
  severity: high
  tags: file,keys,openai,token,ai
file:
  - extensions:
      - all
    extractors:
      - type: regex
        regex:
          - \b(sk-[a-zA-Z0-9]{48})\b
"#,
            None,
        ));
        let matches =
            engine.scan_content("token=sk-abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUV", AuditLevelFilter::All);
        assert!(matches.iter().any(|item| item.rule_id == "openai-key"));
    }

    #[test]
    fn parses_dbx_field_rules() {
        let engine = AuditRuleEngine::from_rules(super::parse_rule_documents(
            r#"
id: dbx-password-field
info:
  name: Password Field
  severity: high
dbx:
  target: field
  kind: password-secret
file:
  - extractors:
      - type: regex
        regex:
          - (?i)(password|passwd|pwd|pass|密码)
"#,
            None,
        ));
        let matches = engine.scan_field("table=users column=password_hash", AuditLevelFilter::All);
        assert_eq!(matches[0].kind, AuditKind::PasswordSecret);
        assert_eq!(matches[0].level, AuditLevel::High);
    }

    #[test]
    fn builtins_keep_existing_detection() {
        let engine = AuditRuleEngine::builtin();
        assert!(engine
            .scan_field("column=mobile", AuditLevelFilter::All)
            .iter()
            .any(|item| item.kind == AuditKind::Phone));
        assert!(engine
            .scan_content("alice@example.com", AuditLevelFilter::All)
            .iter()
            .any(|item| item.kind == AuditKind::Email));
        assert!(engine
            .scan_content("BEGIN RSA PRIVATE KEY", AuditLevelFilter::All)
            .iter()
            .any(|item| item.kind == AuditKind::PrivateKey));
    }

    #[test]
    fn forced_target_is_applied() {
        let rules = super::parse_rule_documents(
            r#"
id: aws-access-key
info:
  name: AWS
  severity: info
file:
  - extractors:
      - type: regex
        regex:
          - "(AKIA)[A-Z0-9]{16}"
"#,
            Some(AuditRuleTarget::Content),
        );
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].0, AuditRuleTarget::Content);
    }
}
