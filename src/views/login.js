import { el, iconImage } from '../components/dom.js'

export function buildMineSkinUrls(playerName){
    const trimmedPlayerName = typeof playerName === 'string' ? playerName.trim() : ''

    if(trimmedPlayerName.length === 0){
        return null
    }

    const encodedPlayerName = encodeURIComponent(trimmedPlayerName)

    return {
        head: `https://mineskin.eu/helm/${encodedPlayerName}/160.png`
    }
}

function viewHeader(title, eyebrow){
    return el('header', { className: 'terminal-view__header' },
        el('div', { className: 'terminal-view__heading' },
            eyebrow != null ? el('p', { className: 'terminal-kicker', text: eyebrow }) : null,
            el('h2', { className: 'terminal-view__title', text: title })
        ),
        el('button', {
            className: 'terminal-icon-button terminal-view__back back-to-landing-button',
            attrs: {
                type: 'button',
                'aria-label': '홈으로 돌아가기'
            }
        },
            iconImage('./assets/game.svg')
        )
    )
}

export function renderLoginView(state){
    const { authSummary: auth } = state.bootstrap

    if(auth.signedIn){
        const skinUrls = buildMineSkinUrls(auth.playerName)

        return el('section', { className: 'terminal-view' },
            viewHeader('계정', '프로필'),
            el('article', { className: 'terminal-panel terminal-panel--center' },
                skinUrls != null
                    ? el('div', { className: 'account-summary' },
                        el('div', { className: 'account-avatar-shell' },
                            el('div', { className: 'account-avatar-card account-avatar-card--head' },
                                el('img', {
                                    className: 'account-avatar account-avatar--head',
                                    attrs: {
                                        src: skinUrls.head,
                                        alt: `${auth.playerName} 헤드 스킨`,
                                        loading: 'eager',
                                        decoding: 'sync',
                                        fetchpriority: 'high'
                                    }
                                })
                            )
                        ),
                        el('div', { className: 'account-profile-copy' },
                            el('h4', { className: 'account-name', text: auth.playerName }),
                            el('button', {
                                className: 'secondary-button icon-only-button account-sign-out-button',
                                attrs: {
                                    id: 'sign-out-button',
                                    type: 'button',
                                    'aria-label': '연결 해제',
                                    title: '연결 해제'
                                }
                            }, iconImage('./assets/logout.svg'))
                        )
                    )
                    : el('p', { className: 'plain-copy', text: `${auth.playerName} 계정으로 로그인했습니다.` })
            )
        )
    }

    return el('section', { className: 'terminal-view' },
        viewHeader('연결 필요', '계정'),
        el('article', { className: 'terminal-panel terminal-panel--center' },
            el('p', { className: 'plain-copy compact', text: '마이크로소프트 계정으로 로그인하세요.' }),
            el('div', { className: 'actions account-connect-actions' },
                el('button', {
                    className: 'primary-button icon-only-button',
                    attrs: {
                        id: 'sign-in-button',
                        type: 'button',
                        'aria-label': '계정 연결',
                        title: '계정 연결'
                    }
                }, iconImage('./assets/login.svg'))
            )
        )
    )
}
