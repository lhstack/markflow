<template>
  <div class="welcome">
    <div class="welcome-hero">
      <svg width="56" height="56" viewBox="0 0 40 40" fill="none">
        <rect width="40" height="40" rx="10" fill="#238636"/>
        <path d="M8 28 L8 12 L16 20 L24 12 L24 28" stroke="white" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
        <path d="M26 16 L32 16 M26 20 L30 20 M26 24 L32 24" stroke="white" stroke-width="2" stroke-linecap="round"/>
      </svg>
      <div>
        <h1>MarkFlow</h1>
        <p>选择左侧文档开始编辑，或新建一篇</p>
      </div>
    </div>
    <div class="feat-grid">
      <div class="feat-item" v-for="f in features" :key="f.title">
        <div class="feat-icon" :style="{ background: f.bg }">{{ f.icon }}</div>
        <div>
          <div class="feat-title">{{ f.title }}</div>
          <div class="feat-desc">{{ f.desc }}</div>
        </div>
      </div>
    </div>
    <div class="shortcuts">
      <div class="shortcuts-title">快捷键</div>
      <div class="shortcut-list">
        <div class="shortcut-item" v-for="s in shortcuts" :key="s.label">
          <span class="shortcut-keys"><kbd v-for="k in s.keys" :key="k">{{ k }}</kbd></span>
          <span>{{ s.label }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
const features = [
  { icon: '✦', bg: 'rgba(88,166,255,0.1)',  title: 'Markdown 编辑', desc: 'CodeMirror 6 驱动，语法高亮' },
  { icon: '⬡', bg: 'rgba(46,160,67,0.1)',   title: '实时预览',     desc: '分屏预览，所见即所得' },
  { icon: '◈', bg: 'rgba(210,153,34,0.1)',  title: '灵活分享',     desc: '密码保护、有效期设置' },
  { icon: '◉', bg: 'rgba(248,81,73,0.1)',   title: '两步验证',     desc: 'TOTP 双因素身份认证' },
]
const shortcuts = [
  { keys: ['Ctrl', 'S'], label: '保存' },
  { keys: ['Ctrl', 'B'], label: '粗体' },
  { keys: ['Ctrl', 'I'], label: '斜体' },
  { keys: ['Tab'], label: '缩进' },
]
</script>

<style scoped>
.welcome { display:flex; flex-direction:column; align-items:center; justify-content:center; min-height:100%; padding:48px 40px; gap:32px; }
.welcome-hero { display:flex; align-items:center; gap:16px; }
.welcome-hero h1 { font-size:22px; font-weight:700; margin-bottom:4px; }
.welcome-hero p { font-size:13px; color:var(--text3); }
.feat-grid { display:grid; grid-template-columns:1fr 1fr; gap:10px; max-width:480px; width:100%; }
.feat-item { display:flex; align-items:flex-start; gap:12px; padding:14px 16px; background:var(--bg2); border:1px solid var(--border); border-radius:var(--r-md); transition:border-color .15s; }
.feat-item:hover { border-color:var(--border2); }
.feat-icon { width:32px; height:32px; border-radius:var(--r-sm); display:flex; align-items:center; justify-content:center; font-size:16px; flex-shrink:0; }
.feat-title { font-size:13px; font-weight:600; margin-bottom:2px; }
.feat-desc { font-size:11px; color:var(--text3); }
.shortcuts { max-width:480px; width:100%; }
.shortcuts-title { font-size:11px; font-weight:600; text-transform:uppercase; letter-spacing:.6px; color:var(--text3); margin-bottom:8px; }
.shortcut-list { display:flex; gap:6px; flex-wrap:wrap; }
.shortcut-item { display:flex; align-items:center; gap:6px; padding:5px 10px; background:var(--bg2); border:1px solid var(--border); border-radius:var(--r-sm); font-size:12px; color:var(--text3); }
.shortcut-keys { display:flex; gap:3px; }
kbd { padding:1px 5px; background:var(--bg3); border:1px solid var(--border2); border-radius:3px; font-size:11px; font-family:var(--mono); color:var(--text2); }
@media (max-width:600px) { .feat-grid { grid-template-columns:1fr; } .welcome { padding:32px 20px; } }
</style>
