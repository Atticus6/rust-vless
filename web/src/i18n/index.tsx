/**
 * 轻量国际化。
 *
 * `zh.ts` 是唯一事实来源，`en.ts` 以 `Record<MessageKey, string>` 声明，
 * 因此 key 集合在编译期保持一致；`t()` 只接受已声明的 key，拼错会直接报错。
 * 语言选择保存在 localStorage，首次访问按浏览器语言自动判断。
 */
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from 'react'
import { en } from './en'
import { zh, type MessageKey, type MessageVars } from './zh'

export type Lang = 'zh' | 'en'
export type { MessageKey, MessageVars }

const LANG_KEY = 'rv_lang'

/**
 * 读取语言偏好。浏览器禁用站点存储时访问 localStorage 会抛
 * `SecurityError`，这里读取失败就回退到浏览器语言，不能阻断渲染。
 *
 * `storage` 参数便于测试注入会抛错的实现。
 */
export function detectLang(
  storage: Pick<Storage, 'getItem'> | null = defaultStorage(),
): Lang {
  let stored: string | null = null
  try {
    stored = storage?.getItem(LANG_KEY) ?? null
  } catch {
    // 存储不可用：继续按浏览器语言判断。
  }
  if (stored === 'zh' || stored === 'en') return stored
  // 纯 Node（bun test 无 DOM）下 navigator 不存在，默认中文。
  const browser = typeof navigator === 'undefined' ? 'zh' : (navigator.language ?? 'zh')
  return browser.toLowerCase().startsWith('zh') ? 'zh' : 'en'
}

/** 持久化语言偏好；写入失败（配额/隐私模式）只影响下次启动，不影响当前切换。 */
export function persistLang(
  lang: Lang,
  storage: Pick<Storage, 'setItem'> | null = defaultStorage(),
): void {
  try {
    storage?.setItem(LANG_KEY, lang)
  } catch {
    // 忽略存储错误：内存状态仍会更新。
  }
}

/** 安全获取 localStorage：部分浏览器在访问属性时就抛错。 */
function defaultStorage(): Storage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage
  } catch {
    return null
  }
}

/**
 * 按语言取文案并替换 `{name}` 占位符。
 *
 * 导出为纯函数，便于单元测试（不依赖 React 上下文）。
 */
export function translate(lang: Lang, key: MessageKey, vars?: MessageVars): string {
  let text = lang === 'en' ? en[key] : zh[key]
  if (vars) {
    for (const [name, value] of Object.entries(vars)) {
      text = text.split(`{${name}}`).join(String(value))
    }
  }
  return text
}

type I18nValue = {
  lang: Lang
  /** 传给 Intl / toLocaleString 的 locale。 */
  locale: string
  setLang: (lang: Lang) => void
  /** 取文案；`vars` 用于替换 `{name}` 占位符。 */
  t: (key: MessageKey, vars?: MessageVars) => string
}

const I18nContext = createContext<I18nValue | null>(null)

/** 没有 Provider（例如单元测试）时按中文渲染，避免组件直接抛错。 */
const fallback: I18nValue = {
  lang: 'zh',
  locale: 'zh-CN',
  setLang: () => {},
  t: (key, vars) => translate('zh', key, vars),
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(() => detectLang())

  const setLang = useCallback((next: Lang) => {
    persistLang(next)
    setLangState(next)
  }, [])

  const t = useCallback((key: MessageKey, vars?: MessageVars) => translate(lang, key, vars), [lang])

  const value = useMemo<I18nValue>(
    () => ({
      lang,
      locale: lang === 'zh' ? 'zh-CN' : 'en-US',
      setLang,
      t,
    }),
    [lang, setLang, t],
  )

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>
}

export function useI18n(): I18nValue {
  return useContext(I18nContext) ?? fallback
}
