import { el } from '../components/dom.js'
import { viewHeader } from '../components/terminal.js'

function asPlainText(value){
    return typeof value === 'string' ? value.trim() : ''
}

function configuredFallbackCards(state){
    const fallbackCards = state.bootstrap?.appConfig?.discordNotices?.fallbackCards

    return Array.isArray(fallbackCards) && fallbackCards.length > 0
        ? fallbackCards
        : []
}

function noticeCardsForState(state){
    return Array.isArray(state.noticeCards) && state.noticeCards.length > 0
        ? state.noticeCards
        : configuredFallbackCards(state)
}

function formatPublishedAt(value){
    const rawValue = asPlainText(value)

    if(rawValue.length === 0){
        return ''
    }

    const date = new Date(rawValue)

    if(Number.isNaN(date.getTime())){
        return ''
    }

    return new Intl.DateTimeFormat('ko-KR', {
        dateStyle: 'medium',
        timeStyle: 'short'
    }).format(date)
}

function noticeImage(card){
    const imageUrl = asPlainText(card.imageUrl)

    if(imageUrl.length === 0){
        return null
    }

    return el('img', {
        className: 'notice-card__image',
        attrs: {
            src: imageUrl,
            alt: ''
        }
    })
}

function noticeCard(card){
    const eyebrow = asPlainText(card.eyebrow) || '알림'
    const title = asPlainText(card.title) || '공지사항'
    const body = asPlainText(card.body) || '공지 내용을 불러오는 중입니다.'
    const publishedAt = formatPublishedAt(card.publishedAt)

    return el('article', { className: 'notice-card' },
        noticeImage(card),
        el('div', { className: 'notice-card__content' },
            el('div', { className: 'notice-card__meta' },
                el('span', { className: 'terminal-kicker', text: eyebrow }),
                publishedAt.length > 0
                    ? el('time', {
                        className: 'notice-card__time',
                        text: publishedAt,
                        attrs: { datetime: asPlainText(card.publishedAt) }
                    })
                    : null
            ),
            el('h3', { className: 'notice-card__title', text: title }),
            el('p', { className: 'notice-card__body', text: body })
        )
    )
}

export function renderNoticesView(state){
    const cards = noticeCardsForState(state)

    return el('section', { className: 'terminal-view content-page notices-view' },
        viewHeader('공지사항', '// NOTICES'),
        el('article', { className: 'terminal-panel content-page__panel' },
            el('div', { className: 'content-page__meta-row' },
                el('span', { className: 'content-page__badge', text: 'DISCORD' }),
                el('span', { className: 'content-page__count', text: `${cards.length}건` })
            ),
            el('div', { className: 'notice-list' }, cards.map(noticeCard))
        )
    )
}
