use serde::Deserialize;
use serenity::{
    async_trait,
    builder::{CreateAttachment, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter, CreateMessage},
    model::{channel::Message, gateway::Ready, id::UserId, Timestamp},
    prelude::*,
};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};
use walkdir::WalkDir;
const IMG: [&str; 6] = ["jpg", "jpeg", "png", "webp", "gif", "avif"];
