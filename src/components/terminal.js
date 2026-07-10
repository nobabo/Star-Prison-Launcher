import { el, iconImage } from './dom.js'

export function viewHeader(title, eyebrow){
    return el('header', { className: 'terminal-view__header' },
        el('div', { className: 'terminal-view__heading' },
            eyebrow != null ? el('p', { className: 'terminal-kicker', text: eyebrow }) : null,
            el('h2', { className: 'terminal-view__title', text: title })
        ),
        el('button', {
            className: 'terminal-icon-button terminal-view__back back-to-landing-button',
            attrs: {
                type: 'button',
                'aria-label': '홈으로 돌아가기',
                title: '홈으로 돌아가기'
            }
        },
            iconImage('./assets/game.svg')
        )
    )
}
