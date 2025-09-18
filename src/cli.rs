use crate::models::*;
use crate::utils::*;
use poise::serenity_prelude::{self as serenity, EditMessage};
use std::{collections::HashSet, path::PathBuf};
use tokio::time;
fn build_completion_message(
    export: &Export,
    no_guild: bool,
    no_category: bool,
    no_channel: bool,
) -> String {
    let footer_text = generate_footer(export, no_guild, no_category, no_channel);
    if footer_text.is_empty() {
        "Successfully imported".to_string()
    } else {
        format!("Successfully imported {footer_text}")
    }
}
fn select_messages(
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
fn split_args(input: &str) -> Vec<String> {
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
fn parse_options(arguments: &[String]) -> Result<ImportOptions, String> {
    fn parse_option(arguments: &[String], index: &mut usize, flag: &str) -> Result<usize, String> {
        *index += 1;
        if *index < arguments.len() {
            arguments[*index]
                .parse::<usize>()
                .map_err(|_| format!("Invalid value for {flag}"))
        } else {
            Err(format!("Missing value for {flag}"))
        }
    }
    let mut options = ImportOptions::default();
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        match argument.as_str() {
            "--no-guild" => options.no_guild = true,
            "--no-category" => options.no_category = true,
            "--no-channel" => options.no_channel = true,
            "--no-timestamp" => options.no_timestamp = true,
            "--no-mentions" => options.no_mentions = true,
            "--no-reactions" => options.no_reactions = true,
            "--no-embed" => options.no_embed = true,
            "--button" => options.button = true,
            "--reaction-users" => options.reaction_users = true,
            "--outside" => options.outside = true,
            "--disable-button" => options.disable_button = true,
            "--range" => {
                index += 1;
                if index < arguments.len() {
                    if let Some((start, end)) = arguments[index].split_once(',') {
                        options.range_start = Some(
                            start
                                .parse()
                                .map_err(|_| "Invalid start value in --range")?,
                        );
                        options.range_end =
                            Some(end.parse().map_err(|_| "Invalid end value in --range")?);
                    } else {
                        return Err("Invalid format for --range. Use start,end".to_string());
                    }
                } else {
                    return Err("Missing value for --range".to_string());
                }
            }
            "--range-start" => {
                options.range_start = Some(parse_option(arguments, &mut index, "--range-start")?);
            }
            "--range-end" => {
                options.range_end = Some(parse_option(arguments, &mut index, "--range-end")?);
            }
            "--first" => {
                options.first = Some(parse_option(arguments, &mut index, "--first")?);
            }
            "--last" => {
                options.last = Some(parse_option(arguments, &mut index, "--last")?);
            }
            unknown => return Err(format!("Unknown option: {unknown}")),
        }
        index += 1;
    }
    if options.no_reactions && options.button {
        return Err("--no-reactions and --button cannot be used together".to_string());
    }
    if options.disable_button && !options.button {
        return Err("--disable-button can only be used with --button".to_string());
    }
    if options.no_embed && !options.outside {
        return Err("--no-embed can only be used with --outside".to_string());
    }
    Ok(options)
}
fn set_cancellation(ctx: &Context<'_>, value: bool) {
    let mut lock = ctx.data().cancellation_flags.lock().unwrap();
    lock.insert(ctx.channel_id(), value);
}
fn remove_cancellation(ctx: &Context<'_>) {
    let mut lock = ctx.data().cancellation_flags.lock().unwrap();
    lock.remove(&ctx.channel_id());
}
fn is_cancelled(ctx: &Context<'_>) -> bool {
    ctx.data()
        .cancellation_flags
        .lock()
        .unwrap()
        .get(&ctx.channel_id())
        .cloned()
        .unwrap_or(false)
}
async fn show_reaction_users(ctx: Context<'_>, reaction_users: bool, reactions: &[ReactionInfo]) {
    if !reaction_users || reactions.is_empty() {
        return;
    }
    let reaction_content = format_reaction_users(reactions);
    if reaction_content.is_empty() {
        return;
    }
    let _ = ctx.say(format!("Reactions:\n{}", reaction_content)).await;
    time::sleep(MESSAGE_DELAY).await;
}
async fn attach_author_avatar(
    reply: poise::CreateReply,
    author_avatar_file: &Option<(PathBuf, String)>,
) -> poise::CreateReply {
    if let Some((path, _)) = author_avatar_file {
        if let Ok(att) = serenity::CreateAttachment::path(path).await {
            return reply.attachment(att);
        }
    }
    reply
}
async fn send_reply(ctx: Context<'_>, reply: poise::CreateReply) -> Option<serenity::Message> {
    let msg = ctx.send(reply).await.ok()?.into_message().await.ok()?;
    time::sleep(MESSAGE_DELAY).await;
    Some(msg)
}
fn add_embeds_to_reply(
    mut reply: poise::CreateReply,
    embeds: Vec<serenity::CreateEmbed>,
) -> poise::CreateReply {
    for embed in embeds {
        reply = reply.embed(embed);
    }
    reply
}
fn add_attachments_to_reply(
    mut reply: poise::CreateReply,
    attachments: Vec<serenity::CreateAttachment>,
) -> poise::CreateReply {
    for attachment in attachments {
        reply = reply.attachment(attachment);
    }
    reply
}
async fn prepare_batch(
    images: &[MediaSource],
    base_embed: &serenity::CreateEmbed,
    author_avatar_file: &Option<(PathBuf, String)>,
    is_first_batch: bool,
    content: &str,
    embed_url: &str,
) -> MessageBatch {
    let mut attachments = Vec::new();
    let mut embeds = Vec::new();
    let mut images_processed = 0;
    if is_first_batch {
        if let Some((avatar_path, _)) = author_avatar_file {
            if let Ok(attachment) = serenity::CreateAttachment::path(avatar_path).await {
                attachments.push(attachment);
            }
        }
    }
    for source in images {
        if embeds.len() >= MAX_EMBEDS {
            break;
        }
        if matches!(source, MediaSource::Local(..)) && attachments.len() >= MAX_ATTACHMENTS {
            break;
        }
        let mut embed_builder = if images_processed == 0 && is_first_batch {
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
            MediaSource::Local(path, filename) => {
                if let Ok(attachment) = serenity::CreateAttachment::path(path).await {
                    attachments.push(attachment);
                    embed_builder = embed_builder.image(format!("attachment://{filename}"));
                } else {
                    continue;
                }
            }
            MediaSource::Remote(url) => {
                embed_builder = embed_builder.image(url.clone());
            }
        }
        embeds.push(embed_builder);
        images_processed += 1;
    }
    MessageBatch {
        attachments,
        embeds,
        count: images_processed,
    }
}
async fn send_text_message(
    ctx: Context<'_>,
    message: &MessageInfo,
    base_embed: serenity::CreateEmbed,
    author_avatar_file: &Option<(PathBuf, String)>,
    no_mentions: bool,
    button: bool,
    reaction_users: bool,
    reactions: &[ReactionInfo],
    disable_button: bool,
) -> Option<serenity::Message> {
    let content = replace_mentions(&message.content, &message.mentions, no_mentions);
    if content.is_empty() && author_avatar_file.is_none() {
        return None;
    }
    let embed_builder = base_embed.description(&content);
    let reply = poise::CreateReply::default().embed(embed_builder);
    let reply = attach_author_avatar(reply, author_avatar_file).await;
    let reply = with_reaction_buttons(reply, button, reactions, disable_button);
    let msg = send_reply(ctx, reply).await?;
    show_reaction_users(ctx, reaction_users, reactions).await;
    Some(msg)
}
async fn send_image_messages(
    ctx: Context<'_>,
    message: &MessageInfo,
    base_embed: serenity::CreateEmbed,
    image_sources: Vec<MediaSource>,
    author_avatar_file: Option<(PathBuf, String)>,
    embed_url: String,
    no_mentions: bool,
    button: bool,
    reaction_users: bool,
    reactions: &[ReactionInfo],
    disable_button: bool,
) -> Option<serenity::Message> {
    let content = replace_mentions(&message.content, &message.mentions, no_mentions);
    let mut remaining_images: &[MediaSource] = &image_sources;
    let mut is_first_batch = true;
    let mut last_msg: Option<serenity::Message> = None;
    while !remaining_images.is_empty() {
        let batch = prepare_batch(
            remaining_images,
            &base_embed,
            &author_avatar_file,
            is_first_batch,
            &content,
            &embed_url,
        )
        .await;
        if !batch.embeds.is_empty() {
            let mut reply = poise::CreateReply::default();
            reply = add_embeds_to_reply(reply, batch.embeds);
            reply = add_attachments_to_reply(reply, batch.attachments);
            if remaining_images.len() <= batch.count {
                reply = with_reaction_buttons(reply, button, reactions, disable_button);
            }
            if let Some(msg) = send_reply(ctx, reply).await {
                last_msg = Some(msg);
            }
        }
        if batch.count == 0 {
            break;
        }
        remaining_images = &remaining_images[batch.count..];
        is_first_batch = false;
    }
    show_reaction_users(ctx, reaction_users, reactions).await;
    last_msg
}
async fn send_attachment_batch(
    ctx: Context<'_>,
    attachments: Vec<serenity::CreateAttachment>,
    content: Option<String>,
    button: bool,
    reactions: &[ReactionInfo],
    disable_button: bool,
) -> Option<serenity::Message> {
    let mut reply = poise::CreateReply::default();
    if let Some(c) = content {
        reply = reply.content(c);
    }
    reply = add_attachments_to_reply(reply, attachments);
    reply = with_reaction_buttons(reply, button, reactions, disable_button);
    send_reply(ctx, reply).await
}
async fn send_outside_message(
    ctx: Context<'_>,
    message: &MessageInfo,
    base_embed: Option<serenity::CreateEmbed>,
    attachment_sources: Vec<MediaSource>,
    author_avatar_file: Option<(PathBuf, String)>,
    no_mentions: bool,
    button: bool,
    reaction_users: bool,
    reactions: &[ReactionInfo],
    disable_button: bool,
) -> Option<serenity::Message> {
    let mut locals: Vec<serenity::CreateAttachment> = Vec::new();
    let mut remotes: Vec<String> = Vec::new();
    for source in attachment_sources {
        match source {
            MediaSource::Local(path, _) => {
                if let Ok(attachment) = serenity::CreateAttachment::path(&path).await {
                    locals.push(attachment);
                }
            }
            MediaSource::Remote(url) => remotes.push(url),
        }
    }
    let mut content = replace_mentions(&message.content, &message.mentions, no_mentions);
    if !remotes.is_empty() {
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&remotes.join("\n"));
    }
    let mut last_attachment_msg: Option<serenity::Message> = None;
    if let Some(embed) = base_embed {
        let reply = poise::CreateReply::default().embed(embed);
        let reply = attach_author_avatar(reply, &author_avatar_file).await;
        if let Some(metadata_msg) = send_reply(ctx, reply).await {
            last_attachment_msg = Some(metadata_msg);
        }
    }
    if !content.is_empty() || !locals.is_empty() {
        let mut remaining_locals = locals;
        let batch_content = if !content.is_empty() {
            Some(content)
        } else {
            None
        };
        let batch_size = MAX_ATTACHMENTS.min(remaining_locals.len());
        let batch: Vec<serenity::CreateAttachment> =
            remaining_locals.drain(0..batch_size).collect();
        if let Some(msg) =
            send_attachment_batch(ctx, batch, batch_content, button, reactions, disable_button)
                .await
        {
            last_attachment_msg = Some(msg);
        }
        while !remaining_locals.is_empty() {
            let batch_size = MAX_ATTACHMENTS.min(remaining_locals.len());
            let batch: Vec<serenity::CreateAttachment> =
                remaining_locals.drain(0..batch_size).collect();
            if let Some(msg) =
                send_attachment_batch(ctx, batch, None, button, reactions, disable_button).await
            {
                last_attachment_msg = Some(msg);
            }
        }
    }
    if let Some(msg) = &last_attachment_msg {
        if button && !reactions.is_empty() {
            let buttons = create_buttons(reactions, disable_button);
            if !buttons.is_empty() {
                let edit_builder = serenity::EditMessage::new()
                    .components(vec![serenity::CreateActionRow::Buttons(buttons)]);
                let _ = msg.clone().edit(ctx, edit_builder).await;
            }
        }
    }
    show_reaction_users(ctx, reaction_users, reactions).await;
    last_attachment_msg
}
async fn add_reactions(ctx: Context<'_>, message: &serenity::Message, reactions: &[ReactionInfo]) {
    let reaction_types = create_reactions(reactions);
    for reaction_type in reaction_types {
        let _ = message.react(&ctx, reaction_type).await;
        time::sleep(MESSAGE_DELAY).await;
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
    no_mentions: bool,
    no_reactions: bool,
    no_embed: bool,
    button: bool,
    reaction_users: bool,
    outside: bool,
    disable_button: bool,
) {
    let author_avatar_file = if no_embed {
        None
    } else {
        file_index
            .as_ref()
            .and_then(|index| find_avatar(&message.author.id, index))
    };
    let last_sent_message = if outside {
        let attachment_sources = collect_sources(message, file_index, seen_paths, |_| true);
        let base_embed = if no_embed {
            None
        } else {
            Some(create_embed_base(
                message,
                export,
                author_avatar_file.as_ref().map(|(_, name)| name),
                no_guild,
                no_category,
                no_channel,
                no_timestamp,
            ))
        };
        send_outside_message(
            ctx,
            message,
            base_embed,
            attachment_sources,
            author_avatar_file,
            no_mentions,
            button,
            reaction_users,
            &message.reactions,
            disable_button,
        )
        .await
    } else {
        let image_sources = collect_sources(message, file_index, seen_paths, |att| {
            is_image_file(&att.file_name)
        });
        let base_embed = create_embed_base(
            message,
            export,
            author_avatar_file.as_ref().map(|(_, name)| name),
            no_guild,
            no_category,
            no_channel,
            no_timestamp,
        );
        if image_sources.is_empty() {
            send_text_message(
                ctx,
                message,
                base_embed,
                &author_avatar_file,
                no_mentions,
                button,
                reaction_users,
                &message.reactions,
                disable_button,
            )
            .await
        } else {
            let author_id = message.author.id;
            let embed_url = user_profile_url(author_id);
            send_image_messages(
                ctx,
                message,
                base_embed,
                image_sources,
                author_avatar_file,
                embed_url,
                no_mentions,
                button,
                reaction_users,
                &message.reactions,
                disable_button,
            )
            .await
        }
    };
    if let Some(sent_msg) = last_sent_message {
        if !button && !no_reactions && !message.reactions.is_empty() {
            add_reactions(ctx, &sent_msg, &message.reactions).await;
        }
    }
}
#[poise::command(prefix_command)]
pub async fn import(ctx: Context<'_>, #[rest] args: String) -> Result<(), Error> {
    let argument_tokens = split_args(&args);
    if argument_tokens.is_empty() || argument_tokens[0].trim().is_empty() {
        ctx.say("Command requires a path to a JSON file.").await?;
        return Ok(());
    }
    let json_path = argument_tokens[0].clone();
    let (media_path, options_tokens) = if argument_tokens.len() > 1 {
        let next_str = &argument_tokens[1];
        if next_str.starts_with("--") {
            (None, &argument_tokens[1..])
        } else {
            (Some(next_str.clone()), &argument_tokens[2..])
        }
    } else {
        (None, &argument_tokens[0..0])
    };
    let options = match parse_options(options_tokens) {
        Ok(opts) => opts,
        Err(e) => {
            ctx.say(format!("Error parsing options: {e}")).await?;
            return Ok(());
        }
    };
    let export = match load_export(&json_path).await {
        Ok(data) => data,
        Err(e) => {
            let _ = ctx.say(e).await;
            return Ok(());
        }
    };
    let messages_to_process = select_messages(
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
        .say(format!(
            "Importing {} messages...",
            messages_to_process.len()
        ))
        .await?;
    let file_index = create_file_index(&media_path, &json_path);
    let mut seen_paths = HashSet::new();
    set_cancellation(&ctx, false);
    let mut cancelled = false;
    for message in messages_to_process {
        if is_cancelled(&ctx) {
            cancelled = true;
            break;
        }
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
            options.no_mentions,
            options.no_reactions,
            options.no_embed,
            options.button,
            options.reaction_users,
            options.outside,
            options.disable_button,
        )
        .await;
    }
    remove_cancellation(&ctx);
    let message = if cancelled {
        "Import cancelled".to_string()
    } else {
        build_completion_message(
            &export,
            options.no_guild,
            options.no_category,
            options.no_channel,
        )
    };
    let _ = ctx.say(message).await?;
    Ok(())
}
#[poise::command(prefix_command, slash_command)]
pub async fn cancel(ctx: Context<'_>, ephemeral: bool) -> Result<(), Error> {
    let should_cancel;
    {
        let mut lock = ctx.data().cancellation_flags.lock().unwrap();
        if lock.contains_key(&ctx.channel_id()) {
            lock.insert(ctx.channel_id(), true);
            should_cancel = true;
        } else {
            should_cancel = false;
        }
    }
    let message = if should_cancel {
        "Cancelling import..."
    } else {
        "No ongoing import in this channel."
    };
    ctx.send(
        poise::CreateReply::default()
            .content(message)
            .ephemeral(ephemeral),
    )
    .await?;
    Ok(())
}
#[poise::command(slash_command)]
pub async fn help(ctx: Context<'_>, ephemeral: bool) -> Result<(), Error> {
    let help_text = r#"
# Dimport
`/import <json_path> <media_path> [options]`
Imports messages from JSON files generated by [DiscordChatExporter](https://github.com/Tyrrrz/DiscordChatExporter) and replaces expired links with media files downloaded by [Dimage](https://github.com/Inc44/Dimage).
- `<json_path>`: Path to the DiscordChatExporter JSON file (required).
- `<media_path>`: Path to the directory containing downloaded media files (optional).
Options:
- `--no-guild`: Hide guild/server name from message footer.
- `--no-category`: Hide category name from message footer.
- `--no-channel`: Hide channel name from message footer.
- `--no-timestamp`: Hide message timestamps.
- `--no-mentions`: Skip converting @mentions to clickable Discord mentions.
- `--no-reactions`: Skip importing reactions entirely.
- `--no-embed`: Skip creating embeds (only works with `--outside`).
- `--button`: Display reactions as interactive buttons instead of native Discord reactions.
- `--reaction-users`: Show detailed list of users who reacted to each message.
- `--outside`: Send metadata embed separately from attachments.
- `--disable-button`: Make reaction buttons unclickable (only works with `--button`).
- `--range <start,end>`: Import messages within specified range (zero-indexed).
- `--range-start <n>`: Set starting message index for import range.
- `--range-end <n>`: Set ending message index for import range.
- `--first <n>`: Import only the first N messages.
- `--last <n>`: Import only the last N messages.
`/cancel [--ephemeral]`
- Cancels the ongoing import in the current channel.
`/help [--ephemeral]`
- Shows this help message.
For more details, see the project [README](https://github.com/Inc44/Dimport/blob/master/README.md) or [Wiki](https://github.com/Inc44/Dimport/wiki).
"#;
    let handle = ctx
        .send(
            poise::CreateReply::default()
                .content(help_text)
                .ephemeral(ephemeral),
        )
        .await?;
    if let Ok(mut msg) = handle.into_message().await {
        let _ = msg
            .edit(&ctx, EditMessage::new().suppress_embeds(true))
            .await;
    }
    Ok(())
}
