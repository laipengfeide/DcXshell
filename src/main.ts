import { createApp } from "vue"
import ElementPlus from "element-plus"
import "element-plus/dist/index.css"
import "element-plus/theme-chalk/dark/css-vars.css"
import "@xterm/xterm/css/xterm.css"
import "./styles/global.css"
import "./i18n"
import App from "./App.vue"

document.documentElement.classList.add("dark")

// 桌面客户端里屏蔽 WebView 默认右键菜单（刷新 / 另存为 / 打印等），右键交互统一使用应用内自定义菜单
document.addEventListener("contextmenu", (event) => event.preventDefault(), { capture: true })

createApp(App).use(ElementPlus).mount("#app")
