use crate::models::*;
use poise::serenity_prelude::{self as serenity};
use std::{
    collections::HashSet,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
const PADDING: &str =
    "\u{2060}\u{200A}\u{2060}\u{200A}\u{2060}\u{200A}\u{2060}\u{200A}\u{2060}\u{200A}\u{2060}";
pub fn is_image_file(filename: &str) -> bool {
    IMAGE_EXTENSIONS
        .iter()
        .any(|ext| filename.to_ascii_lowercase().ends_with(ext))
}
pub fn parse_color(hex: &str) -> Option<u32> {
    u32::from_str_radix(hex.trim_start_matches('#'), 16).ok()
}
pub fn is_url(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}
pub fn extract_export_name(json_path: &str) -> String {
    let last_segment = if is_url(json_path) {
        json_path.rsplit('/').next().unwrap_or("")
    } else {
        json_path
    };
    Path::new(last_segment)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}
pub fn scan_files(paths: &[PathBuf]) -> FileIndex {
    let mut index = FileIndex::new();
    for root in paths {
        for entry in walkdir::WalkDir::new(root)
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
pub fn locate_media_dirs(media_root: &Path, export_name: &str) -> Vec<PathBuf> {
    let has_subdirs = fs::read_dir(media_root).ok().map_or(false, |mut dir| {
        dir.any(|e| {
            e.ok()
                .map_or(false, |de| de.file_type().map_or(false, |ft| ft.is_dir()))
        })
    });
    if !has_subdirs {
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
pub fn is_zip_file(filename: &str) -> bool {
    filename.to_ascii_lowercase().ends_with("zip")
}
fn extract_zip_to_temp(zip_path: &Path) -> Result<tempfile::TempDir, String> {
    let file = fs::File::open(zip_path).map_err(|e| format!("Error opening zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Error reading zip: {e}"))?;
    let tempdir = tempfile::tempdir().map_err(|e| format!("Error creating temp dir: {e}"))?;
    let base_dir = tempdir.path();
    let create_dir =
        |path: &Path| fs::create_dir_all(path).map_err(|e| format!("Create dir failed: {e}"));
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Zip entry error: {e}"))?;
        let output_path = match entry.enclosed_name() {
            Some(path) => base_dir.join(path),
            None => base_dir.join(entry.mangled_name()),
        };
        if entry.is_dir() || entry.name().ends_with('/') {
            create_dir(&output_path)?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            create_dir(parent)?;
        }
        let mut output_file =
            fs::File::create(&output_path).map_err(|e| format!("Create file failed: {e}"))?;
        io::copy(&mut entry, &mut output_file).map_err(|e| format!("Write file failed: {e}"))?;
    }
    Ok(tempdir)
}
async fn fetch_zip_to_tempfile(url: &str) -> Result<tempfile::NamedTempFile, String> {
    let resp = reqwest::get(url)
        .await
        .map_err(|e| format!("Error fetching ZIP: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP error: {}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Error reading ZIP body: {e}"))?;
    let mut tmp =
        tempfile::NamedTempFile::new().map_err(|e| format!("Error creating temp file: {e}"))?;
    tmp.write_all(bytes.as_ref())
        .map_err(|e| format!("Error writing temp ZIP: {e}"))?;
    tmp.flush()
        .map_err(|e| format!("Error flushing temp ZIP: {e}"))?;
    Ok(tmp)
}
pub async fn create_file_index(
    media_path: &Option<String>,
    json_path: &str,
) -> (Option<FileIndex>, Option<tempfile::TempDir>) {
    let path_str = match media_path {
        Some(s) => s,
        None => return (None, None),
    };
    let export_name = extract_export_name(json_path);
    if is_url(path_str) {
        let tmp = match fetch_zip_to_tempfile(path_str).await {
            Ok(t) => t,
            Err(_) => return (None, None),
        };
        let tempdir = match extract_zip_to_temp(tmp.path()) {
            Ok(t) => t,
            Err(_) => return (None, None),
        };
        let search_paths = locate_media_dirs(tempdir.path(), &export_name);
        let index = scan_files(&search_paths);
        return (Some(index), Some(tempdir));
    }
    let path = Path::new(path_str);
    if is_zip_file(path_str) {
        if !path.exists() {
            return (None, None);
        }
        let tempdir = match extract_zip_to_temp(path) {
            Ok(t) => t,
            Err(_) => return (None, None),
        };
        let search_paths = locate_media_dirs(tempdir.path(), &export_name);
        let index = scan_files(&search_paths);
        return (Some(index), Some(tempdir));
    }
    let search_paths = locate_media_dirs(path, &export_name);
    let index = scan_files(&search_paths);
    (Some(index), None)
}
pub fn generate_footer(
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
pub fn user_profile_url(user_id: serenity::UserId) -> String {
    format!("https://discord.com/users/{}", user_id)
}
pub fn create_embed_base(
    message: &MessageInfo,
    export: &Export,
    avatar_filename: Option<&String>,
    current_avatar_url: Option<&str>,
    no_guild: bool,
    no_category: bool,
    no_channel: bool,
    no_timestamp: bool,
    accent_color_value: Option<u32>,
) -> serenity::CreateEmbed {
    let mut author_builder = serenity::CreateEmbedAuthor::new(&message.author.name)
        .url(user_profile_url(message.author.id));
    if let Some(icon_url) = current_avatar_url {
        author_builder = author_builder.icon_url(icon_url.to_string());
    } else if let Some(filename) = avatar_filename {
        author_builder = author_builder.icon_url(format!("attachment://{filename}"));
    } else {
        author_builder = author_builder.icon_url(&message.author.avatar_url);
    }
    let footer_text = generate_footer(export, no_guild, no_category, no_channel);
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
    if let Some(color_value) = accent_color_value {
        embed = embed.color(color_value);
    } else if let Some(color_value) = message.author.color.as_ref().and_then(|s| parse_color(s)) {
        embed = embed.color(color_value);
    }
    embed
}
pub fn find_avatar(
    author_id: &serenity::UserId,
    file_index: &FileIndex,
) -> Option<(PathBuf, String)> {
    IMAGE_EXTENSIONS.iter().find_map(|ext| {
        let filename = format!("{author_id}.{ext}");
        file_index
            .get(&filename)
            .and_then(|paths| paths.first())
            .map(|path| (path.clone(), filename))
    })
}
pub fn find_local_files(
    filename: &str,
    file_index: &FileIndex,
    seen_paths: &mut HashSet<PathBuf>,
) -> Vec<(PathBuf, String)> {
    let mut found_files = Vec::new();
    if let Some(path) = file_index
        .get(&filename.to_ascii_lowercase())
        .and_then(|paths| paths.iter().find(|path| seen_paths.insert((*path).clone())))
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
pub fn collect_sources(
    message: &MessageInfo,
    file_index: &Option<FileIndex>,
    seen_paths: &mut HashSet<PathBuf>,
    filter: impl Fn(&AttachmentInfo) -> bool,
) -> Vec<MediaSource> {
    let mut sources = Vec::new();
    for attachment_info in &message.attachments {
        if !filter(attachment_info) {
            continue;
        }
        let mut found_local = false;
        if let Some(index) = file_index {
            for (path, filename) in find_local_files(&attachment_info.file_name, index, seen_paths)
            {
                sources.push(MediaSource::Local(path, filename));
                found_local = true;
            }
        }
        if !found_local {
            sources.push(MediaSource::Remote(attachment_info.url.clone()));
        }
    }
    sources
}
pub fn replace_mentions(content: &str, mentions: &[Mention], no_mentions: bool) -> String {
    if no_mentions {
        return content.to_string();
    }
    let mut processed_content = content.to_string();
    for mention in mentions {
        let display_name = mention.nickname.as_deref().unwrap_or(&mention.name);
        let mention_pattern = format!("@{}", display_name);
        let clickable_mention = format!("<@{}>", mention.id);
        processed_content = processed_content.replace(&mention_pattern, &clickable_mention);
    }
    processed_content
}
pub fn replace_emojis(content: &str, inline_emojis: &[EmojiInfo]) -> String {
    let mut processed_content = content.to_string();
    for emoji in inline_emojis {
        if let Some(id) = &emoji.id {
            if !id.is_empty() {
                let code = format!(":{}:", emoji.code);
                let formatted_emoji = if emoji.is_animated {
                    format!("<a:{}:{}>", emoji.name, id)
                } else {
                    format!("<:{}:{}>", emoji.name, id)
                };
                processed_content = processed_content.replace(&code, &formatted_emoji);
            }
        }
    }
    processed_content
}
pub fn get_reaction_count(reaction: &ReactionInfo) -> u64 {
    match &reaction.count {
        serde_json::Value::Number(n) => n.as_u64().unwrap_or(1),
        _ => 1,
    }
}
pub fn format_emoji(emoji: &EmojiInfo) -> String {
    if let Some(id) = &emoji.id {
        if !id.is_empty() {
            return if emoji.is_animated {
                format!("<a:{}:{}>", emoji.name, id)
            } else {
                format!("<:{}:{}>", emoji.name, id)
            };
        }
    }
    emoji.name.clone()
}
pub fn format_reaction_users(reactions: &[ReactionInfo]) -> String {
    reactions
        .iter()
        .filter_map(|reaction| {
            let user_mentions: Vec<String> = reaction
                .users
                .iter()
                .filter_map(|user_value| {
                    user_value
                        .as_object()
                        .and_then(|obj| obj.get("id"))
                        .and_then(|id_val| id_val.as_str())
                        .map(|id_str| format!("<@{}>", id_str))
                })
                .collect();
            if user_mentions.is_empty() {
                None
            } else {
                let emoji_display = format_emoji(&reaction.emoji);
                Some(format!("{} : {}", emoji_display, user_mentions.join(", ")))
            }
        })
        .collect::<Vec<String>>()
        .join("\n")
}
pub fn emoji_to_reaction_type(emoji: &EmojiInfo) -> serenity::ReactionType {
    if let Some(id_str) = &emoji.id {
        if let Ok(id) = id_str.parse::<u64>() {
            return serenity::ReactionType::Custom {
                animated: emoji.is_animated,
                id: serenity::EmojiId::new(id),
                name: Some(format_emoji(&emoji)),
            };
        }
    }
    serenity::ReactionType::Unicode(format_emoji(&emoji))
}
pub fn create_buttons(
    reactions: &[ReactionInfo],
    disable_button: bool,
) -> Vec<serenity::CreateButton> {
    reactions
        .iter()
        .map(|reaction| {
            let count = get_reaction_count(reaction);
            let label = format!("{PADDING}{count}");
            let mut button =
                serenity::CreateButton::new(format!("dummy_reaction_{}", reaction.emoji.code))
                    .emoji(emoji_to_reaction_type(&reaction.emoji))
                    .label(label)
                    .style(serenity::ButtonStyle::Secondary);
            if disable_button {
                button = button.disabled(true);
            }
            button
        })
        .collect()
}
pub fn create_reactions(reactions: &[ReactionInfo]) -> Vec<serenity::ReactionType> {
    reactions
        .iter()
        .map(|reaction| emoji_to_reaction_type(&reaction.emoji))
        .collect()
}
pub fn with_reaction_buttons(
    mut reply: poise::CreateReply,
    button: bool,
    reactions: &[ReactionInfo],
    disable_button: bool,
) -> poise::CreateReply {
    if button && !reactions.is_empty() {
        let buttons = create_buttons(reactions, disable_button);
        if !buttons.is_empty() {
            reply = reply.components(vec![serenity::CreateActionRow::Buttons(buttons)]);
        }
    }
    reply
}
pub async fn load_export(json_path: &str) -> Result<Export, String> {
    let content = if is_url(json_path) {
        let resp = reqwest::get(json_path)
            .await
            .map_err(|e| format!("Error fetching JSON: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("HTTP error: {}", resp.status()));
        }
        resp.text()
            .await
            .map_err(|e| format!("Error reading response body: {e}"))?
    } else {
        fs::read_to_string(json_path).map_err(|e| format!("Error reading JSON file: {e}"))?
    };
    serde_json::from_str(&content).map_err(|e| format!("Error parsing JSON: {e}"))
}
pub fn ask_token() -> String {
    print!("Enter DISCORD_TOKEN: ");
    let _ = io::stdout().flush();
    let mut token = String::new();
    io::stdin()
        .read_line(&mut token)
        .expect("stdin read failed");
    let token = token.trim().to_string();
    if token.is_empty() {
        panic!("DISCORD_TOKEN is required");
    }
    token
}
pub fn save_token(token: &str) -> io::Result<()> {
    let path = Path::new(".env");
    let token_line = format!("DISCORD_TOKEN={}\n", token);
    if path.exists() {
        let content = fs::read_to_string(path)?;
        let mut replaced_token = false;
        let mut token_lines = String::new();
        for line in content.lines() {
            if line.starts_with("DISCORD_TOKEN=") {
                token_lines.push_str(&token_line);
                replaced_token = true;
            } else {
                token_lines.push_str(line);
                token_lines.push('\n');
            }
        }
        if !replaced_token {
            token_lines.push_str(&token_line);
        }
        fs::write(path, token_lines)?;
    } else {
        fs::write(path, token_line)?;
    }
    Ok(())
}
