const exactMessageMap: Record<string, string> = {
  'Invalid captcha': '验证码错误',
  'Invalid username or password': '用户名或密码错误',
  'User is disabled': '该账号已被停用，请联系管理员',
  'Registration is disabled': '当前系统已关闭注册',
  'Username already exists': '用户名已存在',
  'Username length must be 3-32': '用户名长度需为 3 到 32 位',
  'Password length must be at least 6': '密码长度至少为 6 位',
  'Invalid 2FA code': '两步验证码错误',
  '2FA challenge expired': '登录验证已过期，请重新登录',
  '2FA is not enabled': '该账号未开启两步验证',
  '2FA required': '该账号需要进行两步验证',
  'Current password is incorrect': '当前密码不正确',
  'New password must be at least 6 characters': '新密码长度至少为 6 位',
  'Please setup 2FA first': '请先完成两步验证初始化',
  'Invalid verification code': '验证码错误',
  'Failed to load system settings': '读取系统配置失败',
}

export function mapAuthErrorMessage(message?: string, fallback = '操作失败') {
  if (!message) return fallback

  if (exactMessageMap[message]) {
    return exactMessageMap[message]
  }

  const lower = message.toLowerCase()

  if (lower.includes('captcha')) return '验证码错误'
  if (lower.includes('username') && lower.includes('password')) return '用户名或密码错误'
  if (lower.includes('2fa') && lower.includes('expired')) return '登录验证已过期，请重新登录'
  if (lower.includes('2fa')) return '两步验证失败'
  if (lower.includes('disabled')) return '该账号已被停用，请联系管理员'
  if (lower.includes('register')) return '注册失败'
  if (lower.includes('login')) return '登录失败'

  return message
}
