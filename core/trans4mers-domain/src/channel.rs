use crate::actor::Actor;
use crate::ids::{ChannelId, ConversationId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: ChannelId,
    pub conversation_id: ConversationId,
    pub name: String,
    pub kind: ChannelKind,
    pub is_read_only: bool,
    pub member_actors: Vec<Actor>, // Empty = all participants can access
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum ChannelKind {
    SharedBlackboard, // Default channel — all agents + human
    General,          // Regular group channel
    DirectMessage,    // 1:1 between two Actors
    System,           // System notifications only
}
