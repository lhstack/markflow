<template>
  <router-view v-slot="{ Component }">
    <transition name="fade" mode="out-in">
      <component :is="Component" />
    </transition>
  </router-view>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { useSystemStore } from '@/stores/system'

const system = useSystemStore()

onMounted(() => {
  void system.fetchPublicSettings().catch(() => {})
})
</script>
