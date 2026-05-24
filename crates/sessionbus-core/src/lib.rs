//! Core contracts for Sessionbus.
//!
//! This crate intentionally has no daemon or database dependencies. Adapters,
//! CLIs, stores, and bridges can all depend on the same portable session,
//! artifact, event, and capability types.

mod models;
mod pack;
mod redaction;
mod schema;

pub use models::*;
pub use pack::*;
pub use redaction::*;
pub use schema::*;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use serde_json::json;

    fn ts(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn session() -> Session {
        Session {
            id: "ses_test".to_string(),
            title: "Fix flaky deploy".to_string(),
            status: SessionStatus::Active,
            workspace: Some(WorkspaceInfo {
                root: "/repo".to_string(),
                git_remote: Some("git@example.com:org/repo.git".to_string()),
                git_branch: Some("main".to_string()),
                head: Some("abc123".to_string()),
            }),
            summary: Some("Deploy fails intermittently in staging.".to_string()),
            created_at: ts("2026-05-24T00:00:00Z"),
            updated_at: ts("2026-05-24T00:00:00Z"),
        }
    }

    #[test]
    fn pack_orders_artifacts_and_redacts_before_rendering() {
        let artifacts = vec![
            Artifact {
                id: "art_b".to_string(),
                session_id: "ses_test".to_string(),
                kind: ArtifactKind::TerminalOutput,
                title: Some("failing command".to_string()),
                uri: None,
                content_hash: None,
                content_ref: None,
                body: Some("TOKEN=super-secret\ncargo test failed".to_string()),
                metadata: json!({"exit_code": 1}),
                created_at: ts("2026-05-24T00:02:00Z"),
            },
            Artifact {
                id: "art_a".to_string(),
                session_id: "ses_test".to_string(),
                kind: ArtifactKind::File,
                title: Some("service.yaml".to_string()),
                uri: Some("file:///repo/service.yaml".to_string()),
                content_hash: Some("sha256:test".to_string()),
                content_ref: Some("sha256:test".to_string()),
                body: Some("name: api".to_string()),
                metadata: json!({}),
                created_at: ts("2026-05-24T00:01:00Z"),
            },
        ];
        let decisions = vec![Decision {
            id: "dec_1".to_string(),
            session_id: "ses_test".to_string(),
            text: "Focus on staging deploy path first.".to_string(),
            rationale: None,
            created_at: ts("2026-05-24T00:03:00Z"),
        }];

        let pack = build_context_pack(
            PackInput {
                session: session(),
                artifacts,
                decisions,
            },
            PackProfile::ChatGpt,
            &RedactionPolicy::default(),
        )
        .unwrap();

        let service_idx = pack.markdown.find("service.yaml").unwrap();
        let terminal_idx = pack.markdown.find("failing command").unwrap();
        assert!(service_idx < terminal_idx);
        assert!(pack.markdown.contains("TOKEN=[REDACTED]"));
        assert!(!pack.markdown.contains("super-secret"));
    }

    #[test]
    fn profile_parsing_accepts_known_targets() {
        assert_eq!(
            "chatgpt".parse::<PackProfile>().unwrap(),
            PackProfile::ChatGpt
        );
        assert_eq!(
            "claude".parse::<PackProfile>().unwrap(),
            PackProfile::Claude
        );
        assert_eq!(
            "cursor".parse::<PackProfile>().unwrap(),
            PackProfile::Cursor
        );
        assert_eq!("acp".parse::<PackProfile>().unwrap(), PackProfile::Acp);
        assert!("unknown".parse::<PackProfile>().is_err());
    }

    #[test]
    fn schema_bundle_contains_public_contracts() {
        let schemas = schema_bundle();
        assert!(schemas.contains_key("Session"));
        assert!(schemas.contains_key("Artifact"));
        assert!(schemas.contains_key("BusEvent"));
        assert!(schemas.contains_key("CapabilityDescriptor"));
    }
}
