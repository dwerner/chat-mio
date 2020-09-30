use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Chat {
    #[serde(default)]
    pub id: u64,
    #[serde(alias = "participantIds")]
    pub participant_ids: [u64; 2],
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    pub id: String,
    #[serde(rename = "sourceUserId")]
    pub source_user_id: u64,
    #[serde(rename = "destinationUserId")]
    pub destination_user_id: u64,
    pub timestamp: u64,
    pub message: String,
}

impl Ord for Message {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.timestamp.cmp(&other.timestamp)
    }
}

impl PartialOrd for Message {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(&other))
    }
}

impl Message {
    pub fn update_timestamp(&mut self) {
        self.timestamp = super::chat_service::timestamp();
    }
}
