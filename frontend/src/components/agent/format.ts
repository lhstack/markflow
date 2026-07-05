// 时间格式化小工具，供会话列表与 MCP 状态共用。

export function formatSessionTime(timestamp: number): string {
  const date = new Date(timestamp)
  const MM = `${date.getMonth() + 1}`.padStart(2, '0')
  const dd = `${date.getDate()}`.padStart(2, '0')
  const hh = `${date.getHours()}`.padStart(2, '0')
  const mm = `${date.getMinutes()}`.padStart(2, '0')
  return `${MM}/${dd} ${hh}:${mm}`
}

export function formatOptionalTimestamp(value: number | null): string {
  if (!value) return '未同步'
  return formatSessionTime(value)
}
