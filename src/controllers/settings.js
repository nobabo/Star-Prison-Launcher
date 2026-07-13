import { el, fragment } from '../components/dom.js'

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

function parseArgumentLines(value){
    return String(value ?? '').split(/\r?\n/).map(argument => argument.trim()).filter(Boolean)
}

function buildUnsafeSettingsOverlayBody(warnings){
    return fragment(
        overlayParagraph('해당 항목은 보안 취약점 또는 세션 변조 위험으로 인해 권고되지 않습니다. 저장하시겠습니까?'),
        overlayList(...warnings.map(warning => overlayListItem('보안 경고', warning)))
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
        const result = await window.starPrisonLauncher.saveSettings(patch, saveOptions)
        if(result.requiresConfirmation){
            promptUnsafeSettingsSave(patch, result.warnings ?? [])
            return false
        }
        await refreshBootstrap(result.bootstrap)
        showOverlay({
            title: successTitle,
            body: overlayParagraph(successMessage)
        })
        state.pendingSettingsPatch = null

        if(typeof onAfterSave === 'function'){
            onAfterSave()
        }
        return true
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
            patch.extraJvmArgs = parseArgumentLines(extraJvmArgsInput.value)
        }

        if(extraGameArgsInput != null){
            patch.extraGameArgs = parseArgumentLines(extraGameArgsInput.value)
        }

        if(allowPrereleaseInput != null){
            patch.allowPrerelease = allowPrereleaseInput.checked
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
                extraJvmArgs: [],
                extraGameArgs: []
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
