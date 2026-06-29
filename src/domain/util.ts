export function uuid(): string {
    return crypto.randomUUID()
}

/** 稳健深拷贝：我们的配置都是 JSON 安全数据，避免 structuredClone 对响应式对象报错。 */
export function clone<T>(v: T): T {
    return JSON.parse(JSON.stringify(v))
}

/**
 * 用默认模板填充后端返回的（可能被裁剪过的）数据：
 * 从 `def` 出发，凡 `raw` 中有定义的字段就覆盖，嵌套对象递归填充。
 * 这样模板里可以安全访问深层字段，而后端导出时仍是精简 JSON。
 */
export function withDefaults<T>(def: T, raw: unknown): T {
    if (raw == null || typeof raw !== 'object') return def
    if (Array.isArray(def)) return (raw as T) ?? def
    const out: Record<string, unknown> = { ...(def as Record<string, unknown>) }
    const r = raw as Record<string, unknown>
    for (const k of Object.keys(out)) {
        const dv = out[k]
        const rv = r[k]
        if (rv === undefined) continue
        if (
            dv != null &&
            typeof dv === 'object' &&
            !Array.isArray(dv) &&
            rv != null &&
            typeof rv === 'object' &&
            !Array.isArray(rv)
        ) {
            out[k] = withDefaults(dv, rv)
        } else {
            out[k] = rv
        }
    }
    return out as T
}
