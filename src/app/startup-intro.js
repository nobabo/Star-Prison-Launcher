const STARTUP_INTRO_STORAGE_KEY = 'star-prison.startup-intro.last-boot-session'
const STARTUP_INTRO_MIN_VISIBLE_MS = 2100
const STARTUP_INTRO_EXIT_MS = 1200
const FALLBACK_BOOT_SESSION_ID = 'legacy-session'

const root = document.documentElement
const startedAt = performance.now()
const query = new URLSearchParams(window.location.search)
const forcePreview = query.get('intro') === 'preview'
const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches
const startupContext = window.__STAR_PRISON_STARTUP__ ?? {}
const bootSessionId = typeof startupContext.bootSessionId === 'string' && startupContext.bootSessionId.length > 0
    ? startupContext.bootSessionId
    : FALLBACK_BOOT_SESSION_ID

function hasSeenStartupIntro(){
    try {
        return window.localStorage.getItem(STARTUP_INTRO_STORAGE_KEY) === bootSessionId
    } catch {
        return false
    }
}

const shouldPlay = forcePreview || !hasSeenStartupIntro()
root.classList.add(shouldPlay ? 'startup-intro-active' : 'startup-intro-skipped')

let revealPromise = null
let revealTimer = null
let exitTimer = null

function cancelRevealTimers(){
    if(revealTimer != null){
        window.clearTimeout(revealTimer)
        revealTimer = null
    }

    if(exitTimer != null){
        window.clearTimeout(exitTimer)
        exitTimer = null
    }
}

function rememberStartupIntro(){
    if(forcePreview){
        return
    }

    try {
        window.localStorage.setItem(STARTUP_INTRO_STORAGE_KEY, bootSessionId)
    } catch {
        // A disabled WebView storage keeps the intro available on the next launch.
    }
}

function resetIntroAnimation(){
    const intro = document.getElementById('startup-intro')

    if(intro == null){
        return
    }

    intro.querySelectorAll('.startup-intro__emblem, .startup-intro__scan').forEach(element => {
        element.style.animation = 'none'
    })
    void intro.offsetWidth
    intro.querySelectorAll('.startup-intro__emblem, .startup-intro__scan').forEach(element => {
        element.style.animation = ''
    })
}

function revealMain({ replay = false } = {}){
    if((!shouldPlay && !replay) || (forcePreview && !replay)){
        return Promise.resolve()
    }

    if(replay){
        cancelRevealTimers()
        revealPromise = null
        root.classList.remove('startup-intro-skipped', 'startup-intro-leaving')
        root.classList.add('startup-intro-active')
        resetIntroAnimation()
    }

    if(revealPromise != null){
        return revealPromise
    }

    revealPromise = new Promise(resolve => {
        const minimumDuration = reducedMotion ? 120 : STARTUP_INTRO_MIN_VISIBLE_MS
        const cycleStartedAt = replay ? performance.now() : startedAt
        const remainingDuration = Math.max(0, minimumDuration - (performance.now() - cycleStartedAt))

        revealTimer = window.setTimeout(() => {
            revealTimer = null
            if(!replay){
                rememberStartupIntro()
            }
            root.classList.add('startup-intro-leaving')

            exitTimer = window.setTimeout(() => {
                exitTimer = null
                root.classList.remove('startup-intro-active', 'startup-intro-leaving')
                root.classList.add('startup-intro-skipped')
                resolve()
            }, reducedMotion ? 20 : STARTUP_INTRO_EXIT_MS)
        }, remainingDuration)
    })

    return revealPromise
}

function replayMain(){
    return revealMain({ replay: true })
}

window.starPrisonStartupIntro = Object.freeze({ revealMain, replayMain })
