import { el } from '../components/dom.js'
import { viewHeader } from '../components/terminal.js'

const DEVELOPER_CREDITS = [
    ['책임', '정곰'],
    ['개발', '정곰, 코코넛, 노밥, 오뎅'],
    ['기획', '노밥, 빡빡이, 오뎅, 디버그'],
    ['디자인', '빡빡이, 디버그, 오뎅']
]

function detailItem(label, value){
    return el('div', { className: 'detail-item' },
        el('dt', { className: 'detail-label', text: label }),
        el('dd', { className: 'detail-value', text: value })
    )
}

export function renderDeveloperView(_state){
    return el('section', { className: 'terminal-view content-page developer-view' },
        viewHeader('개발사', '// DEVELOPER'),
        el('article', { className: 'terminal-panel content-page__panel' },
            el('section', { className: 'developer-profile' },
                el('p', { className: 'terminal-kicker', text: 'STUDIO KOKONUT' }),
                el('h3', { className: 'developer-profile__title', text: '코코넛 스튜디오' })
            ),
            el('dl', { className: 'detail-list developer-details' },
                DEVELOPER_CREDITS.map(([label, value]) => detailItem(label, value))
            )
        )
    )
}
