# StarPrison Discord Notices Worker

Cloudflare Worker that reads recent messages from the Discord notice channel and exposes them as launcher-safe JSON.

## Cloudflare settings

Set this secret in Cloudflare, not in GitHub:

```text
DISCORD_BOT_TOKEN=<바봇 토큰>
```

The non-secret variables are already in `wrangler.toml`:

```text
DISCORD_GUILD_ID=1501856308259983511
DISCORD_CHANNEL_ID=1502417316196122684
NOTICE_LIMIT=20
CACHE_TTL_SECONDS=60
ALLOWED_ORIGIN=http://tauri.localhost
```

## Launcher endpoint

After deployment, use the Worker URL with `/notices`:

```text
https://<worker-name>.<account>.workers.dev/notices
```

Put that URL in `config/app.config.json`:

```json
{
  "discordNotices": {
    "enabled": true,
    "endpointUrl": "https://<worker-name>.<account>.workers.dev/notices"
  }
}
```
