<template>
  <div class="expired-page">
    <div class="expired-card">
      <div class="expired-icon">
        <svg width="64" height="64" viewBox="0 0 64 64" fill="none">
          <circle cx="32" cy="32" r="30" stroke="#f85149" stroke-width="2"/>
          <path d="M32 16 L32 36" stroke="#f85149" stroke-width="3" stroke-linecap="round"/>
          <circle cx="32" cy="46" r="2" fill="#f85149"/>
        </svg>
      </div>
      <h1 class="expired-title">分享链接已过期</h1>
      <p class="expired-desc">
        此分享链接已超出有效期，<br>请联系分享者获取新的链接。
      </p>
      <router-link to="/">
        <el-button type="primary" size="large">返回首页</el-button>
      </router-link>
    </div>
  </div>
</template>

<style scoped>
.expired-page {
  min-height: 100vh;
  background: var(--color-bg);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
}

.expired-card {
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
  max-width: 360px;
}

.expired-icon {
  margin-bottom: 8px;
}

.expired-title {
  font-size: 24px;
  font-weight: 600;
  color: var(--color-text);
}

.expired-desc {
  font-size: 15px;
  color: var(--color-text-secondary);
  line-height: 1.6;
  margin-bottom: 8px;
}

a {
  text-decoration: none;
}
</style>
