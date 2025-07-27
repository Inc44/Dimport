## Export

1. Open [DiscordChatExporter](), follow instructions to get the token, paste, and press `Enter`.

![](export1.png)

2. Select `# chat` you want to export.

![](export2.png)

3. Click on the `Download` button.

![](export3.png)

4. Select `JSON` instead of `TXT`, click on the `Export` button, and select the output directory and filename.

![](export4.png)

## Create

1. Go to [My Applications | Discord Developer Portal](https://discord.com/developers/applications) and click on the `New Application` button.

![](create1.png)

2. Select `Name`, check out Discord [Terms of Service](https://support-dev.discord.com/hc/articles/8562894815383-Discord-Developer-Terms-of-Service) and [Developer Policy](https://support-dev.discord.com/hc/articles/8563934450327-Discord-Developer-Policy), and click on the `Create` button.

![](create2.png)

3. Select `Bot` on the left side panel.

![](create3.png)

4. Click on the `Reset Token` button.

![](create4.png)

5. Click on the `Copy` button.

![](create5.png)

6. Switch on `Message Content Intent`.

![](create6.png)

7. Select `OAuth2` on the left side panel.

![](create7.png)

8. Select `bot` inside `Scopes`.

![](create8.png)

9. Select `Send Messages`, `Manage Messages`, `Embed Links`, `Attach Files`, `Read Message History`, and `View Channels` inside `Bot Permissions` (`permissions=125952`).

![](create9.png)

10. Click on the `Copy` button.

![](create10.png)

## Start

1. [Download the executable](https://github.com/Inc44/Dimport/releases).

2. Add to the environment variable or create .env with `DISCORD_TOKEN=your_bot_token` in the current directory.

3. Double click on the `Dimport.exe` file or run from the command line `Dimport`.