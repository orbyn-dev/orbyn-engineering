//! The recognised formats: the one list of what Orbyn knows as a secret. Crew's rule (H2) uses
//! every entry, always; terminal blocks and what agents read use the entries the person leaves on.

/// One recognised format. `class` is the name Crew's refusals carry (it is never the match).
pub struct Builtin {
    pub id: &'static str,
    pub label: &'static str,
    pub example: &'static str,
    pub class: &'static str,
    pub patterns: &'static [&'static str],
}

/// Order matters: the class of the first format that matches is the one a refusal names.
pub const BUILTINS: &[Builtin] = &[
    Builtin {
        id: "private_key",
        label: "Private key block",
        example: "-----BEGIN … PRIVATE KEY-----",
        class: "private key",
        // The block through its END line when that follows within 8 KiB; otherwise (a cut-off
        // excerpt) the header and everything after it. `PGP ... KEY BLOCK` is the same format.
        patterns: &[
            r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY(?: BLOCK)?-----(?:[A-Za-z0-9+/=:,.\s-]{0,8192}-----END [A-Z ]*PRIVATE KEY(?: BLOCK)?-----|.*)",
        ],
    },
    Builtin {
        id: "api_key",
        label: "API key",
        example: "sk-••••••••",
        class: "api key",
        // OpenAI-style, Stripe secret and restricted keys, GitLab, npm and Google API keys.
        patterns: &[
            r"\bsk-[A-Za-z0-9_-]{20,}",
            r"\b[sr]k_(?:live|test)_[A-Za-z0-9]{16,}",
            r"\bglpat-[A-Za-z0-9_-]{20,}",
            r"\bnpm_[A-Za-z0-9]{36}\b",
            r"\bAIza[0-9A-Za-z_-]{35}",
        ],
    },
    Builtin {
        id: "github_token",
        label: "GitHub token",
        example: "ghp_••••••••",
        class: "github token",
        patterns: &[
            r"\bgh[pousr]_[A-Za-z0-9]{36,}",
            r"\bgithub_pat_[A-Za-z0-9_]{22,}",
        ],
    },
    Builtin {
        id: "aws_access_key",
        label: "AWS access key",
        example: "AKIA••••••••",
        class: "aws key",
        // AKIA is a long-lived key, ASIA a temporary one.
        patterns: &[r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"],
    },
    Builtin {
        id: "aws_secret_key",
        label: "AWS secret key",
        example: "40 characters after an access key",
        class: "aws secret key",
        patterns: &[r#"(?i)\baws_?secret_?(?:access_?)?key["']?\s*[:=]\s*["']?[A-Za-z0-9/+=]{40}"#],
    },
    Builtin {
        id: "slack_token",
        label: "Slack token",
        example: "xoxb-••••••••",
        class: "slack token",
        patterns: &[r"\bxox[abprs]-[A-Za-z0-9-]{10,}"],
    },
    Builtin {
        id: "bearer_token",
        label: "Bearer token",
        example: "Bearer ••••••••",
        class: "bearer token",
        patterns: &[r"(?i)\bbearer\s+[A-Za-z0-9._~+/-]{20,}=*"],
    },
    Builtin {
        id: "jwt",
        label: "JSON Web Token",
        example: "eyJ•••••.•••••.•••••",
        class: "jwt",
        patterns: &[r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}"],
    },
    Builtin {
        id: "named_value",
        label: "Environment values named like a secret",
        example: "KEY, TOKEN, SECRET, PASSWORD",
        class: "credential assignment",
        patterns: &[
            // `_` is a word character, so a prefixed name (`DB_PASSWORD`) has no `\b` before its
            // keyword: the name is read from its first character instead.
            r#"(?i)\b[A-Za-z0-9_-]*(?:password|passwd|secret[_-]?key|secret|api[_-]?key|access[_-]?key|private[_-]?key|token)["']?\s*[:=]\s*["']?[^\s"']{8,}"#,
        ],
    },
];

pub fn find(id: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|builtin| builtin.id == id)
}
