import { reactive } from "vue"

export type Locale = "zh" | "en"

const STORAGE_KEY = "dcx.locale"
export type Dict = Record<string, string>

const dicts: Record<Locale, Dict> = { zh: {}, en: {} }

function systemLocale(): Locale {
  const lang = (typeof navigator !== "undefined" ? navigator.language : "zh") || "zh"
  return lang.toLowerCase().startsWith("zh") ? "zh" : "en"
}

function loadLocale(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (saved === "zh" || saved === "en") return saved
  } catch {
    /* 忽略存储异常 */
  }
  return systemLocale()
}

export const i18n = reactive({
  locale: loadLocale() as Locale,
})

export function setLocale(next: Locale) {
  i18n.locale = next
  try {
    localStorage.setItem(STORAGE_KEY, next)
  } catch {
    /* 忽略存储异常 */
  }
  if (typeof document !== "undefined") {
    document.documentElement.setAttribute("lang", next === "zh" ? "zh-CN" : "en")
  }
}

/** 注册一份词典，供各组件分组维护 */
export function registerDict(locale: Locale, dict: Dict) {
  Object.assign(dicts[locale], dict)
}

function fill(text: string, params?: Record<string, string | number | undefined>) {
  if (!params) return text
  return text.replace(/\{(\w+)\}/g, (_, key: string) => {
    const value = params[key]
    return value === undefined || value === null ? "" : String(value)
  })
}

/** 取当前语言的文案；缺失时回退中文词典，再回退 key 本身 */
export function t(key: string, params?: Record<string, string | number | undefined>): string {
  const text = dicts[i18n.locale][key] ?? dicts.zh[key] ?? key
  return fill(text, params)
}

/** 判断词典里是否存在该 key（用于筛选枚举等场景） */
export function hasKey(key: string) {
  return key in dicts[i18n.locale] || key in dicts.zh
}

/**
 * 翻译后端错误。
 * 后端按 `code|参数0|参数1` 的形式返回，前端按 code 取文案并用 {p0}/{p1} 填充；
 * 未登记的错误原样返回（例如操作系统自身的英文错误）。
 */
export function tErr(raw: unknown): string {
  const message = raw instanceof Error ? raw.message : String(raw ?? "")
  if (!message) return message
  const [code, ...args] = message.split("|")
  if (!hasKey(code)) return message
  const params: Record<string, string> = {}
  args.forEach((value, index) => {
    params[`p${index}`] = value
  })
  return t(code, params)
}
