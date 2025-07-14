use serde::Deserialize;
use serenity::{
    async_trait,
    builder::{CreateAttachment, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter, CreateMessage},
    model::{
        channel::Message,
        gateway::Ready,
        id::{ChannelId, UserId},
        Timestamp,
    },
    prelude::*,
};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};
use walkdir::WalkDir;
const SUPPORTED_IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "webp", "gif", "avif"];
const MAX_EMBEDS_PER_MESSAGE: usize = 10;
const MAX_ATTACHMENTS_PER_MESSAGE: usize = 10;
type FileIndex = HashMap<String, Vec<PathBuf>>;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Export {
    guild: GuildInfo,
    channel: ChannelInfo,
    messages: Vec<MessageInfo>,
}
#[derive(Deserialize)]
struct GuildInfo {
    name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChannelInfo {
    name: String,
    category: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageInfo {
    content: String,
    author: Author,
    timestamp: String,
    #[serde(default)]
    timestamp_edited: Option<String>,
    attachments: Vec<AttachmentInfo>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Author {
    id: UserId,
    name: String,
    avatar_url: String,
    color: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttachmentInfo {
    url: String,
    file_name: String,
}
enum ImageSource {
    Local(PathBuf, String),
    Remote(String),
}
fn is_image_filename(filename: &str) -> bool {
    SUPPORTED_IMAGE_EXTENSIONS
        .iter()
        .any(|ext| filename.to_ascii_lowercase().ends_with(ext))
}
fn parse_hex_color(hex_string: &str) -> Option<u32> {
    u32::from_str_radix(hex_string.trim_start_matches('#'), 16).ok()
}
fn build_file_index(search_paths: &[PathBuf]) -> FileIndex {
    let mut index = FileIndex::new();
    for root in search_paths {
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            index
                .entry(entry.file_name().to_string_lossy().to_ascii_lowercase())
                .or_default()
                .push(entry.path().to_path_buf());
        }
    }
    index
}
fn find_media_directories(media_root: &Path, export_name: &str) -> Vec<PathBuf> {
    if !fs::read_dir(media_root).ok().map_or(false, |mut dir| {
        dir.any(|e| {
            e.ok()
                .map_or(false, |de| de.file_type().map_or(false, |ft| ft.is_dir()))
        })
    }) {
        return vec![media_root.to_path_buf()];
    }
    let mut search_paths = Vec::new();
    for dir_name in ["avatars", "emojis", "icons"] {
        let path = media_root.join(dir_name);
        if path.is_dir() {
            search_paths.push(path);
        }
    }
    let channels_root = media_root.join("channels");
    if channels_root.is_dir() {
        let channel_specific_path = channels_root.join(export_name);
        search_paths.push(if channel_specific_path.is_dir() {
            channel_specific_path
        } else {
            channels_root
        });
    }
    if search_paths.is_empty() {
        search_paths.push(media_root.to_path_buf());
    }
    search_paths
}
fn build_media_index(media_path: &Option<String>, json_path: &str) -> Option<FileIndex> {
    media_path.as_ref().map(|path_str| {
        let export_name = Path::new(json_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let search_paths = find_media_directories(Path::new(path_str), export_name);
        build_file_index(&search_paths)
    })
}
fn create_base_embed(
    message: &MessageInfo,
    export: &Export,
    avatar_filename: Option<&String>,
) -> CreateEmbed {
    let mut author_builder = CreateEmbedAuthor::new(&message.author.name)
        .url(format!("https://discord.com/users/{}", message.author.id));
    if let Some(filename) = avatar_filename {
        author_builder = author_builder.icon_url(format!("attachment://{filename}"));
    } else {
        author_builder = author_builder.icon_url(&message.author.avatar_url);
    }
    let footer_text = format!(
        "{} | {} | {}",
        export.guild.name,
        export.channel.category.as_deref().unwrap_or(""),
        export.channel.name
    );
    let timestamp_str = message
        .timestamp_edited
        .as_deref()
        .unwrap_or(&message.timestamp);
    let mut embed = CreateEmbed::new()
        .author(author_builder)
        .footer(CreateEmbedFooter::new(footer_text))
        .timestamp(Timestamp::parse(timestamp_str).unwrap_or_else(|_| Timestamp::now()));
    if let Some(color_value) = message
        .author
        .color
        .as_ref()
        .and_then(|s| parse_hex_color(s))
    {
        embed = embed.color(color_value);
    }
    embed
}
fn find_author_avatar_file(
    author_id: &UserId,
    file_index: &FileIndex,
) -> Option<(PathBuf, String)> {
    SUPPORTED_IMAGE_EXTENSIONS.iter().find_map(|ext| {
        let filename = format!("{author_id}.{ext}");
        file_index
            .get(&filename)
            .and_then(|paths| paths.first())
            .map(|path| (path.clone(), filename))
    })
}
fn find_file_variants(
    filename: &str,
    file_index: &FileIndex,
    seen_paths: &mut HashSet<PathBuf>,
) -> Vec<(PathBuf, String)> {
    let mut found_files = Vec::new();
    if let Some(path) = file_index
        .get(&filename.to_ascii_lowercase())
        .and_then(|paths| paths.iter().find(|p| seen_paths.insert((*p).clone())))
    {
        found_files.push((path.clone(), filename.to_string()));
        if let (Some(dir), Some(stem), Some(ext)) = (
            path.parent(),
            path.file_stem().and_then(|s| s.to_str()),
            path.extension().and_then(|s| s.to_str()),
        ) {
            for i in 1.. {
                let variant_path = dir.join(format!("{stem}_{i:03}.{ext}"));
                if !variant_path.exists() {
                    break;
                }
                if seen_paths.insert(variant_path.clone()) {
                    found_files.push((
                        variant_path.clone(),
                        variant_path
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                    ));
                }
            }
        }
    }
    found_files
}
fn collect_image_sources(
    message: &MessageInfo,
    file_index: &Option<FileIndex>,
    seen_paths: &mut HashSet<PathBuf>,
) -> Vec<ImageSource> {
    let mut sources = Vec::new();
    for attachment_info in &message.attachments {
        if !is_image_filename(&attachment_info.file_name) {
            continue;
        }
        let mut found_local = false;
        if let Some(index) = file_index {
            for (path, filename) in
                find_file_variants(&attachment_info.file_name, index, seen_paths)
            {
                sources.push(ImageSource::Local(path, filename));
                found_local = true;
            }
        }
        if !found_local {
            sources.push(ImageSource::Remote(attachment_info.url.clone()));
        }
    }
    sources
}
async fn prepare_message_batch<'a>(
    images: &'a [ImageSource],
    base_embed: &CreateEmbed,
    author_avatar_file: &Option<(PathBuf, String)>,
    is_first_message_batch: bool,
    content: &str,
    embed_url: &str,
) -> (Vec<CreateAttachment>, Vec<CreateEmbed>, usize) {
    let mut attachments = Vec::new();
    let mut embeds = Vec::new();
    let mut images_processed = 0;
    if is_first_message_batch {
        if let Some((avatar_path, _)) = author_avatar_file {
            if let Ok(attachment) = CreateAttachment::path(avatar_path).await {
                attachments.push(attachment);
            }
        }
    }
    for source in images {
        if embeds.len() >= MAX_EMBEDS_PER_MESSAGE {
            break;
        }
        if let ImageSource::Local(_, _) = source {
            if attachments.len() >= MAX_ATTACHMENTS_PER_MESSAGE {
                break;
            }
        }
        let mut embed_builder = if images_processed == 0 && is_first_message_batch {
            let mut embed = base_embed.clone();
            if !content.is_empty() {
                embed = embed.description(content);
            }
            embed
        } else {
            CreateEmbed::new()
        };
        embed_builder = embed_builder.url(embed_url);
        match source {
            ImageSource::Local(path, filename) => {
                if let Ok(attachment) = CreateAttachment::path(path).await {
                    attachments.push(attachment);
                    embed_builder = embed_builder.image(format!("attachment://{filename}"));
                } else {
                    continue;
                }
            }
            ImageSource::Remote(url) => {
                embed_builder = embed_builder.image(url.clone());
            }
        }
        embeds.push(embed_builder);
        images_processed += 1;
    }
    (attachments, embeds, images_processed)
}
async fn send_text_message(
    ctx: &Context,
    channel_id: &ChannelId,
    message: &MessageInfo,
    base_embed: CreateEmbed,
    author_avatar_file: &Option<(PathBuf, String)>,
) {
    if message.content.is_empty() && author_avatar_file.is_none() {
        return;
    }
    let embed_builder = base_embed.description(&message.content);
    let mut message_builder = CreateMessage::new().embed(embed_builder);
    if let Some((avatar_path, _)) = author_avatar_file {
        if let Ok(attachment) = CreateAttachment::path(avatar_path).await {
            message_builder = message_builder.add_file(attachment);
        }
    }
    let _ = channel_id.send_message(&ctx.http, message_builder).await;
}
async fn send_image_messages(
    ctx: &Context,
    channel_id: &ChannelId,
    message: &MessageInfo,
    base_embed: CreateEmbed,
    image_sources: Vec<ImageSource>,
    author_avatar_file: Option<(PathBuf, String)>,
    embed_url: String,
) {
    let mut remaining_images: &[ImageSource] = &image_sources;
    let mut is_first_message_batch = true;
    while !remaining_images.is_empty() {
        let (attachments, embeds, images_processed) = prepare_message_batch(
            remaining_images,
            &base_embed,
            &author_avatar_file,
            is_first_message_batch,
            &message.content,
            &embed_url,
        )
        .await;
        if !embeds.is_empty() {
            let _ = channel_id
                .send_message(
                    &ctx.http,
                    CreateMessage::new().embeds(embeds).add_files(attachments),
                )
                .await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        remaining_images = &remaining_images[images_processed..];
        is_first_message_batch = false;
    }
}
async fn process_message(
    ctx: &Context,
    channel_id: &ChannelId,
    message: &MessageInfo,
    export: &Export,
    file_index: &Option<FileIndex>,
    seen_paths: &mut HashSet<PathBuf>,
) {
    let author_avatar_file = file_index
        .as_ref()
        .and_then(|index| find_author_avatar_file(&message.author.id, index));
    let image_sources = collect_image_sources(message, file_index, seen_paths);
    let base_embed = create_base_embed(
        message,
        export,
        author_avatar_file.as_ref().map(|(_, name)| name),
    );
    if image_sources.is_empty() {
        send_text_message(ctx, channel_id, message, base_embed, &author_avatar_file).await;
    } else {
        let embed_url = format!("https://discord.com/users/{}", message.author.id);
        send_image_messages(
            ctx,
            channel_id,
            message,
            base_embed,
            image_sources,
            author_avatar_file,
            embed_url,
        )
        .await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
}
fn load_export_data(json_path: &str) -> Result<Export, String> {
    let content =
        fs::read_to_string(json_path).map_err(|e| format!("Error reading JSON file: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("Error parsing JSON: {e}"))
}
fn parse_import_args(args: &str) -> (String, Option<String>) {
    let mut trimmed_args = args.trim();
    if trimmed_args.starts_with('"') {
        trimmed_args = &trimmed_args[1..];
        if let Some(end_quote_pos) = trimmed_args.find('"') {
            let first_part = trimmed_args[..end_quote_pos].to_string();
            let remaining_part = trimmed_args[end_quote_pos + 1..].trim();
            return (
                first_part,
                if remaining_part.is_empty() {
                    None
                } else {
                    Some(remaining_part.trim_matches('"').to_string())
                },
            );
        }
    }
    match trimmed_args.split_once(' ') {
        Some((first, second)) => (
            first.to_string(),
            if second.trim().is_empty() {
                None
            } else {
                Some(second.trim_matches('"').to_string())
            },
        ),
        None => (trimmed_args.to_string(), None),
    }
}
async fn handle_import_command(ctx: &Context, msg: &Message, args: &str) {
    let (json_path, media_path) = parse_import_args(args);
    if json_path.is_empty() {
        let _ = msg
            .reply(&ctx, "Command requires a path to a JSON file.")
            .await;
        return;
    }
    let export = match load_export_data(&json_path) {
        Ok(data) => data,
        Err(e) => {
            let _ = msg.reply(&ctx, e).await;
            return;
        }
    };
    let _ = msg
        .channel_id
        .say(
            &ctx,
            format!("Importing {} messages…", export.messages.len()),
        )
        .await;
    let file_index = build_media_index(&media_path, &json_path);
    let mut seen_paths = HashSet::new();
    for message in &export.messages {
        process_message(
            ctx,
            &msg.channel_id,
            message,
            &export,
            &file_index,
            &mut seen_paths,
        )
        .await;
    }
    let completion_message = format!(
        "Successfully imported {} | {} | {}",
        export.guild.name,
        export.channel.category.as_deref().unwrap_or(""),
        export.channel.name
    );
    let _ = msg.channel_id.say(&ctx, completion_message).await;
}
struct Handler;
#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} connected", ready.user.name);
    }
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }
        if let Some(args) = msg.content.strip_prefix("/import ") {
            handle_import_command(&ctx, &msg, args).await;
        }
    }
}
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("Expected DISCORD_TOKEN in environment");
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;
    Client::builder(token, intents)
        .event_handler(Handler)
        .await
        .expect("Error creating client")
        .start()
        .await
        .expect("Error running client");
}
