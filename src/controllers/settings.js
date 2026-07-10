import { el, fragment } from '../components/dom.js'

const RISKY_JVM_ARG_PREFIXES = [
    '-javaagent',
    '-agentlib',
    '-agentpath',
    '-Xbootclasspath',
    '-Djava.library.path',
    '-Dorg.lwjgl.librarypath',
    '-Djna.library.path',
    '-Dsun.boot.library.path',
    '-Dlog4j.configurationFile',
    '-Dlog4j2.configurationFile',
    '-Dcom.sun.management.jmxremote',
    '--patch-module',
    '--add-opens',
    '--add-exports'
]
const RISKY_GAME_ARG_NAMES = [
    '--accessToken',
    '--uuid',
    '--username',
    '--userProperties',
    '--xuid',
    '--clientId',
    '--quickPlayPath',
    '--quickPlayMultiplayer',
    '--server',
    '--port'
]

function overlayParagraph(text){
    return el('p', { text: text ?? '' })
}

function overlayListItem(title, message){
    return el('li', {},
        el('strong', { text: title }),
        el('br'),
        document.createTextNode(String(message ?? ''))
    )
}

function overlayList(...items){
    return el('ul', { className: 'overlay-list' }, items.filter(Boolean))
}

function splitUserArgs(value){
    if(typeof value !== 'string'){
        return []
    }

    return value
        .split(/\s+/)
        .map(arg => arg.trim())
        .filter(Boolean)
}

function matchesNamedArg(arg, name){
    return arg === name || arg.startsWith(`${name}=`)
}

function collectUnsafeSettingsWarnings(patch){
    const warnings = []
    const extraJvmArgs = splitUserArgs(patch.extraJvmArgs)
    const extraGameArgs = splitUserArgs(patch.extraGameArgs)

    for(const arg of extraJvmArgs){
        if(RISKY_JVM_ARG_PREFIXES.some(prefix => arg.startsWith(prefix))){
            warnings.push({
                title: `JVM 인자: ${arg}`,
                message: '외부 코드, 네이티브 라이브러리, 런타임 보안 완화 설정으로 악용될 수 있어 권고되지 않습니다.'
            })
        }
    }

    for(const arg of extraGameArgs){
        if(RISKY_GAME_ARG_NAMES.some(name => matchesNamedArg(arg, name))){
            warnings.push({
                title: `게임 인자: ${arg}`,
                message: '계정 세션, 플레이어 식별자, 접속 대상 같은 런처가 관리해야 할 값을 덮어쓸 수 있어 권고되지 않습니다.'
            })
        }
    }

    return warnings
}

function buildUnsafeSettingsOverlayBody(warnings){
    return fragment(
        overlayParagraph('해당 항목은 보안 취약점 또는 세션 변조 위험으로 인해 권고되지 않습니다. 저장하시겠습니까?'),
        overlayList(...warnings.map(warning => overlayListItem(warning.title, warning.message)))
    )
}

export function createSettingController({
    state,
    refreshBootstrap,
    dismissOverlay,
    showOverlay,
    onAfterSave
}){
    async function persistSettingsPatch(patch, options = {}){
        const {
            successTitle = '저장 완료',
            successMessage = '설정을 저장했습니다.',
            ...saveOptions
        } = options
        const bootstrap = await window.starPrisonLauncher.saveSettings(patch, saveOptions)
        await refreshBootstrap(bootstrap)
        showOverlay({
            title: successTitle,
            body: overlayParagraph(successMessage)
        })
        state.pendingSettingsPatch = null

        if(typeof onAfterSave === 'function'){
            onAfterSave()
        }
    }

    function promptUnsafeSettingsSave(patch, warnings){
        state.pendingSettingsPatch = { ...patch }
        showOverlay({
            title: '비권장 설정',
            body: buildUnsafeSettingsOverlayBody(warnings),
            actions: fragment(
                el('button', {
                    className: 'secondary-button overlay-cancel-button',
                    text: '취소',
                    dataset: { cancelSaveSettings: 'true' },
                    attrs: { type: 'button' }
                }),
                el('button', {
                    className: 'secondary-button overlay-confirm-button',
                    text: '저장',
                    dataset: { confirmSaveSettings: 'true' },
                    attrs: { type: 'button' }
                })
            )
        })
    }

    async function handleSaveSettings(event){
        event.preventDefault()

        const patch = {}
        const dataDirectoryInput = document.getElementById('data-directory-input')
        const memoryAllocationInput = document.getElementById('memory-allocation-input')
        const gameResolutionSelect = document.getElementById('game-resolution-select')
        const extraJvmArgsInput = document.getElementById('extra-jvm-args-input')
        const extraGameArgsInput = document.getElementById('extra-game-args-input')
        const allowPrereleaseInput = document.getElementById('allow-prerelease-input')

        if(dataDirectoryInput != null){
            patch.dataDirectory = dataDirectoryInput.value.trim()
        }

        if(memoryAllocationInput != null){
            const parsedValue = Number.parseInt(memoryAllocationInput.value, 10)

            if(Number.isFinite(parsedValue) && parsedValue > 0){
                patch.maxRamMb = parsedValue * 1024
            }
        }

        if(gameResolutionSelect != null){
            patch.gameResolution = gameResolutionSelect.value
        }

        if(extraJvmArgsInput != null){
            patch.extraJvmArgs = extraJvmArgsInput.value.trim()
        }

        if(extraGameArgsInput != null){
            patch.extraGameArgs = extraGameArgsInput.value.trim()
        }

        if(allowPrereleaseInput != null){
            patch.allowPrerelease = allowPrereleaseInput.checked
        }

        const warnings = collectUnsafeSettingsWarnings(patch)

        if(warnings.length > 0){
            promptUnsafeSettingsSave(patch, warnings)
            return
        }

        await persistSettingsPatch(patch)
    }

    async function handleResetSettings(){
        const recommendedRamMb = state.bootstrap.serverManifest?.java?.recommendedRamMb ?? 8192
        await persistSettingsPatch(
            {
                allowPrerelease: false,
                maxRamMb: recommendedRamMb,
                gameResolution: 'default',
                extraJvmArgs: '',
                extraGameArgs: '',
                discordRichPresenceEnabled: false
            },
            {
                successTitle: '초기화 완료',
                successMessage: '환경 설정을 초기화하였습니다.'
            }
        )
    }

    function promptResetSettings(){
        showOverlay({
            title: '설정 초기화',
            body: overlayParagraph('설정이 초기화됩니다.'),
            actions: el('button', {
                className: 'secondary-button overlay-confirm-button',
                text: '확인',
                dataset: { confirmResetSettings: 'true' },
                attrs: { type: 'button' }
            })
        })
    }

    async function handleSelectDataDirectory(){
        const dataDirectoryInput = document.getElementById('data-directory-input')

        if(dataDirectoryInput == null){
            return
        }

        const result = await window.starPrisonLauncher.selectDataDirectory(dataDirectoryInput.value)

        if(result == null || result.canceled || typeof result.path !== 'string'){
            return
        }

        dataDirectoryInput.value = result.path
    }

    async function handleManagedDirectoryClick(event){
        const kind = event.currentTarget?.dataset?.managedDirectory

        if(typeof kind !== 'string' || kind.length === 0){
            return
        }

        try {
            await window.starPrisonLauncher.openManagedDirectory(kind)
        } catch (error) {
            showOverlay({
                title: '폴더 열기 실패',
                body: overlayParagraph(error.message)
            })
        }
    }

    async function confirmPendingSettingsSave(){
        const patch = state.pendingSettingsPatch

        if(patch != null){
            await persistSettingsPatch(patch, { unsafeAcknowledged: true })
        }
    }

    return {
        confirmPendingSettingsSave,
        handleManagedDirectoryClick,
        handleResetSettings,
        handleSaveSettings,
        handleSelectDataDirectory,
        promptResetSettings,
        cancelPendingSettingsSave: dismissOverlay
    }
}
