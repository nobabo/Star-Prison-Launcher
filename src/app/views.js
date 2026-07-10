import { renderDeveloperView } from '../views/developer.js'
import { renderLandingView } from '../views/landing.js'
import { renderLoginView } from '../views/login.js'
import { renderNoticesView } from '../views/notices.js'
import { renderSettingsView } from '../views/settings.js'

export const views = {
    landing: renderLandingView,
    login: renderLoginView,
    notices: renderNoticesView,
    developer: renderDeveloperView,
    settings: renderSettingsView
}
