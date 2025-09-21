use poise::serenity_prelude::{self as serenity};
use serde::Deserialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
pub const IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "webp", "gif", "avif"];
pub const MAX_EMBEDS: usize = 10;
pub const MAX_ATTACHMENTS: usize = 10;
pub const MESSAGE_DELAY: Duration = Duration::from_millis(100);
pub type FileIndex = HashMap<String, Vec<PathBuf>>;
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;
#[derive(Default)]
pub struct Data {
    pub cancellation_flags: Arc<Mutex<HashMap<serenity::ChannelId, bool>>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Export {
    pub guild: GuildInfo,
    pub channel: ChannelInfo,
    pub messages: Vec<MessageInfo>,
}
#[derive(Deserialize)]
pub struct GuildInfo {
    pub name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInfo {
    pub name: String,
    pub category: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageInfo {
    pub content: String,
    pub author: Author,
    pub timestamp: String,
    #[serde(default)]
    pub timestamp_edited: Option<String>,
    pub attachments: Vec<AttachmentInfo>,
    pub mentions: Vec<Mention>,
    pub inline_emojis: Vec<EmojiInfo>,
    pub reactions: Vec<ReactionInfo>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Author {
    pub id: serenity::UserId,
    pub name: String,
    pub avatar_url: String,
    pub color: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfo {
    pub url: String,
    pub file_name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mention {
    pub id: serenity::UserId,
    pub name: String,
    pub nickname: Option<String>,
}
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ReactionInfo {
    pub emoji: EmojiInfo,
    pub count: serde_json::Value,
    pub users: Vec<serde_json::Value>,
}
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct EmojiInfo {
    pub id: Option<String>,
    pub name: String,
    pub code: String,
    pub is_animated: bool,
    pub image_url: String,
}
#[derive(Default)]
pub struct ImportOptions {
    pub no_guild: bool,
    pub no_category: bool,
    pub no_channel: bool,
    pub no_timestamp: bool,
    pub no_mentions: bool,
    pub no_reactions: bool,
    pub no_embed: bool,
    pub button: bool,
    pub reaction_users: bool,
    pub outside: bool,
    pub disable_button: bool,
    pub accent_color: bool,
    pub current_avatar: bool,
    pub range_start: Option<usize>,
    pub range_end: Option<usize>,
    pub first: Option<usize>,
    pub last: Option<usize>,
}
pub enum MediaSource {
    Local(PathBuf, String),
    Remote(String),
}
pub struct MessageBatch {
    pub attachments: Vec<serenity::CreateAttachment>,
    pub embeds: Vec<serenity::CreateEmbed>,
    pub count: usize,
}
