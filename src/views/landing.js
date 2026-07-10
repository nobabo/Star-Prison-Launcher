import { el, iconImage } from '../components/dom.js'

function buildActionMeta(state){
    const { authSummary } = state.bootstrap

    if(authSummary.signedIn){
        if(state.minecraftProcessId != null){
            return {
                action: 'terminate',
                label: '게임 종료',
                iconPath: './assets/login.svg'
            }
        }

        return {
            action: 'launch',
            label: '게임 시작',
            iconPath: './assets/login.svg'
        }
    }

    return {
        action: 'login',
        label: '계정 연결',
        iconPath: './assets/login.svg'
    }
}

function navCell(iconPath, label, dataset){
    return el('button', {
        className: 'center-nav__cell',
        dataset,
        attrs: {
            type: 'button',
            'aria-label': label,
            title: label
        }
    },
        el('img', {
            className: 'center-nav__cell-image',
            attrs: {
                src: iconPath,
                alt: '',
                'aria-hidden': 'true'
            }
        })
    )
}

export function renderLandingView(state){
    const { serverManifest } = state.bootstrap

    if(serverManifest == null){
        return el('section', { className: 'center-stage' },
            el('div', { className: 'center-panel' },
                el('div', { className: 'center-panel__nav', attrs: { 'aria-label': '네비게이션' } },
                    navCell('./assets/notice.svg', '공지사항', { navTarget: 'notices' }),
                    navCell('./assets/settings.svg', '설정', { navTarget: 'settings' }),
                    navCell('./assets/developer.svg', '개발사', { navTarget: 'developer' })
                ),
                el('div', { className: 'center-panel__divider', attrs: { 'aria-hidden': 'true' } }),
                el('div', { className: 'center-panel__body' },
                    el('p', { className: 'center-panel__error', text: '서버 정보를 불러오지 못했어요' })
                )
            )
        )
    }

    const actionMeta = buildActionMeta(state)

    return el('section', { className: 'center-stage' },
        el('div', { className: 'center-header' },
            el('h1', { className: 'center-header__title', text: '별도소' })
        ),
        el('div', { className: 'center-panel' },
            el('div', { className: 'center-panel__nav', attrs: { 'aria-label': '네비게이션' } },
                navCell('./assets/notice.svg', '공지사항', { navTarget: 'notices' }),
                navCell('./assets/settings.svg', '설정', { navTarget: 'settings' }),
                navCell('./assets/developer.svg', '개발사', { navTarget: 'developer' })
            ),
            el('div', { className: 'center-panel__divider', attrs: { 'aria-hidden': 'true' } }),
            el('div', { className: 'center-panel__body' },
                el('button', {
                    className: 'center-launch',
                    dataset: { action: actionMeta.action },
                    attrs: {
                        id: 'landing-login-button',
                        type: 'button',
                        'aria-label': actionMeta.label,
                        title: actionMeta.label
                    }
                },
                    el('span', { className: 'center-launch__icon' }, iconImage(actionMeta.iconPath))
                )
            )
        )
    )
}
