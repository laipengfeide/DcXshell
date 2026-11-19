import { registerDict, setLocale, i18n } from "./core"
import zhBase from "./dicts/zh-base"
import enBase from "./dicts/en-base"
import zhWorkspace from "./dicts/zh-workspace"
import enWorkspace from "./dicts/en-workspace"
import zhLogs from "./dicts/zh-logs"
import enLogs from "./dicts/en-logs"
import zhDialogs from "./dicts/zh-dialogs"
import enDialogs from "./dicts/en-dialogs"
import zhBackend from "./dicts/zh-backend"
import enBackend from "./dicts/en-backend"

registerDict("zh", zhBase)
registerDict("en", enBase)
registerDict("zh", zhWorkspace)
registerDict("en", enWorkspace)
registerDict("zh", zhLogs)
registerDict("en", enLogs)
registerDict("zh", zhDialogs)
registerDict("en", enDialogs)
registerDict("zh", zhBackend)
registerDict("en", enBackend)

setLocale(i18n.locale)

export * from "./core"
