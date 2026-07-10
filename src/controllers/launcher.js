import { el, fragment } from '../components/dom.js'

const LAUNCH_PROGRESS_UPDATE_THROTTLE_MS = 50
const actionButtonContentCache = new WeakMap()

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

function buildLaunchOverlayBody(result){
    const errorDetailItem = typeof result.errorDetail === 'string' && result.errorDetail.trim().length > 0
        ? overlayListItem('오류 상세', result.errorDetail)
        : null

    if(result.preflight == null){
        return fragment(
            overlayParagraph(result.message),
            overlayList(errorDetailItem)
        )
    }

    if(result.mode === 'blocked'){
        const items = result.preflight.diagnostics
            .filter(diagnostic => diagnostic.blocking || diagnostic.level === 'warning' || diagnostic.level === 'error')
            .map(diagnostic => overlayListItem(diagnostic.title, diagnostic.message))

        return fragment(
            overlayParagraph(result.message),
            overlayList(...items, errorDetailItem)
        )
    }

    if(result.mode === 'failed'){
        return fragment(
            overlayParagraph(result.message),
            overlayList(errorDetailItem)
        )
    }

    if(result.mode === 'terminating'){
        return fragment(
            overlayParagraph(result.message),
            result.processId != null ? overlayList(overlayListItem('프로세스 ID', result.processId)) : null
        )
    }

    return overlayParagraph(result.message)
}

function normalizedLaunchProgress(progress){
    return Number.isFinite(progress)
        ? Math.max(0, Math.min(100, Math.round(progress * 100)))
        : 12
}

function formatLaunchProgressLabel(label){
    const normalizedLabel = typeof label === 'string' ? label.trim() : ''

    if(normalizedLabel.length === 0){
        return '실행 설정을 불러옵니다'
    }

    return normalizedLabel
}

function buildLaunchProgressMarkup(progress, label){
    const normalizedProgress = normalizedLaunchProgress(progress)

    return el('div', { className: 'overlay-progress-shell', attrs: { 'aria-label': '게임 실행 진행률' } },
        el('div', { className: 'overlay-progress-track' },
            el('div', {
                className: 'overlay-progress-fill',
                style: {
                    '--progress-value': `${normalizedProgress}%`,
                    width: `${normalizedProgress}%`
                }
            }),
            el('div', { className: 'overlay-progress-meta', text: `${normalizedProgress}%` })
        ),
        el('div', { className: 'overlay-progress-label', text: formatLaunchProgressLabel(label) })
    )
}

function buildLaunchStateOverlay(launchState){
    return {
        title: '게임 시작',
        body: buildLaunchProgressMarkup(launchState.progress, launchState.label)
    }
}

function updateLaunchProgressElements(launchState){
    const normalizedProgress = normalizedLaunchProgress(launchState.progress)
    const fill = document.querySelector('.overlay-progress-fill')
    const meta = document.querySelector('.overlay-progress-meta')

    if(fill == null || meta == null){
        return false
    }

    fill.style.setProperty('--progress-value', `${normalizedProgress}%`)
    fill.style.width = `${normalizedProgress}%`
    meta.textContent = `${normalizedProgress}%`
    return true
}

function createLaunchProgressRenderer(showOverlay){
    let lastLabel = null
    let lastProgressUpdateAt = 0
    let pendingLaunchState = null
    let pendingTimer = null

    const clearPendingTimer = () => {
        if(pendingTimer != null){
            window.clearTimeout(pendingTimer)
            pendingTimer = null
        }
        pendingLaunchState = null
    }

    const renderLayout = launchState => {
        clearPendingTimer()
        lastLabel = formatLaunchProgressLabel(launchState.label)
        lastProgressUpdateAt = performance.now()
        showOverlay(buildLaunchStateOverlay(launchState))
    }

    const renderProgressOnly = launchState => {
        if(!updateLaunchProgressElements(launchState)){
            renderLayout(launchState)
            return
        }

        lastProgressUpdateAt = performance.now()
    }

    const flushPendingProgress = () => {
        pendingTimer = null
        const launchState = pendingLaunchState
        pendingLaunchState = null

        if(launchState == null){
            return
        }

        const nextLabel = formatLaunchProgressLabel(launchState.label)
        if(nextLabel !== lastLabel){
            renderLayout(launchState)
            return
        }

        renderProgressOnly(launchState)
    }

    return {
        reset(){
            clearPendingTimer()
            lastLabel = null
            lastProgressUpdateAt = 0
        },
        render(launchState){
            const nextLabel = formatLaunchProgressLabel(launchState.label)

            if(nextLabel !== lastLabel || document.querySelector('.overlay-progress-shell') == null){
                renderLayout(launchState)
                return
            }

            const elapsed = performance.now() - lastProgressUpdateAt
            if(elapsed >= LAUNCH_PROGRESS_UPDATE_THROTTLE_MS){
                renderProgressOnly(launchState)
                return
            }

            pendingLaunchState = launchState
            if(pendingTimer == null){
                pendingTimer = window.setTimeout(
                    flushPendingProgress,
                    LAUNCH_PROGRESS_UPDATE_THROTTLE_MS - elapsed
                )
            }
        }
    }
}

export function createLauncherController({
    state,
    render,
    handleSignIn,
    dismissOverlay,
    showOverlay,
    hideOverlay
}){
    const progressRenderer = createLaunchProgressRenderer(showOverlay)

    async function handleLaunchEnded(launchState){
        progressRenderer.reset()
        state.minecraftProcessId = null
        render()
        await hideOverlay()
        const exitCode = launchState?.exitCode
        const abnormalExit = launchState?.terminationRequested !== true
            && Number.isInteger(exitCode)
            && exitCode !== 0

        showOverlay({
            title: abnormalExit ? '비정상 종료' : '실행 종료',
            body: abnormalExit
                ? fragment(
                    overlayParagraph(`마인크래프트가 종료 코드 ${exitCode}(으)로 종료되었습니다.`),
                    overlayList(
                        typeof launchState?.logPath === 'string' && launchState.logPath.length > 0
                            ? overlayListItem('프로세스 로그', launchState.logPath)
                            : null
                    )
                )
                : overlayParagraph('마인크래프트가 종료되었습니다.')
        })
    }

    async function handleLaunch(){
        if(state.pendingAction != null){
            return
        }

        state.pendingAction = 'launch'
        updateActionButtons()

        try {
            progressRenderer.render({ progress: 0.06, label: '실행 설정을 불러옵니다' })

            const result = await window.starPrisonLauncher.launch()

            if(result.mode === 'ignored' && typeof result.message !== 'string'){
                await hideOverlay()
                return
            }

            if(result.mode === 'launched' && result.processId != null){
                state.minecraftProcessId = result.processId
                render()
            }

            progressRenderer.reset()
            showOverlay({
                title: result.ok ? '실행 완료' : '경고',
                body: buildLaunchOverlayBody(result)
            })
        } finally {
            state.pendingAction = null
            updateActionButtons()
        }
    }

    function promptTerminateMinecraft(){
        showOverlay({
            title: '게임 종료',
            body: overlayParagraph('게임을 종료하시겠습니까?'),
            actions: fragment(
                el('button', {
                    className: 'secondary-button overlay-confirm-button terminate-confirmation-button',
                    text: '예',
                    dataset: { confirmTerminateMinecraft: 'true' },
                    attrs: { type: 'button' }
                }),
                el('button', {
                    className: 'secondary-button overlay-confirm-button terminate-confirmation-button',
                    text: '아니오',
                    dataset: { cancelTerminateMinecraft: 'true' },
                    attrs: { type: 'button' }
                })
            )
        })
    }

    async function handleTerminateMinecraft(){
        if(state.pendingAction != null){
            return
        }

        state.pendingAction = 'terminate'
        updateActionButtons()

        try {
            progressRenderer.render({ progress: 0.5, label: '종료 명령을 보내는 중' })

            const result = await window.starPrisonLauncher.terminateMinecraft()

            if(result.mode === 'not-running'){
                state.minecraftProcessId = null
                render()
            }

            progressRenderer.reset()
            showOverlay({
                title: result.ok ? '종료 요청' : '경고',
                body: buildLaunchOverlayBody(result)
            })
        } catch (error) {
            progressRenderer.reset()
            showOverlay({
                title: '종료 실패',
                body: overlayParagraph(error.message)
            })
        } finally {
            state.pendingAction = null
            updateActionButtons()
        }
    }

    function updateActionButtons(){
        const isPending = state.pendingAction != null
        const pendingLabel = state.pendingAction === 'sign-in'
            ? '계정 연결 중'
            : state.pendingAction === 'terminate'
                ? '게임 종료 중'
                : '게임 준비 중'

        document.querySelectorAll('#sign-in-button, #sign-out-button, #landing-login-button').forEach(button => {
            button.disabled = isPending
            button.setAttribute('aria-busy', String(isPending))

            if(isPending){
                if(!actionButtonContentCache.has(button)){
                    actionButtonContentCache.set(button, {
                        children: [...button.childNodes].map(child => child.cloneNode(true)),
                        label: button.getAttribute('aria-label') ?? button.textContent
                    })
                }
                button.setAttribute('aria-label', pendingLabel)
                button.setAttribute('title', pendingLabel)
                button.textContent = pendingLabel
                return
            }

            const cachedButton = actionButtonContentCache.get(button)

            if(cachedButton != null){
                button.replaceChildren(...cachedButton.children.map(child => child.cloneNode(true)))
                button.setAttribute('aria-label', cachedButton.label)
                button.setAttribute('title', cachedButton.label)
                actionButtonContentCache.delete(button)
            }
        })
    }

    async function handleLaunchButtonClick(){
        const action = document.getElementById('landing-login-button')?.dataset.action

        if(action === 'launch'){
            handleLaunch()
            return
        }

        if(action === 'terminate'){
            promptTerminateMinecraft()
            return
        }

        const signedIn = await handleSignIn({ launchAfterSignIn: true })

        if(signedIn){
            await handleLaunch()
        }
    }

    async function handleLaunchStateChanged(launchState){
        if(launchState?.stage === 'game-ended'){
            await handleLaunchEnded(launchState)
            return
        }

        if(state.pendingAction == null){
            return
        }

        progressRenderer.render(launchState)
    }

    return {
        handleLaunch,
        handleLaunchButtonClick,
        handleLaunchEnded,
        handleLaunchStateChanged,
        handleTerminateMinecraft,
        promptTerminateMinecraft,
        updateActionButtons,
        cancelTerminateMinecraft: dismissOverlay
    }
}
