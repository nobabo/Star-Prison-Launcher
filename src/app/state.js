export function createInitialState(){
    return {
        activeView: 'landing',
        bootstrap: null,
        backgroundImageUrl: null,
        noticeCards: null,
        noticeError: null,
        noticeRequestId: 0,
        minecraftProcessId: null,
        pendingAction: null,
        pendingSettingsPatch: null,
        windowState: {
            maximized: false
        }
    }
}
