const STARTUP_INTRO_STORAGE_KEY = 'star-prison.startup-intro.seen'
const STARTUP_INTRO_MIN_VISIBLE_MS = 2100
const STARTUP_INTRO_EXIT_MS = 720

const root = document.documentElement
const startedAt = performance.now()
const query = new URLSearchParams(window.location.search)
const forcePreview = query.get('intro') === 'preview'
const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches

function hasSeenStartupIntro(){
    try {
        return window.localStorage.getItem(STARTUP_INTRO_STORAGE_KEY) === 'true'
    } catch {
        return false
    }
}

const shouldPlay = forcePreview || !hasSeenStartupIntro()
root.classList.add(shouldPlay ? 'startup-intro-active' : 'startup-intro-skipped')

let revealPromise = null

function rememberStartupIntro(){
    if(forcePreview){
        return
    }

    try {
        window.localStorage.setItem(STARTUP_INTRO_STORAGE_KEY, 'true')
    } catch {
        // A disabled WebView storage keeps the intro available on the next launch.
    }
}

function revealMain(){
    if(!shouldPlay || forcePreview){
        return Promise.resolve()
    }

    if(revealPromise != null){
        return revealPromise
    }

    revealPromise = new Promise(resolve => {
        const minimumDuration = reducedMotion ? 120 : STARTUP_INTRO_MIN_VISIBLE_MS
        const remainingDuration = Math.max(0, minimumDuration - (performance.now() - startedAt))

        window.setTimeout(() => {
            rememberStartupIntro()
            root.classList.add('startup-intro-leaving')

            window.setTimeout(() => {
                document.getElementById('startup-intro')?.remove()
                root.classList.remove('startup-intro-active', 'startup-intro-leaving')
                root.classList.add('startup-intro-skipped')
                resolve()
            }, reducedMotion ? 20 : STARTUP_INTRO_EXIT_MS)
        }, remainingDuration)
    })

    return revealPromise
}

window.starPrisonStartupIntro = Object.freeze({ revealMain })
