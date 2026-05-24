use serde::{Deserialize, Serialize};
use serde_json::json;
use sessionbus_core::{
    AdapterCapability, AdapterProtocol, CapabilityDescriptor, CreateArtifactRequest, PackProfile,
};

pub const ACP_BRIDGE_ADAPTER_ID: &str = "sessionbus.acp-bridge";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcpSessionRef {
    pub client_name: String,
    pub session_id: String,
    pub thread_id: Option<String>,
    pub workspace_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcpResumeRequest {
    pub acp: AcpSessionRef,
    pub target_profile: PackProfile,
}

pub fn acp_bridge_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor {
        adapter_id: ACP_BRIDGE_ADAPTER_ID.to_string(),
        protocol: AdapterProtocol::Acp,
        version: env!("CARGO_PKG_VERSION").to_string(),
        capabilities: vec![
            AdapterCapability::ImportContext,
            AdapterCapability::ExportContext,
            AdapterCapability::StreamUpdates,
            AdapterCapability::SessionResume,
            AdapterCapability::SessionObserve,
        ],
        metadata: json!({
            "role": "bridge",
            "boundary": "Maps ACP-observable session metadata to Sessionbus events and context packs. It does not expose Sessionbus as an agent runtime."
        }),
    }
}

pub fn acp_observation_artifact(acp: AcpSessionRef) -> CreateArtifactRequest {
    CreateArtifactRequest {
        kind: sessionbus_core::ArtifactKind::ToolInvocation,
        title: Some(format!("ACP session observed from {}", acp.client_name)),
        uri: None,
        body: Some(serde_json::to_string_pretty(&acp).expect("ACP reference serializes")),
        metadata: json!({
            "protocol": "acp",
            "client_name": acp.client_name,
            "acp_session_id": acp.session_id,
            "thread_id": acp.thread_id,
            "workspace_root": acp.workspace_root
        }),
        snapshot: true,
    }
}

pub fn default_resume_request(acp: AcpSessionRef) -> AcpResumeRequest {
    AcpResumeRequest {
        acp,
        target_profile: PackProfile::Acp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sessionbus_core::{AdapterCapability, AdapterProtocol, ArtifactKind};

    #[test]
    fn descriptor_declares_bridge_capabilities_without_agent_runtime_claims() {
        let descriptor = acp_bridge_descriptor();

        assert_eq!(descriptor.protocol, AdapterProtocol::Acp);
        assert!(descriptor
            .capabilities
            .contains(&AdapterCapability::SessionResume));
        assert!(descriptor
            .metadata
            .to_string()
            .contains("does not expose Sessionbus as an agent runtime"));
    }

    #[test]
    fn acp_session_refs_become_snapshot_artifacts() {
        let artifact = acp_observation_artifact(AcpSessionRef {
            client_name: "zed".to_string(),
            session_id: "acp-session".to_string(),
            thread_id: Some("thread-1".to_string()),
            workspace_root: Some("/repo".to_string()),
        });

        assert_eq!(artifact.kind, ArtifactKind::ToolInvocation);
        assert!(artifact.snapshot);
        assert!(artifact.body.unwrap().contains("acp-session"));
    }
}
