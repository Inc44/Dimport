use poise::serenity_prelude as serenity;
use poise::{CreateReply, Framework, FrameworkOptions};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;
const SUPPORTED_IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "webp", "gif", "avif"];
const MAX_EMBEDS_PER_MESSAGE: usize = 10;
const MAX_ATTACHMENTS_PER_MESSAGE: usize = 10;
type FileIndex = HashMap<String, Vec<PathBuf>>;
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, (), Error>;
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
    id: serenity::UserId,
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
#[derive(Default)]
struct ImportOptions {
    no_guild: bool,
    no_category: bool,
    no_channel: bool,
    no_timestamp: bool,
    range_start: Option<usize>,
    range_end: Option<usize>,
    first: Option<usize>,
    last: Option<usize>,
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
fn build_footer_text(
    export: &Export,
    no_guild: bool,
    no_category: bool,
    no_channel: bool,
) -> String {
    let mut parts = Vec::new();
    if !no_guild {
        parts.push(export.guild.name.as_str());
    }
    if !no_category {
        if let Some(category) = export.channel.category.as_deref() {
            parts.push(category);
        }
    }
    if !no_channel {
        parts.push(export.channel.name.as_str());
    }
    parts.join(" | ")
}
fn create_base_embed(
    message: &MessageInfo,
    export: &Export,
    avatar_filename: Option<&String>,
    no_guild: bool,
    no_category: bool,
    no_channel: bool,
    no_timestamp: bool,
) -> serenity::CreateEmbed {
    let mut author_builder = serenity::CreateEmbedAuthor::new(&message.author.name)
        .url(format!("https://discord.com/users/{}", message.author.id));
    if let Some(filename) = avatar_filename {
        author_builder = author_builder.icon_url(format!("attachment://{filename}"));
    } else {
        author_builder = author_builder.icon_url(&message.author.avatar_url);
    }
    let footer_text = build_footer_text(export, no_guild, no_category, no_channel);
    let timestamp_str = message
        .timestamp_edited
        .as_deref()
        .unwrap_or(&message.timestamp);
    let timestamp = if no_timestamp {
        None
    } else {
        serenity::Timestamp::parse(timestamp_str).ok()
    };
    let mut embed = serenity::CreateEmbed::new().author(author_builder);
    if !footer_text.is_empty() {
        embed = embed.footer(serenity::CreateEmbedFooter::new(footer_text));
    }
    if let Some(ts) = timestamp {
        embed = embed.timestamp(ts);
    }
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
    author_id: &serenity::UserId,
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
    base_embed: &serenity::CreateEmbed,
    author_avatar_file: &Option<(PathBuf, String)>,
    is_first_message_batch: bool,
    content: &str,
    embed_url: &str,
) -> (
    Vec<serenity::CreateAttachment>,
    Vec<serenity::CreateEmbed>,
    usize,
) {
    let mut attachments = Vec::new();
    let mut embeds = Vec::new();
    let mut images_processed = 0;
    if is_first_message_batch {
        if let Some((avatar_path, _)) = author_avatar_file {
            if let Ok(attachment) = serenity::CreateAttachment::path(avatar_path).await {
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
            serenity::CreateEmbed::new()
        };
        embed_builder = embed_builder.url(embed_url);
        match source {
            ImageSource::Local(path, filename) => {
                if let Ok(attachment) = serenity::CreateAttachment::path(path).await {
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
    ctx: Context<'_>,
    message: &MessageInfo,
    base_embed: serenity::CreateEmbed,
    author_avatar_file: &Option<(PathBuf, String)>,
) {
    if message.content.is_empty() && author_avatar_file.is_none() {
        return;
    }
    let embed_builder = base_embed.description(&message.content);
    let mut reply = CreateReply::default().embed(embed_builder);
    if let Some((avatar_path, _)) = author_avatar_file {
        if let Ok(attachment) = serenity::CreateAttachment::path(avatar_path).await {
            reply = reply.attachment(attachment);
        }
    }
    let _ = ctx.send(reply).await;
}
async fn send_image_messages(
    ctx: Context<'_>,
    message: &MessageInfo,
    base_embed: serenity::CreateEmbed,
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
            let mut reply = CreateReply::default();
            for embed in embeds {
                reply = reply.embed(embed);
            }
            for attachment in attachments {
                reply = reply.attachment(attachment);
            }
            let _ = ctx.send(reply).await;
        }
        remaining_images = &remaining_images[images_processed..];
        is_first_message_batch = false;
    }
}
async fn process_message(
    ctx: Context<'_>,
    message: &MessageInfo,
    export: &Export,
    file_index: &Option<FileIndex>,
    seen_paths: &mut HashSet<PathBuf>,
    no_guild: bool,
    no_category: bool,
    no_channel: bool,
    no_timestamp: bool,
) {
    let author_avatar_file = file_index
        .as_ref()
        .and_then(|index| find_author_avatar_file(&message.author.id, index));
    let image_sources = collect_image_sources(message, file_index, seen_paths);
    let base_embed = create_base_embed(
        message,
        export,
        author_avatar_file.as_ref().map(|(_, name)| name),
        no_guild,
        no_category,
        no_channel,
        no_timestamp,
    );
    if image_sources.is_empty() {
        send_text_message(ctx, message, base_embed, &author_avatar_file).await;
    } else {
        let embed_url = format!("https://discord.com/users/{}", message.author.id);
        send_image_messages(
            ctx,
            message,
            base_embed,
            image_sources,
            author_avatar_file,
            embed_url,
        )
        .await;
    }
}
fn load_export_data(json_path: &str) -> Result<Export, String> {
    let content =
        fs::read_to_string(json_path).map_err(|e| format!("Error reading JSON file: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("Error parsing JSON: {e}"))
}
fn build_completion_message(
    export: &Export,
    no_guild: bool,
    no_category: bool,
    no_channel: bool,
) -> String {
    let footer_text = build_footer_text(export, no_guild, no_category, no_channel);
    if footer_text.is_empty() {
        "Successfully imported".to_string()
    } else {
        format!("Successfully imported {footer_text}")
    }
}
fn select_messages_to_process(
    messages: &[MessageInfo],
    range_start: Option<usize>,
    range_end: Option<usize>,
    first: Option<usize>,
    last: Option<usize>,
) -> &[MessageInfo] {
    let len = messages.len();
    if let (Some(s), Some(e)) = (range_start, range_end) {
        if s <= e && s < len {
            return &messages[s..=(e.min(len - 1))];
        }
    } else if let Some(n) = first {
        if n > 0 {
            return &messages[0..n.min(len)];
        }
    } else if let Some(n) = last {
        if n > 0 {
            let start = len.saturating_sub(n);
            return &messages[start..];
        }
    }
    messages
}
fn split_arguments_respecting_quotes(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut inside_quotes = false;
    for c in input.chars() {
        if c == '"' {
            inside_quotes = !inside_quotes;
            continue;
        }
        if c.is_whitespace() && !inside_quotes {
            if !current.is_empty() {
                tokens.push(current);
                current = String::new();
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}
fn parse_import_options_from_arguments(arguments: &[String]) -> Result<ImportOptions, String> {
    let mut options = ImportOptions::default();
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        match argument.as_str() {
            "--no-guild" => options.no_guild = true,
            "--no-category" => options.no_category = true,
            "--no-channel" => options.no_channel = true,
            "--no-timestamp" => options.no_timestamp = true,
            "--range" => {
                index += 1;
                if index < arguments.len() {
                    let range_parts: Vec<&str> = arguments[index].split(',').collect();
                    if range_parts.len() == 2 {
                        options.range_start = Some(
                            range_parts[0]
                                .parse()
                                .map_err(|_| "Invalid start value in --range")?,
                        );
                        options.range_end = Some(
                            range_parts[1]
                                .parse()
                                .map_err(|_| "Invalid end value in --range")?,
                        );
                    } else {
                        return Err("Invalid format for --range. Use start,end".to_string());
                    }
                } else {
                    return Err("Missing value for --range".to_string());
                }
            }
            "--range-start" => {
                index += 1;
                if index < arguments.len() {
                    options.range_start = Some(
                        arguments[index]
                            .parse()
                            .map_err(|_| "Invalid value for --range-start")?,
                    );
                } else {
                    return Err("Missing value for --range-start".to_string());
                }
            }
            "--range-end" => {
                index += 1;
                if index < arguments.len() {
                    options.range_end = Some(
                        arguments[index]
                            .parse()
                            .map_err(|_| "Invalid value for --range-end")?,
                    );
                } else {
                    return Err("Missing value for --range-end".to_string());
                }
            }
            "--first" => {
                index += 1;
                if index < arguments.len() {
                    options.first = Some(
                        arguments[index]
                            .parse()
                            .map_err(|_| "Invalid value for --first")?,
                    );
                } else {
                    return Err("Missing value for --first".to_string());
                }
            }
            "--last" => {
                index += 1;
                if index < arguments.len() {
                    options.last = Some(
                        arguments[index]
                            .parse()
                            .map_err(|_| "Invalid value for --last")?,
                    );
                } else {
                    return Err("Missing value for --last".to_string());
                }
            }
            unknown => return Err(format!("Unknown option: {unknown}")),
        }
        index += 1;
    }
    Ok(options)
}
#[poise::command(prefix_command)]
async fn import(
    ctx: Context<'_>,
    json_path: String,
    media_path: Option<String>,
    #[rest] remaining_arguments: String,
) -> Result<(), Error> {
    if json_path.trim().is_empty() {
        ctx.say("Command requires a path to a JSON file.").await?;
        return Ok(());
    }
    let argument_tokens = split_arguments_respecting_quotes(&remaining_arguments);
    let options = match parse_import_options_from_arguments(&argument_tokens) {
        Ok(opts) => opts,
        Err(e) => {
            ctx.say(format!("Error parsing options: {e}")).await?;
            return Ok(());
        }
    };
    let export = match load_export_data(&json_path) {
        Ok(data) => data,
        Err(e) => {
            let _ = ctx.say(e).await;
            return Ok(());
        }
    };
    let messages_to_process = select_messages_to_process(
        &export.messages,
        options.range_start,
        options.range_end,
        options.first,
        options.last,
    );
    if messages_to_process.is_empty() {
        ctx.say("No messages to import.").await?;
        return Ok(());
    }
    let _ = ctx
        .say(format!("Importing {} messages…", messages_to_process.len()))
        .await?;
    let file_index = build_media_index(&media_path, &json_path);
    let mut seen_paths = HashSet::new();
    for message in messages_to_process {
        process_message(
            ctx,
            message,
            &export,
            &file_index,
            &mut seen_paths,
            options.no_guild,
            options.no_category,
            options.no_channel,
            options.no_timestamp,
        )
        .await;
    }
    let completion_message = build_completion_message(
        &export,
        options.no_guild,
        options.no_category,
        options.no_channel,
    );
    let _ = ctx.say(completion_message).await?;
    Ok(())
}
#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenvy::dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("Expected DISCORD_TOKEN in environment");
    let intents = serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::DIRECT_MESSAGES
        | serenity::GatewayIntents::MESSAGE_CONTENT;
    let framework = Framework::builder()
        .options(FrameworkOptions {
            commands: vec![import()],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("/".into()),
                ..Default::default()
            },
            ..Default::default()
        })
        .setup(|ctx, ready, framework| {
            Box::pin(async move {
                println!("{} connected", ready.user.name);
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(())
            })
        })
        .build();
    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await
        .expect("Error creating client");
    client.start().await.expect("Error running client");
    Ok(())
}
