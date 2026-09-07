use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
            pub fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }
            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }
            pub fn as_str(&self) -> String {
                self.0.to_string()
            }
            #[allow(clippy::should_implement_trait)]
            pub fn from_str(s: &str) -> Result<Self, uuid::Error> {
                let u = Uuid::parse_str(s)?;
                Ok(Self(u))
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<Uuid> for $name {
            fn from(id: Uuid) -> Self {
                Self(id)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let u = Uuid::from_str(s)?;
                Ok(Self(u))
            }
        }
    };
}

typed_id!(ProjectId);
typed_id!(ConversationId);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelId(pub String);

impl ChannelId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id.to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Result<Self, std::convert::Infallible> {
        Ok(Self(s.to_string()))
    }
}

impl Default for ChannelId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for ChannelId {
    fn from(id: Uuid) -> Self {
        Self(id.to_string())
    }
}

impl From<&str> for ChannelId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for ChannelId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl FromStr for ChannelId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

typed_id!(MessageId);
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentDefinitionId(pub String);

impl AgentDefinitionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id.to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Result<Self, std::convert::Infallible> {
        Ok(Self(s.to_string()))
    }
}

impl Default for AgentDefinitionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentDefinitionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for AgentDefinitionId {
    fn from(id: Uuid) -> Self {
        Self(id.to_string())
    }
}

impl From<&str> for AgentDefinitionId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for AgentDefinitionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl FromStr for AgentDefinitionId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}
typed_id!(AgentInstanceId);
typed_id!(ActorId);
typed_id!(ExecutionId);
typed_id!(TaskId);
typed_id!(WorkflowId);
typed_id!(WorkflowRunId);
typed_id!(WorkflowNodeId);
typed_id!(MemoryId);
typed_id!(ApprovalId);
typed_id!(EventId);
typed_id!(CheckpointId);
typed_id!(LockId);
typed_id!(ToolExecutionId);
typed_id!(TokenUsageId);
typed_id!(ArtifactId);
typed_id!(InboxMessageId);
typed_id!(LearnedRuleId);
