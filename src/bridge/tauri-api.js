import { invoke } from '../vendor/tauri/core.js'
import { listen } from '../vendor/tauri/event.js'

function onTauriEvent(eventName, callback){
    let unlisten = null
    let disposed = false

    listen(eventName, event => {
        callback(event.payload)
    }).then(value => {
        if(disposed){
            value()
            return
        }
        unlisten = value
    })

    return () => {
        disposed = true
        if(unlisten != null){
            unlisten()
            unlisten = null
        }
    }
}

export function createLauncherBridge(){
    return {
        getInitialView(){
            return ''
        },
        getOverlayDemo(){
            return ''
        },
        getBootstrap(){
            return invoke('get_bootstrap')
        },
        signIn(){
            return invoke('sign_in')
        },
        signOut(){
            return invoke('sign_out')
        },
        selectAccount(profileId){
            return invoke('select_account', { profileId })
        },
        selectDataDirectory(currentPath){
            return invoke('select_data_directory', { currentPath })
        },
        openManagedDirectory(kind){
            return invoke('open_managed_directory', { kind })
        },
        saveSettings(patch, options = {}){
            return invoke('save_settings', {
                patch,
                unsafeAcknowledged: options.unsafeAcknowledged === true
            })
        },
        launch(){
            return invoke('launch')
        },
        terminateMinecraft(){
            return invoke('terminate_minecraft')
        },
        onLaunchStateChanged(callback){
            return onTauriEvent('launcher:launch-state-changed', callback)
        },
        openExternal(url){
            return invoke('open_external', { url })
        },
        getWindowState(){
            return invoke('get_window_state')
        },
        minimizeWindow(){
            return invoke('minimize_window')
        },
        toggleMaximizeWindow(){
            return invoke('toggle_maximize_window')
        },
        closeWindow(){
            return invoke('close_window')
        },
        onWindowStateChanged(callback){
            return onTauriEvent('window:state-changed', callback)
        }
    }
}
