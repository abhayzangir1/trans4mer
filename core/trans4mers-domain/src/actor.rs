use crate::ids::{ActorId, AgentInstanceId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Actor {
    Human { id: ActorId, display_name: String },
    Agent(AgentInstanceId),
    System,
    Tool(String),
}

impl Actor {
    pub fn id_string(&self) -> String {
        match self {
            Self::Human { id, .. } => id.to_string(),
            Self::Agent(id) => id.to_string(),
            Self::System => "system".to_string(),
            Self::Tool(name) => format!("tool:{}", name),
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::Human { display_name, .. } => display_name.clone(),
            Self::Agent(id) => format!("Agent-{}", id),
            Self::System => "System".to_string(),
            Self::Tool(name) => format!("Tool:{}", name),
        }
    }

    pub fn is_human(&self) -> bool {
        matches!(self, Self::Human { .. })
    }
    pub fn is_agent(&self) -> bool {
        matches!(self, Self::Agent(_))
    }
    pub fn is_system(&self) -> bool {
        matches!(self, Self::System)
    }
}

impl std::fmt::Display for Actor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id_string())
    }
}
