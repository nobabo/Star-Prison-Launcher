const MAX_NOTICE_CARDS = 20
const NOTICE_REQUEST_TIMEOUT_MS = 10_000
export const NOTICE_REFRESH_INTERVAL_MS = 60_000

function asPlainText(value){
    return typeof value === 'string' ? value.trim() : ''
}

function isSafeNoticeUrl(value){
    const rawValue = asPlainText(value)

    if(rawValue.length === 0){
        return false
    }

    try {
        const url = new URL(rawValue)
        return url.protocol === 'https:'
    } catch {
        return false
    }
}

function normalizeNoticeCard(item){
    if(item == null || typeof item !== 'object'){
        return null
    }

    const content = asPlainText(item.body ?? item.content ?? item.message ?? item.description)
    const title = asPlainText(item.title ?? item.author?.username ?? item.username)
    const imageUrl = asPlainText(item.imageUrl ?? item.image_url ?? item.attachments?.[0]?.url)
    const url = asPlainText(item.url ?? item.jumpUrl ?? item.jump_url)

    if(title.length === 0 && content.length === 0){
        return null
    }

    return {
        variant: imageUrl.length > 0 ? 'image' : 'text',
        eyebrow: asPlainText(item.eyebrow ?? item.channelName ?? item.channel_name) || 'DISCORD',
        title: title || '새 공지',
        body: content || '디스코드 공지를 확인해 주세요.',
        imageUrl: imageUrl || undefined,
        url: isSafeNoticeUrl(url) ? url : undefined,
        publishedAt: asPlainText(item.publishedAt ?? item.published_at ?? item.timestamp) || undefined
    }
}

function normalizeNoticeCards(payload){
    const source = Array.isArray(payload)
        ? payload
        : payload?.notices ?? payload?.messages ?? payload?.cards

    if(!Array.isArray(source)){
        return []
    }

    return source
        .map(normalizeNoticeCard)
        .filter(Boolean)
        .slice(0, MAX_NOTICE_CARDS)
}

function buildNoticeEndpointUrl(endpointUrl, requestId){
    const url = new URL(endpointUrl)
    url.searchParams.set('refresh', String(requestId))
    return url.href
}

export async function refreshNoticeCards(state, { render }){
    const config = state.bootstrap?.appConfig?.discordNotices
    const requestId = state.noticeRequestId + 1
    state.noticeRequestId = requestId

    if(config?.enabled !== true || !isSafeNoticeUrl(config.endpointUrl)){
        state.noticeCards = null
        return
    }

    const controller = new AbortController()
    const timeoutId = window.setTimeout(() => controller.abort(), NOTICE_REQUEST_TIMEOUT_MS)

    try {
        const response = await fetch(buildNoticeEndpointUrl(config.endpointUrl, requestId), {
            headers: { Accept: 'application/json' },
            cache: 'no-store',
            signal: controller.signal
        })

        if(!response.ok){
            throw new Error(`HTTP ${response.status}`)
        }

        const cards = normalizeNoticeCards(await response.json())

        if(state.noticeRequestId !== requestId){
            return
        }

        state.noticeCards = cards.length > 0 ? cards : null

        if(state.activeView === 'landing' || state.activeView === 'notices'){
            render()
        }
    } catch {
        if(state.noticeRequestId === requestId){
            state.noticeCards = null

            if(state.activeView === 'landing' || state.activeView === 'notices'){
                render()
            }
        }
    } finally {
        window.clearTimeout(timeoutId)
    }
}
