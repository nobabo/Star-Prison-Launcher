const DISCORD_API_BASE_URL = 'https://discord.com/api/v10'
const MAX_NOTICE_LIMIT = 20

function jsonResponse(payload, init = {}, env = {}){
    const headers = new Headers(init.headers)
    headers.set('content-type', 'application/json; charset=utf-8')
    headers.set('access-control-allow-origin', env.ALLOWED_ORIGIN || '*')
    headers.set('access-control-allow-methods', 'GET, OPTIONS')
    headers.set('access-control-allow-headers', 'content-type')

    return new Response(JSON.stringify(payload), {
        ...init,
        headers
    })
}

function clampNoticeLimit(value){
    const parsed = Number.parseInt(value, 10)

    if(Number.isNaN(parsed)){
        return 4
    }

    return Math.min(Math.max(parsed, 1), MAX_NOTICE_LIMIT)
}

function positiveInteger(value, fallback){
    const parsed = Number.parseInt(value, 10)

    if(Number.isNaN(parsed) || parsed < 1){
        return fallback
    }

    return parsed
}

function publicAttachmentUrl(message){
    const attachment = message.attachments?.find(item => {
        const contentType = item.content_type || ''
        return contentType.startsWith('image/') || /\.(png|jpe?g|webp|gif)$/i.test(item.url || '')
    })

    return attachment?.url
}

function firstText(...values){
    for(const value of values){
        if(typeof value !== 'string'){
            continue
        }

        const trimmedValue = value.trim()

        if(trimmedValue.length > 0){
            return trimmedValue
        }
    }

    return ''
}

function embedBody(embed){
    if(embed == null || typeof embed !== 'object'){
        return ''
    }

    const fieldText = Array.isArray(embed.fields)
        ? embed.fields
            .map(field => {
                const name = firstText(field.name)
                const value = firstText(field.value)

                if(name.length > 0 && value.length > 0){
                    return `${name}\n${value}`
                }

                return firstText(value, name)
            })
            .filter(Boolean)
            .join('\n')
        : ''

    return firstText(embed.description, fieldText, embed.title)
}

function embedImageUrl(embed){
    if(embed == null || typeof embed !== 'object'){
        return undefined
    }

    return embed.image?.url || embed.thumbnail?.url
}

function componentText(component){
    if(component == null || typeof component !== 'object'){
        return ''
    }

    const ownText = firstText(
        component.content,
        component.text,
        component.label,
        component.title,
        component.description
    )
    const childText = Array.isArray(component.components)
        ? component.components.map(componentText).filter(Boolean).join('\n')
        : ''

    return firstText(ownText, childText)
}

function messageSources(message){
    const sources = [message]

    if(message.referenced_message != null){
        sources.push(message.referenced_message)
    }

    if(Array.isArray(message.message_snapshots)){
        for(const snapshot of message.message_snapshots){
            if(snapshot?.message != null){
                sources.push(snapshot.message)
                continue
            }

            sources.push(snapshot)
        }
    }

    return sources
}

function messageBody(message){
    for(const source of messageSources(message)){
        const embed = source.embeds?.[0]
        const componentTexts = Array.isArray(source.components)
            ? source.components.map(componentText).filter(Boolean).join('\n')
            : ''
        const attachmentText = Array.isArray(source.attachments)
            ? source.attachments.map(attachment => firstText(attachment.description, attachment.filename)).filter(Boolean).join('\n')
            : ''
        const pollText = firstText(source.poll?.question?.text)
        const body = firstText(source.content, embedBody(embed), componentTexts, pollText, attachmentText)

        if(body.length > 0){
            return body
        }
    }

    return ''
}

function messageTitle(message){
    for(const source of messageSources(message)){
        const embed = source.embeds?.[0]
        const title = firstText(embed?.title, source.author?.global_name, source.author?.username)

        if(title.length > 0){
            return title
        }
    }

    return '공지사항'
}

function messageImageUrl(message){
    for(const source of messageSources(message)){
        const imageUrl = publicAttachmentUrl(source) || embedImageUrl(source.embeds?.[0])

        if(imageUrl != null){
            return imageUrl
        }
    }

    return undefined
}

function normalizeMessage(message, env){
    const body = messageBody(message)

    if(body.length === 0){
        return null
    }

    return {
        eyebrow: 'DISCORD',
        title: messageTitle(message),
        body,
        imageUrl: messageImageUrl(message),
        url: env.DISCORD_GUILD_ID
            ? `https://discord.com/channels/${env.DISCORD_GUILD_ID}/${env.DISCORD_CHANNEL_ID}/${message.id}`
            : undefined,
        publishedAt: message.timestamp
    }
}

async function fetchDiscordNotices(env){
    const token = env.DISCORD_BOT_TOKEN
    const channelId = env.DISCORD_CHANNEL_ID

    if(!token || !channelId){
        return {
            ok: false,
            status: 500,
            payload: {
                notices: [],
                error: 'DISCORD_BOT_TOKEN and DISCORD_CHANNEL_ID must be configured.'
            }
        }
    }

    const limit = clampNoticeLimit(env.NOTICE_LIMIT)
    const response = await fetch(`${DISCORD_API_BASE_URL}/channels/${channelId}/messages?limit=${limit}`, {
        headers: {
            authorization: `Bot ${token}`,
            accept: 'application/json'
        }
    })

    if(!response.ok){
        return {
            ok: false,
            status: response.status,
            payload: {
                notices: [],
                error: `Discord API returned ${response.status}.`
            }
        }
    }

    const messages = await response.json()
    const notices = Array.isArray(messages)
        ? messages.map(message => normalizeMessage(message, env)).filter(Boolean)
        : []

    return {
        ok: true,
        status: 200,
        payload: {
            notices,
            source: 'discord',
            channelId
        }
    }
}

async function handleNotices(request, env, ctx){
    const cache = caches.default
    const url = new URL(request.url)
    const cacheKey = new Request(url.toString(), request)
    const skipCache = url.searchParams.has('refresh')
    const cachedResponse = skipCache ? null : await cache.match(cacheKey)

    if(cachedResponse != null){
        return cachedResponse
    }

    const result = await fetchDiscordNotices(env)
    const response = jsonResponse(result.payload, {
        status: result.status,
        headers: {
            'cache-control': result.ok
                ? `public, max-age=${positiveInteger(env.CACHE_TTL_SECONDS, 60)}`
                : 'no-store'
        }
    }, env)

    if(result.ok && !skipCache){
        ctx.waitUntil(cache.put(cacheKey, response.clone()))
    }

    return response
}

export default {
    async fetch(request, env, ctx){
        if(request.method === 'OPTIONS'){
            return new Response(null, {
                status: 204,
                headers: {
                    'access-control-allow-origin': env.ALLOWED_ORIGIN || '*',
                    'access-control-allow-methods': 'GET, OPTIONS',
                    'access-control-allow-headers': 'content-type'
                }
            })
        }

        const url = new URL(request.url)

        if(request.method !== 'GET'){
            return jsonResponse({ error: 'Method not allowed.' }, { status: 405 }, env)
        }

        if(url.pathname === '/' || url.pathname === '/health'){
            return jsonResponse({ ok: true }, { status: 200 }, env)
        }

        if(url.pathname === '/notices'){
            return handleNotices(request, env, ctx)
        }

        return jsonResponse({ error: 'Not found.' }, { status: 404 }, env)
    }
}
