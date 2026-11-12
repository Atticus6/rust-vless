/**
 * 国际化纯函数测试。
 *
 * 覆盖 key 类型安全之外的运行时行为：中英查表、占位符替换、存储读取失败
 * 时回退浏览器语言、写入失败不抛错。
 */
import { describe, expect, it } from 'bun:test'
import { detectLang, persistLang, translate } from './index'
import { en } from './en'
import { zh } from './zh'

describe('translate', () => {
  it('returns the source text for Chinese', () => {
    expect(translate('zh', 'nav.overview')).toBe('总览')
  })

  it('returns the translation for English', () => {
    expect(translate('en', 'nav.overview')).toBe('Overview')
  })

  it('replaces every {name} placeholder', () => {
    expect(translate('zh', 'nodes.deleteConfirm', { name: 'tokyo' })).toBe('删除节点 tokyo？')
    expect(translate('en', 'nodes.deleteConfirm', { name: 'tokyo' })).toBe('Delete node tokyo?')
  })

  it('keeps placeholders that were not provided', () => {
    expect(translate('en', 'nodes.deleteConfirm')).toBe('Delete node {name}?')
  })

  it('declares the same keys for both languages', () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(zh).sort())
  })

  it('has no empty translations', () => {
    for (const [key, value] of Object.entries(en)) {
      expect(value.length, key).toBeGreaterThan(0)
    }
  })
})

describe('detectLang', () => {
  it('prefers a stored preference', () => {
    expect(detectLang({ getItem: () => 'en' })).toBe('en')
    expect(detectLang({ getItem: () => 'zh' })).toBe('zh')
  })

  it('ignores unknown stored values', () => {
    expect(['zh', 'en']).toContain(detectLang({ getItem: () => 'fr' }))
  })

  it('falls back when reading storage throws', () => {
    const throwing = {
      getItem: () => {
        throw new Error('SecurityError')
      },
    }
    expect(['zh', 'en']).toContain(detectLang(throwing as unknown as Pick<Storage, 'getItem'>))
  })

  it('falls back when storage is unavailable', () => {
    expect(['zh', 'en']).toContain(detectLang(null))
  })
})

describe('persistLang', () => {
  it('writes the preference', () => {
    const written: string[] = []
    persistLang('en', {
      setItem: (key: string, value: string) => {
        written.push(`${key}=${value}`)
      },
    } as unknown as Pick<Storage, 'setItem'>)
    expect(written).toEqual(['rv_lang=en'])
  })

  it('does not throw when writing fails', () => {
    const throwing = {
      setItem: () => {
        throw new Error('QuotaExceededError')
      },
    }
    expect(() => persistLang('en', throwing as unknown as Pick<Storage, 'setItem'>)).not.toThrow()
  })
})
