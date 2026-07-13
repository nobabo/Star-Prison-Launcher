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
    } catch (error) {
        console.warn('[공지사항] URL 파싱에 실패했습니다.', { value: rawValue, error })
        return false
    }
}

function noticeErrorMessage(error){
    if(error instanceof Error && error.message.trim().length > 0){
        return error.message
    }

    return String(error || '알 수 없는 공지사항 오류')
}

function logNoticeFailure(message, details = {}){
    console.error(`[공지사항] ${message}`, details)
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

function assertNoticePayload(payload){
    const source = Array.isArray(payload)
        ? payload
        : payload?.notices ?? payload?.messages ?? payload?.cards

    if(!Array.isArray(source)){
        throw new TypeError('Worker 응답에 notices 배열이 없습니다.')
    }
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
        state.noticeError = '공지사항 설정이 비활성화되었거나 endpointUrl이 올바른 HTTPS 주소가 아닙니다.'
        logNoticeFailure(state.noticeError, {
            enabled: config?.enabled,
            endpointUrl: config?.endpointUrl
        })
        return
    }

    const controller = new AbortController()
    const timeoutId = window.setTimeout(() => controller.abort(), NOTICE_REQUEST_TIMEOUT_MS)

    try {
        const requestUrl = buildNoticeEndpointUrl(config.endpointUrl, requestId)

        const response = await fetch(requestUrl, {
            headers: { Accept: 'application/json' },
            cache: 'no-store',
            signal: controller.signal
        })

        if(!response.ok){
            const responseBody = await response.text().catch(error => {
                logNoticeFailure('실패 응답 본문을 읽지 못했습니다.', { error })
                return ''
            })
            throw new Error(`Worker 요청 실패: HTTP ${response.status}${responseBody ? ` - ${responseBody}` : ''}`)
        }

        const payload = await response.json()
        assertNoticePayload(payload)
        const cards = normalizeNoticeCards(payload)

        if(state.noticeRequestId !== requestId){
            console.warn('[공지사항] 더 최신 요청이 있어 현재 응답을 폐기합니다.', {
                requestId,
                latestRequestId: state.noticeRequestId
            })
            return
        }

        state.noticeCards = cards.length > 0 ? cards : null
        state.noticeError = cards.length > 0 ? null : 'Worker 응답의 notices 배열이 비어 있습니다.'

        if(state.noticeError != null){
            console.warn(`[공지사항] ${state.noticeError}`, { endpointUrl: config.endpointUrl, payload })
        }

        if(state.activeView === 'landing' || state.activeView === 'notices'){
            render()
        }
    } catch (error) {
        if(state.noticeRequestId === requestId){
            state.noticeCards = null
            state.noticeError = noticeErrorMessage(error)
            logNoticeFailure('Discord 공지를 불러오지 못했습니다.', {
                endpointUrl: config.endpointUrl,
                requestId,
                error
            })

            if(state.activeView === 'landing' || state.activeView === 'notices'){
                render()
            }
        } else {
            logNoticeFailure('폐기된 공지 요청에서도 오류가 발생했습니다.', {
                endpointUrl: config.endpointUrl,
                requestId,
                latestRequestId: state.noticeRequestId,
                error
            })
        }
    } finally {
        window.clearTimeout(timeoutId)
    }
}
