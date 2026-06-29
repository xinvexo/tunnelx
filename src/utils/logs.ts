export type LogLevel = 'error' | 'warn' | 'info' | 'debug'

export function detectLogLevel(text: string): LogLevel {
  if (/\[(?:E|F|ERROR|FATAL)\]/.test(text) || /(?:\b(?:fatal|panic|error|failed|failure)\b|start error:)/i.test(text)) return 'error'
  if (/\[(?:W|WARN|WARNING)\]/.test(text) || /(?:\b(?:warn|warning|timeout|unavailable|not listening|already exists)\b|\bself-check\b)/i.test(text)) return 'warn'
  if (/\[(?:D|T|DEBUG|TRACE)\]/.test(text) || /\b(?:debug|trace)\b/i.test(text)) return 'debug'
  return 'info'
}
