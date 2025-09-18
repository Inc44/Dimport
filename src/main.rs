use crate::cli::{cancel, help, import};
use crate::models::{Data, Error};
use crate::utils::{ask_token, save_token};
use poise::serenity_prelude as serenity;
use std::{env, process};
mod cli;
mod models;
mod utils;
#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenvy::dotenv().ok();
    let token = match env::var("DISCORD_TOKEN") {
        Ok(token) => token,
        Err(_) => {
            let token = ask_token();
            let _ = save_token(&token);
            env::set_var("DISCORD_TOKEN", &token);
            token
        }
    };
    let intents = serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::DIRECT_MESSAGES
        | serenity::GatewayIntents::MESSAGE_CONTENT;
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![import(), cancel(), help()],
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
                Ok(Data::default())
            })
        })
        .build();
    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await
        .expect("Error creating client");
    if let Err(e) = client.start().await {
        match e {
            serenity::Error::Gateway(serenity::GatewayError::InvalidAuthentication) => {
                eprintln!("Expected valid DISCORD_TOKEN in environment");
                process::exit(1);
            }
            other => return Err(other.into()),
        }
    }
    Ok(())
}
