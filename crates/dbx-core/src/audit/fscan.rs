use regex::Regex;

use super::types::{ParsedFscanTarget, ParsedFscanTargets};

pub fn parse_fscan_text(text: &str) -> ParsedFscanTargets {
    let mut targets = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if let Some(target) = parse_line(index + 1, line) {
            targets.push(target);
        }
    }
    ParsedFscanTargets { total: targets.len(), targets }
}

fn parse_line(line_number: usize, line: &str) -> Option<ParsedFscanTarget> {
    parse_modern_line(line_number, line).or_else(|| parse_legacy_line(line_number, line))
}

fn parse_modern_line(line_number: usize, line: &str) -> Option<ParsedFscanTarget> {
    let re = Regex::new(
        r"(?ix)
        (?P<host>[a-z0-9_.:-]+):(?P<port>\d+)\s+
        (?P<db>mysql|mariadb|mssql|sqlserver|postgres|postgresql|oracle|redis)\s+
        (?P<user>\S+)[/:](?P<pass>\S+)
        ",
    )
    .ok()?;
    captures_to_target(line_number, line, &re)
}

fn parse_legacy_line(line_number: usize, line: &str) -> Option<ParsedFscanTarget> {
    let re = Regex::new(
        r"(?ix)
        \[\+\]\s*(?P<db>mysql|mariadb|mssql|sqlserver|postgres|postgresql|oracle|redis)\s+
        (?P<host>[a-z0-9_.:-]+):(?P<port>\d+):(?P<user>\S+)\s+(?P<pass>\S+)
        ",
    )
    .ok()?;
    captures_to_target(line_number, line, &re)
}

fn captures_to_target(line_number: usize, line: &str, re: &Regex) -> Option<ParsedFscanTarget> {
    let captures = re.captures(line)?;
    Some(ParsedFscanTarget {
        db_type: normalize_db_type(captures.name("db")?.as_str()),
        host: captures.name("host")?.as_str().to_string(),
        port: captures.name("port")?.as_str().parse().ok()?,
        username: captures.name("user")?.as_str().to_string(),
        password: captures.name("pass")?.as_str().to_string(),
        line: line_number,
        raw: line.to_string(),
    })
}

fn normalize_db_type(input: &str) -> String {
    match input.to_ascii_lowercase().as_str() {
        "postgresql" => "postgres".to_string(),
        "sqlserver" => "mssql".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_fscan_text;

    #[test]
    fn parses_modern_fscan_database_credentials() {
        let parsed = parse_fscan_text("192.0.2.10:3306 mysql root/pass123");
        assert_eq!(parsed.total, 1);
        assert_eq!(parsed.targets[0].db_type, "mysql");
        assert_eq!(parsed.targets[0].host, "192.0.2.10");
        assert_eq!(parsed.targets[0].port, 3306);
        assert_eq!(parsed.targets[0].username, "root");
        assert_eq!(parsed.targets[0].password, "pass123");
    }

    #[test]
    fn parses_legacy_fscan_database_credentials() {
        let parsed = parse_fscan_text("[+] mssql 192.0.2.20:1433:sa pass123");
        assert_eq!(parsed.total, 1);
        assert_eq!(parsed.targets[0].db_type, "mssql");
        assert_eq!(parsed.targets[0].username, "sa");
    }
}
