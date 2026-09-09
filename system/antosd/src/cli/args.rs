//! Typed command-line argument parser for antOS.

#[derive(Debug, Default, Clone)]
pub struct Opts {
    pub assume_yes: bool,
    pub dry_run: bool,
    pub planner: Option<String>,
    pub project: Option<String>,
    pub help: bool,
}

impl Opts {
    pub fn parse_from<I, T>(args: I) -> (Self, Vec<String>)
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        let mut opts = Self::default();
        let mut rest = Vec::new();
        let mut iter = args.into_iter().map(Into::into);

        while let Some(a) = iter.next() {
            match a.as_str() {
                "--si" | "-s" | "--yes" | "-y" | "--assume-yes" => opts.assume_yes = true,
                "--seco" | "-n" | "--dry-run" => opts.dry_run = true,
                "--planificador" | "-p" | "--planner" => opts.planner = iter.next(),
                "--proyecto" | "--project" => opts.project = iter.next(),
                "-h" | "--ayuda" | "--help" => opts.help = true,
                _ => rest.push(a),
            }
        }

        (opts, rest)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_opts_parsing_flags() {
        let args = vec![
            "-s".to_string(),
            "--dry-run".to_string(),
            "--planner".to_string(),
            "claude".to_string(),
            "--project".to_string(),
            "web-app".to_string(),
            "run".to_string(),
            "T1.1".to_string(),
        ];
        let (opts, rest) = Opts::parse_from(args);
        assert!(opts.assume_yes);
        assert!(opts.dry_run);
        assert_eq!(opts.planner.as_deref(), Some("claude"));
        assert_eq!(opts.project.as_deref(), Some("web-app"));
        assert!(!opts.help);
        assert_eq!(rest, vec!["run", "T1.1"]);
    }

    #[test]
    fn test_opts_help_flag() {
        let (opts, rest) = Opts::parse_from(vec!["--help".to_string()]);
        assert!(opts.help);
        assert!(rest.is_empty());

        let (opts2, _) = Opts::parse_from(vec!["-h".to_string()]);
        assert!(opts2.help);
    }
}

