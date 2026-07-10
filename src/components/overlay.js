const overlayState = {
    exitResolver: null,
    exitTimer: null,
    isClosing: false
}

const OVERLAY_EXIT_DURATION_MS = 280

function getOverlayElements(){
    const dialog = document.getElementById('overlay')

    return {
        dialog,
        card: dialog?.querySelector('.overlay-card')
    }
}

function prefersReducedMotion(){
    return window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

function clearExitTimer(){
    if(overlayState.exitTimer != null){
        window.clearTimeout(overlayState.exitTimer)
        overlayState.exitTimer = null
    }
}

function resolvePendingExit(){
    const { exitResolver } = overlayState

    overlayState.exitResolver = null
    overlayState.isClosing = false
    clearExitTimer()
    exitResolver?.()
}

function finishOverlayExit(dialog){
    if(dialog?.open){
        dialog.close()
    }

    dialog?.classList.remove('is-visible', 'is-closing')
    resolvePendingExit()
}

function bindOverlayExit(){
    const { dialog, card } = getOverlayElements()

    if(dialog == null || card == null || card.dataset.exitBound === 'true'){
        return
    }

    card.dataset.exitBound = 'true'
    card.addEventListener('transitionend', event => {
        if(event.target !== card || event.propertyName !== 'opacity'){
            return
        }

        if(!dialog.classList.contains('is-closing')){
            return
        }

        finishOverlayExit(dialog)
    })
}

function startOverlayEntrance(dialog){
    dialog.classList.remove('is-closing')

    requestAnimationFrame(() => {
        dialog.classList.add('is-visible')
    })
}

function normalizeOverlayContent(content){
    if(content == null){
        return []
    }

    if(Array.isArray(content)){
        return content
    }

    return [content]
}

function replaceSafeChildren(host, content){
    host.replaceChildren()

    for(const child of normalizeOverlayContent(content)){
        if(child instanceof Node){
            host.appendChild(child)
            continue
        }

        host.appendChild(document.createTextNode(String(child)))
    }
}

export function showOverlay({ title, body, actions = [] }){
    const dialog = document.getElementById('overlay')
    const bodyHost = document.getElementById('overlay-body')
    const actionsHost = document.getElementById('overlay-actions')

    if(dialog == null || bodyHost == null || actionsHost == null){
        return
    }

    bindOverlayExit()
    clearExitTimer()

    document.getElementById('overlay-title').textContent = title
    replaceSafeChildren(bodyHost, body)
    replaceSafeChildren(actionsHost, actions)

    if(!dialog.open){
        dialog.showModal()
    }

    overlayState.isClosing = false
    dialog.classList.remove('is-closing')

    if(prefersReducedMotion()){
        dialog.classList.add('is-visible')
        return
    }

    dialog.classList.remove('is-visible')
    startOverlayEntrance(dialog)
}

export function hideOverlay(){
    const { dialog } = getOverlayElements()

    if(dialog == null || !dialog.open){
        return Promise.resolve()
    }

    if(overlayState.isClosing){
        return new Promise(resolve => {
            const previousResolver = overlayState.exitResolver
            overlayState.exitResolver = () => {
                previousResolver?.()
                resolve()
            }
        })
    }

    if(prefersReducedMotion()){
        finishOverlayExit(dialog)
        return Promise.resolve()
    }

    overlayState.isClosing = true
    dialog.classList.remove('is-visible')
    dialog.classList.add('is-closing')

    return new Promise(resolve => {
        overlayState.exitResolver = resolve
        overlayState.exitTimer = window.setTimeout(() => {
            if(dialog.classList.contains('is-closing')){
                finishOverlayExit(dialog)
            }
        }, OVERLAY_EXIT_DURATION_MS + 80)
    })
}
