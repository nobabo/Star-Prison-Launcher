import { el } from '../components/dom.js'
import { viewHeader } from '../components/terminal.js'

const GAME_RESOLUTION_OPTIONS = [
    { value: 'default', label: '기본값' },
    { value: '1280x720', label: '1280 x 720' },
    { value: '1366x768', label: '1366 x 768' },
    { value: '1600x900', label: '1600 x 900' },
    { value: '1920x1080', label: '1920 x 1080' },
    { value: '2560x1440', label: '2560 x 1440' }
]

function settingsInput(id, name, value, extraAttrs = {}){
    return el('input', {
        attrs: {
            id,
            name,
            ...extraAttrs
        },
        props: {
            value: value ?? ''
        }
    })
}

function settingsSelect(id, name, value, options){
    const selectedValue = options.some(option => option.value === value) ? value : options[0]?.value
    const select = el('select', {
        attrs: {
            id,
            name
        }
    }, options.map(option => el('option', {
        text: option.label,
        attrs: {
            value: option.value
        }
    })))

    select.value = selectedValue ?? ''
    return select
}

function managedDirectoryButton(kind, text){
    return el('button', {
        className: 'secondary-button managed-directory-button',
        text,
        dataset: { managedDirectory: kind },
        attrs: { type: 'button' }
    })
}

function settingsGroup(label, bodyChildren){
    return el('section', { className: 'settings-group' },
        el('p', { className: 'terminal-kicker settings-group__label', text: label }),
        el('div', { className: 'settings-group__body' }, ...bodyChildren)
    )
}

function helpCopy(text){
    return el('p', { className: 'settings-help', text })
}

export function renderSettingsView(state){
    const { userConfig } = state.bootstrap
    const ramGb = Math.max(1, Math.round((userConfig.settings.maxRamMb ?? 8192) / 1024))
    const gameResolution = userConfig.settings.gameResolution ?? 'default'

    return el('section', { className: 'terminal-view' },
        viewHeader('환경설정', '// SETTINGS'),
        el('article', { className: 'terminal-panel' },
            el('form', { className: 'settings-form', attrs: { id: 'settings-form' } },
                settingsGroup('// DIRECTORY', [
                    el('div', { className: 'directory-management' },
                        el('span', { className: 'field-label', text: '설치 경로' }),
                        el('div', { className: 'actions directory-actions' },
                            managedDirectoryButton('profile', '프로필'),
                            managedDirectoryButton('logs', '로그'),
                            managedDirectoryButton('screenshots', '스크린샷')
                        )
                    )
                ]),
                settingsGroup('// RUNTIME', [
                    el('div', { className: 'settings-grid-2' },
                        el('label', { className: 'field' },
                            el('span', { className: 'field-label', text: '램(GB)' }),
                            settingsInput('memory-allocation-input', 'memoryAllocation', ramGb, {
                                type: 'number',
                                min: '4',
                                step: '1'
                            })
                        ),
                        el('label', { className: 'field' },
                            el('span', { className: 'field-label', text: '게임 화면 크기' }),
                            settingsSelect('game-resolution-select', 'gameResolution', gameResolution, GAME_RESOLUTION_OPTIONS)
                        )
                    )
                ]),
                el('details', { className: 'advanced-settings' },
                    el('summary', { text: '// ADVANCED' }),
                    el('div', { className: 'advanced-settings__body' },
                        el('label', { className: 'field' },
                            el('span', { className: 'field-label', text: '추가 JVM Args' }),
                            settingsInput('extra-jvm-args-input', 'extraJvmArgs', userConfig.settings.extraJvmArgs ?? ''),
                            helpCopy('-Djava.net.preferIPv4Stack=true 등')
                        ),
                        el('label', { className: 'field' },
                            el('span', { className: 'field-label', text: '추가 Game Args' }),
                            settingsInput('extra-game-args-input', 'extraGameArgs', userConfig.settings.extraGameArgs ?? ''),
                            helpCopy('--fullscreen 등')
                        )
                    )
                )
            )
        ),
        el('footer', { className: 'settings-actions' },
            el('button', {
                className: 'primary-button',
                text: '저장',
                attrs: { type: 'submit', form: 'settings-form' }
            }),
            el('button', {
                className: 'secondary-button settings-reset-button',
                text: '초기화',
                attrs: {
                    id: 'settings-reset-button',
                    type: 'button'
                }
            })
        )
    )
}
