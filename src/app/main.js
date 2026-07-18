import { createLauncherBridge } from '../bridge/tauri-api.js'
import { hideOverlay, showOverlay } from '../components/overlay.js'
import { el } from '../components/dom.js'
import { createLauncherController } from '../controllers/launcher.js'
import { createSettingController } from '../controllers/settings.js'
import { NOTICE_REFRESH_INTERVAL_MS, refreshNoticeCards as refreshNoticeCardsForState } from '../services/notices.js'
import { buildMineSkinUrls } from '../views/login.js'
import { createInitialState } from './state.js'
import { views } from './views.js'

window.starPrisonLauncher = createLauncherBridge()

const state = createInitialState()

const PAGE_SCROLL_MULTIPLIER = 5
const BACKGROUND_PARALLAX_RANGE = 3
const FALLING_EASTER_EGG_REMOVE_BUFFER_MS = 400
const ACCOUNT_SWITCHER_EXIT_DURATION_MS = 280
const FALLING_EASTER_EGG_ASSETS = [
    { src: './assets/chick-face.svg', minSize: 48, maxSize: 64 },
    { src: './assets/pig-face.svg', minSize: 62, maxSize: 82 }
]
let mountedViewName = null
let accountSwitcherExitTimer = null
let accountSwitcherClosing = false
const launcherController = createLauncherController({
    state,
    render,
    handleSignIn,
    dismissOverlay,
    showOverlay,
    hideOverlay
})
const settingController = createSettingController({
    state,
    refreshBootstrap,
    dismissOverlay,
    showOverlay,
    onAfterSave: navigateToLanding
})

async function refreshNoticeCards(){
    await refreshNoticeCardsForState(state, { render })
}

function getOverlayDemo(){
    return window.starPrisonLauncher.getOverlayDemo?.() ?? ''
}

function getInitialView(){
    const preloadInitialView = window.starPrisonLauncher.getInitialView?.()

    if(Object.hasOwn(views, preloadInitialView)){
        return preloadInitialView
    }

    const params = new URLSearchParams(window.location.search)
    const requestedView = params.get('view')

    if(Object.hasOwn(views, requestedView)){
        return requestedView
    }

    return 'landing'
}

function getStatusMeta(){
    const { authSummary, preflight, fatalError } = state.bootstrap

    if(fatalError != null){
        return {
            tone: 'error',
            label: '확인 필요'
        }
    }

    if(!authSummary.signedIn){
        return {
            tone: 'warning',
            label: '계정 연결 필요'
        }
    }

    if(preflight.blockingCount > 0){
        return {
            tone: 'warning',
            label: '실행 준비 필요'
        }
    }

    return {
        tone: 'success',
        label: '실행 가능'
    }
}

function renderStatusCard(){
    const statusCard = document.getElementById('status-card')

    if(statusCard == null){
        return
    }

    const statusMeta = getStatusMeta()

    statusCard.className = `status-pill ${statusMeta.tone}`
    statusCard.tabIndex = statusMeta.tone === 'success' ? 0 : -1
    if(statusMeta.tone === 'success'){
        statusCard.setAttribute('role', 'button')
    } else {
        statusCard.removeAttribute('role')
    }
    statusCard.setAttribute('aria-label', statusMeta.tone === 'success' ? `${statusMeta.label} 상태` : statusMeta.label)
    statusCard.setAttribute('title', statusMeta.label)
    statusCard.replaceChildren(
        el('span', { className: 'status-dot', attrs: { 'aria-hidden': 'true' } }),
        el('span', { className: 'status-text', text: statusMeta.label })
    )
}

function renderWindowControls(){
    const maximizeButton = document.getElementById('window-maximize-button')

    if(maximizeButton == null){
        return
    }

    const isMaximized = state.windowState.maximized
    maximizeButton.textContent = isMaximized ? '❐' : '□'
    maximizeButton.setAttribute('aria-label', isMaximized ? '원래 크기로' : '최대화')
    maximizeButton.setAttribute('title', isMaximized ? '원래 크기로' : '최대화')
}

function bindAcceleratedPageScroll(){
    const pageContent = document.querySelector('.page-content')

    if(pageContent == null || pageContent.dataset.acceleratedScrollBound === 'true'){
        return
    }

    pageContent.dataset.acceleratedScrollBound = 'true'
    pageContent.addEventListener('wheel', event => {
        if(!event.cancelable || Math.abs(event.deltaY) <= Math.abs(event.deltaX)){
            return
        }

        const canScroll = pageContent.scrollHeight > pageContent.clientHeight

        if(!canScroll){
            return
        }

        event.preventDefault()
        pageContent.scrollBy({
            top: event.deltaY * PAGE_SCROLL_MULTIPLIER,
            left: 0,
            behavior: 'smooth'
        })
    }, { passive: false })
}

function bindBackgroundParallax(){
    if(window.matchMedia('(prefers-reduced-motion: reduce)').matches){
        document.documentElement.style.setProperty('--background-shift-x', '0px')
        document.documentElement.style.setProperty('--background-shift-y', '0px')
        return
    }

    let animationFrameId = null
    let nextShiftX = 0
    let nextShiftY = 0

    const applyShift = () => {
        animationFrameId = null
        document.documentElement.style.setProperty('--background-shift-x', `${nextShiftX.toFixed(2)}px`)
        document.documentElement.style.setProperty('--background-shift-y', `${nextShiftY.toFixed(2)}px`)
    }

    const scheduleShift = () => {
        if(animationFrameId != null){
            return
        }

        animationFrameId = window.requestAnimationFrame(applyShift)
    }

    window.addEventListener('pointermove', event => {
        if(event.pointerType === 'touch'){
            return
        }

        const viewportWidth = Math.max(window.innerWidth, 1)
        const viewportHeight = Math.max(window.innerHeight, 1)
        const normalizedX = (event.clientX / viewportWidth - 0.5) * 2
        const normalizedY = (event.clientY / viewportHeight - 0.5) * 2

        nextShiftX = normalizedX * BACKGROUND_PARALLAX_RANGE
        nextShiftY = normalizedY * BACKGROUND_PARALLAX_RANGE
        scheduleShift()
    }, { passive: true })
}

function syncBackgroundImage(){
    const backgroundAssets = state.bootstrap?.backgroundAssets ?? []

    if(backgroundAssets.length === 0){
        state.backgroundImageUrl = null
        return
    }

    if(typeof state.backgroundImageUrl === 'string' && backgroundAssets.includes(state.backgroundImageUrl)){
        return
    }

    state.backgroundImageUrl = backgroundAssets[Math.floor(Math.random() * backgroundAssets.length)]
}

function applyGlobalBackgroundImage(){
    const backgroundUrl = state.backgroundImageUrl
    const normalizedBackgroundUrl = typeof backgroundUrl === 'string'
        ? backgroundUrl.replace(/^\.\//, '/')
        : null
    const escapedBackgroundUrl = normalizedBackgroundUrl?.replace(/"/g, '%22') ?? null

    if(escapedBackgroundUrl == null){
        document.documentElement.style.setProperty('--launcher-background-image', 'none')
        return
    }

    document.documentElement.style.setProperty('--launcher-background-image', `url("${escapedBackgroundUrl}")`)
}

function preloadAccountSkin(){
    const playerName = state.bootstrap?.authSummary?.playerName
    const skinUrls = buildMineSkinUrls(playerName)

    if(skinUrls == null){
        return
    }

    const image = new Image()
    image.decoding = 'sync'
    image.src = skinUrls.head
}

function buildAccountSkinImage(playerName, className = 'account-switcher-avatar-image'){
    const skinUrls = buildMineSkinUrls(playerName)

    if(skinUrls == null){
        return el('span', { className: `${className} account-switcher-avatar-fallback`, text: '?' })
    }

    return el('img', {
        className,
        attrs: {
            src: skinUrls.head,
            alt: '',
            'aria-hidden': 'true',
            loading: 'eager',
            decoding: 'async'
        }
    })
}

function clearAccountSwitcherExitTimer(){
    if(accountSwitcherExitTimer != null){
        window.clearTimeout(accountSwitcherExitTimer)
        accountSwitcherExitTimer = null
    }
}

function finishAccountSwitcherExit(dialog){
    clearAccountSwitcherExitTimer()

    if(dialog?.open){
        dialog.close()
    }

    dialog?.classList.remove('is-visible', 'is-closing')
    accountSwitcherClosing = false
}

function closeAccountSwitcher(){
    const dialog = document.getElementById('account-switcher-dialog')

    if(dialog == null || !dialog.open || accountSwitcherClosing){
        return
    }

    if(window.matchMedia('(prefers-reduced-motion: reduce)').matches){
        finishAccountSwitcherExit(dialog)
        return
    }

    accountSwitcherClosing = true
    dialog.classList.remove('is-visible')
    dialog.classList.add('is-closing')
    accountSwitcherExitTimer = window.setTimeout(() => {
        if(dialog.classList.contains('is-closing')){
            finishAccountSwitcherExit(dialog)
        }
    }, ACCOUNT_SWITCHER_EXIT_DURATION_MS + 80)
}

function renderAccountSwitcher(){
    const trigger = document.getElementById('account-switcher-trigger')
    const triggerAvatar = document.getElementById('account-switcher-trigger-avatar')
    const list = document.getElementById('account-switcher-list')
    const addButton = document.getElementById('account-switcher-add')
    const auth = state.bootstrap?.authSummary
    const signedIn = auth?.signedIn === true

    if(trigger == null || triggerAvatar == null || list == null || addButton == null){
        return
    }

    trigger.hidden = !signedIn

    if(!signedIn){
        closeAccountSwitcher()
        list.replaceChildren()
        return
    }

    trigger.setAttribute('aria-label', `${auth.playerName} 계정 선택`)
    trigger.setAttribute('title', `${auth.playerName} · 계정 선택`)
    triggerAvatar.replaceChildren(buildAccountSkinImage(auth.playerName))

    const savedAccounts = Array.isArray(state.bootstrap.authAccounts)
        ? state.bootstrap.authAccounts
        : []
    const accounts = savedAccounts.length > 0 ? savedAccounts : [auth]
    const actionPending = state.pendingAction != null

    list.replaceChildren(...accounts.map(account => {
        const isActive = account.profileId === auth.profileId
        const option = el('button', {
            className: `account-switcher-option${isActive ? ' account-switcher-option--active' : ''}`,
            attrs: {
                type: 'button',
                'aria-label': `${account.playerName} 계정 선택`,
                title: `${account.playerName} 계정 선택`,
                disabled: actionPending ? '' : null
            },
            dataset: {
                accountProfileId: account.profileId
            }
        },
            el('span', { className: 'account-switcher-option__avatar', attrs: { 'aria-hidden': 'true' } },
                buildAccountSkinImage(account.playerName)
            )
        )

        return el('article', {
            className: 'account-switcher-item',
            dataset: {
                accountProfileId: account.profileId
            }
        },
            option,
            el('h3', {
                className: `account-switcher-name${isActive ? ' account-switcher-name--active' : ''}`,
                text: account.playerName
            })
        )
    }))
    addButton.disabled = actionPending
}

function openAccountSwitcher(){
    const dialog = document.getElementById('account-switcher-dialog')

    if(dialog == null || state.bootstrap?.authSummary?.signedIn !== true){
        return
    }

    renderAccountSwitcher()
    if(!dialog.open){
        clearAccountSwitcherExitTimer()
        accountSwitcherClosing = false
        dialog.classList.remove('is-visible', 'is-closing')
        dialog.showModal()
        window.requestAnimationFrame(() => {
            if(dialog.open){
                dialog.classList.add('is-visible')
            }
        })
    }
}

function syncElementState(current, next){
    for(const attribute of [...current.attributes]){
        if(!next.hasAttribute(attribute.name)){
            current.removeAttribute(attribute.name)
        }
    }

    for(const attribute of next.attributes){
        if(current.getAttribute(attribute.name) !== attribute.value){
            current.setAttribute(attribute.name, attribute.value)
        }
    }

    if(current !== document.activeElement && 'value' in current && current.value !== next.value){
        current.value = next.value
    }
    if('checked' in current && current.checked !== next.checked){
        current.checked = next.checked
    }
    if('disabled' in current && current.disabled !== next.disabled){
        current.disabled = next.disabled
    }
}

function patchDomNode(current, next){
    if(current.nodeType !== next.nodeType || current.nodeName !== next.nodeName){
        current.replaceWith(next)
        return next
    }

    if(current.nodeType === Node.TEXT_NODE){
        if(current.nodeValue !== next.nodeValue){
            current.nodeValue = next.nodeValue
        }
        return current
    }

    syncElementState(current, next)
    const currentChildren = [...current.childNodes]
    const nextChildren = [...next.childNodes]
    const childCount = Math.max(currentChildren.length, nextChildren.length)

    for(let index = 0; index < childCount; index += 1){
        const currentChild = currentChildren[index]
        const nextChild = nextChildren[index]

        if(currentChild == null){
            current.appendChild(nextChild)
        } else if(nextChild == null){
            currentChild.remove()
        } else {
            patchDomNode(currentChild, nextChild)
        }
    }

    return current
}

function render(){
    const renderer = views[state.activeView]
    const viewHost = document.getElementById('view-host')

    syncBackgroundImage()
    applyGlobalBackgroundImage()

    renderStatusCard()
    renderWindowControls()
    renderAccountSwitcher()

    const nextView = renderer(state)

    if(mountedViewName !== state.activeView || viewHost.firstChild == null){
        viewHost.replaceChildren(nextView)
        mountedViewName = state.activeView
        return
    }

    patchDomNode(viewHost.firstChild, nextView)
}

function buildLiquidDemoOverlay(){
    return {
        title: '파도 알림 테스트',
        body: el('p', { text: '한 줄 알림용 파도 프리뷰입니다.' })
    }
}

function overlayParagraph(text){
    return el('p', { text: text ?? '' })
}

function dismissOverlay(){
    state.pendingSettingsPatch = null
    hideOverlay()
}

function ensureEasterEggLayer(){
    let layer = document.getElementById('easter-egg-layer')

    if(layer != null){
        return layer
    }

    layer = el('div', {
        className: 'easter-egg-layer',
        attrs: {
            id: 'easter-egg-layer',
            'aria-hidden': 'true'
        }
    })
    document.body.appendChild(layer)
    return layer
}

function randomBetween(min, max){
    return min + Math.random() * (max - min)
}

function spawnFallingEasterEgg(){
    const layer = ensureEasterEggLayer()
    const viewportWidth = Math.max(window.innerWidth, 1)
    const viewportHeight = Math.max(window.innerHeight, 1)
    const mobAsset = FALLING_EASTER_EGG_ASSETS[Math.floor(Math.random() * FALLING_EASTER_EGG_ASSETS.length)]
    const mobSize = randomBetween(mobAsset.minSize, mobAsset.maxSize)
    const edgePadding = mobSize / 2 + 12
    const x = randomBetween(edgePadding, Math.max(edgePadding, viewportWidth - edgePadding))
    const duration = randomBetween(5600, 7600)
    const mob = el('div', {
        className: 'easter-egg-mob',
        style: {
            '--mob-x': `${x}px`,
            '--mob-size': `${mobSize}px`,
            '--mob-start-y': `${randomBetween(-152, -96).toFixed(1)}px`,
            '--mob-end-y': `${(viewportHeight + mobSize * 1.25 + 18).toFixed(1)}px`,
            '--mob-drift-x': `${randomBetween(-42, 42).toFixed(1)}px`,
            '--mob-tilt': `${randomBetween(-7, 7).toFixed(1)}deg`,
            '--mob-duration': `${duration.toFixed(0)}ms`
        }
    },
        el('img', {
            className: 'easter-egg-mob__image',
            attrs: {
                src: mobAsset.src,
                alt: '',
                'aria-hidden': 'true'
            }
        })
    )

    layer.appendChild(mob)
    window.setTimeout(() => mob.remove(), duration + FALLING_EASTER_EGG_REMOVE_BUFFER_MS)
}

function handleLaunchReadyEasterEggFall(){
    if(getStatusMeta().tone !== 'success'){
        return
    }

    spawnFallingEasterEgg()
}

function bindLaunchReadyEasterEggFall(){
    const statusCard = document.getElementById('status-card')

    if(statusCard == null){
        return
    }

    statusCard.onclick = handleLaunchReadyEasterEggFall
    statusCard.onkeydown = event => {
        if(event.key !== 'Enter' && event.key !== ' '){
            return
        }

        event.preventDefault()
        handleLaunchReadyEasterEggFall()
    }
}

async function refreshBootstrap(newBootstrap){
    state.bootstrap = newBootstrap
    syncBackgroundImage()
    preloadAccountSkin()
    await refreshNoticeCards()
    render()
}

async function handleSignIn({ launchAfterSignIn = false } = {}){
    if(state.pendingAction != null){
        return false
    }

    state.pendingAction = 'sign-in'
    launcherController.updateActionButtons()
    renderAccountSwitcher()

    try {
        const result = await window.starPrisonLauncher.signIn()

        if(result.ok){
            await refreshBootstrap(result.bootstrap)

            if(!launchAfterSignIn){
                showOverlay({
                    title: '로그인 완료',
                    body: overlayParagraph(`${result.session.playerName} 계정을 저장하였습니다.`)
                })
            }

            return true
        }

        if(result.code === 'AUTH_CANCELLED'){
            await refreshBootstrap(result.bootstrap)
            return false
        }

        showOverlay({
            title: '로그인 불가',
            body: overlayParagraph(result.message)
        })
        return false
    } catch (error) {
        showOverlay({
            title: '로그인 실패',
            body: overlayParagraph(error.message)
        })
        return false
    } finally {
        state.pendingAction = null
        launcherController.updateActionButtons()
        renderAccountSwitcher()
    }
}

async function handleSelectAccount(profileId){
    const normalizedProfileId = typeof profileId === 'string' ? profileId.trim() : ''

    if(normalizedProfileId.length === 0 || state.pendingAction != null){
        return
    }

    if(normalizedProfileId === state.bootstrap?.authSummary?.profileId){
        closeAccountSwitcher()
        return
    }

    state.pendingAction = 'select-account'
    renderAccountSwitcher()

    try {
        const result = await window.starPrisonLauncher.selectAccount(normalizedProfileId)
        await refreshBootstrap(result.bootstrap)
        closeAccountSwitcher()
    } catch (error) {
        closeAccountSwitcher()
        showOverlay({
            title: '계정 선택 실패',
            body: overlayParagraph(error.message)
        })
    } finally {
        state.pendingAction = null
        renderAccountSwitcher()
    }
}

async function handleSignOut(){
    const result = await window.starPrisonLauncher.signOut()
    await refreshBootstrap(result.bootstrap)
    showOverlay({
        title: '로그아웃',
        body: overlayParagraph('로그아웃이 완료되었습니다.')
    })
}

function navigateToLanding(){
    state.activeView = 'landing'
    render()
}

async function handleDelegatedViewClick(event){
    const viewHost = document.getElementById('view-host')
    const target = event.target.closest?.('button, input, [data-external-url], [data-nav-target], [data-managed-directory]')

    if(target == null || !viewHost.contains(target)){
        return
    }

    try {
        const externalTarget = target.closest?.('[data-external-url]')

        if(externalTarget != null){
            const url = externalTarget.dataset.externalUrl

            if(typeof url !== 'string' || url.trim().length === 0){
                return
            }

            externalTarget.disabled = true
            try {
                await window.starPrisonLauncher.openExternal(url)
            } finally {
                externalTarget.disabled = false
            }
            return
        }

        if(target.closest?.('.back-to-landing-button') != null){
            navigateToLanding()
            return
        }

        const navigationTarget = target.closest?.('[data-nav-target]')?.dataset.navTarget
        if(typeof navigationTarget === 'string'){
            if(Object.hasOwn(views, navigationTarget)){
                state.activeView = navigationTarget
                render()
            }
            return
        }

        const managedDirectoryButton = target.closest?.('[data-managed-directory]')
        if(managedDirectoryButton != null){
            await settingController.handleManagedDirectoryClick({ currentTarget: managedDirectoryButton })
            return
        }

        switch(target.id){
        case 'landing-login-button':
            await launcherController.handleLaunchButtonClick()
            break
        case 'sign-in-button':
            await handleSignIn()
            break
        case 'sign-out-button':
            await handleSignOut()
            break
        case 'settings-reset-button':
            settingController.promptResetSettings()
            break
        case 'data-directory-input':
            await settingController.handleSelectDataDirectory()
            break
        default:
            break
        }
    } catch (error) {
        showOverlay({
            title: '작업 실패',
            body: overlayParagraph(error.message)
        })
    }
}

function bindPersistentActions(){
    const viewHost = document.getElementById('view-host')
    const accountSwitcherDialog = document.getElementById('account-switcher-dialog')

    bindLaunchReadyEasterEggFall()
    document.getElementById('account-switcher-trigger').onclick = openAccountSwitcher
    document.getElementById('account-switcher-close').onclick = closeAccountSwitcher
    document.getElementById('account-switcher-add').onclick = () => {
        closeAccountSwitcher()
        void handleSignIn()
    }
    document.getElementById('account-switcher-list').addEventListener('click', event => {
        const option = event.target.closest?.('[data-account-profile-id]')

        if(option != null){
            void handleSelectAccount(option.dataset.accountProfileId)
        }
    })
    accountSwitcherDialog.addEventListener('cancel', event => {
        event.preventDefault()
        closeAccountSwitcher()
    })
    accountSwitcherDialog.addEventListener('click', event => {
        if(event.target === accountSwitcherDialog){
            closeAccountSwitcher()
        }
    })
    accountSwitcherDialog.addEventListener('transitionend', event => {
        if(event.target !== accountSwitcherDialog || event.propertyName !== 'opacity'){
            return
        }

        if(accountSwitcherDialog.classList.contains('is-closing')){
            finishAccountSwitcherExit(accountSwitcherDialog)
        }
    })
    viewHost.addEventListener('click', event => {
        void handleDelegatedViewClick(event)
    })
    viewHost.addEventListener('submit', event => {
        if(event.target.id === 'settings-form'){
            void settingController.handleSaveSettings(event)
        }
    })

    document.getElementById('window-minimize-button').onclick = () => {
        window.starPrisonLauncher.minimizeWindow()
    }
    document.getElementById('window-maximize-button').onclick = async () => {
        state.windowState = await window.starPrisonLauncher.toggleMaximizeWindow()
        renderWindowControls()
    }
    document.getElementById('window-close-button').onclick = () => {
        window.starPrisonLauncher.closeWindow()
    }
}

async function initialize(){
    state.activeView = getInitialView()
    bindAcceleratedPageScroll()
    bindBackgroundParallax()
    bindPersistentActions()

    window.starPrisonLauncher.onWindowStateChanged(nextWindowState => {
        state.windowState = nextWindowState
        renderWindowControls()
    })
    window.starPrisonLauncher.onLaunchStateChanged(launcherController.handleLaunchStateChanged)

    state.bootstrap = await window.starPrisonLauncher.getBootstrap()
    state.windowState = await window.starPrisonLauncher.getWindowState()
    preloadAccountSkin()
    await refreshNoticeCards()
    render()
    window.setInterval(() => {
        if(state.bootstrap != null){
            refreshNoticeCards()
        }
    }, NOTICE_REFRESH_INTERVAL_MS)

    if(getOverlayDemo() === 'liquid'){
        showOverlay(buildLiquidDemoOverlay())
        return
    }

    if(state.bootstrap.fatalError != null){
        showOverlay({
            title: '초기화 오류',
            body: overlayParagraph(state.bootstrap.fatalError.message)
        })
    }
}

document.getElementById('overlay-close').addEventListener('click', dismissOverlay)
document.getElementById('overlay').addEventListener('click', async event => {
    const saveConfirmButton = event.target.closest?.('[data-confirm-save-settings]')

    if(saveConfirmButton != null){
        await settingController.confirmPendingSettingsSave()
        return
    }

    const saveCancelButton = event.target.closest?.('[data-cancel-save-settings]')

    if(saveCancelButton != null){
        dismissOverlay()
        return
    }

    const resetConfirmButton = event.target.closest?.('[data-confirm-reset-settings]')

    if(resetConfirmButton != null){
        await settingController.handleResetSettings()
        return
    }

    const terminateCancelButton = event.target.closest?.('[data-cancel-terminate-minecraft]')

    if(terminateCancelButton != null){
        dismissOverlay()
        return
    }

    const terminateConfirmButton = event.target.closest?.('[data-confirm-terminate-minecraft]')

    if(terminateConfirmButton != null){
        await launcherController.handleTerminateMinecraft()
        return
    }

    const clickedBackdrop = event.target.id === 'overlay' || event.target.classList?.contains('overlay-scrim')

    if(clickedBackdrop){
        dismissOverlay()
    }
})
document.getElementById('overlay').addEventListener('cancel', event => {
    event.preventDefault()
    dismissOverlay()
})

initialize()
    .catch(error => {
        showOverlay({
            title: '렌더러 초기화 실패',
            body: overlayParagraph(error.message)
        })
    })
    .finally(() => {
        window.starPrisonStartupIntro?.revealMain()
    })
