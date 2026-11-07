use serde::{Deserialize, Serialize};

use super::conversation::ConversationSummary;
use super::ids::Millis;

pub const PUBLIC_MUTE_SECONDS: [u32; 3] = [600, 3_600, 86_400];
pub const PUBLIC_SLOW_MODE_MAX_S: u32 = 600;
pub const PUBLIC_REASON_CHARS_MAX: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicOpened {
    pub conversation: ConversationSummary,
    pub online: u32,
    pub mute: Option<ModeratorMute>,
    pub newcomer: Option<Newcomer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeratorMute {
    pub until: Millis,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Newcomer {
    pub until: Option<Millis>,
    pub body_max: u32,
}

impl Newcomer {
    pub fn fits(&self, text: &str) -> bool {
        crate::envelope::post_body_len(text) <= self.body_max as usize
    }
}
