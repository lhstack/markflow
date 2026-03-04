import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import request from '@/utils/request'

export interface UserInfo {
  id: string
  username: string
  avatar?: string
  totp_enabled: boolean
}

export const useAuthStore = defineStore('auth', () => {
  const token = ref<string | null>(localStorage.getItem('token'))
  const user = ref<UserInfo | null>(JSON.parse(localStorage.getItem('user') || 'null'))

  const isLoggedIn = computed(() => !!token.value)

  function setAuth(t: string, u: UserInfo) {
    token.value = t
    user.value = u
    localStorage.setItem('token', t)
    localStorage.setItem('user', JSON.stringify(u))
  }

  function logout() {
    token.value = null
    user.value = null
    localStorage.removeItem('token')
    localStorage.removeItem('user')
  }

  function updateUser(u: Partial<UserInfo>) {
    if (user.value) {
      user.value = { ...user.value, ...u }
      localStorage.setItem('user', JSON.stringify(user.value))
    }
  }

  return { token, user, isLoggedIn, setAuth, logout, updateUser }
})
